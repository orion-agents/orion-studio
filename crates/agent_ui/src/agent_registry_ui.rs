use std::ops::Range;

use agent_settings::AgentSettings;
use chrono::{DateTime, Utc};
use client::zed_urls;
use collections::HashMap;
use editor::{Editor, EditorElement, EditorStyle};
use fs::Fs;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Focusable, KeyContext, ParentElement, Render,
    RenderOnce, SharedString, Styled, TaskExt, TextStyle, UniformListScrollHandle, Window, point,
    uniform_list,
};
use project::agent_registry_store::ORION_CODE_AGENT_ID;
use project::agent_server_store::{AllAgentServersSettings, CustomAgentServerSettings};
use project::{AgentRegistryStore, RegistryAgent};
use settings::{
    OrionCodeUpdateChannel as SettingsOrionCodeUpdateChannel,
    OrionCodeUpdateMode as SettingsOrionCodeUpdateMode, Settings, SettingsStore,
    update_settings_file,
};
use theme_settings::ThemeSettings;
use ui::{
    ButtonStyle, ScrollableHandle, ToggleButtonGroup, ToggleButtonGroupSize,
    ToggleButtonGroupStyle, ToggleButtonSimple, Tooltip, WithScrollbar, prelude::*,
};
use workspace::{
    Workspace,
    item::{Item, ItemEvent},
};

