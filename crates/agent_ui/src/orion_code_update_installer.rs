use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use chrono::Utc;
use futures::{AsyncReadExt as _, future::BoxFuture};
use http_client::{
    AsyncBody, HttpClient, HttpRequestExt as _, Method, RedirectPolicy, Request, StatusCode,
    http::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_LENGTH, LOCATION},
};
use orion_code_update::{
    MAX_ARCHIVE_BYTES, OrionCodeArchiveFormat, OrionCodeArchiveLimits,
    OrionCodeArchiveVerificationError, OrionCodeCandidate, OrionCodeInstallReceiptV2,
    OrionCodeInstallSource, VerifiedOrionCodeArtifact, extract_orion_code_zip,
    validate_relative_artifact_path, validate_sha256, validate_target_name,
    verify_orion_code_archive, verify_orion_code_artifact,
};
use semver::Version;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use crate::orion_code_update_platform::{
    OrionCodePlatformVerificationError, OrionCodePlatformVerifier,
};

const ARCHIVE_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const ARCHIVE_REDIRECT_LIMIT: u8 = 3;
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;
const DOWNLOAD_PROGRESS_REPORT_INTERVAL_BYTES: u64 = 256 * 1024;
const MAX_RECEIPT_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeStagedIdentity {
    pub version: Version,
    pub target: String,
    pub archive_sha256: String,
    pub command: String,
    pub index_sequence: u64,
    pub artifact_root: PathBuf,
    pub receipt_path: PathBuf,
    pub reused_existing: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrionCodeDownloadProgress {
    received_bytes: u64,
    total_bytes: u64,
}

impl OrionCodeDownloadProgress {
    pub fn new(received_bytes: u64, total_bytes: u64) -> Self {
        let total_bytes = total_bytes.min(MAX_ARCHIVE_BYTES);
        Self {
            received_bytes: received_bytes.min(total_bytes),
            total_bytes,
        }
    }

    pub fn received_bytes(self) -> u64 {
        self.received_bytes
    }

    pub fn total_bytes(self) -> u64 {
        self.total_bytes
    }
}

pub trait OrionCodeDownloadProgressReporter: Send + Sync {
    fn report(&self, progress: OrionCodeDownloadProgress);
}

struct NoopDownloadProgressReporter;

impl OrionCodeDownloadProgressReporter for NoopDownloadProgressReporter {
    fn report(&self, _progress: OrionCodeDownloadProgress) {}
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum OrionCodePreflightError {
    #[error("Orion Code ACP preflight is not configured")]
    Unavailable,
    #[error("Orion Code ACP preflight failed: {diagnostic_code}")]
    Failed { diagnostic_code: String },
}

pub trait OrionCodeUpdatePreflight: Send + Sync {
    fn verify(
        &self,
        artifact_root: PathBuf,
        candidate: OrionCodeCandidate,
    ) -> BoxFuture<'static, Result<(), OrionCodePreflightError>>;
}

#[derive(Debug, Error)]
pub enum OrionCodeArchiveInstallerError {
    #[error("invalid Orion Code archive installer configuration: {0}")]
    Configuration(String),
    #[error("Orion Code ACP preflight is not configured")]
    MissingPreflight,
    #[error("invalid Orion Code update candidate: {0}")]
    InvalidCandidate(String),
    #[error("unsafe Orion Code managed path: {0}")]
    UnsafeManagedPath(String),
    #[error("failed to build Orion Code archive request: {0}")]
    Request(String),
    #[error("Orion Code archive transport failed: {0}")]
    Transport(String),
    #[error("Orion Code archive redirect was rejected: {0}")]
    Redirect(String),
    #[error("Orion Code archive endpoint returned HTTP {0}")]
    HttpStatus(StatusCode),
    #[error("Orion Code archive response omitted Content-Length")]
    MissingContentLength,
    #[error("Orion Code archive response has invalid Content-Length: {0}")]
    InvalidContentLength(String),
    #[error(
        "Orion Code archive Content-Length mismatch: expected {expected} bytes, received {actual}"
    )]
    ContentLengthMismatch { expected: u64, actual: u64 },
    #[error("Orion Code archive body exceeded the {limit} byte limit")]
    BodyTooLarge { limit: u64 },
    #[error("Orion Code archive was truncated: expected {expected} bytes, read {actual}")]
    TruncatedBody { expected: u64, actual: u64 },
    #[error("Orion Code archive SHA-256 mismatch")]
    ArchiveDigestMismatch,
    #[error("Orion Code archive verification failed: {0}")]
    Archive(#[from] OrionCodeArchiveVerificationError),
    #[error("Orion Code platform verification failed: {0}")]
    Platform(#[from] OrionCodePlatformVerificationError),
    #[error("Orion Code ACP preflight failed: {0}")]
    Preflight(#[from] OrionCodePreflightError),
    #[error("Orion Code staging transaction conflicts with existing managed state: {0}")]
    ExistingState(String),
    #[error("invalid Orion Code install receipt: {0}")]
    Receipt(String),
    #[error("Orion Code installer I/O failed while {operation}: {reason}")]
    Io {
        operation: &'static str,
        reason: String,
    },
    #[error("{operation}; cleanup also failed: {cleanup}")]
    Cleanup { operation: String, cleanup: String },
}

pub struct OrionCodeArchiveInstaller {
    canonical_managed_root: PathBuf,
    http_client: Arc<dyn HttpClient>,
    allowed_hosts: HashSet<String>,
    platform_verifier: Arc<dyn OrionCodePlatformVerifier>,
    preflight: Option<Arc<dyn OrionCodeUpdatePreflight>>,
    archive_limits: OrionCodeArchiveLimits,
    file_operations: Arc<dyn InstallerFileOperations>,
}

impl OrionCodeArchiveInstaller {
    pub fn new(
        managed_root: impl AsRef<Path>,
        http_client: Arc<dyn HttpClient>,
        allowed_hosts: impl IntoIterator<Item = String>,
        platform_verifier: Arc<dyn OrionCodePlatformVerifier>,
        preflight: Option<Arc<dyn OrionCodeUpdatePreflight>>,
    ) -> Result<Self, OrionCodeArchiveInstallerError> {
        let allowed_hosts = validate_allowed_hosts(allowed_hosts)?;
        let canonical_managed_root = canonical_managed_root(managed_root.as_ref())?;
        Ok(Self {
            canonical_managed_root,
            http_client,
            allowed_hosts,
            platform_verifier,
            preflight,
            archive_limits: OrionCodeArchiveLimits::default(),
            file_operations: Arc::new(SystemInstallerFileOperations),
        })
    }

    pub async fn stage(
        &self,
        candidate: OrionCodeCandidate,
        index_sequence: u64,
    ) -> Result<OrionCodeStagedIdentity, OrionCodeArchiveInstallerError> {
        self.stage_with_progress(
            candidate,
            index_sequence,
            Arc::new(NoopDownloadProgressReporter),
        )
        .await
    }

    pub async fn stage_with_progress(
        &self,
        candidate: OrionCodeCandidate,
        index_sequence: u64,
        progress_reporter: Arc<dyn OrionCodeDownloadProgressReporter>,
    ) -> Result<OrionCodeStagedIdentity, OrionCodeArchiveInstallerError> {
        let preflight = self
            .preflight
            .as_ref()
            .ok_or(OrionCodeArchiveInstallerError::MissingPreflight)?
            .clone();
        let candidate_identity =
            validate_candidate(&candidate, index_sequence, &self.allowed_hosts)?;
        let layout = self.prepare_layout(&candidate_identity)?;

        if let Some(existing) = self
            .replay_existing(
                &layout,
                &candidate,
                &candidate_identity,
                index_sequence,
                preflight.clone(),
            )
            .await?
        {
            return Ok(existing);
        }

        let operation_id = Uuid::new_v4();
        let mut transaction = StageTransaction::new(&layout, operation_id);
        let operation = self
            .stage_new(
                &layout,
                &mut transaction,
                candidate,
                candidate_identity,
                index_sequence,
                preflight,
                progress_reporter,
            )
            .await;
        self.finish_transaction(operation, &transaction)
    }

    #[cfg(test)]
    fn with_archive_limits(mut self, archive_limits: OrionCodeArchiveLimits) -> Self {
        self.archive_limits = archive_limits;
        self
    }

    #[cfg(test)]
    fn with_file_operations(mut self, file_operations: Arc<dyn InstallerFileOperations>) -> Self {
        self.file_operations = file_operations;
        self
    }

    fn prepare_layout(
        &self,
        identity: &ValidatedCandidateIdentity,
    ) -> Result<InstallerLayout, OrionCodeArchiveInstallerError> {
        validate_existing_directory(
            &self.canonical_managed_root,
            &self.canonical_managed_root,
            "managed root",
        )?;
        let downloads = ensure_managed_directory(
            &self.canonical_managed_root,
            &self.canonical_managed_root,
            "downloads",
        )?;
        let staging = ensure_managed_directory(
            &self.canonical_managed_root,
            &self.canonical_managed_root,
            "staging",
        )?;
        let versions = ensure_managed_directory(
            &self.canonical_managed_root,
            &self.canonical_managed_root,
            "versions",
        )?;
        let receipts = ensure_managed_directory(
            &self.canonical_managed_root,
            &self.canonical_managed_root,
            "receipts",
        )?;

        let downloads_version =
            ensure_managed_directory(&self.canonical_managed_root, &downloads, &identity.version)?;
        let downloads_target = ensure_managed_directory(
            &self.canonical_managed_root,
            &downloads_version,
            &identity.target,
        )?;
        let staging_version =
            ensure_managed_directory(&self.canonical_managed_root, &staging, &identity.version)?;
        let staging_target = ensure_managed_directory(
            &self.canonical_managed_root,
            &staging_version,
            &identity.target,
        )?;
        let versions_version =
            ensure_managed_directory(&self.canonical_managed_root, &versions, &identity.version)?;

        let receipt_stem = format!("{}-{}", identity.version, identity.target);
        Ok(InstallerLayout {
            downloads_target,
            staging_target,
            destination: versions_version.join(&identity.target),
            receipt: receipts.join(format!("{receipt_stem}.json")),
            pending_receipt: receipts.join(format!(".{receipt_stem}.pending.json")),
            receipts,
            versions_version,
        })
    }

    async fn replay_existing(
        &self,
        layout: &InstallerLayout,
        candidate: &OrionCodeCandidate,
        identity: &ValidatedCandidateIdentity,
        index_sequence: u64,
        preflight: Arc<dyn OrionCodeUpdatePreflight>,
    ) -> Result<Option<OrionCodeStagedIdentity>, OrionCodeArchiveInstallerError> {
        let destination_exists = path_exists(&layout.destination)?;
        let receipt_exists = path_exists(&layout.receipt)?;
        let pending_receipt_exists = path_exists(&layout.pending_receipt)?;

        if !destination_exists {
            if receipt_exists || pending_receipt_exists {
                return Err(OrionCodeArchiveInstallerError::ExistingState(
                    "a receipt exists without its version directory".to_string(),
                ));
            }
            return Ok(None);
        }
        if receipt_exists && pending_receipt_exists {
            return Err(OrionCodeArchiveInstallerError::ExistingState(
                "both committed and pending receipts exist".to_string(),
            ));
        }
        let (receipt_path, pending) = if receipt_exists {
            (&layout.receipt, false)
        } else if pending_receipt_exists {
            (&layout.pending_receipt, true)
        } else {
            return Err(OrionCodeArchiveInstallerError::ExistingState(
                "an existing version has no replayable receipt".to_string(),
            ));
        };

        validate_existing_directory(
            &self.canonical_managed_root,
            &layout.destination,
            "existing version",
        )?;
        let receipt = read_install_receipt(receipt_path)?;
        validate_receipt(&receipt, identity, index_sequence)?;
        let verified = self.verify_artifact(&layout.destination, candidate, identity)?;
        self.platform_verifier
            .verify(
                layout.destination.clone(),
                verified.manifest,
                candidate.target.signing_requirement,
            )
            .await?;
        preflight
            .verify(layout.destination.clone(), candidate.clone())
            .await?;

        if pending {
            ensure_path_absent(&layout.receipt, "committed receipt")?;
            self.file_operations
                .rename(&layout.pending_receipt, &layout.receipt)
                .map_err(|error| io_error("committing recovered receipt", error))?;
            self.file_operations
                .sync_directory(&layout.receipts)
                .map_err(|error| io_error("syncing recovered receipt directory", error))?;
        }

        Ok(Some(staged_identity(
            candidate,
            index_sequence,
            layout.destination.clone(),
            layout.receipt.clone(),
            true,
        )))
    }

    async fn stage_new(
        &self,
        layout: &InstallerLayout,
        transaction: &mut StageTransaction,
        candidate: OrionCodeCandidate,
        identity: ValidatedCandidateIdentity,
        index_sequence: u64,
        preflight: Arc<dyn OrionCodeUpdatePreflight>,
        progress_reporter: Arc<dyn OrionCodeDownloadProgressReporter>,
    ) -> Result<OrionCodeStagedIdentity, OrionCodeArchiveInstallerError> {
        self.download_archive(&candidate, &transaction.partial, progress_reporter.as_ref())
            .await?;
        verify_orion_code_archive(
            &transaction.partial,
            candidate.target.archive_bytes,
            &candidate.target.archive_sha256,
        )?;
        extract_orion_code_zip(
            &transaction.partial,
            &transaction.staging,
            self.archive_limits,
        )
        .await?;
        let verified = self.verify_artifact(&transaction.staging, &candidate, &identity)?;
        self.platform_verifier
            .verify(
                transaction.staging.clone(),
                verified.manifest,
                candidate.target.signing_requirement,
            )
            .await?;
        preflight
            .verify(transaction.staging.clone(), candidate.clone())
            .await?;

        self.file_operations
            .sync_directory(&transaction.staging)
            .map_err(|error| io_error("syncing verified staging directory", error))?;
        self.file_operations
            .sync_directory(&layout.staging_target)
            .map_err(|error| io_error("syncing staging parent directory", error))?;
        let receipt = install_receipt(&candidate, index_sequence);
        write_new_install_receipt(&layout.pending_receipt, &receipt)?;
        transaction.pending_receipt_created = true;
        self.file_operations
            .sync_directory(&layout.receipts)
            .map_err(|error| io_error("syncing pending receipt directory", error))?;
        ensure_path_absent(&layout.destination, "version destination")?;
        ensure_path_absent(&layout.receipt, "committed receipt")?;
        self.file_operations
            .rename(&transaction.staging, &layout.destination)
            .map_err(|error| io_error("promoting verified staging directory", error))?;
        transaction.promoted = true;
        self.file_operations
            .sync_directory(&layout.versions_version)
            .map_err(|error| io_error("syncing promoted version directory", error))?;
        self.file_operations
            .rename(&layout.pending_receipt, &layout.receipt)
            .map_err(|error| io_error("committing install receipt", error))?;
        transaction.pending_receipt_created = false;
        self.file_operations
            .sync_directory(&layout.receipts)
            .map_err(|error| io_error("syncing install receipt directory", error))?;

        Ok(staged_identity(
            &candidate,
            index_sequence,
            layout.destination.clone(),
            layout.receipt.clone(),
            false,
        ))
    }

    fn verify_artifact(
        &self,
        artifact_root: &Path,
        candidate: &OrionCodeCandidate,
        identity: &ValidatedCandidateIdentity,
    ) -> Result<VerifiedOrionCodeArtifact, OrionCodeArchiveInstallerError> {
        verify_orion_code_artifact(
            artifact_root,
            &identity.version,
            &identity.target,
            &candidate.target.command,
            &candidate.target.manifest_sha256,
            &candidate.target.sbom_sha256,
            self.archive_limits,
        )
        .map_err(Into::into)
    }

    async fn download_archive(
        &self,
        candidate: &OrionCodeCandidate,
        partial_path: &Path,
        progress_reporter: &dyn OrionCodeDownloadProgressReporter,
    ) -> Result<(), OrionCodeArchiveInstallerError> {
        let mut current_url =
            validate_archive_url(&candidate.target.archive_url, &self.allowed_hosts, false)?;
        for redirect_count in 0..=ARCHIVE_REDIRECT_LIMIT {
            validate_parsed_archive_url(&current_url, &self.allowed_hosts, redirect_count > 0)?;
            let request = Request::builder()
                .method(Method::GET)
                .uri(current_url.as_str())
                .header(ACCEPT, "application/zip, application/octet-stream")
                .header(ACCEPT_ENCODING, "identity")
                .follow_redirects(RedirectPolicy::NoFollow)
                .timeout(ARCHIVE_REQUEST_TIMEOUT)
                .body(AsyncBody::empty())
                .map_err(|error| OrionCodeArchiveInstallerError::Request(error.to_string()))?;
            let mut response =
                self.http_client.send(request).await.map_err(|error| {
                    OrionCodeArchiveInstallerError::Transport(error.to_string())
                })?;
            if is_followable_redirect(response.status()) {
                if redirect_count == ARCHIVE_REDIRECT_LIMIT {
                    return Err(OrionCodeArchiveInstallerError::Redirect(format!(
                        "more than {ARCHIVE_REDIRECT_LIMIT} redirects"
                    )));
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .ok_or_else(|| {
                        OrionCodeArchiveInstallerError::Redirect(
                            "redirect response omitted Location".to_string(),
                        )
                    })?
                    .to_str()
                    .map_err(|error| OrionCodeArchiveInstallerError::Redirect(error.to_string()))?;
                current_url = current_url
                    .join(location)
                    .map_err(|error| OrionCodeArchiveInstallerError::Redirect(error.to_string()))?;
                validate_parsed_archive_url(&current_url, &self.allowed_hosts, true)?;
                continue;
            }
            if response.status() != StatusCode::OK {
                return Err(OrionCodeArchiveInstallerError::HttpStatus(
                    response.status(),
                ));
            }
            let content_length = required_content_length(response.headers())?;
            if content_length != candidate.target.archive_bytes {
                return Err(OrionCodeArchiveInstallerError::ContentLengthMismatch {
                    expected: candidate.target.archive_bytes,
                    actual: content_length,
                });
            }
            if content_length > MAX_ARCHIVE_BYTES {
                return Err(OrionCodeArchiveInstallerError::BodyTooLarge {
                    limit: MAX_ARCHIVE_BYTES,
                });
            }
            return stream_response_to_partial(
                response.body_mut(),
                partial_path,
                candidate.target.archive_bytes,
                &candidate.target.archive_sha256,
                progress_reporter,
            )
            .await;
        }
        Err(OrionCodeArchiveInstallerError::Redirect(
            "redirect loop terminated unexpectedly".to_string(),
        ))
    }

    fn finish_transaction(
        &self,
        operation: Result<OrionCodeStagedIdentity, OrionCodeArchiveInstallerError>,
        transaction: &StageTransaction,
    ) -> Result<OrionCodeStagedIdentity, OrionCodeArchiveInstallerError> {
        let cleanup = self.cleanup_transaction(transaction);
        match (operation, cleanup) {
            (Ok(identity), Ok(())) => Ok(identity),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(cleanup)) => Err(OrionCodeArchiveInstallerError::Cleanup {
                operation: "Orion Code staging completed".to_string(),
                cleanup,
            }),
            (Err(error), Err(cleanup)) => Err(OrionCodeArchiveInstallerError::Cleanup {
                operation: error.to_string(),
                cleanup,
            }),
        }
    }

    fn cleanup_transaction(&self, transaction: &StageTransaction) -> Result<(), String> {
        let mut failures = Vec::new();
        if let Err(error) = cleanup_owned_file(
            self.file_operations.as_ref(),
            &self.canonical_managed_root,
            &transaction.partial,
            "partial download",
        ) {
            failures.push(error);
        }
        if !transaction.promoted
            && let Err(error) = cleanup_owned_directory(
                self.file_operations.as_ref(),
                &self.canonical_managed_root,
                &transaction.staging,
                "staging directory",
            )
        {
            failures.push(error);
        }
        if transaction.pending_receipt_created
            && !transaction.promoted
            && let Err(error) = cleanup_owned_file(
                self.file_operations.as_ref(),
                &self.canonical_managed_root,
                &transaction.pending_receipt,
                "pending receipt",
            )
        {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

#[derive(Clone, Debug)]
struct ValidatedCandidateIdentity {
    version: String,
    target: String,
    archive_sha256: String,
    command: String,
}

struct InstallerLayout {
    downloads_target: PathBuf,
    staging_target: PathBuf,
    destination: PathBuf,
    receipt: PathBuf,
    pending_receipt: PathBuf,
    receipts: PathBuf,
    versions_version: PathBuf,
}

struct StageTransaction {
    partial: PathBuf,
    staging: PathBuf,
    pending_receipt: PathBuf,
    pending_receipt_created: bool,
    promoted: bool,
}

impl StageTransaction {
    fn new(layout: &InstallerLayout, operation_id: Uuid) -> Self {
        let operation_id = operation_id.hyphenated().to_string();
        Self {
            partial: layout
                .downloads_target
                .join(format!("{operation_id}.partial")),
            staging: layout.staging_target.join(operation_id),
            pending_receipt: layout.pending_receipt.clone(),
            pending_receipt_created: false,
            promoted: false,
        }
    }
}

trait InstallerFileOperations: Send + Sync {
    fn rename(&self, source: &Path, destination: &Path) -> std::io::Result<()>;
    fn remove_file(&self, path: &Path) -> std::io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()>;
    fn sync_directory(&self, path: &Path) -> std::io::Result<()>;
}

struct SystemInstallerFileOperations;

impl InstallerFileOperations for SystemInstallerFileOperations {
    fn rename(&self, source: &Path, destination: &Path) -> std::io::Result<()> {
        std::fs::rename(source, destination)
    }

    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    #[cfg(unix)]
    fn sync_directory(&self, path: &Path) -> std::io::Result<()> {
        File::open(path)?.sync_all()
    }

    #[cfg(not(unix))]
    fn sync_directory(&self, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }
}

fn validate_candidate(
    candidate: &OrionCodeCandidate,
    index_sequence: u64,
    allowed_hosts: &HashSet<String>,
) -> Result<ValidatedCandidateIdentity, OrionCodeArchiveInstallerError> {
    if index_sequence == 0 {
        return Err(OrionCodeArchiveInstallerError::InvalidCandidate(
            "index sequence must be greater than zero".to_string(),
        ));
    }
    let version = candidate.version.to_string();
    if version != candidate.release.version {
        return Err(OrionCodeArchiveInstallerError::InvalidCandidate(
            "parsed version does not match release version".to_string(),
        ));
    }
    validate_target_name(&candidate.target_name)
        .map_err(OrionCodeArchiveInstallerError::InvalidCandidate)?;
    let release_target = candidate
        .release
        .targets
        .get(&candidate.target_name)
        .ok_or_else(|| {
            OrionCodeArchiveInstallerError::InvalidCandidate(
                "candidate target is absent from its release".to_string(),
            )
        })?;
    if release_target != &candidate.target {
        return Err(OrionCodeArchiveInstallerError::InvalidCandidate(
            "candidate target does not exactly match its release".to_string(),
        ));
    }
    if candidate.target.format != OrionCodeArchiveFormat::Zip {
        return Err(OrionCodeArchiveInstallerError::InvalidCandidate(
            "unsupported archive format".to_string(),
        ));
    }
    if candidate.target.archive_bytes == 0 || candidate.target.archive_bytes > MAX_ARCHIVE_BYTES {
        return Err(OrionCodeArchiveInstallerError::InvalidCandidate(format!(
            "archive size must be in 1..={MAX_ARCHIVE_BYTES}"
        )));
    }
    for digest in [
        &candidate.target.archive_sha256,
        &candidate.target.manifest_sha256,
        &candidate.target.sbom_sha256,
    ] {
        validate_sha256(digest).map_err(OrionCodeArchiveInstallerError::InvalidCandidate)?;
    }
    validate_relative_artifact_path(&candidate.target.command)
        .map_err(OrionCodeArchiveInstallerError::InvalidCandidate)?;
    validate_archive_url(&candidate.target.archive_url, allowed_hosts, false)?;
    Ok(ValidatedCandidateIdentity {
        version,
        target: candidate.target_name.clone(),
        archive_sha256: candidate.target.archive_sha256.to_ascii_lowercase(),
        command: candidate.target.command.clone(),
    })
}

fn validate_allowed_hosts(
    allowed_hosts: impl IntoIterator<Item = String>,
) -> Result<HashSet<String>, OrionCodeArchiveInstallerError> {
    let mut validated = HashSet::new();
    for host in allowed_hosts {
        let host = host.to_ascii_lowercase();
        if host.is_empty()
            || host.len() > 253
            || host.starts_with('.')
            || host.ends_with('.')
            || host
                .bytes()
                .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')))
        {
            return Err(OrionCodeArchiveInstallerError::Configuration(
                "trusted archive host is invalid".to_string(),
            ));
        }
        validated.insert(host);
    }
    if validated.is_empty() {
        return Err(OrionCodeArchiveInstallerError::Configuration(
            "at least one trusted archive host is required".to_string(),
        ));
    }
    Ok(validated)
}

fn validate_archive_url(
    value: &str,
    allowed_hosts: &HashSet<String>,
    redirect: bool,
) -> Result<Url, OrionCodeArchiveInstallerError> {
    if value.len() > 2048 {
        return Err(url_error(redirect, "URL exceeds 2048 bytes"));
    }
    let url = Url::parse(value).map_err(|error| url_error(redirect, error.to_string()))?;
    validate_parsed_archive_url(&url, allowed_hosts, redirect)?;
    Ok(url)
}

fn validate_parsed_archive_url(
    url: &Url,
    allowed_hosts: &HashSet<String>,
    redirect: bool,
) -> Result<(), OrionCodeArchiveInstallerError> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(url_error(
            redirect,
            "archive URL must be HTTPS on port 443 without credentials, query, or fragment",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| url_error(redirect, "archive URL has no host"))?
        .to_ascii_lowercase();
    if !allowed_hosts.contains(&host) {
        return Err(url_error(redirect, format!("host {host} is not trusted")));
    }
    Ok(())
}

fn url_error(redirect: bool, reason: impl Into<String>) -> OrionCodeArchiveInstallerError {
    if redirect {
        OrionCodeArchiveInstallerError::Redirect(reason.into())
    } else {
        OrionCodeArchiveInstallerError::InvalidCandidate(reason.into())
    }
}

fn required_content_length(
    headers: &http_client::http::HeaderMap,
) -> Result<u64, OrionCodeArchiveInstallerError> {
    let value = headers
        .get(CONTENT_LENGTH)
        .ok_or(OrionCodeArchiveInstallerError::MissingContentLength)?;
    value
        .to_str()
        .map_err(|error| OrionCodeArchiveInstallerError::InvalidContentLength(error.to_string()))?
        .parse::<u64>()
        .map_err(|error| OrionCodeArchiveInstallerError::InvalidContentLength(error.to_string()))
}

async fn stream_response_to_partial(
    body: &mut AsyncBody,
    partial_path: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
    progress_reporter: &dyn OrionCodeDownloadProgressReporter,
) -> Result<(), OrionCodeArchiveInstallerError> {
    ensure_path_absent(partial_path, "partial download")?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(partial_path)
        .map_err(|error| io_error("creating partial download", error))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut last_reported_bytes = 0_u64;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_BYTES];
    let mut body = body.take(expected_bytes.saturating_add(1));
    progress_reporter.report(OrionCodeDownloadProgress::new(0, expected_bytes));
    loop {
        let count = body
            .read(&mut buffer)
            .await
            .map_err(|error| OrionCodeArchiveInstallerError::Transport(error.to_string()))?;
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or(
            OrionCodeArchiveInstallerError::BodyTooLarge {
                limit: expected_bytes,
            },
        )?;
        if total > expected_bytes || total > MAX_ARCHIVE_BYTES {
            return Err(OrionCodeArchiveInstallerError::BodyTooLarge {
                limit: expected_bytes.min(MAX_ARCHIVE_BYTES),
            });
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| io_error("writing partial download", error))?;
        hasher.update(&buffer[..count]);
        if total == expected_bytes
            || total.saturating_sub(last_reported_bytes) >= DOWNLOAD_PROGRESS_REPORT_INTERVAL_BYTES
        {
            progress_reporter.report(OrionCodeDownloadProgress::new(total, expected_bytes));
            last_reported_bytes = total;
        }
    }
    if total != last_reported_bytes {
        progress_reporter.report(OrionCodeDownloadProgress::new(total, expected_bytes));
    }
    output
        .flush()
        .map_err(|error| io_error("flushing partial download", error))?;
    output
        .sync_all()
        .map_err(|error| io_error("syncing partial download", error))?;
    if total != expected_bytes {
        return Err(OrionCodeArchiveInstallerError::TruncatedBody {
            expected: expected_bytes,
            actual: total,
        });
    }
    let actual_sha256 = format!("{:x}", hasher.finalize());
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(OrionCodeArchiveInstallerError::ArchiveDigestMismatch);
    }
    Ok(())
}

fn canonical_managed_root(managed_root: &Path) -> Result<PathBuf, OrionCodeArchiveInstallerError> {
    if managed_root.as_os_str().is_empty() {
        return Err(OrionCodeArchiveInstallerError::Configuration(
            "managed root must not be empty".to_string(),
        ));
    }
    match std::fs::symlink_metadata(managed_root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(OrionCodeArchiveInstallerError::UnsafeManagedPath(
                    "managed root must be a real directory".to_string(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(managed_root)
                .map_err(|error| io_error("creating managed root", error))?;
        }
        Err(error) => return Err(io_error("inspecting managed root", error)),
    }
    let metadata = std::fs::symlink_metadata(managed_root)
        .map_err(|error| io_error("inspecting created managed root", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(OrionCodeArchiveInstallerError::UnsafeManagedPath(
            "managed root must not be a link".to_string(),
        ));
    }
    managed_root
        .canonicalize()
        .map_err(|error| io_error("canonicalizing managed root", error))
}

fn ensure_managed_directory(
    canonical_root: &Path,
    parent: &Path,
    component: &str,
) -> Result<PathBuf, OrionCodeArchiveInstallerError> {
    validate_single_component(component)?;
    validate_existing_directory(canonical_root, parent, "managed parent")?;
    let path = parent.join(component);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(OrionCodeArchiveInstallerError::UnsafeManagedPath(
                    "managed directory component is not a real directory".to_string(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&path)
                .map_err(|error| io_error("creating managed directory", error))?;
        }
        Err(error) => return Err(io_error("inspecting managed directory", error)),
    }
    validate_existing_directory(canonical_root, &path, "managed directory")?;
    Ok(path)
}

fn validate_existing_directory(
    canonical_root: &Path,
    path: &Path,
    label: &'static str,
) -> Result<(), OrionCodeArchiveInstallerError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| io_error("inspecting managed directory", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(OrionCodeArchiveInstallerError::UnsafeManagedPath(format!(
            "{label} is not a real directory"
        )));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| io_error("canonicalizing managed directory", error))?;
    if !canonical.starts_with(canonical_root) || canonical != path {
        return Err(OrionCodeArchiveInstallerError::UnsafeManagedPath(format!(
            "{label} escapes the canonical managed root"
        )));
    }
    Ok(())
}

fn validate_single_component(component: &str) -> Result<(), OrionCodeArchiveInstallerError> {
    let path = Path::new(component);
    if component.is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(OrionCodeArchiveInstallerError::UnsafeManagedPath(
            "managed path component is not normalized".to_string(),
        ));
    }
    Ok(())
}

fn ensure_path_absent(
    path: &Path,
    label: &'static str,
) -> Result<(), OrionCodeArchiveInstallerError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(OrionCodeArchiveInstallerError::ExistingState(format!(
            "{label} already exists"
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspecting managed destination", error)),
    }
}

fn path_exists(path: &Path) -> Result<bool, OrionCodeArchiveInstallerError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("inspecting managed state", error)),
    }
}

fn write_new_install_receipt(
    path: &Path,
    receipt: &OrionCodeInstallReceiptV2,
) -> Result<(), OrionCodeArchiveInstallerError> {
    ensure_path_absent(path, "pending receipt")?;
    let mut bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|error| OrionCodeArchiveInstallerError::Receipt(error.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(OrionCodeArchiveInstallerError::Receipt(
            "receipt exceeds its byte limit".to_string(),
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_error("creating pending receipt", error))?;
    file.write_all(&bytes)
        .map_err(|error| io_error("writing pending receipt", error))?;
    file.flush()
        .map_err(|error| io_error("flushing pending receipt", error))?;
    file.sync_all()
        .map_err(|error| io_error("syncing pending receipt", error))?;
    Ok(())
}

fn read_install_receipt(
    path: &Path,
) -> Result<OrionCodeInstallReceiptV2, OrionCodeArchiveInstallerError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| io_error("inspecting install receipt", error))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(OrionCodeArchiveInstallerError::Receipt(
            "receipt must be a regular file".to_string(),
        ));
    }
    if metadata.len() > MAX_RECEIPT_BYTES {
        return Err(OrionCodeArchiveInstallerError::Receipt(
            "receipt exceeds its byte limit".to_string(),
        ));
    }
    let file = File::open(path).map_err(|error| io_error("opening install receipt", error))?;
    let mut bytes = Vec::new();
    file.take(MAX_RECEIPT_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("reading install receipt", error))?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(OrionCodeArchiveInstallerError::Receipt(
            "receipt exceeds its byte limit".to_string(),
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| OrionCodeArchiveInstallerError::Receipt(error.to_string()))
}

fn install_receipt(
    candidate: &OrionCodeCandidate,
    index_sequence: u64,
) -> OrionCodeInstallReceiptV2 {
    OrionCodeInstallReceiptV2 {
        version: candidate.version.to_string(),
        source: OrionCodeInstallSource::Archive,
        legacy_imported: false,
        target: Some(candidate.target_name.clone()),
        archive_sha256: Some(candidate.target.archive_sha256.to_ascii_lowercase()),
        command: Some(candidate.target.command.clone()),
        installed_at: Some(Utc::now()),
        index_sequence: Some(index_sequence),
    }
}

fn validate_receipt(
    receipt: &OrionCodeInstallReceiptV2,
    identity: &ValidatedCandidateIdentity,
    index_sequence: u64,
) -> Result<(), OrionCodeArchiveInstallerError> {
    let version = Version::parse(&receipt.version)
        .map_err(|error| OrionCodeArchiveInstallerError::Receipt(error.to_string()))?;
    if version.to_string() != receipt.version
        || receipt.version != identity.version
        || receipt.source != OrionCodeInstallSource::Archive
        || receipt.legacy_imported
        || receipt.target.as_deref() != Some(identity.target.as_str())
        || receipt
            .archive_sha256
            .as_deref()
            .is_none_or(|digest| !digest.eq_ignore_ascii_case(&identity.archive_sha256))
        || receipt.command.as_deref() != Some(identity.command.as_str())
        || receipt.installed_at.is_none()
        || receipt.index_sequence != Some(index_sequence)
    {
        return Err(OrionCodeArchiveInstallerError::ExistingState(
            "existing receipt does not match the exact candidate".to_string(),
        ));
    }
    let archive_sha256 = receipt.archive_sha256.as_deref().ok_or_else(|| {
        OrionCodeArchiveInstallerError::Receipt("archive receipt omitted SHA-256".to_string())
    })?;
    validate_sha256(archive_sha256).map_err(OrionCodeArchiveInstallerError::Receipt)?;
    let command = receipt.command.as_deref().ok_or_else(|| {
        OrionCodeArchiveInstallerError::Receipt("archive receipt omitted command".to_string())
    })?;
    validate_relative_artifact_path(command).map_err(OrionCodeArchiveInstallerError::Receipt)?;
    Ok(())
}

fn staged_identity(
    candidate: &OrionCodeCandidate,
    index_sequence: u64,
    artifact_root: PathBuf,
    receipt_path: PathBuf,
    reused_existing: bool,
) -> OrionCodeStagedIdentity {
    OrionCodeStagedIdentity {
        version: candidate.version.clone(),
        target: candidate.target_name.clone(),
        archive_sha256: candidate.target.archive_sha256.to_ascii_lowercase(),
        command: candidate.target.command.clone(),
        index_sequence,
        artifact_root,
        receipt_path,
        reused_existing,
    }
}

fn cleanup_owned_file(
    file_operations: &dyn InstallerFileOperations,
    canonical_root: &Path,
    path: &Path,
    label: &'static str,
) -> Result<(), String> {
    validate_cleanup_parent(canonical_root, path, label)?;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to inspect {label}: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(format!("refused to clean non-regular {label}"));
    }
    file_operations
        .remove_file(path)
        .map_err(|error| format!("failed to clean {label}: {error}"))
}

fn cleanup_owned_directory(
    file_operations: &dyn InstallerFileOperations,
    canonical_root: &Path,
    path: &Path,
    label: &'static str,
) -> Result<(), String> {
    validate_cleanup_parent(canonical_root, path, label)?;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to inspect {label}: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("refused to clean non-directory {label}"));
    }
    file_operations
        .remove_dir_all(path)
        .map_err(|error| format!("failed to clean {label}: {error}"))
}

