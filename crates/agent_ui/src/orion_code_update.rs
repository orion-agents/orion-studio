use std::collections::HashSet;

pub use ::orion_code_update::{
    MAX_INDEX_BYTES, MAX_SIGNATURE_ENVELOPE_BYTES, OrionCodeCandidate, OrionCodeCandidateContext,
    OrionCodeCurrentReleaseState, OrionCodeIndexVerifier, OrionCodeInstallReceiptV2,
    OrionCodeInstallSource, OrionCodeReleaseStatus, OrionCodeUpdateChannel,
    OrionCodeUpdateContractError, OrionCodeUpdateIndexV1, OrionCodeUpdateMode,
    OrionCodeUpdateResolution, VerifiedOrionCodeUpdateIndex, resolve_orion_code_candidate,
};
use ::orion_code_update::{validate_relative_artifact_path, validate_sha256, validate_target_name};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use db::kvp::KeyValueStore;
use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::orion_code_bootstrap::{
    ORION_CODE_BOOTSTRAP_NAMESPACE, OrionCodeBootstrapChoice, OrionCodeBootstrapErrorKind,
    OrionCodeBootstrapRecord,
};

pub const ORION_CODE_UPDATE_STATE_KEY: &str = "state-v2";
pub const ORION_CODE_UPDATE_INDEX_CACHE_KEY: &str = "index-cache-v1";
pub const ORION_CODE_UPDATE_SCHEMA_VERSION: u32 = 2;
pub const ORION_CODE_UPDATE_INDEX_CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeCachedIndex {
    pub sequence: u64,
    pub index_bytes: Vec<u8>,
    pub signature_envelope_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OrionCodeCachedIndexRecordV1 {
    schema_version: u32,
    sequence: u64,
    index_base64: String,
    signature_envelope_base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeStagedReceiptV2 {
    pub version: String,
    pub target: String,
    pub archive_sha256: String,
    pub command: String,
    pub staged_at: DateTime<Utc>,
    pub index_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeActivationReceiptV2 {
    pub from_version: Option<String>,
    pub to_version: String,
    pub started_at: DateTime<Utc>,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeUpdateManagementState {
    #[default]
    Managed,
    CustomConflict,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeUpdateErrorKind {
    Network,
    IndexEnvelope,
    IndexSignature,
    IndexReplay,
    IndexExpired,
    Incompatible,
    Download,
    Checksum,
    Extract,
    Manifest,
    PlatformSignature,
    Notarization,
    Preflight,
    Activation,
    Rollback,
    Configuration,
    Persistence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeUpdateRecordV2 {
    pub schema_version: u32,
    pub choice: OrionCodeBootstrapChoice,
    pub management_state: OrionCodeUpdateManagementState,
    pub current_verified: Option<OrionCodeInstallReceiptV2>,
    pub previous_verified: Option<OrionCodeInstallReceiptV2>,
    pub staged: Option<OrionCodeStagedReceiptV2>,
    pub activation: Option<OrionCodeActivationReceiptV2>,
    pub last_successful_check_at: Option<DateTime<Utc>>,
    pub next_eligible_check_at: Option<DateTime<Utc>>,
    pub highest_index_sequence: u64,
    pub cached_index_etag: Option<String>,
    pub failure_count: u32,
    pub last_error_kind: Option<OrionCodeUpdateErrorKind>,
    #[serde(default)]
    pub app_start_sequence: u64,
    #[serde(default)]
    pub successful_activation_app_start_sequence: Option<u64>,
    #[serde(default)]
    pub known_paused_versions: Vec<String>,
    #[serde(default)]
    pub known_revoked_versions: Vec<String>,
}

impl Default for OrionCodeUpdateRecordV2 {
    fn default() -> Self {
        Self {
            schema_version: ORION_CODE_UPDATE_SCHEMA_VERSION,
            choice: OrionCodeBootstrapChoice::NotAsked,
            management_state: OrionCodeUpdateManagementState::Managed,
            current_verified: None,
            previous_verified: None,
            staged: None,
            activation: None,
            last_successful_check_at: None,
            next_eligible_check_at: None,
            highest_index_sequence: 0,
            cached_index_etag: None,
            failure_count: 0,
            last_error_kind: None,
            app_start_sequence: 0,
            successful_activation_app_start_sequence: None,
            known_paused_versions: Vec::new(),
            known_revoked_versions: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum OrionCodeUpdateStateError {
    #[error("invalid Orion Code update state: {0}")]
    Invalid(String),
    #[error("failed to read Orion Code update state: {0}")]
    Read(String),
    #[error("failed to write Orion Code update state: {0}")]
    Write(String),
}

pub fn migrate_orion_code_update_record_v1(
    bootstrap_record: &OrionCodeBootstrapRecord,
    has_custom_configuration: bool,
) -> Result<OrionCodeUpdateRecordV2, OrionCodeUpdateStateError> {
    let mut record = OrionCodeUpdateRecordV2 {
        choice: bootstrap_record.choice,
        management_state: if has_custom_configuration {
            OrionCodeUpdateManagementState::CustomConflict
        } else {
            OrionCodeUpdateManagementState::Managed
        },
        last_error_kind: bootstrap_record
            .last_error_kind
            .map(map_bootstrap_error_kind),
        ..OrionCodeUpdateRecordV2::default()
    };

    if !has_custom_configuration
        && bootstrap_record.choice == OrionCodeBootstrapChoice::Accepted
        && let Some(version) = bootstrap_record.last_verified_version.as_deref()
    {
        Version::parse(version).map_err(|error| {
            OrionCodeUpdateStateError::Invalid(format!(
                "invalid migrated verified version {version}: {error}"
            ))
        })?;
        record.current_verified = Some(OrionCodeInstallReceiptV2 {
            version: version.to_string(),
            source: bootstrap_record
                .source
                .unwrap_or(OrionCodeInstallSource::Npx),
            legacy_imported: true,
            target: None,
            archive_sha256: None,
            command: None,
            installed_at: None,
            index_sequence: None,
        });
    }
    validate_update_record(&record)?;
    Ok(record)
}

pub fn read_orion_code_update_record(
    key_value_store: &KeyValueStore,
) -> Result<Option<OrionCodeUpdateRecordV2>, OrionCodeUpdateStateError> {
    let json = key_value_store
        .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
        .read(ORION_CODE_UPDATE_STATE_KEY)
        .map_err(|error| OrionCodeUpdateStateError::Read(error.to_string()))?;
    let Some(json) = json else {
        return Ok(None);
    };
    let record: OrionCodeUpdateRecordV2 = serde_json::from_str(&json)
        .map_err(|error| OrionCodeUpdateStateError::Read(error.to_string()))?;
    validate_update_record(&record)?;
    Ok(Some(record))
}

pub async fn write_orion_code_update_record(
    key_value_store: &KeyValueStore,
    record: &OrionCodeUpdateRecordV2,
) -> Result<(), OrionCodeUpdateStateError> {
    validate_update_record(record)?;
    let json = serde_json::to_string(record)
        .map_err(|error| OrionCodeUpdateStateError::Write(error.to_string()))?;
    key_value_store
        .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
        .write(ORION_CODE_UPDATE_STATE_KEY.to_string(), json)
        .await
        .map_err(|error| OrionCodeUpdateStateError::Write(error.to_string()))
}

pub fn begin_orion_code_activation(
    record: &OrionCodeUpdateRecordV2,
    started_at: DateTime<Utc>,
    generation: u64,
) -> Result<OrionCodeUpdateRecordV2, OrionCodeUpdateStateError> {
    validate_update_record(record)?;
    if generation == 0 {
        return Err(OrionCodeUpdateStateError::Invalid(
            "activation generation must be non-zero".to_string(),
        ));
    }
    if record.activation.is_some() {
        return Err(OrionCodeUpdateStateError::Invalid(
            "another activation is already durable".to_string(),
        ));
    }
    let staged = record.staged.as_ref().ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("activation requires a staged release".to_string())
    })?;
    if record.known_revoked_versions.contains(&staged.version) {
        return Err(OrionCodeUpdateStateError::Invalid(
            "a revoked release cannot be activated".to_string(),
        ));
    }
    if record.known_paused_versions.contains(&staged.version) {
        return Err(OrionCodeUpdateStateError::Invalid(
            "a paused release cannot be activated".to_string(),
        ));
    }

    let mut updated = record.clone();
    updated.activation = Some(OrionCodeActivationReceiptV2 {
        from_version: record
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.clone()),
        to_version: staged.version.clone(),
        started_at,
        generation,
    });
    validate_update_record(&updated)?;
    Ok(updated)
}

pub fn commit_orion_code_activation(
    record: &OrionCodeUpdateRecordV2,
    installed_at: DateTime<Utc>,
) -> Result<OrionCodeUpdateRecordV2, OrionCodeUpdateStateError> {
    validate_update_record(record)?;
    let activation = record.activation.as_ref().ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("no durable activation is in progress".to_string())
    })?;
    let staged = record.staged.as_ref().ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("activation lost its staged release".to_string())
    })?;
    if activation.to_version != staged.version {
        return Err(OrionCodeUpdateStateError::Invalid(
            "activation and staged release versions differ".to_string(),
        ));
    }

    let current = OrionCodeInstallReceiptV2 {
        version: staged.version.clone(),
        source: OrionCodeInstallSource::Archive,
        legacy_imported: false,
        target: Some(staged.target.clone()),
        archive_sha256: Some(staged.archive_sha256.clone()),
        command: Some(staged.command.clone()),
        installed_at: Some(installed_at),
        index_sequence: Some(staged.index_sequence),
    };
    let mut updated = record.clone();
    if updated
        .current_verified
        .as_ref()
        .is_some_and(|receipt| receipt.version != current.version)
    {
        updated.previous_verified = updated.current_verified.clone();
    }
    updated.current_verified = Some(current);
    updated.staged = None;
    updated.activation = None;
    updated.last_error_kind = None;
    if record.app_start_sequence > 0 {
        updated.successful_activation_app_start_sequence = Some(record.app_start_sequence);
    }
    validate_update_record(&updated)?;
    Ok(updated)
}

pub fn abort_orion_code_activation(
    record: &OrionCodeUpdateRecordV2,
    kind: OrionCodeUpdateErrorKind,
) -> Result<OrionCodeUpdateRecordV2, OrionCodeUpdateStateError> {
    validate_update_record(record)?;
    if record.activation.is_none() {
        return Err(OrionCodeUpdateStateError::Invalid(
            "no durable activation is in progress".to_string(),
        ));
    }
    let mut updated = record.clone();
    updated.staged = None;
    updated.activation = None;
    updated.last_error_kind = Some(kind);
    validate_update_record(&updated)?;
    Ok(updated)
}

pub fn stage_previous_orion_code_release(
    record: &OrionCodeUpdateRecordV2,
    staged_at: DateTime<Utc>,
) -> Result<OrionCodeUpdateRecordV2, OrionCodeUpdateStateError> {
    validate_update_record(record)?;
    if record.activation.is_some() {
        return Err(OrionCodeUpdateStateError::Invalid(
            "cannot prepare rollback during activation".to_string(),
        ));
    }
    let previous = record.previous_verified.as_ref().ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("no previous verified release exists".to_string())
    })?;
    if previous.source != OrionCodeInstallSource::Archive || previous.legacy_imported {
        return Err(OrionCodeUpdateStateError::Invalid(
            "the previous release is not a managed archive".to_string(),
        ));
    }
    if record.known_revoked_versions.contains(&previous.version) {
        return Err(OrionCodeUpdateStateError::Invalid(
            "the previous release has been revoked".to_string(),
        ));
    }
    if record.known_paused_versions.contains(&previous.version) {
        return Err(OrionCodeUpdateStateError::Invalid(
            "the previous release has been paused".to_string(),
        ));
    }
    let target = previous.target.clone().ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("previous release target is missing".to_string())
    })?;
    let archive_sha256 = previous.archive_sha256.clone().ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("previous release digest is missing".to_string())
    })?;
    let command = previous.command.clone().ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("previous release command is missing".to_string())
    })?;
    let index_sequence = previous.index_sequence.ok_or_else(|| {
        OrionCodeUpdateStateError::Invalid("previous release sequence is missing".to_string())
    })?;

    let mut updated = record.clone();
    updated.staged = Some(OrionCodeStagedReceiptV2 {
        version: previous.version.clone(),
        target,
        archive_sha256,
        command,
        staged_at,
        index_sequence,
    });
    validate_update_record(&updated)?;
    Ok(updated)
}