use crate::orion_code_update::{
    OrionCodeCurrentReleaseState, OrionCodeInstallSource, OrionCodeReleaseStatus,
    OrionCodeUpdateChannel, OrionCodeUpdateErrorKind, OrionCodeUpdateManagementState,
    OrionCodeUpdateMode, OrionCodeUpdateRecordV2, OrionCodeUpdateResolution,
};
use crate::{
    OrionCodeActivationCoordinator, OrionCodeActivationStatus, OrionCodeBootstrap,
    OrionCodeBootstrapChoice, OrionCodeBootstrapPhase, OrionCodeUpdateCoordinator,
    OrionCodeUpdateCoordinatorStatus, OrionCodeUpdateDisabledReason, OrionCodeUpdatePipeline,
    OrionCodeUpdatePipelineStatus,
};
use orion_code_update::{MAX_ARCHIVE_BYTES, OrionCodeSigningRequirement};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistryFilter {
    All,
    Installed,
    NotInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistryInstallStatus {
    NotInstalled,
    InstalledRegistry,
    InstalledCustom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrionCodeUpdatePreference {
    Stable,
    Beta,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrionCodePipelineAction {
    Download,
    Retry,
}

impl OrionCodeUpdatePreference {
    fn from_effective_settings(settings: &AgentSettings) -> Self {
        match settings.orion_code_update_mode {
            SettingsOrionCodeUpdateMode::Manual => Self::Manual,
            SettingsOrionCodeUpdateMode::Automatic => match settings.orion_code_update_channel {
                SettingsOrionCodeUpdateChannel::Stable => Self::Stable,
                SettingsOrionCodeUpdateChannel::Beta => Self::Beta,
            },
        }
    }

    fn selected_index(self) -> usize {
        match self {
            Self::Stable => 0,
            Self::Beta => 1,
            Self::Manual => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OrionCodeUpdateUiState {
    current: String,
    candidate: String,
    update_status: String,
    pipeline_status: String,
    activation_status: String,
    last_check: String,
    next_check: String,
    code_signing: String,
    notarization: String,
    diagnostic_code: Option<String>,
    pipeline_action: Option<OrionCodePipelineAction>,
    pipeline_button_label: &'static str,
    can_check_now: bool,
    can_activate_when_idle: bool,
    can_use_previous: bool,
    can_use_orion_agent: bool,
    waiting_for_idle: bool,
    needs_attention: bool,
}

impl OrionCodeUpdateUiState {
    fn from_globals(installed: bool, cx: &App) -> Self {
        let settings = AgentSettings::get_global(cx);
        let update_mode = match settings.orion_code_update_mode {
            SettingsOrionCodeUpdateMode::Automatic => OrionCodeUpdateMode::Automatic,
            SettingsOrionCodeUpdateMode::Manual => OrionCodeUpdateMode::Manual,
        };

        let coordinator = OrionCodeUpdateCoordinator::try_global(cx);
        let (record, coordinator_status, resolution) = coordinator.as_ref().map_or_else(
            || (OrionCodeUpdateRecordV2::default(), None, None),
            |coordinator| {
                let coordinator = coordinator.read(cx);
                (
                    coordinator.record().clone(),
                    Some(coordinator.status().clone()),
                    coordinator.latest_resolution().cloned(),
                )
            },
        );
        let activation_status = OrionCodeActivationCoordinator::try_global(cx)
            .map(|coordinator| coordinator.read(cx).status().clone());
        let pipeline = OrionCodeUpdatePipeline::try_global(cx);
        let pipeline_status = pipeline
            .as_ref()
            .map(|pipeline| pipeline.read(cx).status().clone());

        let update_status = update_status_copy(coordinator_status.as_ref(), update_mode);
        let pipeline_status_copy = pipeline_status_copy(pipeline_status.as_ref());
        let activation_status_copy = activation_status_copy(activation_status.as_ref());
        let candidate = candidate_copy(resolution.as_ref());
        let (code_signing, notarization) = trust_diagnostic_copy(&record, resolution.as_ref());
        let current_release_state = resolution
            .as_ref()
            .map(|resolution| resolution.current_release_state)
            .unwrap_or(OrionCodeCurrentReleaseState::Unknown);
        let coordinator_diagnostic = coordinator_status
            .as_ref()
            .and_then(coordinator_diagnostic_code);
        let activation_diagnostic = activation_status
            .as_ref()
            .and_then(activation_diagnostic_code);
        let pipeline_diagnostic = pipeline_status.as_ref().and_then(pipeline_diagnostic_code);
        let diagnostic_code = combine_diagnostic_codes(
            coordinator_diagnostic.as_deref(),
            activation_diagnostic.as_deref(),
            pipeline_diagnostic.as_deref(),
            record.last_error_kind,
        );
        let record_allows_updates = record.choice == OrionCodeBootstrapChoice::Accepted
            && record.management_state == OrionCodeUpdateManagementState::Managed;
        let can_check_now = installed
            && record_allows_updates
            && matches!(
                coordinator_status.as_ref(),
                Some(
                    OrionCodeUpdateCoordinatorStatus::Idle
                        | OrionCodeUpdateCoordinatorStatus::Scheduled { .. }
                        | OrionCodeUpdateCoordinatorStatus::ManualCheckFailed { .. }
                )
            );
        let can_activate_when_idle = installed
            && record.staged.is_some()
            && matches!(
                activation_status.as_ref(),
                Some(OrionCodeActivationStatus::Staged { .. })
            );
        let waiting_for_idle = matches!(
            activation_status.as_ref(),
            Some(OrionCodeActivationStatus::WaitingForIdle { .. })
        );
        let pipeline_action = match pipeline_status.as_ref() {
            Some(OrionCodeUpdatePipelineStatus::CandidateAvailable { .. }) => {
                Some(OrionCodePipelineAction::Download)
            }
            Some(OrionCodeUpdatePipelineStatus::Failed {
                version: Some(_), ..
            }) => Some(OrionCodePipelineAction::Retry),
            _ => None,
        };
        let pipeline_button_label = match pipeline_status.as_ref() {
            Some(OrionCodeUpdatePipelineStatus::Downloading { .. }) => "Downloading…",
            Some(OrionCodeUpdatePipelineStatus::Staged { .. }) => "Downloaded",
            Some(OrionCodeUpdatePipelineStatus::Failed { .. }) => "Retry Download",
            _ => "Download",
        };
        let activation_is_busy = matches!(
            activation_status.as_ref(),
            Some(
                OrionCodeActivationStatus::StagingState { .. }
                    | OrionCodeActivationStatus::WaitingForIdle { .. }
                    | OrionCodeActivationStatus::Activating { .. }
            )
        );
        let pipeline_is_busy = matches!(
            pipeline_status.as_ref(),
            Some(OrionCodeUpdatePipelineStatus::Downloading { .. })
        );
        let can_use_previous = installed
            && !activation_is_busy
            && !pipeline_is_busy
            && previous_release_is_usable(&record);
        let can_use_orion_agent =
            installed && !activation_is_busy && !pipeline_is_busy && record.activation.is_none();
        let needs_attention = coordinator.is_none()
            || pipeline.is_none()
            || matches!(
                coordinator_status.as_ref(),
                Some(
                    OrionCodeUpdateCoordinatorStatus::Disabled { .. }
                        | OrionCodeUpdateCoordinatorStatus::ManualCheckFailed { .. }
                )
            )
            || matches!(
                activation_status.as_ref(),
                Some(
                    OrionCodeActivationStatus::Disabled { .. }
                        | OrionCodeActivationStatus::Failed { .. }
                )
            )
            || matches!(
                pipeline_status.as_ref(),
                Some(
                    OrionCodeUpdatePipelineStatus::Disabled { .. }
                        | OrionCodeUpdatePipelineStatus::Failed { .. }
                )
            )
            || matches!(
                current_release_state,
                OrionCodeCurrentReleaseState::Paused | OrionCodeCurrentReleaseState::Revoked
            );

        Self {
            current: current_version_copy(&record),
            candidate,
            update_status,
            pipeline_status: pipeline_status_copy,
            activation_status: activation_status_copy,
            last_check: record
                .last_successful_check_at
                .as_ref()
                .map(format_utc)
                .unwrap_or_else(|| "Never".to_string()),
            next_check: next_check_copy(coordinator_status.as_ref(), &record, update_mode),
            code_signing,
            notarization,
            diagnostic_code,
            pipeline_action,
            pipeline_button_label,
            can_check_now,
            can_activate_when_idle,
            can_use_previous,
            can_use_orion_agent,
            waiting_for_idle,
            needs_attention,
        }
    }
}

fn apply_orion_code_update_preference(
    content: &mut settings::SettingsContent,
    preference: OrionCodeUpdatePreference,
    current_channel: SettingsOrionCodeUpdateChannel,
) {
    let orion_code = content
        .agent
        .get_or_insert_default()
        .orion_code
        .get_or_insert_default();
    match preference {
        OrionCodeUpdatePreference::Stable => {
            orion_code.update_channel = Some(SettingsOrionCodeUpdateChannel::Stable);
            orion_code.update_mode = Some(SettingsOrionCodeUpdateMode::Automatic);
        }
        OrionCodeUpdatePreference::Beta => {
            orion_code.update_channel = Some(SettingsOrionCodeUpdateChannel::Beta);
            orion_code.update_mode = Some(SettingsOrionCodeUpdateMode::Automatic);
        }
        OrionCodeUpdatePreference::Manual => {
            orion_code.update_channel.get_or_insert(current_channel);
            orion_code.update_mode = Some(SettingsOrionCodeUpdateMode::Manual);
        }
    }
}

fn format_utc(time: &DateTime<Utc>) -> String {
    time.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn current_version_copy(record: &OrionCodeUpdateRecordV2) -> String {
    record.current_verified.as_ref().map_or_else(
        || "No verified managed version".to_string(),
        |receipt| match receipt.source {
            OrionCodeInstallSource::Archive => format!("v{} · managed archive", receipt.version),
            OrionCodeInstallSource::Npx => format!("v{} · registry fallback", receipt.version),
        },
    )
}

fn previous_release_is_usable(record: &OrionCodeUpdateRecordV2) -> bool {
    record.previous_verified.as_ref().is_some_and(|previous| {
        previous.source == OrionCodeInstallSource::Archive
            && !previous.legacy_imported
            && previous.target.is_some()
            && previous.archive_sha256.is_some()
            && previous.command.is_some()
            && previous.index_sequence.is_some()
            && !record.known_revoked_versions.contains(&previous.version)
            && record.activation.is_none()
    })
}

fn candidate_copy(resolution: Option<&OrionCodeUpdateResolution>) -> String {
    let Some(resolution) = resolution else {
        return "Not checked yet".to_string();
    };
    if let Some(candidate) = resolution.candidate.as_ref() {
        return match candidate.release.status {
            OrionCodeReleaseStatus::Active if candidate.is_signed_rollback => {
                format!("Verified rollback v{} is available", candidate.version)
            }
            OrionCodeReleaseStatus::Active
                if resolution.current_release_state == OrionCodeCurrentReleaseState::Revoked =>
            {
                format!(
                    "Verified replacement v{} is available; the current release is revoked",
                    candidate.version
                )
            }
            OrionCodeReleaseStatus::Active => {
                format!("Verified candidate v{} is available", candidate.version)
            }
            OrionCodeReleaseStatus::Paused => format!(
                "Candidate v{} is paused and will not be downloaded",
                candidate.version
            ),
            OrionCodeReleaseStatus::Revoked => format!(
                "Candidate v{} is revoked and cannot be activated",
                candidate.version
            ),
        };
    }

    match resolution.current_release_state {
        OrionCodeCurrentReleaseState::Unknown => "No verified candidate".to_string(),
        OrionCodeCurrentReleaseState::Active => "Current version is up to date".to_string(),
        OrionCodeCurrentReleaseState::Paused => {
            "Current release is paused; Orion Studio will not switch automatically".to_string()
        }
        OrionCodeCurrentReleaseState::Revoked => {
            "Current release is revoked; activation is blocked until a verified replacement is available"
                .to_string()
        }
    }
}

fn update_status_copy(
    status: Option<&OrionCodeUpdateCoordinatorStatus>,
    update_mode: OrionCodeUpdateMode,
) -> String {
    let Some(status) = status else {
        return "Update service is unavailable in this session".to_string();
    };
    match status {
        OrionCodeUpdateCoordinatorStatus::Initializing => {
            "Initializing managed update state".to_string()
        }
        OrionCodeUpdateCoordinatorStatus::Disabled { reason, .. } => {
            update_disabled_reason_copy(*reason).to_string()
        }
        OrionCodeUpdateCoordinatorStatus::Idle if update_mode == OrionCodeUpdateMode::Manual => {
            "Manual mode; no background check is scheduled".to_string()
        }
        OrionCodeUpdateCoordinatorStatus::Idle => "Ready for update checks".to_string(),
        OrionCodeUpdateCoordinatorStatus::Scheduled { .. } => {
            "Automatic check scheduled".to_string()
        }
        OrionCodeUpdateCoordinatorStatus::Checking { trigger, channel } => format!(
            "{} check in progress on the {} channel",
            match trigger {
                crate::OrionCodeUpdateCheckTrigger::Automatic => "Automatic",
                crate::OrionCodeUpdateCheckTrigger::Manual => "Manual",
            },
            match channel {
                OrionCodeUpdateChannel::Stable => "Stable",
                OrionCodeUpdateChannel::Beta => "Beta",
            }
        ),
        OrionCodeUpdateCoordinatorStatus::SavingCheckResult { .. } => {
            "Saving verified update metadata".to_string()
        }
        OrionCodeUpdateCoordinatorStatus::ManualCheckFailed { kind, .. } => {
            format!("Manual check failed safely: {}", update_error_copy(*kind))
        }
    }
}

fn update_disabled_reason_copy(reason: OrionCodeUpdateDisabledReason) -> &'static str {
    match reason {
        OrionCodeUpdateDisabledReason::NotAccepted => {
            "Install Orion Code before enabling managed updates"
        }
        OrionCodeUpdateDisabledReason::Declined => "Managed updates were declined",
        OrionCodeUpdateDisabledReason::Removed => "Orion Code was removed",
        OrionCodeUpdateDisabledReason::CustomConfiguration => {
            "Managed updates are disabled for a custom Orion Code configuration"
        }
        OrionCodeUpdateDisabledReason::SignedFeedUnavailable => {
            "Signed first-party update feed is not configured in this build"
        }
        OrionCodeUpdateDisabledReason::StateReadFailed => {
            "Update state could not be read; managed updates are disabled"
        }
        OrionCodeUpdateDisabledReason::StateMigrationFailed => {
            "Update state migration failed; managed updates are disabled"
        }
        OrionCodeUpdateDisabledReason::StatePersistenceFailed => {
            "Update state could not be saved; managed updates are disabled"
        }
        OrionCodeUpdateDisabledReason::GenerationExhausted => {
            "Update coordination needs an Orion Studio restart"
        }
        OrionCodeUpdateDisabledReason::AppQuitting => "Update checks are stopping",
    }
}

fn pipeline_status_copy(status: Option<&OrionCodeUpdatePipelineStatus>) -> String {
    let Some(status) = status else {
        return "Download pipeline is unavailable in this session".to_string();
    };
    match status {
        OrionCodeUpdatePipelineStatus::Disabled { .. } => {
            "Managed archive download is disabled in this build".to_string()
        }
        OrionCodeUpdatePipelineStatus::Idle => "No download pending".to_string(),
        OrionCodeUpdatePipelineStatus::CandidateAvailable {
            version,
            archive_bytes,
            signed_rollback,
            ..
        } => format!(
            "{} v{} is ready to download ({} MiB)",
            if *signed_rollback {
                "Verified rollback"
            } else {
                "Verified candidate"
            },
            version,
            archive_bytes.div_ceil(1024 * 1024)
        ),
        OrionCodeUpdatePipelineStatus::Downloading {
            version,
            received_bytes,
            total_bytes,
            ..
        } => format!(
            "Downloading v{} · {}",
            version,
            compact_download_progress(*received_bytes, *total_bytes)
        ),
        OrionCodeUpdatePipelineStatus::Staged { version } => {
            format!("v{version} was downloaded, verified, and staged")
        }
        OrionCodeUpdatePipelineStatus::Failed { kind, .. } => {
            format!(
                "Download pipeline failed safely: {}",
                update_error_copy(*kind)
            )
        }
    }
}

fn activation_status_copy(status: Option<&OrionCodeActivationStatus>) -> String {
    let Some(status) = status else {
        return "Activation service is unavailable in this session".to_string();
    };
    match status {
        OrionCodeActivationStatus::Idle => "No activation pending".to_string(),
        OrionCodeActivationStatus::StagingState { version } => {
            format!("Recording staged v{version}")
        }
        OrionCodeActivationStatus::Staged { version } => {
            format!("v{version} is staged and ready")
        }
        OrionCodeActivationStatus::WaitingForIdle { version } => {
            format!("Waiting for active Orion Code tasks to finish before switching to v{version}")
        }
        OrionCodeActivationStatus::Activating { version } => {
            format!("Switching to v{version}")
        }
        OrionCodeActivationStatus::Activated { version } => {
            format!("v{version} activated")
        }
        OrionCodeActivationStatus::Disabled { .. } => {
            "Activation is disabled until verified preflight is configured".to_string()
        }
        OrionCodeActivationStatus::Failed { kind, .. } => {
            format!("Activation failed safely: {}", update_error_copy(*kind))
        }
    }
}

fn next_check_copy(
    status: Option<&OrionCodeUpdateCoordinatorStatus>,
    record: &OrionCodeUpdateRecordV2,
    update_mode: OrionCodeUpdateMode,
) -> String {
    if update_mode == OrionCodeUpdateMode::Manual {
        return "Manual only".to_string();
    }
    let scheduled = match status {
        Some(OrionCodeUpdateCoordinatorStatus::Scheduled { next_check_at, .. }) => {
            Some(next_check_at)
        }
        Some(OrionCodeUpdateCoordinatorStatus::ManualCheckFailed {
            next_automatic_check_at,
            ..
        }) => next_automatic_check_at.as_ref(),
        _ => None,
    };
    scheduled
        .or(record.next_eligible_check_at.as_ref())
        .map(format_utc)
        .unwrap_or_else(|| "Not scheduled".to_string())
}

fn trust_diagnostic_copy(
    record: &OrionCodeUpdateRecordV2,
    resolution: Option<&OrionCodeUpdateResolution>,
) -> (String, String) {
    if record.staged.is_some() {
        return (
            "Passed before staging the candidate archive".to_string(),
            "Passed before staging the candidate archive".to_string(),
        );
    }
    if record.current_verified.as_ref().is_some_and(|receipt| {
        receipt.source == OrionCodeInstallSource::Archive && !receipt.legacy_imported
    }) {
        return (
            "Passed before activating the current archive".to_string(),
            "Passed before activating the current archive".to_string(),
        );
    }
    match resolution
        .and_then(|resolution| resolution.candidate.as_ref())
        .map(|candidate| candidate.target.signing_requirement)
    {
        Some(OrionCodeSigningRequirement::DeveloperIdAndNotarized) => (
            "Required before staging: Developer ID".to_string(),
            "Required before staging: notarized and stapled".to_string(),
        ),
        Some(OrionCodeSigningRequirement::Authenticode) => (
            "Required before staging: Authenticode".to_string(),
            "Not required for this target".to_string(),
        ),
        Some(OrionCodeSigningRequirement::DigestOnly) => (
            "Signed index requires digest verification only".to_string(),
            "Not required by this target policy".to_string(),
        ),
        None => (
            "Pending; checked before archive staging".to_string(),
            "Pending; checked before archive staging".to_string(),
        ),
    }
}

fn coordinator_diagnostic_code(status: &OrionCodeUpdateCoordinatorStatus) -> Option<String> {
    match status {
        OrionCodeUpdateCoordinatorStatus::Disabled {
            diagnostic_code: Some(code),
            ..
        }
        | OrionCodeUpdateCoordinatorStatus::ManualCheckFailed {
            diagnostic_code: code,
            ..
        } => Some(sanitize_diagnostic_code(code)),
        _ => None,
    }
}

fn activation_diagnostic_code(status: &OrionCodeActivationStatus) -> Option<String> {
    match status {
        OrionCodeActivationStatus::Disabled { diagnostic_code }
        | OrionCodeActivationStatus::Failed {
            diagnostic_code, ..
        } => Some(sanitize_diagnostic_code(diagnostic_code)),
        _ => None,
    }
}

fn pipeline_diagnostic_code(status: &OrionCodeUpdatePipelineStatus) -> Option<String> {
    match status {
        OrionCodeUpdatePipelineStatus::Disabled { diagnostic_code }
        | OrionCodeUpdatePipelineStatus::Failed {
            diagnostic_code, ..
        } => Some(sanitize_diagnostic_code(diagnostic_code)),
        OrionCodeUpdatePipelineStatus::Downloading { .. } => {
            Some("archive_download_in_progress".to_string())
        }
        _ => None,
    }
}

fn compact_download_progress(received_bytes: u64, total_bytes: u64) -> String {
    let total_bytes = total_bytes.min(MAX_ARCHIVE_BYTES);
    let received_bytes = received_bytes.min(total_bytes);
    let percentage = if total_bytes == 0 {
        0
    } else {
        ((u128::from(received_bytes) * 100) / u128::from(total_bytes)) as u64
    };
    format!(
        "{} / {} · {}%",
        compact_binary_bytes(received_bytes),
        compact_binary_bytes(total_bytes),
        percentage
    )
}

fn compact_binary_bytes(bytes: u64) -> String {
    const KIBIBYTE: u64 = 1024;
    const MEBIBYTE: u64 = KIBIBYTE * 1024;
    const GIBIBYTE: u64 = MEBIBYTE * 1024;

    let bytes = bytes.min(MAX_ARCHIVE_BYTES);
    let (unit_bytes, unit_name) = if bytes >= GIBIBYTE {
        (GIBIBYTE, "GiB")
    } else if bytes >= MEBIBYTE {
        (MEBIBYTE, "MiB")
    } else if bytes >= KIBIBYTE {
        (KIBIBYTE, "KiB")
    } else {
        return format!("{bytes} B");
    };
    let whole = bytes / unit_bytes;
    let tenths = ((u128::from(bytes % unit_bytes) * 10) / u128::from(unit_bytes)) as u64;
    if tenths == 0 {
        format!("{whole} {unit_name}")
    } else {
        format!("{whole}.{tenths} {unit_name}")
    }
}

fn combine_diagnostic_codes(
    coordinator_code: Option<&str>,
    activation_code: Option<&str>,
    pipeline_code: Option<&str>,
    last_error_kind: Option<OrionCodeUpdateErrorKind>,
) -> Option<String> {
    let mut diagnostics = Vec::new();
    if let Some(code) = coordinator_code {
        diagnostics.push(format!("update={}", sanitize_diagnostic_code(code)));
    } else if let Some(kind) = last_error_kind {
        diagnostics.push(format!("update={}", update_error_code(kind)));
    }
    if let Some(code) = activation_code {
        diagnostics.push(format!("activation={}", sanitize_diagnostic_code(code)));
    }
    if let Some(code) = pipeline_code {
        diagnostics.push(format!("pipeline={}", sanitize_diagnostic_code(code)));
    }
    (!diagnostics.is_empty()).then(|| diagnostics.join("; "))
}

fn sanitize_diagnostic_code(code: &str) -> String {
    if code.is_empty()
        || code.len() > 96
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        "orion_code_update_error".to_string()
    } else {
        code.to_string()
    }
}

fn update_error_code(kind: OrionCodeUpdateErrorKind) -> &'static str {
    match kind {
        OrionCodeUpdateErrorKind::Network => "network",
        OrionCodeUpdateErrorKind::IndexEnvelope => "index_envelope",
        OrionCodeUpdateErrorKind::IndexSignature => "index_signature",
        OrionCodeUpdateErrorKind::IndexReplay => "index_replay",
        OrionCodeUpdateErrorKind::IndexExpired => "index_expired",
        OrionCodeUpdateErrorKind::Incompatible => "incompatible",
        OrionCodeUpdateErrorKind::Download => "download",
        OrionCodeUpdateErrorKind::Checksum => "checksum",
        OrionCodeUpdateErrorKind::Extract => "extract",
        OrionCodeUpdateErrorKind::Manifest => "manifest",
        OrionCodeUpdateErrorKind::PlatformSignature => "platform_signature",
        OrionCodeUpdateErrorKind::Notarization => "notarization",
        OrionCodeUpdateErrorKind::Preflight => "preflight",
        OrionCodeUpdateErrorKind::Activation => "activation",
        OrionCodeUpdateErrorKind::Rollback => "rollback",
        OrionCodeUpdateErrorKind::Configuration => "configuration",
        OrionCodeUpdateErrorKind::Persistence => "persistence",
    }
}

fn update_error_copy(kind: OrionCodeUpdateErrorKind) -> &'static str {
    match kind {
        OrionCodeUpdateErrorKind::Network => "network unavailable",
        OrionCodeUpdateErrorKind::IndexEnvelope => "invalid index envelope",
        OrionCodeUpdateErrorKind::IndexSignature => "index signature rejected",
        OrionCodeUpdateErrorKind::IndexReplay => "replayed index rejected",
        OrionCodeUpdateErrorKind::IndexExpired => "expired index rejected",
        OrionCodeUpdateErrorKind::Incompatible => "no compatible release",
        OrionCodeUpdateErrorKind::Download => "archive download rejected",
        OrionCodeUpdateErrorKind::Checksum => "archive checksum rejected",
        OrionCodeUpdateErrorKind::Extract => "archive extraction rejected",
        OrionCodeUpdateErrorKind::Manifest => "artifact manifest rejected",
        OrionCodeUpdateErrorKind::PlatformSignature => "platform signature rejected",
        OrionCodeUpdateErrorKind::Notarization => "notarization rejected",
        OrionCodeUpdateErrorKind::Preflight => "ACP preflight rejected",
        OrionCodeUpdateErrorKind::Activation => "activation rejected",
        OrionCodeUpdateErrorKind::Rollback => "rollback rejected",
        OrionCodeUpdateErrorKind::Configuration => "configuration unavailable",
        OrionCodeUpdateErrorKind::Persistence => "state could not be saved",
    }
}

#[derive(IntoElement)]
struct AgentRegistryCard {
    children: Vec<AnyElement>,
}

impl AgentRegistryCard {
    fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }
}

impl ParentElement for AgentRegistryCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for AgentRegistryCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div().w_full().child(
            v_flex()
                .p_3()
                .mt_4()
                .w_full()
                .min_h(rems_from_px(86_f32))
                .gap_2()
                .bg(cx.theme().colors().elevated_surface_background.opacity(0.5))
                .border_1()
                .border_color(cx.theme().colors().border_variant)
                .rounded_md()
                .children(self.children),
        )
    }
}

