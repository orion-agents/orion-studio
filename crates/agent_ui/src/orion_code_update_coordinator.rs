use std::{collections::BTreeSet, future, sync::Arc, time::Duration};

use agent_settings::AgentSettings;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use db::kvp::KeyValueStore;
use futures::{FutureExt as _, future::BoxFuture};
use gpui::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use project::{
    agent_registry_store::{AgentRegistryStore, ORION_CODE_AGENT_ID},
    agent_server_store::{
        AllAgentServersSettings, CustomAgentServerSettings, OrionCodeManagedArchiveRuntime,
        orion_code_managed_archive_runtime, set_orion_code_managed_archive_runtime,
    },
};
use release_channel::AppVersion;
use semver::Version;
use settings::{
    OrionCodeUpdateChannel as SettingsUpdateChannel, OrionCodeUpdateMode as SettingsUpdateMode,
    Settings as _, SettingsStore,
};
use util::ResultExt as _;

use crate::{
    orion_code_bootstrap::{OrionCodeBootstrap, OrionCodeBootstrapChoice},
    orion_code_update::{
        OrionCodeCandidateContext, OrionCodeCurrentReleaseState, OrionCodeInstallSource,
        OrionCodeReleaseStatus, OrionCodeUpdateChannel, OrionCodeUpdateErrorKind,
        OrionCodeUpdateManagementState, OrionCodeUpdateMode, OrionCodeUpdateRecordV2,
        OrionCodeUpdateResolution, VerifiedOrionCodeUpdateIndex,
        migrate_orion_code_update_record_v1, preferred_orion_code_runtime_receipt,
        read_orion_code_update_record, resolve_orion_code_candidate, validate_update_record,
        write_orion_code_update_record,
    },
    orion_code_update_retention::{
        OrionCodeManagedInstallIdentity, OrionCodeRetentionInput, OrionCodeRetentionReferences,
        OrionCodeRetentionReport, OrionCodeRetentionRestartGrace, apply_orion_code_retention_plan,
        discover_orion_code_receipt_owned_installs, plan_orion_code_retention,
    },
};