pub fn preferred_orion_code_runtime_receipt(
    record: &OrionCodeUpdateRecordV2,
) -> Option<&OrionCodeInstallReceiptV2> {
    record
        .current_verified
        .as_ref()
        .filter(|receipt| !record.known_revoked_versions.contains(&receipt.version))
        .or_else(|| {
            record.previous_verified.as_ref().filter(|receipt| {
                !record.known_revoked_versions.contains(&receipt.version)
                    && !record.known_paused_versions.contains(&receipt.version)
            })
        })
}

pub fn current_orion_code_release_is_revoked(record: &OrionCodeUpdateRecordV2) -> bool {
    record
        .current_verified
        .as_ref()
        .is_some_and(|receipt| record.known_revoked_versions.contains(&receipt.version))
}

pub fn read_orion_code_cached_index(
    key_value_store: &KeyValueStore,
) -> Result<Option<OrionCodeCachedIndex>, OrionCodeUpdateStateError> {
    let json = key_value_store
        .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
        .read(ORION_CODE_UPDATE_INDEX_CACHE_KEY)
        .map_err(|error| OrionCodeUpdateStateError::Read(error.to_string()))?;
    let Some(json) = json else {
        return Ok(None);
    };
    let record: OrionCodeCachedIndexRecordV1 = serde_json::from_str(&json)
        .map_err(|error| OrionCodeUpdateStateError::Read(error.to_string()))?;
    if record.schema_version != ORION_CODE_UPDATE_INDEX_CACHE_SCHEMA_VERSION || record.sequence == 0
    {
        return Err(OrionCodeUpdateStateError::Invalid(
            "cached index has an unsupported schema or zero sequence".to_string(),
        ));
    }
    let index_bytes =
        decode_bounded_cache_bytes(&record.index_base64, MAX_INDEX_BYTES, "cached update index")?;
    let signature_envelope_bytes = decode_bounded_cache_bytes(
        &record.signature_envelope_base64,
        MAX_SIGNATURE_ENVELOPE_BYTES,
        "cached signature envelope",
    )?;
    Ok(Some(OrionCodeCachedIndex {
        sequence: record.sequence,
        index_bytes,
        signature_envelope_bytes,
    }))
}