pub struct AgentRegistryPage {
    registry_store: Entity<AgentRegistryStore>,
    list: UniformListScrollHandle,
    registry_agents: Vec<RegistryAgent>,
    filtered_registry_indices: Vec<usize>,
    installed_statuses: HashMap<String, RegistryInstallStatus>,
    query_editor: Entity<Editor>,
    filter: RegistryFilter,
    _subscriptions: Vec<gpui::Subscription>,
}

impl AgentRegistryPage {
    pub fn new(
        _workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let registry_store = AgentRegistryStore::global(cx);
            let query_editor = cx.new(|cx| {
                let mut input = Editor::single_line(window, cx);
                input.set_placeholder_text("Search agents...", window, cx);
                input
            });
            cx.subscribe(&query_editor, Self::on_query_change).detach();

            let mut subscriptions = Vec::new();
            subscriptions.push(cx.observe(&registry_store, |this, _, cx| {
                this.reload_registry_agents(cx);
            }));
            subscriptions.push(cx.observe_global::<SettingsStore>(|this, cx| {
                this.filter_registry_agents(cx);
            }));
            if let Some(coordinator) = OrionCodeUpdateCoordinator::try_global(cx) {
                subscriptions.push(cx.observe(&coordinator, |_, _, cx| cx.notify()));
            }
            if let Some(coordinator) = OrionCodeActivationCoordinator::try_global(cx) {
                subscriptions.push(cx.observe(&coordinator, |_, _, cx| cx.notify()));
            }
            if let Some(pipeline) = OrionCodeUpdatePipeline::try_global(cx) {
                subscriptions.push(cx.observe(&pipeline, |_, _, cx| cx.notify()));
            }

            let mut this = Self {
                registry_store,
                list: UniformListScrollHandle::new(),
                registry_agents: Vec::new(),
                filtered_registry_indices: Vec::new(),
                installed_statuses: HashMap::default(),
                query_editor,
                filter: RegistryFilter::All,
                _subscriptions: subscriptions,
            };

            this.reload_registry_agents(cx);
            this.registry_store
                .update(cx, |store, cx| store.refresh(cx));

            this
        })
    }

    fn reload_registry_agents(&mut self, cx: &mut Context<Self>) {
        self.registry_agents = self.registry_store.read(cx).agents().to_vec();
        self.registry_agents.sort_by(|left, right| {
            left.name()
                .as_ref()
                .to_lowercase()
                .cmp(&right.name().as_ref().to_lowercase())
                .then_with(|| {
                    left.id()
                        .as_ref()
                        .to_lowercase()
                        .cmp(&right.id().as_ref().to_lowercase())
                })
        });
        self.filter_registry_agents(cx);
    }

    fn refresh_installed_statuses(&mut self, cx: &mut Context<Self>) {
        let settings = cx
            .global::<SettingsStore>()
            .get::<AllAgentServersSettings>(None);
        self.installed_statuses.clear();
        for (id, settings) in settings.iter() {
            let status = match settings {
                CustomAgentServerSettings::Registry { .. } => {
                    RegistryInstallStatus::InstalledRegistry
                }
                CustomAgentServerSettings::Custom { .. } => RegistryInstallStatus::InstalledCustom,
            };
            self.installed_statuses.insert(id.clone(), status);
        }
    }

    fn install_status(&self, id: &str) -> RegistryInstallStatus {
        self.installed_statuses
            .get(id)
            .copied()
            .unwrap_or(RegistryInstallStatus::NotInstalled)
    }

    fn search_query(&self, cx: &mut App) -> Option<String> {
        let search = self.query_editor.read(cx).text(cx);
        if search.trim().is_empty() {
            None
        } else {
            Some(search)
        }
    }

    fn filter_registry_agents(&mut self, cx: &mut Context<Self>) {
        self.refresh_installed_statuses(cx);
        let search = self.search_query(cx).map(|search| search.to_lowercase());
        let filter = self.filter;
        let installed_statuses = self.installed_statuses.clone();

        let filtered_indices = self
            .registry_agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                let matches_search = search.as_ref().is_none_or(|query| {
                    let query = query.as_str();
                    agent.id().as_ref().to_lowercase().contains(query)
                        || agent.name().as_ref().to_lowercase().contains(query)
                        || agent.description().as_ref().to_lowercase().contains(query)
                });

                let install_status = installed_statuses
                    .get(agent.id().as_ref())
                    .copied()
                    .unwrap_or(RegistryInstallStatus::NotInstalled);
                let matches_filter = match filter {
                    RegistryFilter::All => true,
                    RegistryFilter::Installed => {
                        install_status != RegistryInstallStatus::NotInstalled
                    }
                    RegistryFilter::NotInstalled => {
                        install_status == RegistryInstallStatus::NotInstalled
                    }
                };

                matches_search && matches_filter
            })
            .map(|(index, _)| index)
            .collect();

        self.filtered_registry_indices = filtered_indices;

        cx.notify();
    }

    fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.list.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn on_query_change(
        &mut self,
        _: Entity<Editor>,
        event: &editor::EditorEvent,
        cx: &mut Context<Self>,
    ) {
        if let editor::EditorEvent::Edited { .. } = event {
            self.filter_registry_agents(cx);
            self.scroll_to_top(cx);
        }
    }

    fn render_search(&self, cx: &mut Context<Self>) -> Div {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("BufferSearchBar");

        h_flex()
            .key_context(key_context)
            .h_8()
            .min_w(rems_from_px(384_f32))
            .flex_1()
            .pl_1p5()
            .pr_2()
            .gap_2()
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_md()
            .child(Icon::new(IconName::MagnifyingGlass).color(Color::Muted))
            .child(self.render_text_input(&self.query_editor, cx))
    }

    fn render_text_input(
        &self,
        editor: &Entity<Editor>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings = ThemeSettings::get_global(cx);
        let text_style = TextStyle {
            color: if editor.read(cx).read_only(cx) {
                cx.theme().colors().text_disabled
            } else {
                cx.theme().colors().text
            },
            font_family: settings.ui_font.family.clone(),
            font_features: settings.ui_font.features.clone(),
            font_fallbacks: settings.ui_font.fallbacks.clone(),
            font_size: rems(0.875).into(),
            font_weight: settings.ui_font.weight,
            line_height: relative(1.3),
            ..Default::default()
        };

        EditorElement::new(
            editor,
            EditorStyle {
                background: cx.theme().colors().editor_background,
                local_player: cx.theme().players().local(),
                text: text_style,
                ..Default::default()
            },
        )
    }

    fn set_orion_code_update_preference(preference: OrionCodeUpdatePreference, cx: &mut App) {
        let current_channel = AgentSettings::get_global(cx).orion_code_update_channel;
        let fs = <dyn Fs>::global(cx);
        update_settings_file(fs, cx, move |content, _| {
            apply_orion_code_update_preference(content, preference, current_channel);
        });
    }

    fn render_orion_code_update_panel(
        &self,
        install_status: RegistryInstallStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let installed = install_status == RegistryInstallStatus::InstalledRegistry;
        let state = OrionCodeUpdateUiState::from_globals(installed, cx);
        let preference =
            OrionCodeUpdatePreference::from_effective_settings(AgentSettings::get_global(cx));
        let update_coordinator = OrionCodeUpdateCoordinator::try_global(cx);
        let activation_coordinator = OrionCodeActivationCoordinator::try_global(cx);
        let previous_activation_coordinator = activation_coordinator.clone();
        let update_pipeline = OrionCodeUpdatePipeline::try_global(cx);
        let pipeline_action = state.pipeline_action;
        let mut tab_index = 0_isize;
        let preference_control = ToggleButtonGroup::single_row(
            "orion-code-update-preference",
            [
                ToggleButtonSimple::new("Stable", |_, _, cx| {
                    Self::set_orion_code_update_preference(OrionCodeUpdatePreference::Stable, cx);
                }),
                ToggleButtonSimple::new("Beta", |_, _, cx| {
                    Self::set_orion_code_update_preference(OrionCodeUpdatePreference::Beta, cx);
                }),
                ToggleButtonSimple::new("Manual", |_, _, cx| {
                    Self::set_orion_code_update_preference(OrionCodeUpdatePreference::Manual, cx);
                }),
            ],
        )
        .style(ToggleButtonGroupStyle::Outlined)
        .size(ToggleButtonGroupSize::Custom(rems_from_px(30_f32)))
        .label_size(LabelSize::Small)
        .selected_index(preference.selected_index())
        .tab_index(&mut tab_index);
        let check_now_tab_index = tab_index;
        tab_index += 1;
        let download_tab_index = tab_index;
        tab_index += 1;
        let activate_tab_index = tab_index;
        tab_index += 1;
        let previous_tab_index = tab_index;
        tab_index += 1;
        let use_orion_agent_tab_index = tab_index;

        v_flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .pt_2()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_start()
                    .gap_2()
                    .when(state.needs_attention, |this| {
                        this.child(
                            Icon::new(IconName::Warning)
                                .size(IconSize::Small)
                                .color(Color::Warning),
                        )
                    })
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_0p5()
                            .child(Headline::new("Managed updates").size(HeadlineSize::Small))
                            .child(
                                Label::new(
                                    "Stable and Beta update automatically. Manual checks only when requested.",
                                )
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .child(
                        Label::new("Update preference")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(preference_control),
            )
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("orion-code-check-now", "Check Now")
                            .style(ButtonStyle::Outlined)
                            .size(ButtonSize::Compact)
                            .tab_index(check_now_tab_index)
                            .disabled(!state.can_check_now)
                            .on_click(move |_, _, cx| {
                                if let Some(coordinator) = update_coordinator.as_ref() {
                                    coordinator.update(cx, |coordinator, cx| {
                                        coordinator.check_now(cx)
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new(
                            "orion-code-download-update",
                            state.pipeline_button_label,
                        )
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Compact)
                        .tab_index(download_tab_index)
                        .disabled(pipeline_action.is_none())
                        .on_click(move |_, _, cx| {
                            if let (Some(pipeline), Some(action)) =
                                (update_pipeline.as_ref(), pipeline_action)
                            {
                                pipeline.update(cx, |pipeline, cx| match action {
                                    OrionCodePipelineAction::Download => {
                                        pipeline.download_available(cx)
                                    }
                                    OrionCodePipelineAction::Retry => pipeline.retry(cx),
                                });
                            }
                        }),
                    )
                    .child(
                        Button::new(
                            "orion-code-activate-when-idle",
                            if state.waiting_for_idle {
                                "Waiting for Idle"
                            } else {
                                "Activate When Idle"
                            },
                        )
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Compact)
                        .tab_index(activate_tab_index)
                        .disabled(!state.can_activate_when_idle)
                        .on_click(move |_, _, cx| {
                            if let Some(coordinator) = activation_coordinator.as_ref() {
                                coordinator.update(cx, |coordinator, cx| {
                                    coordinator.activate_when_idle(cx)
                                });
                            }
                        }),
                    )
                    .child(
                        Button::new("orion-code-use-previous", "Use Previous")
                            .style(ButtonStyle::Outlined)
                            .size(ButtonSize::Compact)
                            .tab_index(previous_tab_index)
                            .disabled(!state.can_use_previous)
                            .on_click(move |_, _, cx| {
                                if let Some(coordinator) =
                                    previous_activation_coordinator.as_ref()
                                {
                                    coordinator.update(cx, |coordinator, cx| {
                                        coordinator.use_previous_when_idle(cx)
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("orion-code-use-orion-agent", "Use Orion Agent")
                            .style(ButtonStyle::OutlinedGhost)
                            .size(ButtonSize::Compact)
                            .tab_index(use_orion_agent_tab_index)
                            .disabled(!state.can_use_orion_agent)
                            .on_click(move |_, window, cx| {
                                window.dispatch_action(
                                    Box::new(zed_actions::agent::SelectAgent {
                                        agent: agent::ORION_AGENT_ID.to_string(),
                                    }),
                                    cx,
                                );
                            }),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .child(Self::render_orion_code_diagnostic("Current", state.current))
                    .child(Self::render_orion_code_diagnostic(
                        "Candidate",
                        state.candidate,
                    ))
                    .child(Self::render_orion_code_diagnostic(
                        "Update check",
                        state.update_status,
                    ))
                    .child(Self::render_orion_code_diagnostic(
                        "Download pipeline",
                        state.pipeline_status,
                    ))
                    .child(Self::render_orion_code_diagnostic(
                        "Activation",
                        state.activation_status,
                    ))
                    .child(Self::render_orion_code_diagnostic(
                        "Last check",
                        state.last_check,
                    ))
                    .child(Self::render_orion_code_diagnostic(
                        "Next check",
                        state.next_check,
                    ))
                    .child(Self::render_orion_code_diagnostic(
                        "Code signing",
                        state.code_signing,
                    ))
                    .child(Self::render_orion_code_diagnostic(
                        "Notarization",
                        state.notarization,
                    ))
                    .when_some(state.diagnostic_code, |this, diagnostic_code| {
                        this.child(Self::render_orion_code_diagnostic(
                            "Diagnostic code",
                            diagnostic_code,
                        ))
                    }),
            )
    }

    fn render_orion_code_diagnostic(label: &'static str, value: impl Into<SharedString>) -> Div {
        v_flex()
            .w_full()
            .min_w_0()
            .gap_0p5()
            .child(
                Label::new(label)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .child(Label::new(value).size(LabelSize::Small))
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_search = self.search_query(cx).is_some();
        let registry_store = self.registry_store.read(cx);
        let is_fetching = registry_store.is_fetching();
        let fetch_error = registry_store.fetch_error();

        let message = if is_fetching {
            "Loading registry..."
        } else if fetch_error.is_some() {
            "Failed to load the agent registry. Please check your connection and try again."
        } else {
            match self.filter {
                RegistryFilter::All => {
                    if has_search {
                        "No agents match your search."
                    } else {
                        "No agents available."
                    }
                }
                RegistryFilter::Installed => {
                    if has_search {
                        "No installed agents match your search."
                    } else {
                        "No installed agents."
                    }
                }
                RegistryFilter::NotInstalled => {
                    if has_search {
                        "No uninstalled agents match your search."
                    } else {
                        "No uninstalled agents."
                    }
                }
            }
        };

        h_flex()
            .py_4()
            .min_w_0()
            .w_full()
            .gap_1p5()
            .items_start()
            .when(fetch_error.is_some(), |this| {
                this.child(
                    Icon::new(IconName::Warning)
                        .size(IconSize::Small)
                        .color(Color::Warning),
                )
            })
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(Label::new(message))
                    .when_some(fetch_error.clone(), |this, fetch_error| {
                        this.child(
                            Label::new(fetch_error)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    }),
            )
            .when_some(fetch_error, |this, _| {
                let registry_store = self.registry_store.clone();
                this.child(
                    Button::new("retry-agent-registry", "Retry")
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Compact)
                        .on_click(move |_, _, cx| {
                            registry_store.update(cx, |store, cx| store.refresh(cx));
                        }),
                )
            })
    }

    fn render_agents(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AgentRegistryCard> {
        range
            .map(|index| {
                let Some(agent_index) = self.filtered_registry_indices.get(index).copied() else {
                    return self.render_missing_agent();
                };
                let Some(agent) = self.registry_agents.get(agent_index) else {
                    return self.render_missing_agent();
                };
                self.render_registry_agent(agent, cx)
            })
            .collect()
    }

    fn render_missing_agent(&self) -> AgentRegistryCard {
        AgentRegistryCard::new().child(
            Label::new("Missing registry entry.")
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
    }

    fn render_registry_agent(
        &self,
        agent: &RegistryAgent,
        cx: &mut Context<Self>,
    ) -> AgentRegistryCard {
        let install_status = self.install_status(agent.id().as_ref());
        let supports_current_platform = agent.supports_current_platform();
        let unavailable_reason = agent.unavailable_reason().cloned();

        let icon = match agent.icon_path() {
            Some(icon_path) => Icon::from_external_svg(icon_path.clone()),
            None => Icon::new(IconName::Sparkle),
        }
        .size(IconSize::Medium)
        .color(Color::Muted);

        let install_button =
            self.install_button(agent, install_status, supports_current_platform, cx);

        let repository_button = agent.repository().map(|repository| {
            let repository_for_tooltip = repository.clone();
            let repository_for_click = repository.to_string();

            IconButton::new(
                SharedString::from(format!("agent-repo-{}", agent.id())),
                IconName::Github,
            )
            .icon_size(IconSize::Small)
            .tooltip(move |_, cx| {
                Tooltip::with_meta(
                    "Visit Agent Repository",
                    None,
                    repository_for_tooltip.clone(),
                    cx,
                )
            })
            .on_click(move |_, _, cx| {
                cx.open_url(&repository_for_click);
            })
        });

        let website_button = agent.website().map(|website| {
            let website = website.clone();
            let website_for_click = website.clone();
            IconButton::new(
                SharedString::from(format!("agent-website-{}", agent.id())),
                IconName::Link,
            )
            .icon_size(IconSize::Small)
            .tooltip(move |_, cx| {
                Tooltip::with_meta("Visit Agent Website", None, website.clone(), cx)
            })
            .on_click(move |_, _, cx| {
                cx.open_url(&website_for_click);
            })
        });

        let card = AgentRegistryCard::new()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .flex_wrap()
                    .justify_between()
                    .child(
                        h_flex()
                            .min_w_0()
                            .flex_wrap()
                            .gap_2()
                            .child(icon)
                            .child(Headline::new(agent.name().clone()).size(HeadlineSize::Small))
                            .when(!agent.version().is_empty(), |this| {
                                this.child(
                                    Label::new(format!("v{}", agent.version())).color(Color::Muted),
                                )
                            })
                            .when_some(unavailable_reason, |this, reason| {
                                this.child(
                                    Label::new(reason)
                                        .size(LabelSize::Small)
                                        .color(Color::Warning),
                                )
                            })
                            .when(
                                !supports_current_platform && agent.unavailable_reason().is_none(),
                                |this| {
                                    this.child(
                                        Label::new("Not supported on this platform")
                                            .size(LabelSize::Small)
                                            .color(Color::Warning),
                                    )
                                },
                            ),
                    )
                    .child(install_button),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(
                        Label::new(agent.description().clone())
                            .size(LabelSize::Small)
                            .truncate(),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Label::new(format!("ID: {}", agent.id()))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                            .when_some(repository_button, |this, button| this.child(button))
                            .when_some(website_button, |this, button| this.child(button)),
                    ),
            );

        if agent.id().as_ref() == ORION_CODE_AGENT_ID {
            card.child(self.render_orion_code_update_panel(install_status, cx))
        } else {
            card
        }
    }

    fn install_button(
        &self,
        agent: &RegistryAgent,
        install_status: RegistryInstallStatus,
        supports_current_platform: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let button_id = SharedString::from(format!("install-agent-{}", agent.id()));

        if !supports_current_platform {
            return Button::new(button_id, "Unavailable")
                .style(ButtonStyle::OutlinedGhost)
                .disabled(true);
        }

        match install_status {
            RegistryInstallStatus::NotInstalled => {
                let fs = <dyn Fs>::global(cx);
                let agent_id = agent.id().to_string();
                Button::new(button_id, "Install")
                    .style(ButtonStyle::Tinted(ui::TintColor::Accent))
                    .start_icon(
                        Icon::new(IconName::Download)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .on_click(move |_, window, cx| {
                        if agent_id == ORION_CODE_AGENT_ID {
                            let Some(bootstrap) = OrionCodeBootstrap::try_global(cx) else {
                                log::error!("Orion Code bootstrap is not initialized");
                                return;
                            };
                            bootstrap
                                .update(cx, |bootstrap, cx| {
                                    bootstrap.accept_and_configure(fs.clone(), cx)
                                })
                                .detach_and_log_err(cx);
                            return;
                        }
                        update_settings_file(fs.clone(), cx, {
                            let agent_id = agent_id.clone();
                            move |settings, _| {
                                let agent_servers = settings.agent_servers.get_or_insert_default();
                                agent_servers.entry(agent_id).or_insert_with(|| {
                                    settings::CustomAgentServerSettings::Registry {
                                        default_mode: None,
                                        env: Default::default(),
                                        default_config_options: HashMap::default(),
                                        favorite_config_option_values: HashMap::default(),
                                    }
                                });
                            }
                        });
                        window.dispatch_action(
                            Box::new(zed_actions::agent::SelectAgent {
                                agent: agent_id.clone(),
                            }),
                            cx,
                        );
                    })
            }
            RegistryInstallStatus::InstalledRegistry => {
                let fs = <dyn Fs>::global(cx);
                let agent_id = agent.id().to_string();
                let orion_code_needs_retry = agent_id == ORION_CODE_AGENT_ID
                    && OrionCodeBootstrap::try_global(cx).is_some_and(|bootstrap| {
                        let bootstrap = bootstrap.read(cx);
                        bootstrap.phase() == OrionCodeBootstrapPhase::Failed
                            || (bootstrap.phase() == OrionCodeBootstrapPhase::Ready
                                && bootstrap.last_error().is_some())
                    });
                if orion_code_needs_retry {
                    return Button::new(button_id, "Retry")
                        .style(ButtonStyle::Tinted(ui::TintColor::Accent))
                        .start_icon(
                            Icon::new(IconName::RotateCw)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .on_click(move |_, _, cx| {
                            let Some(bootstrap) = OrionCodeBootstrap::try_global(cx) else {
                                log::error!("Orion Code bootstrap is not initialized");
                                return;
                            };
                            bootstrap
                                .update(cx, |bootstrap, cx| {
                                    bootstrap.accept_and_configure(fs.clone(), cx)
                                })
                                .detach_and_log_err(cx);
                        });
                }
                Button::new(button_id, "Remove")
                    .style(ButtonStyle::OutlinedGhost)
                    .on_click(move |_, window, cx| {
                        if agent_id == ORION_CODE_AGENT_ID {
                            let Some(bootstrap) = OrionCodeBootstrap::try_global(cx) else {
                                log::error!("Orion Code bootstrap is not initialized");
                                return;
                            };
                            bootstrap
                                .update(cx, |bootstrap, cx| {
                                    bootstrap.remove_and_disable(fs.clone(), cx)
                                })
                                .detach_and_log_err(cx);
                            window.dispatch_action(
                                Box::new(zed_actions::agent::SelectAgent {
                                    agent: agent::ORION_AGENT_ID.to_string(),
                                }),
                                cx,
                            );
                            return;
                        }
                        let agent_id = agent_id.clone();
                        update_settings_file(fs.clone(), cx, move |settings, _| {
                            let Some(agent_servers) = settings.agent_servers.as_mut() else {
                                return;
                            };
                            if let Some(entry) = agent_servers.get(agent_id.as_str())
                                && matches!(
                                    entry,
                                    settings::CustomAgentServerSettings::Registry { .. }
                                )
                            {
                                agent_servers.remove(agent_id.as_str());
                            }
                        });
                    })
            }
            RegistryInstallStatus::InstalledCustom => Button::new(button_id, "Installed")
                .style(ButtonStyle::OutlinedGhost)
                .disabled(true),
        }
    }
}

impl Render for AgentRegistryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .p_4()
                    .gap_4()
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        h_flex()
                            .w_full()
                            .gap_1p5()
                            .justify_between()
                            .child(Headline::new("ACP Registry").size(HeadlineSize::Large))
                            .child(
                                Button::new("learn-more", "Learn More")
                                    .style(ButtonStyle::Outlined)
                                    .size(ButtonSize::Medium)
                                    .end_icon(
                                        Icon::new(IconName::ArrowUpRight)
                                            .size(IconSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .on_click(move |_, _, cx| {
                                        cx.open_url(&zed_urls::acp_registry_blog(cx))
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .gap_2()
                            .child(self.render_search(cx))
                            .child(
                                div().child(
                                    ToggleButtonGroup::single_row(
                                        "registry-filter-buttons",
                                        [
                                            ToggleButtonSimple::new(
                                                "All",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = RegistryFilter::All;
                                                    this.filter_registry_agents(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                            ToggleButtonSimple::new(
                                                "Installed",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = RegistryFilter::Installed;
                                                    this.filter_registry_agents(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                            ToggleButtonSimple::new(
                                                "Not Installed",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = RegistryFilter::NotInstalled;
                                                    this.filter_registry_agents(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                        ],
                                    )
                                    .style(ToggleButtonGroupStyle::Outlined)
                                    .size(ToggleButtonGroupSize::Custom(rems_from_px(30_f32)))
                                    .label_size(LabelSize::Default)
                                    .auto_width()
                                    .selected_index(match self.filter {
                                        RegistryFilter::All => 0,
                                        RegistryFilter::Installed => 1,
                                        RegistryFilter::NotInstalled => 2,
                                    })
                                    .into_any_element(),
                                ),
                            ),
                    ),
            )
            .child(v_flex().px_4().size_full().overflow_y_hidden().map(|this| {
                let count = self.filtered_registry_indices.len();
                if count == 0 {
                    this.child(self.render_empty_state(cx)).into_any_element()
                } else {
                    let scroll_handle = &self.list;
                    this.child(
                        uniform_list("registry-entries", count, cx.processor(Self::render_agents))
                            .flex_grow_1()
                            .pb_4()
                            .track_scroll(scroll_handle),
                    )
                    .vertical_scrollbar_for(scroll_handle, window, cx)
                    .into_any_element()
                }
            }))
    }
}

impl EventEmitter<ItemEvent> for AgentRegistryPage {}

impl Focusable for AgentRegistryPage {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.query_editor.read(cx).focus_handle(cx)
    }
}

impl Item for AgentRegistryPage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "ACP Registry".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("ACP Registry Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(workspace::item::ItemEvent)) {
        f(*event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_beta_preferences_enable_automatic_updates() {
        for (preference, expected_channel) in [
            (
                OrionCodeUpdatePreference::Stable,
                SettingsOrionCodeUpdateChannel::Stable,
            ),
            (
                OrionCodeUpdatePreference::Beta,
                SettingsOrionCodeUpdateChannel::Beta,
            ),
        ] {
            let mut content = settings::SettingsContent::default();
            apply_orion_code_update_preference(
                &mut content,
                preference,
                SettingsOrionCodeUpdateChannel::Stable,
            );
            let orion_code = content
                .agent
                .and_then(|agent| agent.orion_code)
                .expect("Orion Code update settings");
            assert_eq!(orion_code.update_channel, Some(expected_channel));
            assert_eq!(
                orion_code.update_mode,
                Some(SettingsOrionCodeUpdateMode::Automatic)
            );
        }
    }

    #[test]
    fn manual_preference_preserves_the_most_recent_channel() {
        let mut content = settings::SettingsContent::default();
        apply_orion_code_update_preference(
            &mut content,
            OrionCodeUpdatePreference::Beta,
            SettingsOrionCodeUpdateChannel::Stable,
        );
        apply_orion_code_update_preference(
            &mut content,
            OrionCodeUpdatePreference::Manual,
            SettingsOrionCodeUpdateChannel::Stable,
        );
        let orion_code = content
            .agent
            .and_then(|agent| agent.orion_code)
            .expect("Orion Code update settings");
        assert_eq!(
            orion_code.update_channel,
            Some(SettingsOrionCodeUpdateChannel::Beta)
        );
        assert_eq!(
            orion_code.update_mode,
            Some(SettingsOrionCodeUpdateMode::Manual)
        );

        let mut inherited_beta = settings::SettingsContent::default();
        apply_orion_code_update_preference(
            &mut inherited_beta,
            OrionCodeUpdatePreference::Manual,
            SettingsOrionCodeUpdateChannel::Beta,
        );
        assert_eq!(
            inherited_beta
                .agent
                .and_then(|agent| agent.orion_code)
                .and_then(|orion_code| orion_code.update_channel),
            Some(SettingsOrionCodeUpdateChannel::Beta)
        );
    }

    #[test]
    fn paused_and_revoked_release_copy_is_fail_closed() {
        let paused = OrionCodeUpdateResolution {
            candidate: None,
            current_release_state: OrionCodeCurrentReleaseState::Paused,
        };
        let revoked = OrionCodeUpdateResolution {
            candidate: None,
            current_release_state: OrionCodeCurrentReleaseState::Revoked,
        };

        assert!(candidate_copy(Some(&paused)).contains("will not switch automatically"));
        assert!(candidate_copy(Some(&revoked)).contains("activation is blocked"));
    }

    #[test]
    fn diagnostics_never_render_paths_or_unbounded_values() {
        assert_eq!(
            sanitize_diagnostic_code("/Users/example/.config/token"),
            "orion_code_update_error"
        );
        assert_eq!(
            sanitize_diagnostic_code("signed_feed_unavailable"),
            "signed_feed_unavailable"
        );
        assert_eq!(
            coordinator_diagnostic_code(&OrionCodeUpdateCoordinatorStatus::Disabled {
                reason: OrionCodeUpdateDisabledReason::SignedFeedUnavailable,
                diagnostic_code: Some("/private/tmp/token".to_string()),
            })
            .as_deref(),
            Some("orion_code_update_error")
        );
        assert_eq!(
            activation_diagnostic_code(&OrionCodeActivationStatus::Failed {
                kind: OrionCodeUpdateErrorKind::Activation,
                diagnostic_code: "candidate_identity_mismatch".to_string(),
            })
            .as_deref(),
            Some("candidate_identity_mismatch")
        );
    }

    #[test]
    fn download_progress_copy_is_compact_and_uses_a_fixed_diagnostic() {
        let status = OrionCodeUpdatePipelineStatus::Downloading {
            version: "0.4.0".to_string(),
            archive_bytes: 8 * 1024 * 1024,
            received_bytes: 3 * 1024 * 1024,
            total_bytes: 8 * 1024 * 1024,
        };

        assert_eq!(
            pipeline_status_copy(Some(&status)),
            "Downloading v0.4.0 · 3 MiB / 8 MiB · 37%"
        );
        assert_eq!(
            pipeline_diagnostic_code(&status).as_deref(),
            Some("archive_download_in_progress")
        );
        assert_eq!(compact_binary_bytes(1536), "1.5 KiB");
        assert_eq!(
            compact_download_progress(u64::MAX, u64::MAX),
            compact_download_progress(MAX_ARCHIVE_BYTES, MAX_ARCHIVE_BYTES)
        );
    }

    #[test]
    fn manual_mode_never_advertises_a_background_check() {
        let record = OrionCodeUpdateRecordV2 {
            next_eligible_check_at: Some(Utc::now()),
            ..OrionCodeUpdateRecordV2::default()
        };
        assert_eq!(
            next_check_copy(None, &record, OrionCodeUpdateMode::Manual),
            "Manual only"
        );
        assert_eq!(
            update_status_copy(
                Some(&OrionCodeUpdateCoordinatorStatus::Idle),
                OrionCodeUpdateMode::Manual,
            ),
            "Manual mode; no background check is scheduled"
        );
    }
}
