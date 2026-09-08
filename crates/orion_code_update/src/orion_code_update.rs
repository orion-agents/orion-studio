use std::collections::{BTreeMap, HashSet};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use url::Url;

mod archive;

pub use archive::{
    OrionCodeArchiveLimits, OrionCodeArchiveVerificationError, VerifiedOrionCodeArtifact,
    extract_orion_code_zip, verify_orion_code_archive, verify_orion_code_artifact,
};

pub const UPDATE_INDEX_SCHEMA_VERSION: u32 = 1;
pub const SIGNATURE_ENVELOPE_SCHEMA_VERSION: u32 = 1;
pub const ARTIFACT_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const MAX_INDEX_BYTES: usize = 1024 * 1024;
pub const MAX_SIGNATURE_ENVELOPE_BYTES: usize = 4096;
pub const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const SUPPORTED_ACP_PROTOCOL: u32 = 1;

const MAX_CLOCK_SKEW: Duration = Duration::minutes(5);
const ED25519_SIGNATURE_BYTES: usize = 64;
pub const ED25519_PUBLIC_KEY_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeUpdateChannel {
    #[default]
    Stable,
    Beta,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeUpdateMode {
    #[default]
    Automatic,
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeInstallSource {
    Npx,
    Archive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeReleaseStatus {
    Active,
    Paused,
    Revoked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeArchiveFormat {
    Zip,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeSigningRequirement {
    DeveloperIdAndNotarized,
    Authenticode,
    DigestOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeUpdateTargetV1 {
    pub archive_url: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub format: OrionCodeArchiveFormat,
    pub command: String,
    pub manifest_sha256: String,
    pub sbom_sha256: String,
    pub signing_requirement: OrionCodeSigningRequirement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeUpdateReleaseV1 {
    pub version: String,
    pub channel: OrionCodeUpdateChannel,
    pub status: OrionCodeReleaseStatus,
    pub published_at: DateTime<Utc>,
    pub studio_version_requirement: String,
    pub acp_protocol: u32,
    pub rollout_basis_points: u16,
    pub rollout_salt: String,
    pub rollback_to: Option<String>,
    pub release_notes_url: String,
    pub targets: BTreeMap<String, OrionCodeUpdateTargetV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeUpdateIndexV1 {
    pub schema_version: u32,
    pub sequence: u64,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub releases: Vec<OrionCodeUpdateReleaseV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeUpdateSignatureEnvelopeV1 {
    pub schema_version: u32,
    pub algorithm: String,
    pub key_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeArtifactFileV1 {
    pub path: String,
    pub mode: u32,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeArtifactManifestV1 {
    pub schema_version: u32,
    pub version: String,
    pub git_sha: String,
    pub target: String,
    pub built_at: DateTime<Utc>,
    pub acp_protocol: u32,
    pub studio_version_requirement: String,
    pub node_version: String,
    pub node_abi: String,
    pub native_modules: Vec<String>,
    pub command: String,
    pub sbom_path: String,
    pub sbom_sha256: String,
    pub notices_path: String,
    pub notices_sha256: String,
    pub files: Vec<OrionCodeArtifactFileV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrionCodeInstallReceiptV2 {
    pub version: String,
    pub source: OrionCodeInstallSource,
    pub legacy_imported: bool,
    pub target: Option<String>,
    pub archive_sha256: Option<String>,
    pub command: Option<String>,
    pub installed_at: Option<DateTime<Utc>>,
    pub index_sequence: Option<u64>,
}

#[derive(Debug, Error)]
pub enum OrionCodeUpdateContractError {
    #[error("Orion Code update index exceeded the {limit} byte limit")]
    IndexTooLarge { limit: usize },
    #[error("Orion Code signature envelope exceeded the {limit} byte limit")]
    SignatureEnvelopeTooLarge { limit: usize },
    #[error("invalid Orion Code signature envelope: {0}")]
    InvalidSignatureEnvelope(String),
    #[error("unsupported Orion Code signature envelope schema {0}")]
    UnsupportedSignatureEnvelopeSchema(u32),
    #[error("unsupported Orion Code update signature algorithm {0}")]
    UnsupportedSignatureAlgorithm(String),
    #[error("unknown Orion Code update signing key {0}")]
    UnknownSigningKey(String),
    #[error("invalid Orion Code update signing public key {0}")]
    InvalidSigningKey(String),
    #[error("Orion Code update index signature verification failed")]
    InvalidSignature,
    #[error("invalid Orion Code update index JSON: {0}")]
    InvalidIndexJson(String),
    #[error("unsupported Orion Code update index schema {0}")]
    UnsupportedIndexSchema(u32),
    #[error("Orion Code update index sequence must be greater than zero")]
    ZeroSequence,
    #[error("Orion Code update index sequence {received} replays accepted sequence {highest}")]
    SequenceReplay { received: u64, highest: u64 },
    #[error(
        "cached Orion Code update index sequence {received} does not match accepted sequence {expected}"
    )]
    CachedSequenceMismatch { received: u64, expected: u64 },
    #[error("Orion Code update index was generated too far in the future")]
    GeneratedInFuture,
    #[error("Orion Code update index has expired")]
    ExpiredIndex,
    #[error("Orion Code update index expiration must be after generation")]
    InvalidIndexLifetime,
    #[error("invalid Orion Code release {version}: {reason}")]
    InvalidRelease { version: String, reason: String },
}

#[derive(Clone, Debug)]
pub struct VerifiedOrionCodeUpdateIndex {
    pub index: OrionCodeUpdateIndexV1,
    pub key_id: String,
    pub payload_sha256: String,
}

pub struct OrionCodeIndexVerifier {
    trusted_keys: BTreeMap<String, VerifyingKey>,
    allowed_archive_hosts: HashSet<String>,
}

impl OrionCodeIndexVerifier {
    pub fn new(
        trusted_keys: impl IntoIterator<Item = (String, [u8; ED25519_PUBLIC_KEY_BYTES])>,
        allowed_archive_hosts: impl IntoIterator<Item = String>,
    ) -> Result<Self, OrionCodeUpdateContractError> {
        let mut parsed_keys = BTreeMap::new();
        for (key_id, bytes) in trusted_keys {
            if key_id.trim().is_empty() || key_id.len() > 128 {
                return Err(OrionCodeUpdateContractError::InvalidSigningKey(key_id));
            }
            let key = VerifyingKey::from_bytes(&bytes)
                .map_err(|_| OrionCodeUpdateContractError::InvalidSigningKey(key_id.clone()))?;
            if key.is_weak() || parsed_keys.insert(key_id.clone(), key).is_some() {
                return Err(OrionCodeUpdateContractError::InvalidSigningKey(key_id));
            }
        }
        let allowed_archive_hosts = allowed_archive_hosts
            .into_iter()
            .map(|host| host.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        if parsed_keys.is_empty() {
            return Err(OrionCodeUpdateContractError::InvalidSigningKey(
                "no trusted keys configured".to_string(),
            ));
        }
        if allowed_archive_hosts.is_empty() {
            return Err(OrionCodeUpdateContractError::InvalidSigningKey(
                "no trusted archive hosts configured".to_string(),
            ));
        }
        Ok(Self {
            trusted_keys: parsed_keys,
            allowed_archive_hosts,
        })
    }

    pub fn verify(
        &self,
        index_bytes: &[u8],
        signature_envelope_bytes: &[u8],
        now: DateTime<Utc>,
        highest_accepted_sequence: u64,
    ) -> Result<VerifiedOrionCodeUpdateIndex, OrionCodeUpdateContractError> {
        let verified = self.authenticate_and_parse(index_bytes, signature_envelope_bytes)?;
        self.validate_index(
            &verified.index,
            now,
            SequenceValidation::NewerThan(highest_accepted_sequence),
        )?;
        Ok(verified)
    }

    pub fn verify_cached(
        &self,
        index_bytes: &[u8],
        signature_envelope_bytes: &[u8],
        now: DateTime<Utc>,
        expected_sequence: u64,
    ) -> Result<VerifiedOrionCodeUpdateIndex, OrionCodeUpdateContractError> {
        let verified = self.authenticate_and_parse(index_bytes, signature_envelope_bytes)?;
        self.validate_index(
            &verified.index,
            now,
            SequenceValidation::Exactly(expected_sequence),
        )?;
        Ok(verified)
    }

    fn authenticate_and_parse(
        &self,
        index_bytes: &[u8],
        signature_envelope_bytes: &[u8],
    ) -> Result<VerifiedOrionCodeUpdateIndex, OrionCodeUpdateContractError> {
        if index_bytes.len() > MAX_INDEX_BYTES {
            return Err(OrionCodeUpdateContractError::IndexTooLarge {
                limit: MAX_INDEX_BYTES,
            });
        }
        if signature_envelope_bytes.len() > MAX_SIGNATURE_ENVELOPE_BYTES {
            return Err(OrionCodeUpdateContractError::SignatureEnvelopeTooLarge {
                limit: MAX_SIGNATURE_ENVELOPE_BYTES,
            });
        }

        let envelope: OrionCodeUpdateSignatureEnvelopeV1 =
            serde_json::from_slice(signature_envelope_bytes).map_err(|error| {
                OrionCodeUpdateContractError::InvalidSignatureEnvelope(error.to_string())
            })?;
        if envelope.schema_version != SIGNATURE_ENVELOPE_SCHEMA_VERSION {
            return Err(
                OrionCodeUpdateContractError::UnsupportedSignatureEnvelopeSchema(
                    envelope.schema_version,
                ),
            );
        }
        if envelope.algorithm != "ed25519" {
            return Err(OrionCodeUpdateContractError::UnsupportedSignatureAlgorithm(
                envelope.algorithm,
            ));
        }
        let verifying_key = self.trusted_keys.get(&envelope.key_id).ok_or_else(|| {
            OrionCodeUpdateContractError::UnknownSigningKey(envelope.key_id.clone())
        })?;
        let signature_bytes = BASE64_STANDARD
            .decode(envelope.signature.as_bytes())
            .map_err(|_| {
                OrionCodeUpdateContractError::InvalidSignatureEnvelope(
                    "signature is not valid base64".to_string(),
                )
            })?;
        let signature_bytes: [u8; ED25519_SIGNATURE_BYTES] =
            signature_bytes.try_into().map_err(|_| {
                OrionCodeUpdateContractError::InvalidSignatureEnvelope(
                    "Ed25519 signature must contain 64 bytes".to_string(),
                )
            })?;
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify_strict(index_bytes, &signature)
            .map_err(|_| OrionCodeUpdateContractError::InvalidSignature)?;

        // No payload field can influence key selection or product state before
        // the exact bytes have passed strict Ed25519 verification.
        let index: OrionCodeUpdateIndexV1 = serde_json::from_slice(index_bytes)
            .map_err(|error| OrionCodeUpdateContractError::InvalidIndexJson(error.to_string()))?;
        Ok(VerifiedOrionCodeUpdateIndex {
            index,
            key_id: envelope.key_id,
            payload_sha256: lowercase_sha256(index_bytes),
        })
    }

    fn validate_index(
        &self,
        index: &OrionCodeUpdateIndexV1,
        now: DateTime<Utc>,
        sequence_validation: SequenceValidation,
    ) -> Result<(), OrionCodeUpdateContractError> {
        if index.schema_version != UPDATE_INDEX_SCHEMA_VERSION {
            return Err(OrionCodeUpdateContractError::UnsupportedIndexSchema(
                index.schema_version,
            ));
        }
        if index.sequence == 0 {
            return Err(OrionCodeUpdateContractError::ZeroSequence);
        }
        match sequence_validation {
            SequenceValidation::NewerThan(highest_accepted_sequence)
                if index.sequence <= highest_accepted_sequence =>
            {
                return Err(OrionCodeUpdateContractError::SequenceReplay {
                    received: index.sequence,
                    highest: highest_accepted_sequence,
                });
            }
            SequenceValidation::Exactly(expected_sequence)
                if index.sequence != expected_sequence =>
            {
                return Err(OrionCodeUpdateContractError::CachedSequenceMismatch {
                    received: index.sequence,
                    expected: expected_sequence,
                });
            }
            SequenceValidation::NewerThan(_) | SequenceValidation::Exactly(_) => {}
        }
        if index.generated_at > now + MAX_CLOCK_SKEW {
            return Err(OrionCodeUpdateContractError::GeneratedInFuture);
        }
        if index.expires_at <= index.generated_at {
            return Err(OrionCodeUpdateContractError::InvalidIndexLifetime);
        }
        if index.expires_at <= now {
            return Err(OrionCodeUpdateContractError::ExpiredIndex);
        }
        let mut versions = HashSet::new();
        for release in &index.releases {
            if !versions.insert(release.version.clone()) {
                return Err(invalid_release(release, "duplicate version in index"));
            }
            self.validate_release(release)?;
        }
        Ok(())
    }

    fn validate_release(
        &self,
        release: &OrionCodeUpdateReleaseV1,
    ) -> Result<(), OrionCodeUpdateContractError> {
        let version = Version::parse(&release.version)
            .map_err(|error| invalid_release(release, format!("invalid exact semver: {error}")))?;
        if release.channel == OrionCodeUpdateChannel::Stable && !version.pre.is_empty() {
            return Err(invalid_release(
                release,
                "Stable releases cannot use prerelease semver",
            ));
        }
        VersionReq::parse(&release.studio_version_requirement).map_err(|error| {
            invalid_release(
                release,
                format!("invalid Studio version requirement: {error}"),
            )
        })?;
        if release.acp_protocol == 0 {
            return Err(invalid_release(release, "ACP protocol must be non-zero"));
        }
        if release.rollout_basis_points > 10_000 {
            return Err(invalid_release(
                release,
                "rollout_basis_points must be in 0..=10000",
            ));
        }
        if release.rollout_salt.is_empty() || release.rollout_salt.len() > 128 {
            return Err(invalid_release(
                release,
                "rollout_salt must contain 1..=128 bytes",
            ));
        }
        if let Some(rollback_to) = &release.rollback_to {
            let rollback_version = Version::parse(rollback_to).map_err(|error| {
                invalid_release(release, format!("invalid rollback_to semver: {error}"))
            })?;
            if rollback_version >= version {
                return Err(invalid_release(
                    release,
                    "rollback_to must be lower than the source version",
                ));
            }
            if release.status == OrionCodeReleaseStatus::Active {
                return Err(invalid_release(
                    release,
                    "rollback_to requires a paused or revoked source release",
                ));
            }
        }
        validate_https_url(&release.release_notes_url, None)
            .map_err(|reason| invalid_release(release, reason))?;
        if release.status == OrionCodeReleaseStatus::Active && release.targets.is_empty() {
            return Err(invalid_release(
                release,
                "active release must contain at least one target",
            ));
        }

        for (target_name, target) in &release.targets {
            validate_target_name(target_name).map_err(|reason| invalid_release(release, reason))?;
            validate_https_url(&target.archive_url, Some(&self.allowed_archive_hosts))
                .map_err(|reason| invalid_release(release, reason))?;
            if target.archive_bytes == 0 || target.archive_bytes > MAX_ARCHIVE_BYTES {
                return Err(invalid_release(
                    release,
                    format!(
                        "target {target_name} archive_bytes must be in 1..={MAX_ARCHIVE_BYTES}"
                    ),
                ));
            }
            for (field, digest) in [
                ("archive_sha256", &target.archive_sha256),
                ("manifest_sha256", &target.manifest_sha256),
                ("sbom_sha256", &target.sbom_sha256),
            ] {
                validate_sha256(digest).map_err(|reason| {
                    invalid_release(release, format!("target {target_name} {field}: {reason}"))
                })?;
            }
            validate_relative_artifact_path(&target.command).map_err(|reason| {
                invalid_release(release, format!("target {target_name} command: {reason}"))
            })?;
            if release.channel == OrionCodeUpdateChannel::Stable {
                match (target_name.as_str(), target.signing_requirement) {
                    ("darwin-aarch64", OrionCodeSigningRequirement::DeveloperIdAndNotarized)
                    | ("windows-x86_64", OrionCodeSigningRequirement::Authenticode)
                    | ("linux-x86_64" | "linux-aarch64", OrionCodeSigningRequirement::DigestOnly) =>
                        {}
                    _ => {
                        return Err(invalid_release(
                            release,
                            format!(
                                "target {target_name} has an invalid Stable signing requirement"
                            ),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum SequenceValidation {
    NewerThan(u64),
    Exactly(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrionCodeCurrentReleaseState {
    Unknown,
    Active,
    Paused,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeCandidate {
    pub version: Version,
    pub release: OrionCodeUpdateReleaseV1,
    pub target: OrionCodeUpdateTargetV1,
    pub target_name: String,
    pub is_signed_rollback: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeUpdateResolution {
    pub candidate: Option<OrionCodeCandidate>,
    pub current_release_state: OrionCodeCurrentReleaseState,
}

pub struct OrionCodeCandidateContext<'a> {
    pub channel: OrionCodeUpdateChannel,
    pub studio_version: &'a Version,
    pub current_version: Option<&'a Version>,
    pub target_name: &'a str,
    pub installation_id: &'a str,
    pub known_revoked_versions: &'a [Version],
}

pub fn resolve_orion_code_candidate(
    verified_index: &VerifiedOrionCodeUpdateIndex,
    context: OrionCodeCandidateContext<'_>,
) -> Result<OrionCodeUpdateResolution, OrionCodeUpdateContractError> {
    let current_release = context.current_version.and_then(|current_version| {
        verified_index.index.releases.iter().find(|release| {
            Version::parse(&release.version).is_ok_and(|version| version == *current_version)
        })
    });
    let current_release_state = if context
        .current_version
        .is_some_and(|current| context.known_revoked_versions.contains(current))
    {
        OrionCodeCurrentReleaseState::Revoked
    } else {
        current_release.map_or(
            OrionCodeCurrentReleaseState::Unknown,
            |release| match release.status {
                OrionCodeReleaseStatus::Active => OrionCodeCurrentReleaseState::Active,
                OrionCodeReleaseStatus::Paused => OrionCodeCurrentReleaseState::Paused,
                OrionCodeReleaseStatus::Revoked => OrionCodeCurrentReleaseState::Revoked,
            },
        )
    };

    if let Some(source_release) = current_release
        && source_release.status != OrionCodeReleaseStatus::Active
        && let Some(rollback_to) = source_release.rollback_to.as_deref()
        && let Some(candidate) = rollback_candidate(verified_index, rollback_to, &context)?
    {
        return Ok(OrionCodeUpdateResolution {
            candidate: Some(candidate),
            current_release_state,
        });
    }

    let mut candidates = Vec::new();
    for release in &verified_index.index.releases {
        if release.status != OrionCodeReleaseStatus::Active
            || !channel_accepts(context.channel, release.channel)
        {
            continue;
        }
        let version = Version::parse(&release.version)
            .map_err(|error| invalid_release(release, error.to_string()))?;
        if context.known_revoked_versions.contains(&version) {
            continue;
        }
        if context
            .current_version
            .is_some_and(|current_version| version <= *current_version)
            || !release_is_compatible(release, &context)?
        {
            continue;
        }
        let Some(target) = release.targets.get(context.target_name) else {
            continue;
        };
        if rollout_cohort(
            context.installation_id,
            &release.version,
            &release.rollout_salt,
        ) >= release.rollout_basis_points
        {
            continue;
        }
        candidates.push(OrionCodeCandidate {
            version,
            release: release.clone(),
            target: target.clone(),
            target_name: context.target_name.to_string(),
            is_signed_rollback: false,
        });
    }
    candidates.sort_by(|left, right| right.version.cmp(&left.version));
    Ok(OrionCodeUpdateResolution {
        candidate: candidates.into_iter().next(),
        current_release_state,
    })
}

fn rollback_candidate(
    verified_index: &VerifiedOrionCodeUpdateIndex,
    rollback_to: &str,
    context: &OrionCodeCandidateContext<'_>,
) -> Result<Option<OrionCodeCandidate>, OrionCodeUpdateContractError> {
    let Some(release) = verified_index
        .index
        .releases
        .iter()
        .find(|release| release.version == rollback_to)
    else {
        return Ok(None);
    };
    if release.status == OrionCodeReleaseStatus::Revoked
        || !channel_accepts(context.channel, release.channel)
        || !release_is_compatible(release, context)?
    {
        return Ok(None);
    }
    let Some(target) = release.targets.get(context.target_name) else {
        return Ok(None);
    };
    let version = Version::parse(&release.version)
        .map_err(|error| invalid_release(release, error.to_string()))?;
    if context.known_revoked_versions.contains(&version) {
        return Ok(None);
    }
    Ok(Some(OrionCodeCandidate {
        version,
        release: release.clone(),
        target: target.clone(),
        target_name: context.target_name.to_string(),
        is_signed_rollback: true,
    }))
}

fn release_is_compatible(
    release: &OrionCodeUpdateReleaseV1,
    context: &OrionCodeCandidateContext<'_>,
) -> Result<bool, OrionCodeUpdateContractError> {
    let requirement = VersionReq::parse(&release.studio_version_requirement)
        .map_err(|error| invalid_release(release, error.to_string()))?;
    Ok(requirement.matches(context.studio_version)
        && release.acp_protocol == SUPPORTED_ACP_PROTOCOL
        && release.targets.contains_key(context.target_name))
}

fn channel_accepts(configured: OrionCodeUpdateChannel, release: OrionCodeUpdateChannel) -> bool {
    match configured {
        OrionCodeUpdateChannel::Stable => release == OrionCodeUpdateChannel::Stable,
        OrionCodeUpdateChannel::Beta => true,
    }
}

pub fn rollout_cohort(installation_id: &str, version: &str, rollout_salt: &str) -> u16 {
    let mut hasher = Sha256::new();
    hasher.update(installation_id.as_bytes());
    hasher.update(version.as_bytes());
    hasher.update(rollout_salt.as_bytes());
    let digest = hasher.finalize();
    let Some(first_bytes) = digest.as_slice().first_chunk::<8>() else {
        return 0;
    };
    (u64::from_be_bytes(*first_bytes) % 10_000) as u16
}

pub fn validate_target_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!("invalid platform target {value:?}"));
    }
    Ok(())
}

pub fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("SHA-256 must contain exactly 64 hexadecimal characters".to_string());
    }
    Ok(())
}

pub fn validate_relative_artifact_path(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 512
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
        })
    {
        return Err("path must be a normalized relative slash-separated path".to_string());
    }
    Ok(())
}

fn validate_https_url(value: &str, allowed_hosts: Option<&HashSet<String>>) -> Result<(), String> {
    if value.len() > 2048 {
        return Err("URL exceeds 2048 bytes".to_string());
    }
    let url = Url::parse(value).map_err(|error| format!("invalid URL: {error}"))?;
    if url.scheme() != "https" {
        return Err("URL must use HTTPS".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL must not contain credentials".to_string());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("URL must be immutable and cannot contain a query or fragment".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "URL is missing a host".to_string())?
        .to_ascii_lowercase();
    if let Some(allowed_hosts) = allowed_hosts
        && !allowed_hosts.contains(&host)
    {
        return Err(format!("URL host {host} is not trusted"));
    }
    Ok(())
}

fn invalid_release(
    release: &OrionCodeUpdateReleaseV1,
    reason: impl Into<String>,
) -> OrionCodeUpdateContractError {
    OrionCodeUpdateContractError::InvalidRelease {
        version: release.version.clone(),
        reason: reason.into(),
    }
}

fn lowercase_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};

    const TEST_KEY_ID: &str = "orion-release-test-1";
    const TEST_HOST: &str = "updates.example.invalid";

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn verifier() -> OrionCodeIndexVerifier {
        let signing_key = signing_key();
        OrionCodeIndexVerifier::new(
            [(
                TEST_KEY_ID.to_string(),
                signing_key.verifying_key().to_bytes(),
            )],
            [TEST_HOST.to_string()],
        )
        .expect("valid test verifier")
    }

    fn target(version: &str) -> OrionCodeUpdateTargetV1 {
        OrionCodeUpdateTargetV1 {
            archive_url: format!("https://{TEST_HOST}/orion-code/{version}/darwin-aarch64.zip"),
            archive_sha256: "a".repeat(64),
            archive_bytes: 1024,
            format: OrionCodeArchiveFormat::Zip,
            command: "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string(),
            manifest_sha256: "b".repeat(64),
            sbom_sha256: "c".repeat(64),
            signing_requirement: OrionCodeSigningRequirement::DeveloperIdAndNotarized,
        }
    }

    fn release(version: &str, channel: OrionCodeUpdateChannel) -> OrionCodeUpdateReleaseV1 {
        OrionCodeUpdateReleaseV1 {
            version: version.to_string(),
            channel,
            status: OrionCodeReleaseStatus::Active,
            published_at: DateTime::parse_from_rfc3339("2026-08-31T00:00:00Z")
                .expect("valid time")
                .with_timezone(&Utc),
            studio_version_requirement: ">=1.0.0,<2.0.0".to_string(),
            acp_protocol: SUPPORTED_ACP_PROTOCOL,
            rollout_basis_points: 10_000,
            rollout_salt: "release-salt".to_string(),
            rollback_to: None,
            release_notes_url: "https://github.com/orion-agents/orion-code/releases/tag/v0.4.0"
                .to_string(),
            targets: [("darwin-aarch64".to_string(), target(version))]
                .into_iter()
                .collect(),
        }
    }

    fn index() -> OrionCodeUpdateIndexV1 {
        OrionCodeUpdateIndexV1 {
            schema_version: UPDATE_INDEX_SCHEMA_VERSION,
            sequence: 42,
            generated_at: DateTime::parse_from_rfc3339("2026-08-31T00:00:00Z")
                .expect("valid time")
                .with_timezone(&Utc),
            expires_at: DateTime::parse_from_rfc3339("2026-09-07T00:00:00Z")
                .expect("valid time")
                .with_timezone(&Utc),
            releases: vec![release("0.4.0", OrionCodeUpdateChannel::Stable)],
        }
    }

    fn signed(index: &OrionCodeUpdateIndexV1) -> (Vec<u8>, Vec<u8>) {
        let payload = serde_json::to_vec(index).expect("serialize test index");
        let signature = signing_key().sign(&payload);
        let envelope = OrionCodeUpdateSignatureEnvelopeV1 {
            schema_version: SIGNATURE_ENVELOPE_SCHEMA_VERSION,
            algorithm: "ed25519".to_string(),
            key_id: TEST_KEY_ID.to_string(),
            signature: BASE64_STANDARD.encode(signature.to_bytes()),
        };
        (
            payload,
            serde_json::to_vec(&envelope).expect("serialize signature envelope"),
        )
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc)
    }

    #[test]
    fn verifies_exact_bytes_before_parsing_index() {
        let (payload, signature) = signed(&index());
        let verified = verifier()
            .verify(&payload, &signature, now(), 41)
            .expect("valid signed index");
        assert_eq!(verified.index.sequence, 42);
        assert_eq!(verified.key_id, TEST_KEY_ID);
        assert_eq!(verified.payload_sha256, lowercase_sha256(&payload));
        assert!(
            verifier()
                .verify_cached(&payload, &signature, now(), 42)
                .is_ok()
        );
        assert!(matches!(
            verifier().verify_cached(&payload, &signature, now(), 41),
            Err(OrionCodeUpdateContractError::CachedSequenceMismatch { .. })
        ));

        let mut changed = payload;
        changed[0] ^= 1;
        assert!(matches!(
            verifier().verify(&changed, &signature, now(), 41),
            Err(OrionCodeUpdateContractError::InvalidSignature)
        ));
    }

    #[test]
    fn rejects_key_replay_expiry_and_signed_unknown_fields() {
        let (payload, mut signature) = signed(&index());
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&signature).expect("parse envelope");
        envelope["key_id"] = serde_json::Value::String("unknown".to_string());
        signature = serde_json::to_vec(&envelope).expect("serialize envelope");
        assert!(matches!(
            verifier().verify(&payload, &signature, now(), 41),
            Err(OrionCodeUpdateContractError::UnknownSigningKey(_))
        ));

        let (payload, signature) = signed(&index());
        assert!(matches!(
            verifier().verify(&payload, &signature, now(), 42),
            Err(OrionCodeUpdateContractError::SequenceReplay { .. })
        ));

        let mut expired = index();
        expired.expires_at = now();
        let (payload, signature) = signed(&expired);
        assert!(matches!(
            verifier().verify(&payload, &signature, now(), 0),
            Err(OrionCodeUpdateContractError::ExpiredIndex)
        ));

        let mut unknown_field = serde_json::to_value(index()).expect("index value");
        unknown_field["authority"] = serde_json::Value::String("registry".to_string());
        let payload = serde_json::to_vec(&unknown_field).expect("serialize altered index");
        let envelope = OrionCodeUpdateSignatureEnvelopeV1 {
            schema_version: 1,
            algorithm: "ed25519".to_string(),
            key_id: TEST_KEY_ID.to_string(),
            signature: BASE64_STANDARD.encode(signing_key().sign(&payload).to_bytes()),
        };
        assert!(matches!(
            verifier().verify(
                &payload,
                &serde_json::to_vec(&envelope).expect("serialize envelope"),
                now(),
                0
            ),
            Err(OrionCodeUpdateContractError::InvalidIndexJson(_))
        ));
    }

    #[test]
    fn rejects_invalid_rollout_url_command_and_stable_prerelease() {
        let mut invalid = index();
        invalid.releases[0].rollout_basis_points = 10_001;
        let (payload, signature) = signed(&invalid);
        assert!(matches!(
            verifier().verify(&payload, &signature, now(), 0),
            Err(OrionCodeUpdateContractError::InvalidRelease { .. })
        ));

        let mut invalid = index();
        invalid.releases[0]
            .targets
            .get_mut("darwin-aarch64")
            .expect("target")
            .archive_url = "https://attacker.invalid/orion-code.zip".to_string();
        let (payload, signature) = signed(&invalid);
        assert!(verifier().verify(&payload, &signature, now(), 0).is_err());

        let mut invalid = index();
        invalid.releases[0]
            .targets
            .get_mut("darwin-aarch64")
            .expect("target")
            .command = "../orion-code".to_string();
        let (payload, signature) = signed(&invalid);
        assert!(verifier().verify(&payload, &signature, now(), 0).is_err());

        let mut invalid = index();
        invalid.releases[0].version = "0.4.0-beta.1".to_string();
        let (payload, signature) = signed(&invalid);
        assert!(verifier().verify(&payload, &signature, now(), 0).is_err());
    }

    #[test]
    fn resolver_respects_channel_compatibility_rollout_and_signed_rollback() {
        let mut update_index = index();
        update_index
            .releases
            .push(release("0.5.0-beta.1", OrionCodeUpdateChannel::Beta));
        let (payload, signature) = signed(&update_index);
        let verified = verifier()
            .verify(&payload, &signature, now(), 0)
            .expect("valid index");
        let studio_version = Version::parse("1.2.0").expect("valid Studio version");
        let current_version = Version::parse("0.3.2").expect("valid current version");

        let stable = resolve_orion_code_candidate(
            &verified,
            OrionCodeCandidateContext {
                channel: OrionCodeUpdateChannel::Stable,
                studio_version: &studio_version,
                current_version: Some(&current_version),
                target_name: "darwin-aarch64",
                installation_id: "local-only-installation",
                known_revoked_versions: &[],
            },
        )
        .expect("stable resolution")
        .candidate
        .expect("stable candidate");
        assert_eq!(stable.version, Version::parse("0.4.0").unwrap());

        let known_revoked = [Version::parse("0.4.0").expect("revoked version")];
        let resolution = resolve_orion_code_candidate(
            &verified,
            OrionCodeCandidateContext {
                channel: OrionCodeUpdateChannel::Stable,
                studio_version: &studio_version,
                current_version: Some(&current_version),
                target_name: "darwin-aarch64",
                installation_id: "local-only-installation",
                known_revoked_versions: &known_revoked,
            },
        )
        .expect("known revoke resolution");
        assert!(resolution.candidate.is_none());

        let beta = resolve_orion_code_candidate(
            &verified,
            OrionCodeCandidateContext {
                channel: OrionCodeUpdateChannel::Beta,
                studio_version: &studio_version,
                current_version: Some(&current_version),
                target_name: "darwin-aarch64",
                installation_id: "local-only-installation",
                known_revoked_versions: &[],
            },
        )
        .expect("beta resolution")
        .candidate
        .expect("beta candidate");
        assert_eq!(beta.version, Version::parse("0.5.0-beta.1").unwrap());

        let cohort = rollout_cohort("local-only-installation", "0.4.0", "release-salt");
        let mut rollout_index = index();
        rollout_index.releases[0].rollout_basis_points = cohort;
        let (payload, signature) = signed(&rollout_index);
        let verified = verifier()
            .verify(&payload, &signature, now(), 0)
            .expect("valid rollout index");
        let resolution = resolve_orion_code_candidate(
            &verified,
            OrionCodeCandidateContext {
                channel: OrionCodeUpdateChannel::Stable,
                studio_version: &studio_version,
                current_version: Some(&current_version),
                target_name: "darwin-aarch64",
                installation_id: "local-only-installation",
                known_revoked_versions: &[],
            },
        )
        .expect("rollout resolution");
        assert!(resolution.candidate.is_none());

        let mut rollback_index = index();
        rollback_index.releases[0] = release("0.3.2", OrionCodeUpdateChannel::Stable);
        let mut revoked = release("0.4.0", OrionCodeUpdateChannel::Stable);
        revoked.status = OrionCodeReleaseStatus::Revoked;
        revoked.rollback_to = Some("0.3.2".to_string());
        rollback_index.releases.push(revoked);
        let (payload, signature) = signed(&rollback_index);
        let verified = verifier()
            .verify(&payload, &signature, now(), 0)
            .expect("valid rollback index");
        let current_version = Version::parse("0.4.0").expect("valid current version");
        let resolution = resolve_orion_code_candidate(
            &verified,
            OrionCodeCandidateContext {
                channel: OrionCodeUpdateChannel::Stable,
                studio_version: &studio_version,
                current_version: Some(&current_version),
                target_name: "darwin-aarch64",
                installation_id: "local-only-installation",
                known_revoked_versions: &[],
            },
        )
        .expect("rollback resolution");
        assert_eq!(
            resolution.current_release_state,
            OrionCodeCurrentReleaseState::Revoked
        );
        let candidate = resolution.candidate.expect("rollback candidate");
        assert_eq!(candidate.version, Version::parse("0.3.2").unwrap());
        assert!(candidate.is_signed_rollback);
    }
}