pub async fn write_orion_code_cached_index(
    key_value_store: &KeyValueStore,
    verified_index: &VerifiedOrionCodeUpdateIndex,
    index_bytes: &[u8],
    signature_envelope_bytes: &[u8],
) -> Result<(), OrionCodeUpdateStateError> {
    if index_bytes.len() > MAX_INDEX_BYTES
        || signature_envelope_bytes.len() > MAX_SIGNATURE_ENVELOPE_BYTES
    {
        return Err(OrionCodeUpdateStateError::Invalid(
            "verified index cache exceeds its byte limits".to_string(),
        ));
    }
    let parsed_index: OrionCodeUpdateIndexV1 = serde_json::from_slice(index_bytes)
        .map_err(|error| OrionCodeUpdateStateError::Invalid(error.to_string()))?;
    if parsed_index != verified_index.index || parsed_index.sequence == 0 {
        return Err(OrionCodeUpdateStateError::Invalid(
            "verified index metadata does not match the cached exact bytes".to_string(),
        ));
    }
    let record = OrionCodeCachedIndexRecordV1 {
        schema_version: ORION_CODE_UPDATE_INDEX_CACHE_SCHEMA_VERSION,
        sequence: parsed_index.sequence,
        index_base64: BASE64_STANDARD.encode(index_bytes),
        signature_envelope_base64: BASE64_STANDARD.encode(signature_envelope_bytes),
    };
    let json = serde_json::to_string(&record)
        .map_err(|error| OrionCodeUpdateStateError::Write(error.to_string()))?;
    key_value_store
        .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
        .write(ORION_CODE_UPDATE_INDEX_CACHE_KEY.to_string(), json)
        .await
        .map_err(|error| OrionCodeUpdateStateError::Write(error.to_string()))
}