fn validate_cleanup_parent(
    canonical_root: &Path,
    path: &Path,
    label: &'static str,
) -> Result<(), String> {
    if path == canonical_root || !path.starts_with(canonical_root) {
        return Err(format!("refused to clean {label} outside managed root"));
    }
    if path
        .strip_prefix(canonical_root)
        .ok()
        .is_none_or(|relative| {
            relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        })
    {
        return Err(format!("refused to clean unnormalized {label}"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("refused to clean parentless {label}"))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize {label} parent: {error}"))?;
    if canonical_parent != parent || !canonical_parent.starts_with(canonical_root) {
        return Err(format!("refused to clean escaped {label}"));
    }
    Ok(())
}

fn is_followable_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn io_error(
    operation: &'static str,
    error: impl std::fmt::Display,
) -> OrionCodeArchiveInstallerError {
    OrionCodeArchiveInstallerError::Io {
        operation,
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        io,
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
    };

    use chrono::{DateTime, Utc};
    use futures::AsyncRead;
    use http_client::{
        FakeHttpClient, RequestTimeout, Response,
        http::header::{CONTENT_LENGTH, LOCATION},
    };
    use orion_code_update::{
        ARTIFACT_MANIFEST_SCHEMA_VERSION, OrionCodeArtifactFileV1, OrionCodeArtifactManifestV1,
        OrionCodeReleaseStatus, OrionCodeSigningRequirement, OrionCodeUpdateChannel,
        OrionCodeUpdateReleaseV1, OrionCodeUpdateTargetV1, SUPPORTED_ACP_PROTOCOL,
    };
    use parking_lot::Mutex;
    use tempfile::TempDir;

    use super::*;

    const TEST_HOST: &str = "updates.example.invalid";
    const TEST_TARGET: &str = "darwin-aarch64";
    const TEST_VERSION: &str = "0.4.0";
    const TEST_INDEX_SEQUENCE: u64 = 42;

    #[derive(Default)]
    struct RecordingProgressReporter {
        updates: Mutex<Vec<OrionCodeDownloadProgress>>,
    }

    impl OrionCodeDownloadProgressReporter for RecordingProgressReporter {
        fn report(&self, progress: OrionCodeDownloadProgress) {
            self.updates.lock().push(progress);
        }
    }

    struct RecordingPlatformVerifier {
        calls: AtomicUsize,
        fail: bool,
    }

    impl OrionCodePlatformVerifier for RecordingPlatformVerifier {
        fn verify(
            &self,
            artifact_root: PathBuf,
            manifest: OrionCodeArtifactManifestV1,
            signing_requirement: OrionCodeSigningRequirement,
        ) -> BoxFuture<'static, Result<(), OrionCodePlatformVerificationError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = if self.fail {
                Err(OrionCodePlatformVerificationError::TrustMismatch {
                    component: "fixture".to_string(),
                    reason: "injected platform failure".to_string(),
                })
            } else {
                assert!(artifact_root.join(&manifest.command).is_file());
                assert_eq!(
                    signing_requirement,
                    OrionCodeSigningRequirement::DeveloperIdAndNotarized
                );
                Ok(())
            };
            Box::pin(async move { result })
        }
    }

    struct RecordingPreflight {
        calls: AtomicUsize,
        fail: bool,
    }

    impl OrionCodeUpdatePreflight for RecordingPreflight {
        fn verify(
            &self,
            artifact_root: PathBuf,
            candidate: OrionCodeCandidate,
        ) -> BoxFuture<'static, Result<(), OrionCodePreflightError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let fail = self.fail;
            Box::pin(async move {
                assert!(artifact_root.join(&candidate.target.command).is_file());
                if fail {
                    Err(OrionCodePreflightError::Failed {
                        diagnostic_code: "fixture_preflight_failed".to_string(),
                    })
                } else {
                    Ok(())
                }
            })
        }
    }

    struct ChunkedReader {
        bytes: Vec<u8>,
        offset: usize,
        chunk_bytes: usize,
    }

    impl AsyncRead for ChunkedReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            buffer: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            if self.offset == self.bytes.len() {
                return Poll::Ready(Ok(0));
            }
            let count = buffer
                .len()
                .min(self.chunk_bytes)
                .min(self.bytes.len() - self.offset);
            let end = self.offset + count;
            buffer[..count].copy_from_slice(&self.bytes[self.offset..end]);
            self.offset = end;
            Poll::Ready(Ok(count))
        }
    }

    struct CleanupFaultOperations;

    impl InstallerFileOperations for CleanupFaultOperations {
        fn rename(&self, source: &Path, destination: &Path) -> io::Result<()> {
            std::fs::rename(source, destination)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            if path
                .extension()
                .is_some_and(|extension| extension == "partial")
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected cleanup fault",
                ));
            }
            std::fs::remove_file(path)
        }

        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            std::fs::remove_dir_all(path)
        }

        #[cfg(unix)]
        fn sync_directory(&self, path: &Path) -> io::Result<()> {
            File::open(path)?.sync_all()
        }

        #[cfg(not(unix))]
        fn sync_directory(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }
    }

    struct Fixture {
        candidate: OrionCodeCandidate,
        archive: Vec<u8>,
    }

    fn fixture() -> Fixture {
        let files = vec![
            ("LICENSE", b"GPL-3.0\n".to_vec(), 0o644_u32),
            (
                "SBOM.cdx.json",
                br#"{"bomFormat":"CycloneDX"}"#.to_vec(),
                0o644,
            ),
            (
                "THIRD_PARTY_NOTICES",
                b"dependency notices\n".to_vec(),
                0o644,
            ),
            ("bin/orion-code-acp", b"fixture binary\n".to_vec(), 0o755),
        ];
        let manifest_files = files
            .iter()
            .map(|(path, bytes, mode)| OrionCodeArtifactFileV1 {
                path: (*path).to_string(),
                mode: *mode,
                bytes: bytes.len() as u64,
                sha256: sha256(bytes),
            })
            .collect::<Vec<_>>();
        let manifest = OrionCodeArtifactManifestV1 {
            schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION,
            version: TEST_VERSION.to_string(),
            git_sha: "a".repeat(40),
            target: TEST_TARGET.to_string(),
            built_at: DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
                .expect("fixture timestamp")
                .with_timezone(&Utc),
            acp_protocol: SUPPORTED_ACP_PROTOCOL,
            studio_version_requirement: ">=0.1.0,<1.0.0".to_string(),
            node_version: "22.22.0".to_string(),
            node_abi: "127".to_string(),
            native_modules: Vec::new(),
            command: "bin/orion-code-acp".to_string(),
            sbom_path: "SBOM.cdx.json".to_string(),
            sbom_sha256: sha256(&files[1].1),
            notices_path: "THIRD_PARTY_NOTICES".to_string(),
            notices_sha256: sha256(&files[2].1),
            files: manifest_files,
        };
        let manifest_bytes = serde_json::to_vec(&manifest).expect("serialize fixture manifest");
        let manifest_sha256 = sha256(&manifest_bytes);
        let mut zip_entries = files
            .into_iter()
            .map(|(path, bytes, mode)| (path.to_string(), bytes, 0o100000 | mode))
            .collect::<Vec<_>>();
        zip_entries.push(("manifest.json".to_string(), manifest_bytes, 0o100644));
        let archive = stored_zip(&zip_entries);
        let target = OrionCodeUpdateTargetV1 {
            archive_url: format!("https://{TEST_HOST}/orion-code/{TEST_VERSION}/{TEST_TARGET}.zip"),
            archive_sha256: sha256(&archive),
            archive_bytes: archive.len() as u64,
            format: OrionCodeArchiveFormat::Zip,
            command: "bin/orion-code-acp".to_string(),
            manifest_sha256,
            sbom_sha256: sha256(&zip_entries[1].1),
            signing_requirement: OrionCodeSigningRequirement::DeveloperIdAndNotarized,
        };
        let mut targets = BTreeMap::new();
        targets.insert(TEST_TARGET.to_string(), target.clone());
        let release = OrionCodeUpdateReleaseV1 {
            version: TEST_VERSION.to_string(),
            channel: OrionCodeUpdateChannel::Stable,
            status: OrionCodeReleaseStatus::Active,
            published_at: DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
                .expect("fixture timestamp")
                .with_timezone(&Utc),
            studio_version_requirement: ">=0.1.0,<1.0.0".to_string(),
            acp_protocol: SUPPORTED_ACP_PROTOCOL,
            rollout_basis_points: 10_000,
            rollout_salt: "fixture-rollout-salt".to_string(),
            rollback_to: None,
            release_notes_url: format!("https://{TEST_HOST}/releases/{TEST_VERSION}"),
            targets,
        };
        Fixture {
            candidate: OrionCodeCandidate {
                version: Version::parse(TEST_VERSION).expect("fixture version"),
                release,
                target,
                target_name: TEST_TARGET.to_string(),
                is_signed_rollback: false,
            },
            archive,
        }
    }

    fn installer(
        root: &Path,
        client: Arc<dyn HttpClient>,
        platform: Arc<RecordingPlatformVerifier>,
        preflight: Option<Arc<RecordingPreflight>>,
    ) -> OrionCodeArchiveInstaller {
        OrionCodeArchiveInstaller::new(
            root,
            client,
            [TEST_HOST.to_string()],
            platform,
            preflight.map(|preflight| preflight as Arc<dyn OrionCodeUpdatePreflight>),
        )
        .expect("fixture installer")
        .with_archive_limits(OrionCodeArchiveLimits {
            max_entries: 32,
            max_file_bytes: 16 * 1024,
            max_uncompressed_bytes: 64 * 1024,
        })
    }

    fn archive_client(archive: Vec<u8>) -> Arc<dyn HttpClient> {
        let archive = Arc::new(archive);
        FakeHttpClient::create(move |request| {
            assert_eq!(
                request.extensions().get::<RedirectPolicy>(),
                Some(&RedirectPolicy::NoFollow)
            );
            assert_eq!(
                request.extensions().get::<RequestTimeout>(),
                Some(&RequestTimeout(ARCHIVE_REQUEST_TIMEOUT))
            );
            assert_eq!(
                request
                    .headers()
                    .get(ACCEPT_ENCODING)
                    .and_then(|value| value.to_str().ok()),
                Some("identity")
            );
            let archive = archive.clone();
            async move {
                let length = archive.len();
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_LENGTH, length.to_string())
                    .body(AsyncBody::from_reader(ChunkedReader {
                        bytes: archive.as_ref().clone(),
                        offset: 0,
                        chunk_bytes: 17,
                    }))
                    .expect("fixture response"))
            }
        }) as Arc<dyn HttpClient>
    }

    fn response_client(
        status: StatusCode,
        content_length: Option<u64>,
        body: Vec<u8>,
        location: Option<&'static str>,
    ) -> Arc<dyn HttpClient> {
        FakeHttpClient::create(move |_| {
            let body = body.clone();
            async move {
                let mut response = Response::builder().status(status);
                if let Some(content_length) = content_length {
                    response = response.header(CONTENT_LENGTH, content_length.to_string());
                }
                if let Some(location) = location {
                    response = response.header(LOCATION, location);
                }
                Ok(response
                    .body(AsyncBody::from(body))
                    .expect("fixture response"))
            }
        }) as Arc<dyn HttpClient>
    }

    #[test]
    fn stages_streamed_archive_and_replays_exact_existing_receipt() {
        futures::executor::block_on(async {
            let temporary = TempDir::new().expect("temporary directory");
            let managed_root = temporary.path().join("managed");
            let fixture = fixture();
            let platform = Arc::new(RecordingPlatformVerifier {
                calls: AtomicUsize::new(0),
                fail: false,
            });
            let preflight = Arc::new(RecordingPreflight {
                calls: AtomicUsize::new(0),
                fail: false,
            });
            let installer = installer(
                &managed_root,
                archive_client(fixture.archive.clone()),
                platform.clone(),
                Some(preflight.clone()),
            );
            let progress_reporter = Arc::new(RecordingProgressReporter::default());

            let staged = installer
                .stage_with_progress(
                    fixture.candidate.clone(),
                    TEST_INDEX_SEQUENCE,
                    progress_reporter.clone(),
                )
                .await
                .expect("stage fixture");
            assert!(!staged.reused_existing);
            assert!(staged.artifact_root.join("bin/orion-code-acp").is_file());
            assert!(staged.receipt_path.is_file());
            assert_eq!(platform.calls.load(Ordering::SeqCst), 1);
            assert_eq!(preflight.calls.load(Ordering::SeqCst), 1);
            {
                let progress_updates = progress_reporter.updates.lock();
                assert_eq!(
                    progress_updates.first().copied(),
                    Some(OrionCodeDownloadProgress::new(
                        0,
                        fixture.archive.len() as u64,
                    ))
                );
                assert_eq!(
                    progress_updates.last().copied(),
                    Some(OrionCodeDownloadProgress::new(
                        fixture.archive.len() as u64,
                        fixture.archive.len() as u64,
                    ))
                );
                assert!(progress_updates.windows(2).all(|updates| {
                    updates[0].received_bytes() <= updates[1].received_bytes()
                        && updates[1].received_bytes() <= updates[1].total_bytes()
                }));
            }

            let replayed = installer
                .stage(fixture.candidate, TEST_INDEX_SEQUENCE)
                .await
                .expect("replay staged fixture");
            assert!(replayed.reused_existing);
            assert_eq!(replayed.artifact_root, staged.artifact_root);
            assert_eq!(platform.calls.load(Ordering::SeqCst), 2);
            assert_eq!(preflight.calls.load(Ordering::SeqCst), 2);
        });
    }

    #[test]
    fn download_progress_values_are_bounded_without_context() {
        let progress = OrionCodeDownloadProgress::new(u64::MAX, u64::MAX);

        assert_eq!(progress.received_bytes(), MAX_ARCHIVE_BYTES);
        assert_eq!(progress.total_bytes(), MAX_ARCHIVE_BYTES);
        assert_eq!(
            OrionCodeDownloadProgress::new(1, 0),
            OrionCodeDownloadProgress::new(0, 0)
        );
    }

    #[test]
    fn rejects_oversize_truncation_digest_mismatch_and_redirect_escape() {
        futures::executor::block_on(async {
            let cases: [(&str, fn(&Fixture) -> Arc<dyn HttpClient>, &str); 4] = [
                (
                    "oversize",
                    |fixture: &Fixture| {
                        response_client(
                            StatusCode::OK,
                            Some(fixture.archive.len() as u64 + 1),
                            Vec::new(),
                            None,
                        )
                    },
                    "Content-Length mismatch",
                ),
                (
                    "truncated",
                    |fixture: &Fixture| {
                        response_client(
                            StatusCode::OK,
                            Some(fixture.archive.len() as u64),
                            fixture.archive[..fixture.archive.len() - 1].to_vec(),
                            None,
                        )
                    },
                    "was truncated",
                ),
                (
                    "digest",
                    |fixture: &Fixture| {
                        let mut archive = fixture.archive.clone();
                        archive[0] ^= 1;
                        response_client(StatusCode::OK, Some(archive.len() as u64), archive, None)
                    },
                    "SHA-256 mismatch",
                ),
                (
                    "redirect",
                    |_fixture: &Fixture| {
                        response_client(
                            StatusCode::FOUND,
                            None,
                            Vec::new(),
                            Some("https://attacker.invalid/orion-code.zip"),
                        )
                    },
                    "not trusted",
                ),
            ];

            for (name, make_client, expected_error) in cases {
                let temporary = TempDir::new().expect("temporary directory");
                let managed_root = temporary.path().join(name);
                let current = managed_root.join("current");
                let previous = managed_root.join("previous");
                std::fs::create_dir_all(&current).expect("current sentinel directory");
                std::fs::create_dir_all(&previous).expect("previous sentinel directory");
                std::fs::write(current.join("sentinel"), b"current").expect("current sentinel");
                std::fs::write(previous.join("sentinel"), b"previous").expect("previous sentinel");
                let fixture = fixture();
                let installer = installer(
                    &managed_root,
                    make_client(&fixture),
                    Arc::new(RecordingPlatformVerifier {
                        calls: AtomicUsize::new(0),
                        fail: false,
                    }),
                    Some(Arc::new(RecordingPreflight {
                        calls: AtomicUsize::new(0),
                        fail: false,
                    })),
                );
                let error = installer
                    .stage(fixture.candidate, TEST_INDEX_SEQUENCE)
                    .await
                    .expect_err("reject unsafe response");
                assert!(
                    error.to_string().contains(expected_error),
                    "{name}: {error}"
                );
                assert_eq!(
                    std::fs::read(current.join("sentinel")).ok(),
                    Some(b"current".to_vec())
                );
                assert_eq!(
                    std::fs::read(previous.join("sentinel")).ok(),
                    Some(b"previous".to_vec())
                );
                assert!(
                    !managed_root
                        .join("versions")
                        .join(TEST_VERSION)
                        .join(TEST_TARGET)
                        .exists()
                );
            }
        });
    }

    #[test]
    fn requires_real_preflight_and_rejects_receipt_mismatch() {
        futures::executor::block_on(async {
            let temporary = TempDir::new().expect("temporary directory");
            let fixture = fixture();
            let platform = Arc::new(RecordingPlatformVerifier {
                calls: AtomicUsize::new(0),
                fail: false,
            });
            let missing = installer(
                &temporary.path().join("missing-preflight"),
                archive_client(fixture.archive.clone()),
                platform.clone(),
                None,
            );
            assert!(matches!(
                missing
                    .stage(fixture.candidate.clone(), TEST_INDEX_SEQUENCE)
                    .await,
                Err(OrionCodeArchiveInstallerError::MissingPreflight)
            ));

            let managed_root = temporary.path().join("receipt-replay");
            let complete = installer(
                &managed_root,
                archive_client(fixture.archive.clone()),
                platform,
                Some(Arc::new(RecordingPreflight {
                    calls: AtomicUsize::new(0),
                    fail: false,
                })),
            );
            let staged = complete
                .stage(fixture.candidate.clone(), TEST_INDEX_SEQUENCE)
                .await
                .expect("initial stage");
            let mut receipt = read_install_receipt(&staged.receipt_path).expect("read receipt");
            receipt.index_sequence = Some(TEST_INDEX_SEQUENCE + 1);
            std::fs::write(
                &staged.receipt_path,
                serde_json::to_vec(&receipt).expect("serialize mismatched receipt"),
            )
            .expect("replace receipt fixture");
            assert!(matches!(
                complete.stage(fixture.candidate, TEST_INDEX_SEQUENCE).await,
                Err(OrionCodeArchiveInstallerError::ExistingState(_))
            ));
            assert!(staged.artifact_root.is_dir());
        });
    }

    #[test]
    fn platform_and_preflight_failures_never_promote_staging() {
        futures::executor::block_on(async {
            for (name, platform_fails, preflight_fails, expected_error) in [
                ("platform", true, false, "platform verification failed"),
                ("preflight", false, true, "ACP preflight failed"),
            ] {
                let temporary = TempDir::new().expect("temporary directory");
                let managed_root = temporary.path().join(name);
                let fixture = fixture();
                let installer = installer(
                    &managed_root,
                    archive_client(fixture.archive.clone()),
                    Arc::new(RecordingPlatformVerifier {
                        calls: AtomicUsize::new(0),
                        fail: platform_fails,
                    }),
                    Some(Arc::new(RecordingPreflight {
                        calls: AtomicUsize::new(0),
                        fail: preflight_fails,
                    })),
                );
                let error = installer
                    .stage(fixture.candidate, TEST_INDEX_SEQUENCE)
                    .await
                    .expect_err("verification gate failure");
                assert!(
                    error.to_string().contains(expected_error),
                    "{name}: {error}"
                );
                assert!(
                    !managed_root
                        .join("versions")
                        .join(TEST_VERSION)
                        .join(TEST_TARGET)
                        .exists()
                );
                assert!(collect_files_with_extension(&managed_root, "partial").is_empty());
            }
        });
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_managed_paths_and_target_escape() {
        use std::os::unix::fs::symlink;

        futures::executor::block_on(async {
            let temporary = TempDir::new().expect("temporary directory");
            let managed_root = temporary.path().join("managed");
            let outside = temporary.path().join("outside");
            std::fs::create_dir_all(&managed_root).expect("managed root");
            std::fs::create_dir_all(&outside).expect("outside root");
            symlink(&outside, managed_root.join("downloads")).expect("downloads symlink");
            let fixture = fixture();
            let archive_installer = installer(
                &managed_root,
                archive_client(fixture.archive.clone()),
                Arc::new(RecordingPlatformVerifier {
                    calls: AtomicUsize::new(0),
                    fail: false,
                }),
                Some(Arc::new(RecordingPreflight {
                    calls: AtomicUsize::new(0),
                    fail: false,
                })),
            );
            assert!(matches!(
                archive_installer
                    .stage(fixture.candidate.clone(), TEST_INDEX_SEQUENCE)
                    .await,
                Err(OrionCodeArchiveInstallerError::UnsafeManagedPath(_))
            ));
            assert!(
                std::fs::read_dir(&outside)
                    .expect("outside directory")
                    .next()
                    .is_none()
            );

            let safe_root = temporary.path().join("safe");
            let mut escaped = fixture.candidate;
            escaped.target_name = "../outside".to_string();
            let safe_installer = installer(
                &safe_root,
                response_client(StatusCode::OK, Some(0), Vec::new(), None),
                Arc::new(RecordingPlatformVerifier {
                    calls: AtomicUsize::new(0),
                    fail: false,
                }),
                Some(Arc::new(RecordingPreflight {
                    calls: AtomicUsize::new(0),
                    fail: false,
                })),
            );
            assert!(matches!(
                safe_installer.stage(escaped, TEST_INDEX_SEQUENCE).await,
                Err(OrionCodeArchiveInstallerError::InvalidCandidate(_))
            ));
        });
    }

    #[test]
    fn reports_cleanup_fault_without_touching_current_or_previous() {
        futures::executor::block_on(async {
            let temporary = TempDir::new().expect("temporary directory");
            let managed_root = temporary.path().join("managed");
            let current = managed_root.join("current");
            let previous = managed_root.join("previous");
            std::fs::create_dir_all(&current).expect("current sentinel directory");
            std::fs::create_dir_all(&previous).expect("previous sentinel directory");
            std::fs::write(current.join("sentinel"), b"current").expect("current sentinel");
            std::fs::write(previous.join("sentinel"), b"previous").expect("previous sentinel");
            let fixture = fixture();
            let mut corrupted = fixture.archive.clone();
            corrupted[0] ^= 1;
            let installer = installer(
                &managed_root,
                response_client(
                    StatusCode::OK,
                    Some(corrupted.len() as u64),
                    corrupted,
                    None,
                ),
                Arc::new(RecordingPlatformVerifier {
                    calls: AtomicUsize::new(0),
                    fail: false,
                }),
                Some(Arc::new(RecordingPreflight {
                    calls: AtomicUsize::new(0),
                    fail: false,
                })),
            )
            .with_file_operations(Arc::new(CleanupFaultOperations));
            let error = installer
                .stage(fixture.candidate, TEST_INDEX_SEQUENCE)
                .await
                .expect_err("injected cleanup failure");
            assert!(matches!(
                &error,
                OrionCodeArchiveInstallerError::Cleanup { .. }
            ));
            assert!(error.to_string().contains("injected cleanup fault"));
            assert_eq!(
                std::fs::read(current.join("sentinel")).ok(),
                Some(b"current".to_vec())
            );
            assert_eq!(
                std::fs::read(previous.join("sentinel")).ok(),
                Some(b"previous".to_vec())
            );
            let partials = collect_files_with_extension(&managed_root, "partial");
            assert_eq!(partials.len(), 1);
            assert!(partials[0].starts_with(managed_root.join("downloads")));
        });
    }

    fn collect_files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut matches = Vec::new();
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|value| value == extension) {
                    matches.push(path);
                }
            }
        }
        matches
    }

    fn sha256(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn stored_zip(entries: &[(String, Vec<u8>, u32)]) -> Vec<u8> {
        struct CentralEntry {
            name: Vec<u8>,
            crc32: u32,
            bytes: u32,
            mode: u32,
            offset: u32,
        }

        let mut output = Vec::new();
        let mut central_entries = Vec::new();
        for (name, bytes, mode) in entries {
            let name = name.as_bytes().to_vec();
            let offset = u32::try_from(output.len()).expect("fixture ZIP offset");
            let bytes_length = u32::try_from(bytes.len()).expect("fixture ZIP entry length");
            let crc32 = crc32(bytes);
            push_u32(&mut output, 0x0403_4b50);
            push_u16(&mut output, 20);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u32(&mut output, crc32);
            push_u32(&mut output, bytes_length);
            push_u32(&mut output, bytes_length);
            push_u16(
                &mut output,
                u16::try_from(name.len()).expect("fixture ZIP name length"),
            );
            push_u16(&mut output, 0);
            output.extend_from_slice(&name);
            output.extend_from_slice(bytes);
            central_entries.push(CentralEntry {
                name,
                crc32,
                bytes: bytes_length,
                mode: *mode,
                offset,
            });
        }

        let central_offset = u32::try_from(output.len()).expect("fixture central offset");
        for entry in &central_entries {
            push_u32(&mut output, 0x0201_4b50);
            push_u16(&mut output, 0x0314);
            push_u16(&mut output, 20);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u32(&mut output, entry.crc32);
            push_u32(&mut output, entry.bytes);
            push_u32(&mut output, entry.bytes);
            push_u16(
                &mut output,
                u16::try_from(entry.name.len()).expect("fixture ZIP name length"),
            );
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u16(&mut output, 0);
            push_u32(&mut output, entry.mode << 16);
            push_u32(&mut output, entry.offset);
            output.extend_from_slice(&entry.name);
        }
        let central_bytes = u32::try_from(output.len()).expect("fixture ZIP size") - central_offset;
        let entry_count = u16::try_from(central_entries.len()).expect("fixture ZIP entry count");
        push_u32(&mut output, 0x0605_4b50);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, entry_count);
        push_u16(&mut output, entry_count);
        push_u32(&mut output, central_bytes);
        push_u32(&mut output, central_offset);
        push_u16(&mut output, 0);
        output
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    fn push_u16(output: &mut Vec<u8>, value: u16) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(output: &mut Vec<u8>, value: u32) {
        output.extend_from_slice(&value.to_le_bytes());
    }
}