const STARTUP_JITTER_MINIMUM: Duration = Duration::from_secs(30);
const STARTUP_JITTER_RANGE_SECONDS: u64 = 61;
const PERIODIC_CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const FIRST_NETWORK_RETRY: Duration = Duration::from_secs(15 * 60);
const SECOND_NETWORK_RETRY: Duration = Duration::from_secs(60 * 60);
const FINAL_NETWORK_RETRY: Duration = PERIODIC_CHECK_INTERVAL;
const INSTALLATION_ID_KEY: &str = "installation_id";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrionCodeUpdateCheckTrigger {
    Automatic,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrionCodeUpdateScheduleReason {
    Startup,
    SettingsChanged,
    Periodic,
    NetworkBackoff,
    SystemWake,
    BootstrapStateChanged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrionCodeUpdateDisabledReason {
    NotAccepted,
    Declined,
    Removed,
    CustomConfiguration,
    SignedFeedUnavailable,
    StateReadFailed,
    StateMigrationFailed,
    StatePersistenceFailed,
    GenerationExhausted,
    AppQuitting,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrionCodeUpdateCoordinatorStatus {
    Initializing,
    Disabled {
        reason: OrionCodeUpdateDisabledReason,
        diagnostic_code: Option<String>,
    },
    Idle,
    Scheduled {
        next_check_at: DateTime<Utc>,
        reason: OrionCodeUpdateScheduleReason,
    },
    Checking {
        trigger: OrionCodeUpdateCheckTrigger,
        channel: OrionCodeUpdateChannel,
    },
    SavingCheckResult {
        trigger: OrionCodeUpdateCheckTrigger,
    },
    ManualCheckFailed {
        kind: OrionCodeUpdateErrorKind,
        diagnostic_code: String,
        next_automatic_check_at: Option<DateTime<Utc>>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeUpdateCheckRequest {
    pub trigger: OrionCodeUpdateCheckTrigger,
    pub channel: OrionCodeUpdateChannel,
    pub highest_accepted_sequence: u64,
    pub cached_etag: Option<String>,
}

#[derive(Clone, Debug)]
pub enum OrionCodeUpdateFeedResponse {
    NotModified {
        index: VerifiedOrionCodeUpdateIndex,
        etag: Option<String>,
    },
    VerifiedIndex {
        index: VerifiedOrionCodeUpdateIndex,
        etag: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeUpdateFeedError {
    pub kind: OrionCodeUpdateErrorKind,
    pub diagnostic_code: String,
}

impl OrionCodeUpdateFeedError {
    pub fn new(kind: OrionCodeUpdateErrorKind, diagnostic_code: impl Into<String>) -> Self {
        Self {
            kind,
            diagnostic_code: sanitize_diagnostic_code(diagnostic_code.into()),
        }
    }
}

pub trait OrionCodeUpdateFeed: Send + Sync {
    fn disabled_diagnostic_code(&self) -> Option<&'static str>;

    fn check(
        &self,
        request: OrionCodeUpdateCheckRequest,
    ) -> BoxFuture<'static, Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>>;
}

struct DisabledOrionCodeUpdateFeed;

impl OrionCodeUpdateFeed for DisabledOrionCodeUpdateFeed {
    fn disabled_diagnostic_code(&self) -> Option<&'static str> {
        Some("first_party_signed_feed_not_configured")
    }

    fn check(
        &self,
        _request: OrionCodeUpdateCheckRequest,
    ) -> BoxFuture<'static, Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>> {
        future::ready(Err(OrionCodeUpdateFeedError::new(
            OrionCodeUpdateErrorKind::Configuration,
            "first_party_signed_feed_not_configured",
        )))
        .boxed()
    }
}

trait OrionCodeUpdateClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

struct SystemOrionCodeUpdateClock;

impl OrionCodeUpdateClock for SystemOrionCodeUpdateClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OrionCodeUpdateSettingsSnapshot {
    channel: OrionCodeUpdateChannel,
    mode: OrionCodeUpdateMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveCheck {
    generation: u64,
    trigger: OrionCodeUpdateCheckTrigger,
}

#[derive(Clone, Debug)]
struct PersistedCheckSummary {
    trigger: OrionCodeUpdateCheckTrigger,
    error: Option<OrionCodeUpdateFeedError>,
    schedule_reason: OrionCodeUpdateScheduleReason,
}

struct GlobalOrionCodeUpdateCoordinator(Entity<OrionCodeUpdateCoordinator>);

impl Global for GlobalOrionCodeUpdateCoordinator {}

pub struct OrionCodeUpdateCoordinator {
    record: OrionCodeUpdateRecordV2,
    settings: OrionCodeUpdateSettingsSnapshot,
    status: OrionCodeUpdateCoordinatorStatus,
    feed: Arc<dyn OrionCodeUpdateFeed>,
    clock: Arc<dyn OrionCodeUpdateClock>,
    installation_id: String,
    studio_version: Version,
    target_name: Option<&'static str>,
    latest_resolution: Option<OrionCodeUpdateResolution>,
    startup_jitter_seed: u64,
    generation: u64,
    active_check: Option<ActiveCheck>,
    pending_manual_check: bool,
    scheduled_task: Option<Task<()>>,
    network_task: Option<Task<()>>,
    persistence_task: Option<Task<()>>,
    retention_task: Option<Task<()>>,
    managed_operation_generation: Option<u64>,
    pending_control_reconcile: bool,
    hard_disabled: Option<(OrionCodeUpdateDisabledReason, Option<String>)>,
    initializing: bool,
    quitting: bool,
    _subscriptions: Vec<Subscription>,
}

impl OrionCodeUpdateCoordinator {
    pub fn init_global(cx: &mut App) -> Entity<Self> {
        Self::init_global_with_feed(Arc::new(DisabledOrionCodeUpdateFeed), cx)
    }

    pub fn init_global_with_feed(feed: Arc<dyn OrionCodeUpdateFeed>, cx: &mut App) -> Entity<Self> {
        if let Some(coordinator) = Self::try_global(cx) {
            return coordinator;
        }

        let bootstrap = OrionCodeBootstrap::global(cx);
        let bootstrap_record = bootstrap.read(cx).record().clone();
        let has_custom_configuration = has_custom_orion_code_configuration(cx);
        let (mut record, mut needs_initial_persistence, mut hard_disabled) =
            match read_orion_code_update_record(&KeyValueStore::global(cx)) {
                Ok(Some(record)) => (record, false, None),
                Ok(None) => match migrate_orion_code_update_record_v1(
                    &bootstrap_record,
                    has_custom_configuration,
                ) {
                    Ok(record) => (record, true, None),
                    Err(_) => (
                        OrionCodeUpdateRecordV2::default(),
                        false,
                        Some((
                            OrionCodeUpdateDisabledReason::StateMigrationFailed,
                            Some("state_v1_migration_failed".to_string()),
                        )),
                    ),
                },
                Err(_) => (
                    OrionCodeUpdateRecordV2::default(),
                    false,
                    Some((
                        OrionCodeUpdateDisabledReason::StateReadFailed,
                        Some("state_v2_read_failed".to_string()),
                    )),
                ),
            };
        if hard_disabled.is_none()
            && has_custom_configuration
            && record.management_state != OrionCodeUpdateManagementState::CustomConflict
        {
            record.management_state = OrionCodeUpdateManagementState::CustomConflict;
            needs_initial_persistence = true;
        }
        if hard_disabled.is_none() {
            if let Some(app_start_sequence) = record.app_start_sequence.checked_add(1) {
                record.app_start_sequence = app_start_sequence;
                needs_initial_persistence = true;
            } else {
                hard_disabled = Some((
                    OrionCodeUpdateDisabledReason::GenerationExhausted,
                    Some("app_start_sequence_exhausted".to_string()),
                ));
            }
        }
        let settings = read_update_settings(cx);
        let installation_id = match KeyValueStore::global(cx).read_kvp(INSTALLATION_ID_KEY) {
            Ok(Some(installation_id)) => installation_id,
            Ok(None) => "orion-code-update-fallback-seed".to_string(),
            Err(_error) => {
                log::warn!(
                    "Failed to read the installation ID for Orion Code update jitter; using a fallback seed"
                );
                "orion-code-update-fallback-seed".to_string()
            }
        };
        let startup_jitter_seed = scheduler_seed(&installation_id);
        let studio_version = AppVersion::global(cx);
        let target_name = current_orion_code_target();
        if hard_disabled.is_none() && apply_runtime_for_record(&record, cx).is_err() {
            hard_disabled = Some((
                OrionCodeUpdateDisabledReason::StateReadFailed,
                Some("managed_runtime_restore_failed".to_string()),
            ));
        }

        let coordinator = cx.new(|cx| {
            let mut coordinator = Self {
                record,
                settings,
                status: OrionCodeUpdateCoordinatorStatus::Initializing,
                feed,
                clock: Arc::new(SystemOrionCodeUpdateClock),
                installation_id,
                studio_version,
                target_name,
                latest_resolution: None,
                startup_jitter_seed,
                generation: 1,
                active_check: None,
                pending_manual_check: false,
                scheduled_task: None,
                network_task: None,
                persistence_task: None,
                retention_task: None,
                managed_operation_generation: None,
                pending_control_reconcile: false,
                hard_disabled,
                initializing: needs_initial_persistence,
                quitting: false,
                _subscriptions: Vec::new(),
            };

            let settings_subscription = cx.observe_global::<SettingsStore>(
                |coordinator: &mut OrionCodeUpdateCoordinator, cx| {
                    coordinator.handle_settings_changed(cx);
                },
            );
            let bootstrap_subscription = cx.observe(
                &bootstrap,
                |coordinator: &mut OrionCodeUpdateCoordinator, bootstrap, cx| {
                    let choice = bootstrap.read(cx).record().choice;
                    coordinator.handle_bootstrap_changed(choice, cx);
                },
            );
            let wake_subscription = cx.on_system_wake({
                let coordinator = cx.entity().downgrade();
                move |cx| {
                    coordinator
                        .update(cx, |coordinator, cx| coordinator.handle_system_wake(cx))
                        .log_err();
                }
            });
            let quit_subscription = cx.on_app_quit(|coordinator, _cx| {
                coordinator.quitting = true;
                coordinator.scheduled_task.take();
                coordinator.network_task.take();
                coordinator.retention_task.take();
                coordinator.active_check = None;
                coordinator.status = OrionCodeUpdateCoordinatorStatus::Disabled {
                    reason: OrionCodeUpdateDisabledReason::AppQuitting,
                    diagnostic_code: None,
                };
                future::ready(())
            });
            coordinator._subscriptions = vec![
                settings_subscription,
                bootstrap_subscription,
                wake_subscription,
                quit_subscription,
            ];

            if needs_initial_persistence && coordinator.hard_disabled.is_none() {
                coordinator.persist_initial_state(cx);
            } else {
                coordinator.initializing = false;
                coordinator.reconcile_schedule(OrionCodeUpdateScheduleReason::Startup, true, cx);
            }
            coordinator
        });
        cx.set_global(GlobalOrionCodeUpdateCoordinator(coordinator.clone()));
        coordinator
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalOrionCodeUpdateCoordinator>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalOrionCodeUpdateCoordinator>()
            .map(|coordinator| coordinator.0.clone())
    }

    pub fn status(&self) -> &OrionCodeUpdateCoordinatorStatus {
        &self.status
    }

    pub fn record(&self) -> &OrionCodeUpdateRecordV2 {
        &self.record
    }

    pub fn latest_resolution(&self) -> Option<&OrionCodeUpdateResolution> {
        self.latest_resolution.as_ref()
    }

    pub fn configure_feed(&mut self, feed: Arc<dyn OrionCodeUpdateFeed>, cx: &mut Context<Self>) {
        self.feed = feed;
        if self.initializing || self.quitting {
            return;
        }
        if self.managed_operation_generation.is_some() {
            self.pending_control_reconcile = true;
            return;
        }
        if !self.invalidate_async_work(cx) {
            return;
        }
        self.reconcile_schedule(OrionCodeUpdateScheduleReason::Startup, true, cx);
    }

    pub(crate) fn automatic_install_enabled(&self) -> bool {
        self.settings.mode == OrionCodeUpdateMode::Automatic && record_allows_updates(&self.record)
    }

    pub(crate) fn managed_operations_ready(&self) -> bool {
        !self.quitting
            && !self.initializing
            && self.hard_disabled.is_none()
            && self.persistence_task.is_none()
            && self.managed_operation_generation.is_none()
            && self.active_check.is_none()
            && self.network_task.is_none()
    }

    pub fn check_now(&mut self, cx: &mut Context<Self>) {
        if self.quitting
            || self.initializing
            || self.hard_disabled.is_some()
            || self.managed_operation_generation.is_some()
        {
            return;
        }
        if !record_allows_updates(&self.record) {
            self.reconcile_schedule(OrionCodeUpdateScheduleReason::SettingsChanged, false, cx);
            return;
        }
        if let Some(active_check) = self.active_check.as_mut() {
            if self.network_task.is_some() {
                active_check.trigger = OrionCodeUpdateCheckTrigger::Manual;
                self.status = OrionCodeUpdateCoordinatorStatus::Checking {
                    trigger: OrionCodeUpdateCheckTrigger::Manual,
                    channel: self.settings.channel,
                };
                cx.notify();
            } else {
                self.pending_manual_check = true;
            }
            return;
        }
        if self.persistence_task.is_some() {
            self.pending_manual_check = true;
            return;
        }
        self.scheduled_task.take();
        if self.advance_generation(cx).is_none() {
            return;
        }
        self.start_check(OrionCodeUpdateCheckTrigger::Manual, cx);
    }

    fn persist_initial_state(&mut self, cx: &mut Context<Self>) {
        let generation = self.generation;
        let record = self.record.clone();
        let key_value_store = KeyValueStore::global(cx);
        self.status = OrionCodeUpdateCoordinatorStatus::Initializing;
        self.persistence_task = Some(cx.spawn(async move |coordinator, cx| {
            let result = write_orion_code_update_record(&key_value_store, &record).await;
            coordinator
                .update(cx, |coordinator, cx| {
                    if coordinator.generation != generation {
                        return;
                    }
                    coordinator.persistence_task = None;
                    coordinator.initializing = false;
                    if result.is_err() {
                        coordinator.disable_hard(
                            OrionCodeUpdateDisabledReason::StatePersistenceFailed,
                            Some("state_v2_initial_write_failed".to_string()),
                            cx,
                        );
                        return;
                    }
                    coordinator.schedule_startup_retention(cx);
                    if coordinator.synchronize_control_state_after_initialization(cx) {
                        return;
                    }
                    coordinator.reconcile_schedule(
                        OrionCodeUpdateScheduleReason::Startup,
                        true,
                        cx,
                    );
                })
                .log_err();
        }));
    }

    fn schedule_startup_retention(&mut self, cx: &mut Context<Self>) {
        if cfg!(any(test, feature = "test-support")) {
            return;
        }
        if self.retention_task.is_some() {
            return;
        }
        let Some(target_name) = self.target_name else {
            return;
        };
        let managed_root = paths::external_agents_dir()
            .join("registry")
            .join(ORION_CODE_AGENT_ID);
        let record = self.record.clone();
        let known_targets = BTreeSet::from([target_name.to_string()]);
        let background_task = cx.background_spawn(async move {
            if !managed_root.exists() {
                return Ok(OrionCodeRetentionReport {
                    deleted_installs: Vec::new(),
                });
            }
            let receipt_owned_installs = discover_orion_code_receipt_owned_installs(&managed_root)?;
            let references = OrionCodeRetentionReferences {
                current: record
                    .current_verified
                    .as_ref()
                    .and_then(managed_install_identity_from_receipt),
                previous: record
                    .previous_verified
                    .as_ref()
                    .and_then(managed_install_identity_from_receipt),
                staged: record
                    .staged
                    .as_ref()
                    .map(|staged| OrionCodeManagedInstallIdentity {
                        version: staged.version.clone(),
                        target: staged.target.clone(),
                    }),
            };
            let plan = plan_orion_code_retention(OrionCodeRetentionInput {
                managed_root,
                known_targets,
                receipt_owned_installs,
                references,
                revoked_versions: record.known_revoked_versions.into_iter().collect(),
                restart_grace: OrionCodeRetentionRestartGrace {
                    successful_activation_app_start_sequence: record
                        .successful_activation_app_start_sequence,
                    current_app_start_sequence: record.app_start_sequence,
                },
            })?;
            apply_orion_code_retention_plan(&plan)
        });
        self.retention_task = Some(cx.spawn(async move |coordinator, cx| {
            let result = background_task.await;
            coordinator
                .update(cx, |coordinator, _cx| {
                    coordinator.retention_task = None;
                    match result {
                        Ok(report) if !report.deleted_installs.is_empty() => {
                            log::info!(
                                "Pruned {} old Orion Code managed install(s)",
                                report.deleted_installs.len()
                            );
                        }
                        Ok(_report) => {}
                        Err(error) => {
                            log::warn!("Orion Code managed retention was skipped: {error}");
                        }
                    }
                })
                .log_err();
        }));
    }

    fn handle_settings_changed(&mut self, cx: &mut Context<Self>) {
        let settings = read_update_settings(cx);
        let custom_configuration = has_custom_orion_code_configuration(cx);
        if self.managed_operation_generation.is_some() {
            self.settings = settings;
            self.pending_control_reconcile = true;
            return;
        }
        if self.initializing {
            self.settings = settings;
            return;
        }

        let settings_changed = settings != self.settings;
        let management_changed = custom_configuration
            && self.record.management_state != OrionCodeUpdateManagementState::CustomConflict;
        if !settings_changed && !management_changed {
            return;
        }

        self.settings = settings;
        if !self.invalidate_async_work(cx) {
            return;
        }
        if management_changed {
            let mut record = self.record.clone();
            record.management_state = OrionCodeUpdateManagementState::CustomConflict;
            self.persist_control_record(record, OrionCodeUpdateScheduleReason::SettingsChanged, cx);
        } else {
            self.reconcile_schedule(OrionCodeUpdateScheduleReason::SettingsChanged, true, cx);
        }
    }

    fn handle_bootstrap_changed(
        &mut self,
        bootstrap_choice: OrionCodeBootstrapChoice,
        cx: &mut Context<Self>,
    ) {
        if self.initializing || self.quitting {
            return;
        }
        if self.managed_operation_generation.is_some() {
            self.pending_control_reconcile = true;
            return;
        }
        let should_adopt_choice = bootstrap_choice != OrionCodeBootstrapChoice::NotAsked
            && bootstrap_choice != self.record.choice;
        if !should_adopt_choice {
            return;
        }

        if !self.invalidate_async_work(cx) {
            return;
        }
        let mut record = self.record.clone();
        record.choice = bootstrap_choice;
        if has_custom_orion_code_configuration(cx) {
            record.management_state = OrionCodeUpdateManagementState::CustomConflict;
        }
        self.persist_control_record(
            record,
            OrionCodeUpdateScheduleReason::BootstrapStateChanged,
            cx,
        );
    }

    fn synchronize_control_state_after_initialization(&mut self, cx: &mut Context<Self>) -> bool {
        let bootstrap_choice = OrionCodeBootstrap::global(cx).read(cx).record().choice;
        let mut record = self.record.clone();
        if bootstrap_choice != OrionCodeBootstrapChoice::NotAsked {
            record.choice = bootstrap_choice;
        }
        if has_custom_orion_code_configuration(cx) {
            record.management_state = OrionCodeUpdateManagementState::CustomConflict;
        }
        if record == self.record {
            return false;
        }
        self.persist_control_record(
            record,
            OrionCodeUpdateScheduleReason::BootstrapStateChanged,
            cx,
        );
        true
    }

    fn persist_control_record(
        &mut self,
        record: OrionCodeUpdateRecordV2,
        schedule_reason: OrionCodeUpdateScheduleReason,
        cx: &mut Context<Self>,
    ) {
        if let Err(_error) = validate_update_record(&record) {
            self.disable_hard(
                OrionCodeUpdateDisabledReason::StatePersistenceFailed,
                Some("state_v2_control_record_invalid".to_string()),
                cx,
            );
            return;
        }
        let generation = self.generation;
        let key_value_store = KeyValueStore::global(cx);
        self.persistence_task = Some(cx.spawn(async move |coordinator, cx| {
            let result = write_orion_code_update_record(&key_value_store, &record).await;
            coordinator
                .update(cx, |coordinator, cx| {
                    if coordinator.generation != generation {
                        return;
                    }
                    coordinator.persistence_task = None;
                    if result.is_err() {
                        coordinator.disable_hard(
                            OrionCodeUpdateDisabledReason::StatePersistenceFailed,
                            Some("state_v2_control_write_failed".to_string()),
                            cx,
                        );
                        return;
                    }
                    if apply_runtime_for_record(&record, cx).is_err() {
                        coordinator.disable_hard(
                            OrionCodeUpdateDisabledReason::StatePersistenceFailed,
                            Some("managed_runtime_control_apply_failed".to_string()),
                            cx,
                        );
                        return;
                    }
                    coordinator.record = record;
                    if coordinator.take_pending_manual_check() {
                        coordinator.start_check(OrionCodeUpdateCheckTrigger::Manual, cx);
                    } else {
                        coordinator.reconcile_schedule(schedule_reason, true, cx);
                    }
                })
                .log_err();
        }));
    }

    fn handle_system_wake(&mut self, cx: &mut Context<Self>) {
        if self.quitting || self.initializing || self.hard_disabled.is_some() {
            return;
        }
        if self.persistence_task.is_some() {
            return;
        }

        if self.network_task.is_some() {
            let trigger = self
                .active_check
                .map(|active_check| active_check.trigger)
                .unwrap_or(OrionCodeUpdateCheckTrigger::Automatic);
            self.network_task.take();
            self.active_check = None;
            if self.advance_generation(cx).is_none() {
                return;
            }
            if trigger == OrionCodeUpdateCheckTrigger::Manual {
                self.start_check(OrionCodeUpdateCheckTrigger::Manual, cx);
                return;
            }

            let now = self.clock.now();
            let wake_error = OrionCodeUpdateFeedError::new(
                OrionCodeUpdateErrorKind::Network,
                "network_check_cancelled_after_wake",
            );
            let (record, summary) = record_after_check(
                &self.record,
                OrionCodeUpdateCheckTrigger::Automatic,
                Err(wake_error),
                now,
            );
            self.persist_check_record(record, summary, cx);
            return;
        }

        self.scheduled_task.take();
        if self.advance_generation(cx).is_none() {
            return;
        }
        self.reconcile_schedule(OrionCodeUpdateScheduleReason::SystemWake, true, cx);
    }

    fn start_check(&mut self, trigger: OrionCodeUpdateCheckTrigger, cx: &mut Context<Self>) {
        if self.quitting || self.hard_disabled.is_some() || !record_allows_updates(&self.record) {
            return;
        }
        if trigger == OrionCodeUpdateCheckTrigger::Automatic
            && self.settings.mode != OrionCodeUpdateMode::Automatic
        {
            self.status = OrionCodeUpdateCoordinatorStatus::Idle;
            cx.notify();
            return;
        }
        if let Some(diagnostic_code) = self.feed.disabled_diagnostic_code() {
            self.status = OrionCodeUpdateCoordinatorStatus::Disabled {
                reason: OrionCodeUpdateDisabledReason::SignedFeedUnavailable,
                diagnostic_code: Some(sanitize_diagnostic_code(diagnostic_code.to_string())),
            };
            cx.notify();
            return;
        }
        if self.active_check.is_some() || self.persistence_task.is_some() {
            if trigger == OrionCodeUpdateCheckTrigger::Manual {
                self.pending_manual_check = true;
            }
            return;
        }

        let generation = self.generation;
        let request = OrionCodeUpdateCheckRequest {
            trigger,
            channel: self.settings.channel,
            highest_accepted_sequence: self.record.highest_index_sequence,
            cached_etag: self.record.cached_index_etag.clone(),
        };
        let feed = self.feed.clone();
        let clock = self.clock.clone();
        self.active_check = Some(ActiveCheck {
            generation,
            trigger,
        });
        self.status = OrionCodeUpdateCoordinatorStatus::Checking {
            trigger,
            channel: self.settings.channel,
        };
        cx.notify();

        self.network_task = Some(cx.spawn(async move |coordinator, cx| {
            let result = feed.check(request).await;
            let completed_at = clock.now();
            coordinator
                .update(cx, |coordinator, cx| {
                    coordinator.finish_network_check(generation, result, completed_at, cx);
                })
                .log_err();
        }));
    }

    fn finish_network_check(
        &mut self,
        generation: u64,
        result: Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>,
        completed_at: DateTime<Utc>,
        cx: &mut Context<Self>,
    ) {
        if !completion_is_current(self.active_check, self.generation, generation) {
            return;
        }
        self.network_task = None;
        let result = result.and_then(|response| {
            let verified_index = match &response {
                OrionCodeUpdateFeedResponse::NotModified { index, .. }
                | OrionCodeUpdateFeedResponse::VerifiedIndex { index, .. } => index,
            };
            match self.resolve_index(verified_index) {
                Ok(resolution) => {
                    self.latest_resolution = Some(resolution);
                    Ok(response)
                }
                Err(error) => {
                    self.latest_resolution = None;
                    Err(error)
                }
            }
        });
        let trigger = self
            .active_check
            .map(|active_check| active_check.trigger)
            .unwrap_or(OrionCodeUpdateCheckTrigger::Automatic);
        let (record, summary) = record_after_check(&self.record, trigger, result, completed_at);
        self.persist_check_record(record, summary, cx);
    }

    fn resolve_index(
        &self,
        verified_index: &VerifiedOrionCodeUpdateIndex,
    ) -> Result<OrionCodeUpdateResolution, OrionCodeUpdateFeedError> {
        let Some(target_name) = self.target_name else {
            return Ok(OrionCodeUpdateResolution {
                candidate: None,
                current_release_state: OrionCodeCurrentReleaseState::Unknown,
            });
        };
        let current_version = self
            .record
            .current_verified
            .as_ref()
            .map(|receipt| Version::parse(&receipt.version))
            .transpose()
            .map_err(|_| {
                OrionCodeUpdateFeedError::new(
                    OrionCodeUpdateErrorKind::Persistence,
                    "current_receipt_version_invalid",
                )
            })?;
        let known_revoked_versions = self
            .record
            .known_revoked_versions
            .iter()
            .map(|version| Version::parse(version))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                OrionCodeUpdateFeedError::new(
                    OrionCodeUpdateErrorKind::Persistence,
                    "revoked_receipt_version_invalid",
                )
            })?;
        resolve_orion_code_candidate(
            verified_index,
            OrionCodeCandidateContext {
                channel: self.settings.channel,
                studio_version: &self.studio_version,
                current_version: current_version.as_ref(),
                target_name,
                installation_id: &self.installation_id,
                known_revoked_versions: &known_revoked_versions,
            },
        )
        .map_err(|_| {
            OrionCodeUpdateFeedError::new(
                OrionCodeUpdateErrorKind::IndexEnvelope,
                "candidate_resolution_failed",
            )
        })
    }

    fn persist_check_record(
        &mut self,
        record: OrionCodeUpdateRecordV2,
        summary: PersistedCheckSummary,
        cx: &mut Context<Self>,
    ) {
        let (record, summary) = match validate_update_record(&record) {
            Ok(()) => (record, summary),
            Err(_) => record_after_check(
                &self.record,
                summary.trigger,
                Err(OrionCodeUpdateFeedError::new(
                    OrionCodeUpdateErrorKind::IndexEnvelope,
                    "invalid_check_result_metadata",
                )),
                self.clock.now(),
            ),
        };
        let generation = self.generation;
        let key_value_store = KeyValueStore::global(cx);
        self.status = OrionCodeUpdateCoordinatorStatus::SavingCheckResult {
            trigger: summary.trigger,
        };
        cx.notify();
        self.persistence_task = Some(cx.spawn(async move |coordinator, cx| {
            let result = write_orion_code_update_record(&key_value_store, &record).await;
            coordinator
                .update(cx, |coordinator, cx| {
                    if coordinator.generation != generation {
                        return;
                    }
                    coordinator.persistence_task = None;
                    coordinator.active_check = None;
                    if result.is_err() {
                        coordinator.disable_hard(
                            OrionCodeUpdateDisabledReason::StatePersistenceFailed,
                            Some("state_v2_check_write_failed".to_string()),
                            cx,
                        );
                        return;
                    }
                    coordinator.record = record;
                    cx.notify();
                    coordinator.finish_persisted_check(summary, cx);
                })
                .log_err();
        }));
    }

    fn finish_persisted_check(&mut self, summary: PersistedCheckSummary, cx: &mut Context<Self>) {
        if self.take_pending_manual_check() {
            self.start_check(OrionCodeUpdateCheckTrigger::Manual, cx);
            return;
        }

        if summary.trigger == OrionCodeUpdateCheckTrigger::Manual
            && let Some(error) = summary.error
        {
            let next_automatic_check_at = (self.settings.mode == OrionCodeUpdateMode::Automatic)
                .then_some(self.record.next_eligible_check_at)
                .flatten();
            self.status = OrionCodeUpdateCoordinatorStatus::ManualCheckFailed {
                kind: error.kind,
                diagnostic_code: error.diagnostic_code,
                next_automatic_check_at,
            };
            if self.settings.mode == OrionCodeUpdateMode::Automatic {
                self.arm_automatic_timer(summary.schedule_reason, true, cx);
            } else {
                cx.notify();
            }
            return;
        }

        if self.settings.mode == OrionCodeUpdateMode::Automatic {
            self.arm_automatic_timer(summary.schedule_reason, false, cx);
        } else {
            self.status = OrionCodeUpdateCoordinatorStatus::Idle;
            cx.notify();
        }
    }

    fn reconcile_schedule(
        &mut self,
        reason: OrionCodeUpdateScheduleReason,
        apply_startup_jitter: bool,
        cx: &mut Context<Self>,
    ) {
        self.scheduled_task.take();
        if self.managed_operation_generation.is_some() {
            self.status = OrionCodeUpdateCoordinatorStatus::Idle;
            cx.notify();
            return;
        }
        if let Some((disabled_reason, diagnostic_code)) = &self.hard_disabled {
            self.status = OrionCodeUpdateCoordinatorStatus::Disabled {
                reason: *disabled_reason,
                diagnostic_code: diagnostic_code.clone(),
            };
            cx.notify();
            return;
        }
        if self.quitting {
            self.status = OrionCodeUpdateCoordinatorStatus::Disabled {
                reason: OrionCodeUpdateDisabledReason::AppQuitting,
                diagnostic_code: None,
            };
            cx.notify();
            return;
        }
        if let Some(reason) = disabled_reason_for_record(&self.record) {
            self.status = OrionCodeUpdateCoordinatorStatus::Disabled {
                reason,
                diagnostic_code: None,
            };
            cx.notify();
            return;
        }
        if let Some(diagnostic_code) = self.feed.disabled_diagnostic_code() {
            self.status = OrionCodeUpdateCoordinatorStatus::Disabled {
                reason: OrionCodeUpdateDisabledReason::SignedFeedUnavailable,
                diagnostic_code: Some(sanitize_diagnostic_code(diagnostic_code.to_string())),
            };
            cx.notify();
            return;
        }
        if self.settings.mode == OrionCodeUpdateMode::Manual {
            self.status = OrionCodeUpdateCoordinatorStatus::Idle;
            cx.notify();
            return;
        }

        if apply_startup_jitter {
            self.arm_timer_with_startup_jitter(reason, cx);
        } else {
            self.arm_automatic_timer(reason, false, cx);
        }
    }

    fn arm_timer_with_startup_jitter(
        &mut self,
        reason: OrionCodeUpdateScheduleReason,
        cx: &mut Context<Self>,
    ) {
        let now = self.clock.now();
        let delay = startup_check_delay(
            self.startup_jitter_seed,
            now,
            self.record.next_eligible_check_at,
        );
        let next_check_at = date_time_after(now, delay);
        self.arm_timer(delay, next_check_at, reason, false, cx);
    }

    fn arm_automatic_timer(
        &mut self,
        reason: OrionCodeUpdateScheduleReason,
        preserve_status: bool,
        cx: &mut Context<Self>,
    ) {
        let now = self.clock.now();
        let next_check_at = self.record.next_eligible_check_at.unwrap_or(now);
        let delay = remaining_delay(now, next_check_at);
        self.arm_timer(delay, next_check_at, reason, preserve_status, cx);
    }

    fn arm_timer(
        &mut self,
        delay: Duration,
        next_check_at: DateTime<Utc>,
        reason: OrionCodeUpdateScheduleReason,
        preserve_status: bool,
        cx: &mut Context<Self>,
    ) {
        self.scheduled_task.take();
        let generation = self.generation;
        if !preserve_status {
            self.status = OrionCodeUpdateCoordinatorStatus::Scheduled {
                next_check_at,
                reason,
            };
        }
        cx.notify();
        self.scheduled_task = Some(cx.spawn(async move |coordinator, cx| {
            cx.background_executor().timer(delay).await;
            coordinator
                .update(cx, |coordinator, cx| {
                    if coordinator.generation != generation {
                        return;
                    }
                    coordinator.scheduled_task = None;
                    coordinator.start_check(OrionCodeUpdateCheckTrigger::Automatic, cx);
                })
                .log_err();
        }));
    }

    fn invalidate_async_work(&mut self, cx: &mut Context<Self>) -> bool {
        if self.managed_operation_generation.is_some() {
            self.pending_control_reconcile = true;
            return false;
        }
        self.scheduled_task.take();
        self.network_task.take();
        self.persistence_task.take();
        self.active_check = None;
        self.pending_manual_check = false;
        self.advance_generation(cx).is_some()
    }

    fn advance_generation(&mut self, cx: &mut Context<Self>) -> Option<u64> {
        let Some(generation) = self.generation.checked_add(1) else {
            self.disable_hard(
                OrionCodeUpdateDisabledReason::GenerationExhausted,
                Some("coordinator_generation_exhausted".to_string()),
                cx,
            );
            return None;
        };
        self.generation = generation;
        Some(generation)
    }

    fn disable_hard(
        &mut self,
        reason: OrionCodeUpdateDisabledReason,
        diagnostic_code: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.scheduled_task.take();
        self.network_task.take();
        self.persistence_task.take();
        self.active_check = None;
        self.pending_manual_check = false;
        self.hard_disabled = Some((reason, diagnostic_code.clone()));
        self.status = OrionCodeUpdateCoordinatorStatus::Disabled {
            reason,
            diagnostic_code,
        };
        cx.notify();
    }

    fn take_pending_manual_check(&mut self) -> bool {
        std::mem::take(&mut self.pending_manual_check)
            && !self.quitting
            && self.hard_disabled.is_none()
            && record_allows_updates(&self.record)
    }

    pub(crate) fn begin_managed_operation(
        &mut self,
        expected_record: &OrionCodeUpdateRecordV2,
        cx: &mut Context<Self>,
    ) -> Result<u64, String> {
        if self.quitting || self.initializing || self.hard_disabled.is_some() {
            return Err("update coordinator is unavailable".to_string());
        }
        if self.managed_operation_generation.is_some()
            || self.persistence_task.is_some()
            || self.active_check.is_some()
            || self.network_task.is_some()
        {
            return Err("another update operation is active".to_string());
        }
        if &self.record != expected_record {
            return Err("update state changed before the operation began".to_string());
        }
        self.scheduled_task.take();
        let generation = self
            .advance_generation(cx)
            .ok_or_else(|| "update generation is unavailable".to_string())?;
        self.managed_operation_generation = Some(generation);
        self.status = OrionCodeUpdateCoordinatorStatus::Idle;
        cx.notify();
        Ok(generation)
    }

    pub(crate) fn checkpoint_managed_operation(
        &mut self,
        generation: u64,
        record: OrionCodeUpdateRecordV2,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.managed_operation_generation != Some(generation) || self.generation != generation {
            return Err("managed update operation is stale".to_string());
        }
        validate_update_record(&record)
            .map_err(|_| "managed update state is invalid".to_string())?;
        self.record = record;
        cx.notify();
        Ok(())
    }

    pub(crate) fn finish_managed_operation(
        &mut self,
        generation: u64,
        record: OrionCodeUpdateRecordV2,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.checkpoint_managed_operation(generation, record, cx)?;
        if apply_runtime_for_record(&self.record, cx).is_err() {
            return Err("managed runtime apply failed".to_string());
        }
        self.managed_operation_generation = None;
        if std::mem::take(&mut self.pending_control_reconcile)
            && self.synchronize_control_state_after_initialization(cx)
        {
            return Ok(());
        }
        self.reconcile_schedule(OrionCodeUpdateScheduleReason::Periodic, false, cx);
        Ok(())
    }

    pub(crate) fn finish_managed_operation_without_runtime_change(
        &mut self,
        generation: u64,
        record: OrionCodeUpdateRecordV2,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.checkpoint_managed_operation(generation, record, cx)?;
        self.managed_operation_generation = None;
        if std::mem::take(&mut self.pending_control_reconcile)
            && self.synchronize_control_state_after_initialization(cx)
        {
            return Ok(());
        }
        self.reconcile_schedule(OrionCodeUpdateScheduleReason::Periodic, false, cx);
        Ok(())
    }

    pub(crate) fn cancel_managed_operation(
        &mut self,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.managed_operation_generation != Some(generation) || self.generation != generation {
            return Err("managed update operation is stale".to_string());
        }
        self.managed_operation_generation = None;
        self.reconcile_schedule(OrionCodeUpdateScheduleReason::Periodic, false, cx);
        Ok(())
    }
}

fn managed_install_identity_from_receipt(
    receipt: &crate::orion_code_update::OrionCodeInstallReceiptV2,
) -> Option<OrionCodeManagedInstallIdentity> {
    if receipt.source != OrionCodeInstallSource::Archive || receipt.legacy_imported {
        return None;
    }
    Some(OrionCodeManagedInstallIdentity {
        version: receipt.version.clone(),
        target: receipt.target.clone()?,
    })
}

fn apply_runtime_for_record(record: &OrionCodeUpdateRecordV2, cx: &mut App) -> Result<(), String> {
    let receipt = record_allows_updates(record)
        .then(|| preferred_orion_code_runtime_receipt(record))
        .flatten();
    let runtime = receipt
        .filter(|receipt| receipt.source == OrionCodeInstallSource::Archive)
        .map(|receipt| {
            OrionCodeManagedArchiveRuntime::new(
                &receipt.version,
                receipt
                    .target
                    .as_deref()
                    .ok_or_else(|| "managed runtime target is missing".to_string())?,
                receipt
                    .command
                    .as_deref()
                    .ok_or_else(|| "managed runtime command is missing".to_string())?,
            )
            .map_err(|_| "managed runtime receipt is invalid".to_string())
        })
        .transpose()?;

    let previous_runtime = orion_code_managed_archive_runtime(cx);
    set_orion_code_managed_archive_runtime(runtime, cx);
    if let Some(registry) = AgentRegistryStore::try_global(cx) {
        let result = registry.update(cx, |registry, cx| {
            if let Some(receipt) = receipt {
                registry.pin_orion_code_to_verified_version(&receipt.version, cx)
            } else {
                registry.clear_orion_code_version_pin_and_refresh(cx);
                Ok(())
            }
        });
        if result.is_err() {
            set_orion_code_managed_archive_runtime(previous_runtime, cx);
            return Err("managed runtime registry binding failed".to_string());
        }
    }
    Ok(())
}

fn read_update_settings(cx: &App) -> OrionCodeUpdateSettingsSnapshot {
    let settings = AgentSettings::get_global(cx);
    let channel = match settings.orion_code_update_channel {
        SettingsUpdateChannel::Stable => OrionCodeUpdateChannel::Stable,
        SettingsUpdateChannel::Beta => OrionCodeUpdateChannel::Beta,
    };
    let mode = match settings.orion_code_update_mode {
        SettingsUpdateMode::Automatic => OrionCodeUpdateMode::Automatic,
        SettingsUpdateMode::Manual => OrionCodeUpdateMode::Manual,
    };
    OrionCodeUpdateSettingsSnapshot { channel, mode }
}

fn has_custom_orion_code_configuration(cx: &App) -> bool {
    matches!(
        AllAgentServersSettings::get_global(cx).get(ORION_CODE_AGENT_ID),
        Some(CustomAgentServerSettings::Custom { .. })
    )
}

fn current_orion_code_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-aarch64"),
        ("macos", "x86_64") => Some("darwin-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("windows", "aarch64") => Some("windows-aarch64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        _ => None,
    }
}

fn record_allows_updates(record: &OrionCodeUpdateRecordV2) -> bool {
    record.choice == OrionCodeBootstrapChoice::Accepted
        && record.management_state == OrionCodeUpdateManagementState::Managed
}

fn completion_is_current(
    active_check: Option<ActiveCheck>,
    current_generation: u64,
    completed_generation: u64,
) -> bool {
    current_generation == completed_generation
        && active_check.is_some_and(|active_check| active_check.generation == completed_generation)
}

fn disabled_reason_for_record(
    record: &OrionCodeUpdateRecordV2,
) -> Option<OrionCodeUpdateDisabledReason> {
    if record.management_state == OrionCodeUpdateManagementState::CustomConflict {
        return Some(OrionCodeUpdateDisabledReason::CustomConfiguration);
    }
    match record.choice {
        OrionCodeBootstrapChoice::Accepted => None,
        OrionCodeBootstrapChoice::NotAsked => Some(OrionCodeUpdateDisabledReason::NotAccepted),
        OrionCodeBootstrapChoice::Declined => Some(OrionCodeUpdateDisabledReason::Declined),
        OrionCodeBootstrapChoice::Removed => Some(OrionCodeUpdateDisabledReason::Removed),
    }
}

fn record_after_check(
    current: &OrionCodeUpdateRecordV2,
    trigger: OrionCodeUpdateCheckTrigger,
    result: Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>,
    completed_at: DateTime<Utc>,
) -> (OrionCodeUpdateRecordV2, PersistedCheckSummary) {
    let mut record = current.clone();
    match result {
        Ok(OrionCodeUpdateFeedResponse::NotModified { index, etag }) => {
            if current.highest_index_sequence == 0
                || index.index.sequence != current.highest_index_sequence
            {
                return record_after_check(
                    current,
                    trigger,
                    Err(OrionCodeUpdateFeedError::new(
                        OrionCodeUpdateErrorKind::IndexReplay,
                        "cached_index_sequence_mismatch",
                    )),
                    completed_at,
                );
            }
            if let Some(etag) = etag {
                record.cached_index_etag = Some(etag);
            }
            apply_signed_control_states(&mut record, &index);
            record.last_successful_check_at = Some(completed_at);
            record.next_eligible_check_at =
                Some(date_time_after(completed_at, PERIODIC_CHECK_INTERVAL));
            record.failure_count = 0;
            record.last_error_kind = None;
            (
                record,
                PersistedCheckSummary {
                    trigger,
                    error: None,
                    schedule_reason: OrionCodeUpdateScheduleReason::Periodic,
                },
            )
        }
        Ok(OrionCodeUpdateFeedResponse::VerifiedIndex { index, etag }) => {
            if index.index.sequence <= current.highest_index_sequence {
                return record_after_check(
                    current,
                    trigger,
                    Err(OrionCodeUpdateFeedError::new(
                        OrionCodeUpdateErrorKind::IndexReplay,
                        "verified_index_sequence_replay",
                    )),
                    completed_at,
                );
            }
            record.highest_index_sequence = index.index.sequence;
            record.cached_index_etag = etag;
            apply_signed_control_states(&mut record, &index);
            record.last_successful_check_at = Some(completed_at);
            record.next_eligible_check_at =
                Some(date_time_after(completed_at, PERIODIC_CHECK_INTERVAL));
            record.failure_count = 0;
            record.last_error_kind = None;
            (
                record,
                PersistedCheckSummary {
                    trigger,
                    error: None,
                    schedule_reason: OrionCodeUpdateScheduleReason::Periodic,
                },
            )
        }
        Err(error) => {
            let schedule_reason = if error.kind == OrionCodeUpdateErrorKind::Network {
                record.failure_count = record.failure_count.saturating_add(1);
                OrionCodeUpdateScheduleReason::NetworkBackoff
            } else {
                record.failure_count = 0;
                OrionCodeUpdateScheduleReason::Periodic
            };
            let delay = check_failure_delay(error.kind, record.failure_count);
            record.next_eligible_check_at = Some(date_time_after(completed_at, delay));
            record.last_error_kind = Some(error.kind);
            (
                record,
                PersistedCheckSummary {
                    trigger,
                    error: Some(error),
                    schedule_reason,
                },
            )
        }
    }
}

fn apply_signed_control_states(
    record: &mut OrionCodeUpdateRecordV2,
    verified_index: &VerifiedOrionCodeUpdateIndex,
) {
    record.known_paused_versions = verified_index
        .index
        .releases
        .iter()
        .filter(|release| release.status == OrionCodeReleaseStatus::Paused)
        .map(|release| release.version.clone())
        .collect();
    record.known_paused_versions.sort();
    for release in &verified_index.index.releases {
        if release.status == OrionCodeReleaseStatus::Revoked
            && !record.known_revoked_versions.contains(&release.version)
        {
            record.known_revoked_versions.push(release.version.clone());
        }
    }
    record.known_revoked_versions.sort();
    record
        .known_paused_versions
        .retain(|version| !record.known_revoked_versions.contains(version));
}

fn startup_check_delay(
    jitter_seed: u64,
    now: DateTime<Utc>,
    next_eligible_check_at: Option<DateTime<Utc>>,
) -> Duration {
    let jitter = deterministic_startup_jitter(jitter_seed);
    let eligibility_delay = next_eligible_check_at
        .map(|next_check_at| remaining_delay(now, next_check_at))
        .unwrap_or(Duration::ZERO);
    jitter.max(eligibility_delay)
}

fn scheduler_seed(installation_id: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in b"orion-code-update-startup-v1"
        .iter()
        .chain(installation_id.as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn deterministic_startup_jitter(seed: u64) -> Duration {
    STARTUP_JITTER_MINIMUM + Duration::from_secs(seed % STARTUP_JITTER_RANGE_SECONDS)
}

fn check_failure_delay(kind: OrionCodeUpdateErrorKind, failure_count: u32) -> Duration {
    if kind != OrionCodeUpdateErrorKind::Network {
        return PERIODIC_CHECK_INTERVAL;
    }
    match failure_count {
        0 | 1 => FIRST_NETWORK_RETRY,
        2 => SECOND_NETWORK_RETRY,
        _ => FINAL_NETWORK_RETRY,
    }
}

fn remaining_delay(now: DateTime<Utc>, next_check_at: DateTime<Utc>) -> Duration {
    next_check_at
        .signed_duration_since(now)
        .to_std()
        .unwrap_or(Duration::ZERO)
}

fn date_time_after(now: DateTime<Utc>, delay: Duration) -> DateTime<Utc> {
    let seconds = i64::try_from(delay.as_secs()).unwrap_or(i64::MAX);
    now.checked_add_signed(ChronoDuration::seconds(seconds))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

fn sanitize_diagnostic_code(code: String) -> String {
    if code.is_empty()
        || code.len() > 96
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        "orion_code_update_error".to_string()
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use futures::FutureExt as _;
    use gpui::{AppContext as _, TestAppContext};
    use parking_lot::Mutex;

    use super::*;

    struct FakeOrionCodeUpdateClock {
        now: Mutex<DateTime<Utc>>,
    }

    impl FakeOrionCodeUpdateClock {
        fn new(now: DateTime<Utc>) -> Self {
            Self {
                now: Mutex::new(now),
            }
        }

        fn set(&self, now: DateTime<Utc>) {
            *self.now.lock() = now;
        }
    }

    impl OrionCodeUpdateClock for FakeOrionCodeUpdateClock {
        fn now(&self) -> DateTime<Utc> {
            *self.now.lock()
        }
    }

    #[derive(Default)]
    struct PendingOrionCodeUpdateFeed {
        requests: Mutex<Vec<OrionCodeUpdateCheckRequest>>,
        call_count: AtomicUsize,
    }

    impl PendingOrionCodeUpdateFeed {
        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }

        fn requests(&self) -> Vec<OrionCodeUpdateCheckRequest> {
            self.requests.lock().clone()
        }
    }

    impl OrionCodeUpdateFeed for PendingOrionCodeUpdateFeed {
        fn disabled_diagnostic_code(&self) -> Option<&'static str> {
            None
        }

        fn check(
            &self,
            request: OrionCodeUpdateCheckRequest,
        ) -> BoxFuture<'static, Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>>
        {
            self.requests.lock().push(request);
            self.call_count.fetch_add(1, Ordering::SeqCst);
            future::pending().boxed()
        }
    }

    struct SchedulerHarness {
        coordinator: Entity<OrionCodeUpdateCoordinator>,
        clock: Arc<FakeOrionCodeUpdateClock>,
        feed: Arc<PendingOrionCodeUpdateFeed>,
    }

    fn scheduler_harness(cx: &mut TestAppContext, now: DateTime<Utc>) -> SchedulerHarness {
        let clock = Arc::new(FakeOrionCodeUpdateClock::new(now));
        let feed = Arc::new(PendingOrionCodeUpdateFeed::default());
        let coordinator = cx.update(|cx| {
            cx.new(|_cx| OrionCodeUpdateCoordinator {
                record: OrionCodeUpdateRecordV2 {
                    choice: OrionCodeBootstrapChoice::Accepted,
                    management_state: OrionCodeUpdateManagementState::Managed,
                    ..OrionCodeUpdateRecordV2::default()
                },
                settings: OrionCodeUpdateSettingsSnapshot {
                    channel: OrionCodeUpdateChannel::Stable,
                    mode: OrionCodeUpdateMode::Automatic,
                },
                status: OrionCodeUpdateCoordinatorStatus::Idle,
                feed: feed.clone(),
                clock: clock.clone(),
                installation_id: "scheduler-test-installation".to_string(),
                studio_version: Version::new(1, 0, 0),
                target_name: Some("darwin-aarch64"),
                latest_resolution: None,
                startup_jitter_seed: 0,
                generation: 1,
                active_check: None,
                pending_manual_check: false,
                scheduled_task: None,
                network_task: None,
                persistence_task: None,
                retention_task: None,
                managed_operation_generation: None,
                pending_control_reconcile: false,
                hard_disabled: None,
                initializing: false,
                quitting: false,
                _subscriptions: Vec::new(),
            })
        });
        SchedulerHarness {
            coordinator,
            clock,
            feed,
        }
    }

    fn persisted_check_summary(
        record: &OrionCodeUpdateRecordV2,
        completed_at: DateTime<Utc>,
        result: Result<OrionCodeUpdateFeedResponse, OrionCodeUpdateFeedError>,
    ) -> (OrionCodeUpdateRecordV2, PersistedCheckSummary) {
        record_after_check(
            record,
            OrionCodeUpdateCheckTrigger::Automatic,
            result,
            completed_at,
        )
    }

    fn verified_empty_index(sequence: u64) -> VerifiedOrionCodeUpdateIndex {
        VerifiedOrionCodeUpdateIndex {
            index: serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "sequence": sequence,
                "generated_at": "2026-09-01T00:00:00Z",
                "expires_at": "2026-09-08T00:00:00Z",
                "releases": []
            }))
            .expect("scheduler test index must be valid"),
            key_id: "scheduler-test-key".to_string(),
            payload_sha256: "a".repeat(64),
        }
    }

    fn advance_scheduler(cx: &mut TestAppContext, duration: Duration) {
        cx.background_executor.advance_clock(duration);
        cx.run_until_parked();
    }

    fn time(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("valid test timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn startup_jitter_is_deterministic_and_bounded() {
        for seed in ["installation-a", "installation-b", "installation-c"] {
            let seed = scheduler_seed(seed);
            let first = deterministic_startup_jitter(seed);
            let second = deterministic_startup_jitter(seed);
            assert_eq!(first, second);
            assert!(first >= Duration::from_secs(30));
            assert!(first <= Duration::from_secs(90));
        }
    }

    #[test]
    fn startup_delay_never_checks_before_persisted_eligibility() {
        let now = time("2026-09-01T00:00:00Z");
        let four_hours_later = time("2026-09-01T04:00:00Z");
        assert_eq!(
            startup_check_delay(
                scheduler_seed("installation-a"),
                now,
                Some(four_hours_later)
            ),
            Duration::from_secs(4 * 60 * 60)
        );

        let due = time("2026-08-31T23:00:00Z");
        let delay = startup_check_delay(scheduler_seed("installation-a"), now, Some(due));
        assert!(delay >= Duration::from_secs(30));
        assert!(delay <= Duration::from_secs(90));
    }

    #[test]
    fn automatic_network_backoff_follows_the_required_table() {
        let cases = [
            (0, Duration::from_secs(15 * 60)),
            (1, Duration::from_secs(15 * 60)),
            (2, Duration::from_secs(60 * 60)),
            (3, Duration::from_secs(6 * 60 * 60)),
            (20, Duration::from_secs(6 * 60 * 60)),
        ];
        for (failure_count, expected) in cases {
            assert_eq!(
                check_failure_delay(OrionCodeUpdateErrorKind::Network, failure_count),
                expected
            );
        }
        assert_eq!(
            check_failure_delay(OrionCodeUpdateErrorKind::IndexSignature, 1),
            Duration::from_secs(6 * 60 * 60)
        );
    }

    #[test]
    fn declined_removed_and_custom_records_never_become_schedulable() {
        let mut record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            ..OrionCodeUpdateRecordV2::default()
        };
        assert!(record_allows_updates(&record));

        for choice in [
            OrionCodeBootstrapChoice::Declined,
            OrionCodeBootstrapChoice::Removed,
            OrionCodeBootstrapChoice::NotAsked,
        ] {
            record.choice = choice;
            assert!(!record_allows_updates(&record));
        }

        record.choice = OrionCodeBootstrapChoice::Accepted;
        record.management_state = OrionCodeUpdateManagementState::CustomConflict;
        assert!(!record_allows_updates(&record));
    }

    #[test]
    fn stale_or_cancelled_generations_cannot_complete() {
        let active_check = Some(ActiveCheck {
            generation: 7,
            trigger: OrionCodeUpdateCheckTrigger::Automatic,
        });
        assert!(completion_is_current(active_check, 7, 7));
        assert!(!completion_is_current(active_check, 8, 7));
        assert!(!completion_is_current(active_check, 7, 6));
        assert!(!completion_is_current(None, 7, 7));
    }

    #[test]
    fn check_results_update_only_schedule_metadata() {
        let now = time("2026-09-01T00:00:00Z");
        let record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            highest_index_sequence: 42,
            failure_count: 2,
            ..OrionCodeUpdateRecordV2::default()
        };
        let cached_index = VerifiedOrionCodeUpdateIndex {
            index: serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "sequence": 42,
                "generated_at": "2026-08-31T00:00:00Z",
                "expires_at": "2026-09-07T00:00:00Z",
                "releases": []
            }))
            .expect("cached index"),
            key_id: "test-key".to_string(),
            payload_sha256: "a".repeat(64),
        };
        let (successful, summary) = record_after_check(
            &record,
            OrionCodeUpdateCheckTrigger::Automatic,
            Ok(OrionCodeUpdateFeedResponse::NotModified {
                index: cached_index,
                etag: Some("fixture-etag".to_string()),
            }),
            now,
        );
        assert_eq!(successful.failure_count, 0);
        assert_eq!(successful.last_successful_check_at, Some(now));
        assert_eq!(
            successful.next_eligible_check_at,
            Some(time("2026-09-01T06:00:00Z"))
        );
        assert!(summary.error.is_none());

        let (failed, summary) = record_after_check(
            &record,
            OrionCodeUpdateCheckTrigger::Manual,
            Err(OrionCodeUpdateFeedError::new(
                OrionCodeUpdateErrorKind::Network,
                "offline",
            )),
            now,
        );
        assert_eq!(failed.failure_count, 3);
        assert_eq!(
            failed.next_eligible_check_at,
            Some(time("2026-09-01T06:00:00Z"))
        );
        assert_eq!(
            summary.error.map(|error| error.diagnostic_code),
            Some("offline".to_string())
        );
    }

    #[test]
    fn newer_signed_control_state_blocks_a_previously_staged_paused_release() {
        let now = time("2026-09-01T00:00:00Z");
        let record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            highest_index_sequence: 42,
            staged: Some(crate::orion_code_update::OrionCodeStagedReceiptV2 {
                version: "0.5.0".to_string(),
                target: "darwin-aarch64".to_string(),
                archive_sha256: "a".repeat(64),
                command: "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string(),
                staged_at: now,
                index_sequence: 42,
            }),
            ..OrionCodeUpdateRecordV2::default()
        };
        let paused_index = VerifiedOrionCodeUpdateIndex {
            index: serde_json::from_value(serde_json::json!({
                "schema_version": 1,
                "sequence": 43,
                "generated_at": "2026-09-01T00:00:00Z",
                "expires_at": "2026-09-08T00:00:00Z",
                "releases": [{
                    "version": "0.5.0",
                    "channel": "stable",
                    "status": "paused",
                    "published_at": "2026-09-01T00:00:00Z",
                    "studio_version_requirement": ">=1.0.0,<2.0.0",
                    "acp_protocol": 1,
                    "rollout_basis_points": 0,
                    "rollout_salt": "immutable-release-salt",
                    "rollback_to": null,
                    "release_notes_url": "https://orion-agents.com/releases/0.5.0",
                    "targets": {}
                }]
            }))
            .expect("paused index"),
            key_id: "release-key".to_string(),
            payload_sha256: "b".repeat(64),
        };
        let (paused, summary) = record_after_check(
            &record,
            OrionCodeUpdateCheckTrigger::Automatic,
            Ok(OrionCodeUpdateFeedResponse::VerifiedIndex {
                index: paused_index,
                etag: Some("paused-etag".to_string()),
            }),
            now,
        );

        assert!(summary.error.is_none());
        assert_eq!(paused.known_paused_versions, vec!["0.5.0"]);
        assert_eq!(paused.staged, record.staged);
        assert!(crate::orion_code_update::begin_orion_code_activation(&paused, now, 9).is_err());
    }

    #[test]
    fn diagnostic_codes_cannot_contain_paths_or_control_characters() {
        assert_eq!(
            sanitize_diagnostic_code("network_timeout".to_string()),
            "network_timeout"
        );
        assert_eq!(
            sanitize_diagnostic_code("/Users/example/private".to_string()),
            "orion_code_update_error"
        );
        assert_eq!(
            sanitize_diagnostic_code("bad\ncode".to_string()),
            "orion_code_update_error"
        );
    }

    #[gpui::test]
    fn gpui_startup_jitter_waits_then_fires_once(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();
        let now = time("2026-09-01T00:00:00Z");
        let harness = scheduler_harness(cx, now);

        harness.coordinator.update(cx, |coordinator, cx| {
            coordinator.reconcile_schedule(OrionCodeUpdateScheduleReason::Startup, true, cx);
        });
        assert_eq!(
            harness
                .coordinator
                .read_with(cx, |coordinator, _cx| coordinator.status().clone()),
            OrionCodeUpdateCoordinatorStatus::Scheduled {
                next_check_at: time("2026-09-01T00:00:30Z"),
                reason: OrionCodeUpdateScheduleReason::Startup,
            }
        );

        advance_scheduler(cx, Duration::from_secs(29));
        assert_eq!(harness.feed.call_count(), 0);
        advance_scheduler(cx, Duration::from_secs(1));
        assert_eq!(harness.feed.call_count(), 1);
        advance_scheduler(cx, Duration::from_secs(5 * 60));
        assert_eq!(harness.feed.call_count(), 1);
    }

    #[gpui::test]
    fn gpui_successful_check_rearms_exact_six_hour_period(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();
        let now = time("2026-09-01T00:00:00Z");
        let harness = scheduler_harness(cx, now);
        let current = harness
            .coordinator
            .read_with(cx, |coordinator, _cx| coordinator.record().clone());
        let (record, summary) = persisted_check_summary(
            &current,
            now,
            Ok(OrionCodeUpdateFeedResponse::VerifiedIndex {
                index: verified_empty_index(1),
                etag: Some("scheduler-etag".to_string()),
            }),
        );

        harness.coordinator.update(cx, |coordinator, cx| {
            coordinator.record = record;
            coordinator.finish_persisted_check(summary, cx);
        });
        assert_eq!(
            harness
                .coordinator
                .read_with(cx, |coordinator, _cx| coordinator.status().clone()),
            OrionCodeUpdateCoordinatorStatus::Scheduled {
                next_check_at: time("2026-09-01T06:00:00Z"),
                reason: OrionCodeUpdateScheduleReason::Periodic,
            }
        );

        advance_scheduler(cx, PERIODIC_CHECK_INTERVAL - Duration::from_secs(1));
        assert_eq!(harness.feed.call_count(), 0);
        advance_scheduler(cx, Duration::from_secs(1));
        assert_eq!(harness.feed.call_count(), 1);
    }

    #[gpui::test]
    fn gpui_network_failures_use_escalating_backoff_timers(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();
        let now = time("2026-09-01T00:00:00Z");
        let harnesses = (0..3)
            .map(|failure_count| {
                let harness = scheduler_harness(cx, now);
                let mut current = harness
                    .coordinator
                    .read_with(cx, |coordinator, _cx| coordinator.record().clone());
                current.failure_count = failure_count;
                let (record, summary) = persisted_check_summary(
                    &current,
                    now,
                    Err(OrionCodeUpdateFeedError::new(
                        OrionCodeUpdateErrorKind::Network,
                        "scheduler_network_failure",
                    )),
                );
                harness.coordinator.update(cx, |coordinator, cx| {
                    coordinator.record = record;
                    coordinator.finish_persisted_check(summary, cx);
                });
                harness
            })
            .collect::<Vec<_>>();

        advance_scheduler(cx, FIRST_NETWORK_RETRY);
        assert_eq!(harnesses[0].feed.call_count(), 1);
        assert_eq!(harnesses[1].feed.call_count(), 0);
        assert_eq!(harnesses[2].feed.call_count(), 0);

        advance_scheduler(cx, SECOND_NETWORK_RETRY - FIRST_NETWORK_RETRY);
        assert_eq!(harnesses[1].feed.call_count(), 1);
        assert_eq!(harnesses[2].feed.call_count(), 0);

        advance_scheduler(cx, FINAL_NETWORK_RETRY - SECOND_NETWORK_RETRY);
        assert_eq!(harnesses[2].feed.call_count(), 1);
    }

    #[gpui::test]
    fn gpui_system_wake_rearms_an_overdue_check(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();
        let before_sleep = time("2026-09-01T00:00:00Z");
        let after_wake = time("2026-09-01T02:00:00Z");
        let harness = scheduler_harness(cx, before_sleep);
        harness.coordinator.update(cx, |coordinator, cx| {
            coordinator.record.next_eligible_check_at = Some(time("2026-09-01T01:00:00Z"));
            coordinator.reconcile_schedule(OrionCodeUpdateScheduleReason::Periodic, false, cx);
        });

        harness.clock.set(after_wake);
        harness.coordinator.update(cx, |coordinator, cx| {
            coordinator.handle_system_wake(cx);
        });
        assert_eq!(
            harness
                .coordinator
                .read_with(cx, |coordinator, _cx| coordinator.status().clone()),
            OrionCodeUpdateCoordinatorStatus::Scheduled {
                next_check_at: time("2026-09-01T02:00:30Z"),
                reason: OrionCodeUpdateScheduleReason::SystemWake,
            }
        );

        advance_scheduler(cx, STARTUP_JITTER_MINIMUM - Duration::from_secs(1));
        assert_eq!(harness.feed.call_count(), 0);
        advance_scheduler(cx, Duration::from_secs(1));
        assert_eq!(harness.feed.call_count(), 1);
    }

    #[gpui::test]
    fn gpui_manual_checks_coalesce_into_the_active_single_flight(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();
        let harness = scheduler_harness(cx, time("2026-09-01T00:00:00Z"));
        harness.coordinator.update(cx, |coordinator, cx| {
            coordinator.start_check(OrionCodeUpdateCheckTrigger::Automatic, cx);
        });
        cx.run_until_parked();
        assert_eq!(harness.feed.call_count(), 1);

        harness.coordinator.update(cx, |coordinator, cx| {
            coordinator.check_now(cx);
            coordinator.check_now(cx);
        });
        cx.run_until_parked();

        assert_eq!(harness.feed.call_count(), 1);
        assert_eq!(
            harness.feed.requests(),
            vec![OrionCodeUpdateCheckRequest {
                trigger: OrionCodeUpdateCheckTrigger::Automatic,
                channel: OrionCodeUpdateChannel::Stable,
                highest_accepted_sequence: 0,
                cached_etag: None,
            }]
        );
        assert_eq!(
            harness
                .coordinator
                .read_with(cx, |coordinator, _cx| coordinator.status().clone()),
            OrionCodeUpdateCoordinatorStatus::Checking {
                trigger: OrionCodeUpdateCheckTrigger::Manual,
                channel: OrionCodeUpdateChannel::Stable,
            }
        );
        assert!(harness.coordinator.read_with(cx, |coordinator, _cx| {
            coordinator.active_check.is_some_and(|active_check| {
                active_check.trigger == OrionCodeUpdateCheckTrigger::Manual
            }) && !coordinator.pending_manual_check
        }));
    }
}