fn decode_bounded_cache_bytes(
    encoded: &str,
    byte_limit: usize,
    label: &str,
) -> Result<Vec<u8>, OrionCodeUpdateStateError> {
    let encoded_limit = byte_limit
        .saturating_mul(4)
        .saturating_div(3)
        .saturating_add(8);
    if encoded.len() > encoded_limit {
        return Err(OrionCodeUpdateStateError::Invalid(format!(
            "{label} exceeds its encoded byte limit"
        )));
    }
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|error| OrionCodeUpdateStateError::Invalid(error.to_string()))?;
    if bytes.len() > byte_limit {
        return Err(OrionCodeUpdateStateError::Invalid(format!(
            "{label} exceeds its decoded byte limit"
        )));
    }
    Ok(bytes)
}

pub fn validate_update_record(
    record: &OrionCodeUpdateRecordV2,
) -> Result<(), OrionCodeUpdateStateError> {
    if record.schema_version != ORION_CODE_UPDATE_SCHEMA_VERSION {
        return Err(OrionCodeUpdateStateError::Invalid(format!(
            "unsupported schema version {}",
            record.schema_version
        )));
    }
    for receipt in [
        record.current_verified.as_ref(),
        record.previous_verified.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        Version::parse(&receipt.version).map_err(|error| {
            OrionCodeUpdateStateError::Invalid(format!(
                "invalid installed version {}: {error}",
                receipt.version
            ))
        })?;
        if receipt.legacy_imported {
            if receipt.index_sequence.is_some()
                || receipt.installed_at.is_some()
                || receipt.target.is_some()
                || receipt.archive_sha256.is_some()
                || receipt.command.is_some()
            {
                return Err(OrionCodeUpdateStateError::Invalid(
                    "legacy receipt must not claim v2 archive evidence".to_string(),
                ));
            }
        } else if receipt.source == OrionCodeInstallSource::Archive {
            let target = receipt.target.as_deref().ok_or_else(|| {
                OrionCodeUpdateStateError::Invalid(
                    "archive receipt is missing its target".to_string(),
                )
            })?;
            validate_target_name(target).map_err(OrionCodeUpdateStateError::Invalid)?;
            let digest = receipt.archive_sha256.as_deref().ok_or_else(|| {
                OrionCodeUpdateStateError::Invalid(
                    "archive receipt is missing its SHA-256".to_string(),
                )
            })?;
            validate_sha256(digest).map_err(OrionCodeUpdateStateError::Invalid)?;
            let command = receipt.command.as_deref().ok_or_else(|| {
                OrionCodeUpdateStateError::Invalid(
                    "archive receipt is missing its command".to_string(),
                )
            })?;
            validate_relative_artifact_path(command).map_err(OrionCodeUpdateStateError::Invalid)?;
        }
        if let Some(index_sequence) = receipt.index_sequence
            && (index_sequence == 0 || index_sequence > record.highest_index_sequence)
        {
            return Err(OrionCodeUpdateStateError::Invalid(
                "installed receipt has an invalid index sequence".to_string(),
            ));
        }
    }
    if record
        .current_verified
        .as_ref()
        .zip(record.previous_verified.as_ref())
        .is_some_and(|(current, previous)| current.version == previous.version)
    {
        return Err(OrionCodeUpdateStateError::Invalid(
            "current and previous receipts must reference different versions".to_string(),
        ));
    }
    if let Some(staged) = &record.staged {
        Version::parse(&staged.version).map_err(|error| {
            OrionCodeUpdateStateError::Invalid(format!(
                "invalid staged version {}: {error}",
                staged.version
            ))
        })?;
        validate_target_name(&staged.target).map_err(OrionCodeUpdateStateError::Invalid)?;
        validate_sha256(&staged.archive_sha256).map_err(OrionCodeUpdateStateError::Invalid)?;
        validate_relative_artifact_path(&staged.command)
            .map_err(OrionCodeUpdateStateError::Invalid)?;
        if staged.index_sequence == 0 || staged.index_sequence > record.highest_index_sequence {
            return Err(OrionCodeUpdateStateError::Invalid(
                "staged receipt has an invalid index sequence".to_string(),
            ));
        }
    }
    if let Some(activation) = &record.activation {
        Version::parse(&activation.to_version).map_err(|error| {
            OrionCodeUpdateStateError::Invalid(format!(
                "invalid activating version {}: {error}",
                activation.to_version
            ))
        })?;
        if activation.generation == 0 {
            return Err(OrionCodeUpdateStateError::Invalid(
                "activation generation must be non-zero".to_string(),
            ));
        }
        if record
            .staged
            .as_ref()
            .is_none_or(|staged| staged.version != activation.to_version)
        {
            return Err(OrionCodeUpdateStateError::Invalid(
                "activation receipt must reference the staged version".to_string(),
            ));
        }
    }
    if record
        .cached_index_etag
        .as_ref()
        .is_some_and(|etag| etag.len() > 512 || etag.chars().any(char::is_control))
    {
        return Err(OrionCodeUpdateStateError::Invalid(
            "cached index ETag is invalid".to_string(),
        ));
    }
    if record
        .successful_activation_app_start_sequence
        .is_some_and(|sequence| sequence == 0 || sequence > record.app_start_sequence)
    {
        return Err(OrionCodeUpdateStateError::Invalid(
            "successful activation App start sequence is invalid".to_string(),
        ));
    }
    let paused = validate_sorted_control_versions(&record.known_paused_versions, "paused")?;
    let revoked = validate_sorted_control_versions(&record.known_revoked_versions, "revoked")?;
    if paused.iter().any(|version| revoked.contains(version)) {
        return Err(OrionCodeUpdateStateError::Invalid(
            "a release cannot be both paused and revoked".to_string(),
        ));
    }
    Ok(())
}

fn validate_sorted_control_versions<'a>(
    versions: &'a [String],
    status: &str,
) -> Result<HashSet<&'a String>, OrionCodeUpdateStateError> {
    let mut seen = HashSet::new();
    let mut previous: Option<&str> = None;
    for version in versions {
        let parsed = Version::parse(version).map_err(|error| {
            OrionCodeUpdateStateError::Invalid(format!(
                "invalid {status} version {version}: {error}"
            ))
        })?;
        if parsed.to_string() != *version {
            return Err(OrionCodeUpdateStateError::Invalid(format!(
                "{status} version {version} is not in canonical semantic-version form"
            )));
        }
        if previous.is_some_and(|previous| previous >= version.as_str()) {
            return Err(OrionCodeUpdateStateError::Invalid(format!(
                "{status} versions must be sorted and unique"
            )));
        }
        if !seen.insert(version) {
            return Err(OrionCodeUpdateStateError::Invalid(format!(
                "duplicate {status} version {version}"
            )));
        }
        previous = Some(version);
    }
    Ok(seen)
}

fn map_bootstrap_error_kind(kind: OrionCodeBootstrapErrorKind) -> OrionCodeUpdateErrorKind {
    match kind {
        OrionCodeBootstrapErrorKind::RegistryUnreachable => OrionCodeUpdateErrorKind::Network,
        OrionCodeBootstrapErrorKind::UnsupportedPlatform => OrionCodeUpdateErrorKind::Incompatible,
        OrionCodeBootstrapErrorKind::NpmInstall | OrionCodeBootstrapErrorKind::Download => {
            OrionCodeUpdateErrorKind::Download
        }
        OrionCodeBootstrapErrorKind::Checksum => OrionCodeUpdateErrorKind::Checksum,
        OrionCodeBootstrapErrorKind::Extract => OrionCodeUpdateErrorKind::Extract,
        OrionCodeBootstrapErrorKind::MissingCommand
        | OrionCodeBootstrapErrorKind::Configuration => OrionCodeUpdateErrorKind::Configuration,
        OrionCodeBootstrapErrorKind::Spawn
        | OrionCodeBootstrapErrorKind::Initialize
        | OrionCodeBootstrapErrorKind::RuntimeExit => OrionCodeUpdateErrorKind::Preflight,
        OrionCodeBootstrapErrorKind::Persistence => OrionCodeUpdateErrorKind::Persistence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc)
    }

    fn archive_receipt(version: &str, index_sequence: u64) -> OrionCodeInstallReceiptV2 {
        OrionCodeInstallReceiptV2 {
            version: version.to_string(),
            source: OrionCodeInstallSource::Archive,
            legacy_imported: false,
            target: Some("darwin-aarch64".to_string()),
            archive_sha256: Some(format!("{:064x}", index_sequence)),
            command: Some("OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string()),
            installed_at: Some(now()),
            index_sequence: Some(index_sequence),
        }
    }

    #[test]
    fn migrates_v1_without_reenabling_declined_removed_or_custom_state() {
        let accepted = OrionCodeBootstrapRecord {
            choice: OrionCodeBootstrapChoice::Accepted,
            source: Some(OrionCodeInstallSource::Npx),
            last_verified_version: Some("0.3.2".to_string()),
            ..OrionCodeBootstrapRecord::default()
        };
        let migrated =
            migrate_orion_code_update_record_v1(&accepted, false).expect("migrate accepted state");
        let receipt = migrated.current_verified.expect("import verified version");
        assert_eq!(receipt.version, "0.3.2");
        assert!(receipt.legacy_imported);

        for choice in [
            OrionCodeBootstrapChoice::Declined,
            OrionCodeBootstrapChoice::Removed,
        ] {
            let record = OrionCodeBootstrapRecord {
                choice,
                ..OrionCodeBootstrapRecord::default()
            };
            let migrated = migrate_orion_code_update_record_v1(&record, false)
                .expect("migrate disabled state");
            assert_eq!(migrated.choice, choice);
            assert!(migrated.current_verified.is_none());
        }

        let custom = migrate_orion_code_update_record_v1(&accepted, true)
            .expect("record custom conflict without takeover");
        assert_eq!(
            custom.management_state,
            OrionCodeUpdateManagementState::CustomConflict
        );
        assert!(custom.current_verified.is_none());
    }

    #[test]
    fn migration_rejects_corrupt_v1_without_writing_v2() {
        let corrupt = OrionCodeBootstrapRecord {
            choice: OrionCodeBootstrapChoice::Accepted,
            last_verified_version: Some("latest".to_string()),
            ..OrionCodeBootstrapRecord::default()
        };
        assert!(migrate_orion_code_update_record_v1(&corrupt, false).is_err());
    }

    #[gpui::test]
    async fn state_v2_round_trips_without_removing_v1() {
        let key_value_store = KeyValueStore::open_test_db("orion_code_update_v2_round_trip").await;
        let record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            highest_index_sequence: 42,
            current_verified: Some(OrionCodeInstallReceiptV2 {
                version: "0.4.0".to_string(),
                source: OrionCodeInstallSource::Archive,
                legacy_imported: false,
                target: Some("darwin-aarch64".to_string()),
                archive_sha256: Some("a".repeat(64)),
                command: Some("OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string()),
                installed_at: Some(now()),
                index_sequence: Some(42),
            }),
            ..OrionCodeUpdateRecordV2::default()
        };
        key_value_store
            .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
            .write("state-v1".to_string(), "preserve-me".to_string())
            .await
            .expect("write v1 sentinel");
        write_orion_code_update_record(&key_value_store, &record)
            .await
            .expect("write state v2");
        assert_eq!(
            read_orion_code_update_record(&key_value_store)
                .expect("read state v2")
                .expect("state v2 exists"),
            record
        );
        assert_eq!(
            key_value_store
                .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
                .read("state-v1")
                .expect("read v1 sentinel")
                .as_deref(),
            Some("preserve-me")
        );

        let index_bytes = br#"{
            "schema_version": 1,
            "sequence": 42,
            "generated_at": "2026-09-01T00:00:00Z",
            "expires_at": "2026-09-02T00:00:00Z",
            "releases": []
        }"#;
        let index: OrionCodeUpdateIndexV1 =
            serde_json::from_slice(index_bytes).expect("parse cached index");
        let verified = VerifiedOrionCodeUpdateIndex {
            index,
            key_id: "test-key".to_string(),
            payload_sha256: "a".repeat(64),
        };
        write_orion_code_cached_index(&key_value_store, &verified, index_bytes, b"signature")
            .await
            .expect("write exact cached bytes");
        assert_eq!(
            read_orion_code_cached_index(&key_value_store)
                .expect("read cached index")
                .expect("cache exists"),
            OrionCodeCachedIndex {
                sequence: 42,
                index_bytes: index_bytes.to_vec(),
                signature_envelope_bytes: b"signature".to_vec(),
            }
        );
    }

    #[test]
    fn activation_commit_is_atomic_and_preserves_one_previous_release() {
        let record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            highest_index_sequence: 43,
            app_start_sequence: 7,
            current_verified: Some(archive_receipt("0.4.0", 42)),
            staged: Some(OrionCodeStagedReceiptV2 {
                version: "0.5.0".to_string(),
                target: "darwin-aarch64".to_string(),
                archive_sha256: "b".repeat(64),
                command: "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string(),
                staged_at: now(),
                index_sequence: 43,
            }),
            ..OrionCodeUpdateRecordV2::default()
        };

        let activating =
            begin_orion_code_activation(&record, now(), 7).expect("begin durable activation");
        assert_eq!(
            activating
                .activation
                .as_ref()
                .and_then(|activation| activation.from_version.as_deref()),
            Some("0.4.0")
        );
        let committed =
            commit_orion_code_activation(&activating, now()).expect("commit activation");
        assert_eq!(
            committed
                .current_verified
                .as_ref()
                .map(|receipt| receipt.version.as_str()),
            Some("0.5.0")
        );
        assert_eq!(
            committed
                .previous_verified
                .as_ref()
                .map(|receipt| receipt.version.as_str()),
            Some("0.4.0")
        );
        assert!(committed.staged.is_none());
        assert!(committed.activation.is_none());
        assert_eq!(committed.successful_activation_app_start_sequence, Some(7));
    }

    #[test]
    fn failed_activation_clears_candidate_without_changing_current() {
        let record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            highest_index_sequence: 43,
            current_verified: Some(archive_receipt("0.4.0", 42)),
            staged: Some(OrionCodeStagedReceiptV2 {
                version: "0.5.0".to_string(),
                target: "darwin-aarch64".to_string(),
                archive_sha256: "b".repeat(64),
                command: "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string(),
                staged_at: now(),
                index_sequence: 43,
            }),
            ..OrionCodeUpdateRecordV2::default()
        };
        let activating =
            begin_orion_code_activation(&record, now(), 8).expect("begin durable activation");
        let aborted =
            abort_orion_code_activation(&activating, OrionCodeUpdateErrorKind::Activation)
                .expect("abort activation");
        assert_eq!(aborted.current_verified, record.current_verified);
        assert!(aborted.staged.is_none());
        assert!(aborted.activation.is_none());
        assert_eq!(
            aborted.last_error_kind,
            Some(OrionCodeUpdateErrorKind::Activation)
        );
    }

    #[test]
    fn a_staged_release_cannot_activate_after_it_is_paused() {
        let record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            highest_index_sequence: 43,
            current_verified: Some(archive_receipt("0.4.0", 42)),
            staged: Some(OrionCodeStagedReceiptV2 {
                version: "0.5.0".to_string(),
                target: "darwin-aarch64".to_string(),
                archive_sha256: "b".repeat(64),
                command: "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string(),
                staged_at: now(),
                index_sequence: 43,
            }),
            known_paused_versions: vec!["0.5.0".to_string()],
            ..OrionCodeUpdateRecordV2::default()
        };

        let error = begin_orion_code_activation(&record, now(), 9)
            .expect_err("paused candidate must remain staged and inactive");
        assert!(error.to_string().contains("paused release"));
        assert_eq!(
            record.staged.as_ref().map(|staged| staged.version.as_str()),
            Some("0.5.0")
        );
    }

    #[test]
    fn signed_control_versions_must_be_canonical_sorted_and_disjoint() {
        let unsorted = OrionCodeUpdateRecordV2 {
            known_paused_versions: vec!["0.5.0".to_string(), "0.4.0".to_string()],
            ..OrionCodeUpdateRecordV2::default()
        };
        assert!(validate_update_record(&unsorted).is_err());

        let overlapping = OrionCodeUpdateRecordV2 {
            known_paused_versions: vec!["0.5.0".to_string()],
            known_revoked_versions: vec!["0.5.0".to_string()],
            ..OrionCodeUpdateRecordV2::default()
        };
        assert!(validate_update_record(&overlapping).is_err());
    }

    #[test]
    fn previous_release_can_be_restaged_but_revoked_versions_never_win_startup_selection() {
        let record = OrionCodeUpdateRecordV2 {
            choice: OrionCodeBootstrapChoice::Accepted,
            highest_index_sequence: 43,
            current_verified: Some(archive_receipt("0.5.0", 43)),
            previous_verified: Some(archive_receipt("0.4.0", 42)),
            known_revoked_versions: vec!["0.5.0".to_string()],
            ..OrionCodeUpdateRecordV2::default()
        };
        assert_eq!(
            preferred_orion_code_runtime_receipt(&record).map(|receipt| receipt.version.as_str()),
            Some("0.4.0")
        );

        let staged =
            stage_previous_orion_code_release(&record, now()).expect("stage safe previous");
        let activating =
            begin_orion_code_activation(&staged, now(), 9).expect("begin rollback activation");
        let rolled_back =
            commit_orion_code_activation(&activating, now()).expect("commit rollback");
        assert_eq!(
            rolled_back
                .current_verified
                .as_ref()
                .map(|receipt| receipt.version.as_str()),
            Some("0.4.0")
        );
        assert_eq!(
            rolled_back
                .previous_verified
                .as_ref()
                .map(|receipt| receipt.version.as_str()),
            Some("0.5.0")
        );
    }
}
