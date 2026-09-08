use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use async_zip::base::read::seek::ZipFileReader;
use futures::{AsyncReadExt as _, AsyncWriteExt as _, io::BufReader as AsyncBufReader};
use semver::{Version, VersionReq};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use walkdir::WalkDir;

use crate::{
    ARTIFACT_MANIFEST_SCHEMA_VERSION, MAX_ARCHIVE_BYTES, OrionCodeArtifactManifestV1,
    SUPPORTED_ACP_PROTOCOL, validate_relative_artifact_path, validate_sha256, validate_target_name,
};

const MANIFEST_FILE_NAME: &str = "manifest.json";
const LICENSE_FILE_NAME: &str = "LICENSE";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const UNIX_FILE_TYPE_MASK: u16 = 0o170000;
const UNIX_DIRECTORY_TYPE: u16 = 0o040000;
const UNIX_REGULAR_FILE_TYPE: u16 = 0o100000;
const UNIX_SPECIAL_MODE_MASK: u16 = 0o7000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrionCodeArchiveLimits {
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_uncompressed_bytes: u64,
}

impl Default for OrionCodeArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 200_000,
            max_file_bytes: 2 * 1024 * 1024 * 1024,
            max_uncompressed_bytes: 4 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedOrionCodeArtifact {
    pub manifest: OrionCodeArtifactManifestV1,
    pub manifest_sha256: String,
    pub file_count: usize,
    pub total_file_bytes: u64,
}

#[derive(Debug, Error)]
pub enum OrionCodeArchiveVerificationError {
    #[error("invalid Orion Code archive input: {0}")]
    InvalidInput(String),
    #[error("failed to access Orion Code archive path {path}: {reason}")]
    Io { path: String, reason: String },
    #[error("Orion Code archive size mismatch: expected {expected} bytes, found {actual}")]
    ArchiveSizeMismatch { expected: u64, actual: u64 },
    #[error("Orion Code archive SHA-256 mismatch")]
    ArchiveDigestMismatch,
    #[error("invalid Orion Code ZIP archive: {0}")]
    InvalidZip(String),
    #[error("unsafe Orion Code ZIP entry {path:?}: {reason}")]
    UnsafeEntry { path: String, reason: String },
    #[error("Orion Code ZIP expands beyond its {limit} byte limit")]
    ExpandedSizeLimit { limit: u64 },
    #[error("Orion Code ZIP contains more than {limit} entries")]
    EntryCountLimit { limit: usize },
    #[error("Orion Code staging destination already exists: {0}")]
    DestinationExists(String),
    #[error("failed to clean incomplete Orion Code staging directory after {operation}: {cleanup}")]
    Cleanup { operation: String, cleanup: String },
    #[error("invalid Orion Code artifact manifest: {0}")]
    InvalidManifest(String),
    #[error("Orion Code artifact file mismatch for {path}: {reason}")]
    FileMismatch { path: String, reason: String },
}

#[derive(Clone, Debug)]
struct ValidatedZipEntry {
    index: usize,
    path: String,
    is_directory: bool,
    bytes: u64,
    mode: u16,
}

pub fn verify_orion_code_archive(
    archive_path: &Path,
    expected_bytes: u64,
    expected_sha256: &str,
) -> Result<(), OrionCodeArchiveVerificationError> {
    if expected_bytes == 0 || expected_bytes > MAX_ARCHIVE_BYTES {
        return Err(OrionCodeArchiveVerificationError::InvalidInput(format!(
            "expected archive size must be in 1..={MAX_ARCHIVE_BYTES}"
        )));
    }
    validate_sha256(expected_sha256).map_err(OrionCodeArchiveVerificationError::InvalidInput)?;
    let metadata =
        std::fs::symlink_metadata(archive_path).map_err(|error| io_error(archive_path, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(OrionCodeArchiveVerificationError::InvalidInput(
            "archive path must be a regular file, not a link".to_string(),
        ));
    }
    if metadata.len() != expected_bytes {
        return Err(OrionCodeArchiveVerificationError::ArchiveSizeMismatch {
            expected: expected_bytes,
            actual: metadata.len(),
        });
    }

    let digest = sha256_file(archive_path, expected_bytes)?;
    if !digest.eq_ignore_ascii_case(expected_sha256) {
        return Err(OrionCodeArchiveVerificationError::ArchiveDigestMismatch);
    }
    Ok(())
}

pub async fn extract_orion_code_zip(
    archive_path: &Path,
    destination: &Path,
    limits: OrionCodeArchiveLimits,
) -> Result<(), OrionCodeArchiveVerificationError> {
    validate_limits(limits)?;
    let destination_name = destination.file_name().ok_or_else(|| {
        OrionCodeArchiveVerificationError::InvalidInput(
            "staging destination must have a final path component".to_string(),
        )
    })?;
    let parent = destination.parent().ok_or_else(|| {
        OrionCodeArchiveVerificationError::InvalidInput(
            "staging destination must have a parent".to_string(),
        )
    })?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| io_error(parent, error))?;
    let canonical_destination = canonical_parent.join(destination_name);
    match std::fs::symlink_metadata(&canonical_destination) {
        Ok(_) => {
            return Err(OrionCodeArchiveVerificationError::DestinationExists(
                canonical_destination.display().to_string(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(&canonical_destination, error)),
    }

    let archive = async_fs::File::open(archive_path)
        .await
        .map_err(|error| io_error(archive_path, error))?;
    let mut reader = ZipFileReader::new(AsyncBufReader::new(archive))
        .await
        .map_err(|error| OrionCodeArchiveVerificationError::InvalidZip(error.to_string()))?;
    let entries = validate_zip_entries(&reader, limits)?;

    async_fs::create_dir(&canonical_destination)
        .await
        .map_err(|error| io_error(&canonical_destination, error))?;
    let extraction = extract_validated_entries(&mut reader, &canonical_destination, &entries).await;
    if let Err(error) = extraction {
        return match async_fs::remove_dir_all(&canonical_destination).await {
            Ok(()) => Err(error),
            Err(cleanup) => Err(OrionCodeArchiveVerificationError::Cleanup {
                operation: error.to_string(),
                cleanup: cleanup.to_string(),
            }),
        };
    }
    Ok(())
}

pub fn verify_orion_code_artifact(
    artifact_root: &Path,
    expected_version: &str,
    expected_target: &str,
    expected_command: &str,
    expected_manifest_sha256: &str,
    expected_sbom_sha256: &str,
    limits: OrionCodeArchiveLimits,
) -> Result<VerifiedOrionCodeArtifact, OrionCodeArchiveVerificationError> {
    validate_limits(limits)?;
    validate_exact_semver(expected_version)?;
    validate_target_name(expected_target)
        .map_err(OrionCodeArchiveVerificationError::InvalidInput)?;
    validate_relative_artifact_path(expected_command)
        .map_err(OrionCodeArchiveVerificationError::InvalidInput)?;
    validate_sha256(expected_manifest_sha256)
        .map_err(OrionCodeArchiveVerificationError::InvalidInput)?;
    validate_sha256(expected_sbom_sha256)
        .map_err(OrionCodeArchiveVerificationError::InvalidInput)?;

    let root_metadata =
        std::fs::symlink_metadata(artifact_root).map_err(|error| io_error(artifact_root, error))?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(OrionCodeArchiveVerificationError::InvalidInput(
            "artifact root must be a real directory".to_string(),
        ));
    }

    let manifest_path = artifact_root.join(MANIFEST_FILE_NAME);
    let manifest_bytes = read_bounded_file(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest_sha256 = lowercase_sha256(&manifest_bytes);
    if !manifest_sha256.eq_ignore_ascii_case(expected_manifest_sha256) {
        return Err(OrionCodeArchiveVerificationError::FileMismatch {
            path: MANIFEST_FILE_NAME.to_string(),
            reason: "manifest SHA-256 does not match the signed update target".to_string(),
        });
    }
    let manifest: OrionCodeArtifactManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| OrionCodeArchiveVerificationError::InvalidManifest(error.to_string()))?;
    validate_manifest_identity(
        &manifest,
        expected_version,
        expected_target,
        expected_command,
        limits,
    )?;

    let expected_files = manifest_file_map(&manifest)?;
    let (file_count, total_file_bytes) =
        verify_extracted_files(artifact_root, &expected_files, limits)?;
    verify_manifest_bound_files(
        artifact_root,
        &manifest,
        expected_sbom_sha256,
        &expected_files,
    )?;

    Ok(VerifiedOrionCodeArtifact {
        manifest,
        manifest_sha256,
        file_count,
        total_file_bytes,
    })
}

fn validate_limits(
    limits: OrionCodeArchiveLimits,
) -> Result<(), OrionCodeArchiveVerificationError> {
    if limits.max_entries == 0
        || limits.max_file_bytes == 0
        || limits.max_uncompressed_bytes == 0
        || limits.max_file_bytes > limits.max_uncompressed_bytes
    {
        return Err(OrionCodeArchiveVerificationError::InvalidInput(
            "archive limits must be non-zero and max_file_bytes must not exceed total bytes"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_zip_entries<R>(
    reader: &ZipFileReader<R>,
    limits: OrionCodeArchiveLimits,
) -> Result<Vec<ValidatedZipEntry>, OrionCodeArchiveVerificationError>
where
    R: futures::AsyncBufRead + futures::AsyncSeek + Unpin,
{
    let archive_entries = reader.file().entries();
    if archive_entries.len() > limits.max_entries {
        return Err(OrionCodeArchiveVerificationError::EntryCountLimit {
            limit: limits.max_entries,
        });
    }
    let mut entries = Vec::with_capacity(archive_entries.len());
    let mut paths = BTreeMap::<String, bool>::new();
    let mut case_folded_paths = HashSet::new();
    let mut total_bytes = 0_u64;

    for (index, entry) in archive_entries.iter().enumerate() {
        let raw_path = entry.filename().as_str().map_err(|error| {
            OrionCodeArchiveVerificationError::UnsafeEntry {
                path: "<non-UTF-8>".to_string(),
                reason: error.to_string(),
            }
        })?;
        let is_directory =
            entry
                .dir()
                .map_err(|error| OrionCodeArchiveVerificationError::UnsafeEntry {
                    path: raw_path.to_string(),
                    reason: error.to_string(),
                })?;
        let normalized_path = if is_directory {
            raw_path.strip_suffix('/').unwrap_or(raw_path)
        } else {
            raw_path
        };
        validate_relative_artifact_path(normalized_path).map_err(|reason| {
            OrionCodeArchiveVerificationError::UnsafeEntry {
                path: raw_path.to_string(),
                reason,
            }
        })?;
        if paths.contains_key(normalized_path)
            || !case_folded_paths.insert(normalized_path.to_ascii_lowercase())
        {
            return Err(OrionCodeArchiveVerificationError::UnsafeEntry {
                path: normalized_path.to_string(),
                reason: "duplicate or case-colliding entry".to_string(),
            });
        }
        validate_path_conflicts(&paths, normalized_path, is_directory)?;

        let bytes = entry.uncompressed_size();
        if bytes > limits.max_file_bytes {
            return Err(OrionCodeArchiveVerificationError::UnsafeEntry {
                path: normalized_path.to_string(),
                reason: format!("entry exceeds {} bytes", limits.max_file_bytes),
            });
        }
        total_bytes = total_bytes.checked_add(bytes).ok_or(
            OrionCodeArchiveVerificationError::ExpandedSizeLimit {
                limit: limits.max_uncompressed_bytes,
            },
        )?;
        if total_bytes > limits.max_uncompressed_bytes {
            return Err(OrionCodeArchiveVerificationError::ExpandedSizeLimit {
                limit: limits.max_uncompressed_bytes,
            });
        }

        let mode = validated_zip_mode(normalized_path, entry.unix_permissions(), is_directory)?;
        paths.insert(normalized_path.to_string(), is_directory);
        entries.push(ValidatedZipEntry {
            index,
            path: normalized_path.to_string(),
            is_directory,
            bytes,
            mode,
        });
    }
    Ok(entries)
}

fn validate_path_conflicts(
    paths: &BTreeMap<String, bool>,
    path: &str,
    is_directory: bool,
) -> Result<(), OrionCodeArchiveVerificationError> {
    let mut ancestor = String::new();
    let mut segments = path.split('/').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            break;
        }
        if !ancestor.is_empty() {
            ancestor.push('/');
        }
        ancestor.push_str(segment);
        if paths
            .get(&ancestor)
            .is_some_and(|ancestor_is_directory| !ancestor_is_directory)
        {
            return Err(OrionCodeArchiveVerificationError::UnsafeEntry {
                path: path.to_string(),
                reason: format!("parent {ancestor:?} is already a file"),
            });
        }
    }
    if !is_directory {
        let descendant_prefix = format!("{path}/");
        if paths
            .range(descendant_prefix.clone()..)
            .next()
            .is_some_and(|(existing, _)| existing.starts_with(&descendant_prefix))
        {
            return Err(OrionCodeArchiveVerificationError::UnsafeEntry {
                path: path.to_string(),
                reason: "file conflicts with an existing descendant".to_string(),
            });
        }
    }
    Ok(())
}

fn validated_zip_mode(
    path: &str,
    archive_mode: Option<u16>,
    is_directory: bool,
) -> Result<u16, OrionCodeArchiveVerificationError> {
    let mode = archive_mode.unwrap_or(if is_directory { 0o755 } else { 0o644 });
    let file_type = mode & UNIX_FILE_TYPE_MASK;
    let expected_type = if is_directory {
        UNIX_DIRECTORY_TYPE
    } else {
        UNIX_REGULAR_FILE_TYPE
    };
    if (file_type != 0 && file_type != expected_type) || mode & UNIX_SPECIAL_MODE_MASK != 0 {
        return Err(OrionCodeArchiveVerificationError::UnsafeEntry {
            path: path.to_string(),
            reason: "links, devices, sockets, and special permission bits are forbidden"
                .to_string(),
        });
    }
    Ok(if is_directory {
        0o755
    } else if mode & 0o111 != 0 {
        0o755
    } else {
        0o644
    })
}

async fn extract_validated_entries<R>(
    reader: &mut ZipFileReader<R>,
    destination: &Path,
    entries: &[ValidatedZipEntry],
) -> Result<(), OrionCodeArchiveVerificationError>
where
    R: futures::AsyncBufRead + futures::AsyncSeek + Unpin,
{
    for entry in entries {
        let output_path = destination.join(Path::new(&entry.path));
        if entry.is_directory {
            async_fs::create_dir_all(&output_path)
                .await
                .map_err(|error| io_error(&output_path, error))?;
            set_sanitized_permissions(&output_path, entry.mode).await?;
            continue;
        }
        let parent =
            output_path
                .parent()
                .ok_or_else(|| OrionCodeArchiveVerificationError::UnsafeEntry {
                    path: entry.path.clone(),
                    reason: "file has no parent".to_string(),
                })?;
        async_fs::create_dir_all(parent)
            .await
            .map_err(|error| io_error(parent, error))?;
        let mut output = async_fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .await
            .map_err(|error| io_error(&output_path, error))?;
        let mut entry_reader = reader
            .reader_with_entry(entry.index)
            .await
            .map_err(|error| OrionCodeArchiveVerificationError::InvalidZip(error.to_string()))?;
        let mut bounded_reader = (&mut entry_reader).take(entry.bytes.saturating_add(1));
        let copied = futures::io::copy(&mut bounded_reader, &mut output)
            .await
            .map_err(|error| io_error(&output_path, error))?;
        if copied != entry.bytes {
            return Err(OrionCodeArchiveVerificationError::FileMismatch {
                path: entry.path.clone(),
                reason: format!(
                    "ZIP metadata declared {} bytes but extraction produced {copied}",
                    entry.bytes
                ),
            });
        }
        output
            .flush()
            .await
            .map_err(|error| io_error(&output_path, error))?;
        output
            .sync_all()
            .await
            .map_err(|error| io_error(&output_path, error))?;
        set_sanitized_permissions(&output_path, entry.mode).await?;
    }
    Ok(())
}

#[cfg(unix)]
async fn set_sanitized_permissions(
    path: &Path,
    mode: u16,
) -> Result<(), OrionCodeArchiveVerificationError> {
    use std::os::unix::fs::PermissionsExt as _;

    async_fs::set_permissions(path, std::fs::Permissions::from_mode(u32::from(mode)))
        .await
        .map_err(|error| io_error(path, error))
}

#[cfg(not(unix))]
async fn set_sanitized_permissions(
    _path: &Path,
    _mode: u16,
) -> Result<(), OrionCodeArchiveVerificationError> {
    Ok(())
}

fn validate_manifest_identity(
    manifest: &OrionCodeArtifactManifestV1,
    expected_version: &str,
    expected_target: &str,
    expected_command: &str,
    limits: OrionCodeArchiveLimits,
) -> Result<(), OrionCodeArchiveVerificationError> {
    if manifest.schema_version != ARTIFACT_MANIFEST_SCHEMA_VERSION {
        return Err(invalid_manifest(format!(
            "unsupported schema version {}",
            manifest.schema_version
        )));
    }
    validate_exact_semver(&manifest.version)?;
    if manifest.version != expected_version {
        return Err(invalid_manifest(
            "version does not match the signed update target",
        ));
    }
    if (manifest.git_sha.len() != 40 && manifest.git_sha.len() != 64)
        || !manifest
            .git_sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid_manifest(
            "git_sha must be a full 40 or 64 digit hex object ID",
        ));
    }
    validate_target_name(&manifest.target).map_err(invalid_manifest)?;
    if manifest.target != expected_target {
        return Err(invalid_manifest(
            "target does not match the signed update target",
        ));
    }
    if manifest.acp_protocol != SUPPORTED_ACP_PROTOCOL {
        return Err(invalid_manifest(
            "ACP protocol is not supported by this Studio",
        ));
    }
    VersionReq::parse(&manifest.studio_version_requirement)
        .map_err(|error| invalid_manifest(format!("invalid Studio requirement: {error}")))?;
    if manifest.node_version.trim().is_empty() || manifest.node_abi.trim().is_empty() {
        return Err(invalid_manifest(
            "embedded Node version and ABI are required",
        ));
    }
    validate_relative_artifact_path(&manifest.command).map_err(invalid_manifest)?;
    if manifest.command != expected_command {
        return Err(invalid_manifest(
            "entry command does not match the signed update target",
        ));
    }
    for path in [&manifest.sbom_path, &manifest.notices_path] {
        validate_relative_artifact_path(path).map_err(invalid_manifest)?;
    }
    for digest in [&manifest.sbom_sha256, &manifest.notices_sha256] {
        validate_sha256(digest).map_err(invalid_manifest)?;
    }
    if manifest.files.is_empty() || manifest.files.len() > limits.max_entries {
        return Err(invalid_manifest(
            "file list is empty or exceeds the entry limit",
        ));
    }
    let mut native_modules = HashSet::new();
    let mut previous_module: Option<&str> = None;
    for native_module in &manifest.native_modules {
        validate_relative_artifact_path(native_module).map_err(invalid_manifest)?;
        if previous_module.is_some_and(|previous| previous >= native_module.as_str())
            || !native_modules.insert(native_module)
        {
            return Err(invalid_manifest("native_modules must be sorted and unique"));
        }
        previous_module = Some(native_module);
    }
    Ok(())
}

fn manifest_file_map(
    manifest: &OrionCodeArtifactManifestV1,
) -> Result<BTreeMap<String, &crate::OrionCodeArtifactFileV1>, OrionCodeArchiveVerificationError> {
    let mut files = BTreeMap::new();
    let mut case_folded_paths = HashSet::new();
    let mut previous_path: Option<&str> = None;
    for file in &manifest.files {
        validate_relative_artifact_path(&file.path).map_err(invalid_manifest)?;
        if file.path == MANIFEST_FILE_NAME {
            return Err(invalid_manifest("manifest.json cannot hash itself"));
        }
        if previous_path.is_some_and(|previous| previous >= file.path.as_str()) {
            return Err(invalid_manifest("files must be sorted by path and unique"));
        }
        if !case_folded_paths.insert(file.path.to_ascii_lowercase()) {
            return Err(invalid_manifest("files contain a case-colliding path"));
        }
        if file.mode & !0o777 != 0 || file.mode & 0o444 == 0 {
            return Err(invalid_manifest(format!(
                "file {} has unsafe permissions",
                file.path
            )));
        }
        validate_sha256(&file.sha256).map_err(invalid_manifest)?;
        files.insert(file.path.clone(), file);
        previous_path = Some(&file.path);
    }
    for required in [
        manifest.command.as_str(),
        manifest.sbom_path.as_str(),
        manifest.notices_path.as_str(),
        LICENSE_FILE_NAME,
    ] {
        if !files.contains_key(required) {
            return Err(invalid_manifest(format!(
                "required file {required:?} is not listed"
            )));
        }
    }
    if manifest
        .native_modules
        .iter()
        .any(|native_module| !files.contains_key(native_module))
    {
        return Err(invalid_manifest(
            "a native module is missing from the file list",
        ));
    }
    if files
        .get(&manifest.command)
        .is_some_and(|command| command.mode & 0o111 == 0)
    {
        return Err(invalid_manifest("entry command is not executable"));
    }
    Ok(files)
}

fn verify_extracted_files(
    artifact_root: &Path,
    expected_files: &BTreeMap<String, &crate::OrionCodeArtifactFileV1>,
    limits: OrionCodeArchiveLimits,
) -> Result<(usize, u64), OrionCodeArchiveVerificationError> {
    let mut observed = HashSet::new();
    let mut total_bytes = 0_u64;
    for entry in WalkDir::new(artifact_root).follow_links(false) {
        let entry = entry.map_err(|error| OrionCodeArchiveVerificationError::Io {
            path: artifact_root.display().to_string(),
            reason: error.to_string(),
        })?;
        if entry.path() == artifact_root {
            continue;
        }
        if entry.file_type().is_symlink()
            || (!entry.file_type().is_dir() && !entry.file_type().is_file())
        {
            return Err(OrionCodeArchiveVerificationError::FileMismatch {
                path: entry.path().display().to_string(),
                reason: "links and non-regular files are forbidden".to_string(),
            });
        }
        if entry.file_type().is_dir() {
            continue;
        }
        let relative = entry.path().strip_prefix(artifact_root).map_err(|error| {
            OrionCodeArchiveVerificationError::FileMismatch {
                path: entry.path().display().to_string(),
                reason: error.to_string(),
            }
        })?;
        let path = slash_path(relative)?;
        if path == MANIFEST_FILE_NAME {
            continue;
        }
        let expected = expected_files.get(&path).ok_or_else(|| {
            OrionCodeArchiveVerificationError::FileMismatch {
                path: path.clone(),
                reason: "file is not declared by manifest".to_string(),
            }
        })?;
        if !observed.insert(path.clone()) {
            return Err(OrionCodeArchiveVerificationError::FileMismatch {
                path,
                reason: "file was observed more than once".to_string(),
            });
        }
        let metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|error| io_error(entry.path(), error))?;
        if metadata.len() != expected.bytes || metadata.len() > limits.max_file_bytes {
            return Err(OrionCodeArchiveVerificationError::FileMismatch {
                path,
                reason: format!(
                    "expected {} bytes, found {}",
                    expected.bytes,
                    metadata.len()
                ),
            });
        }
        total_bytes = total_bytes.checked_add(metadata.len()).ok_or(
            OrionCodeArchiveVerificationError::ExpandedSizeLimit {
                limit: limits.max_uncompressed_bytes,
            },
        )?;
        if total_bytes > limits.max_uncompressed_bytes {
            return Err(OrionCodeArchiveVerificationError::ExpandedSizeLimit {
                limit: limits.max_uncompressed_bytes,
            });
        }
        let digest = sha256_file(entry.path(), expected.bytes)?;
        if !digest.eq_ignore_ascii_case(&expected.sha256) {
            return Err(OrionCodeArchiveVerificationError::FileMismatch {
                path,
                reason: "SHA-256 mismatch".to_string(),
            });
        }
        verify_file_mode(entry.path(), expected.mode)?;
    }
    if observed.len() != expected_files.len() {
        let missing = expected_files
            .keys()
            .find(|path| !observed.contains(*path))
            .cloned()
            .unwrap_or_else(|| "<unknown>".to_string());
        return Err(OrionCodeArchiveVerificationError::FileMismatch {
            path: missing,
            reason: "manifest file is missing from the artifact".to_string(),
        });
    }
    Ok((observed.len(), total_bytes))
}

#[cfg(unix)]
fn verify_file_mode(
    path: &Path,
    expected_mode: u32,
) -> Result<(), OrionCodeArchiveVerificationError> {
    use std::os::unix::fs::PermissionsExt as _;

    let metadata = std::fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    let actual_mode = metadata.permissions().mode() & 0o777;
    if actual_mode != expected_mode {
        return Err(OrionCodeArchiveVerificationError::FileMismatch {
            path: path.display().to_string(),
            reason: format!("expected mode {expected_mode:o}, found {actual_mode:o}"),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_file_mode(
    _path: &Path,
    _expected_mode: u32,
) -> Result<(), OrionCodeArchiveVerificationError> {
    Ok(())
}

fn verify_manifest_bound_files(
    artifact_root: &Path,
    manifest: &OrionCodeArtifactManifestV1,
    expected_sbom_sha256: &str,
    expected_files: &BTreeMap<String, &crate::OrionCodeArtifactFileV1>,
) -> Result<(), OrionCodeArchiveVerificationError> {
    let sbom = expected_files
        .get(&manifest.sbom_path)
        .ok_or_else(|| invalid_manifest("SBOM is not listed"))?;
    if !sbom.sha256.eq_ignore_ascii_case(&manifest.sbom_sha256)
        || !sbom.sha256.eq_ignore_ascii_case(expected_sbom_sha256)
    {
        return Err(invalid_manifest(
            "SBOM digest does not match the signed target",
        ));
    }
    let notices = expected_files
        .get(&manifest.notices_path)
        .ok_or_else(|| invalid_manifest("notices file is not listed"))?;
    if !notices
        .sha256
        .eq_ignore_ascii_case(&manifest.notices_sha256)
    {
        return Err(invalid_manifest("notices digest does not match manifest"));
    }

    let sbom_bytes = read_bounded_file(
        &artifact_root.join(&manifest.sbom_path),
        sbom.bytes.min(MAX_MANIFEST_BYTES),
    )?;
    let sbom_json: serde_json::Value = serde_json::from_slice(&sbom_bytes)
        .map_err(|error| invalid_manifest(format!("SBOM is not valid JSON: {error}")))?;
    if sbom_json.get("bomFormat").and_then(|value| value.as_str()) != Some("CycloneDX") {
        return Err(invalid_manifest("SBOM must use CycloneDX"));
    }
    let notices_bytes = read_bounded_file(
        &artifact_root.join(&manifest.notices_path),
        notices.bytes.min(MAX_MANIFEST_BYTES),
    )?;
    if notices_bytes.is_empty() {
        return Err(invalid_manifest("THIRD_PARTY_NOTICES must not be empty"));
    }
    Ok(())
}

fn validate_exact_semver(version: &str) -> Result<(), OrionCodeArchiveVerificationError> {
    let parsed = Version::parse(version)
        .map_err(|error| OrionCodeArchiveVerificationError::InvalidInput(error.to_string()))?;
    if parsed.to_string() != version {
        return Err(OrionCodeArchiveVerificationError::InvalidInput(
            "version must be canonical exact semver".to_string(),
        ));
    }
    Ok(())
}

fn slash_path(path: &Path) -> Result<String, OrionCodeArchiveVerificationError> {
    let mut output = String::new();
    for component in path.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(OrionCodeArchiveVerificationError::InvalidInput(
                "artifact path is not normalized".to_string(),
            ));
        };
        let component = component.to_str().ok_or_else(|| {
            OrionCodeArchiveVerificationError::InvalidInput(
                "artifact path is not valid UTF-8".to_string(),
            )
        })?;
        if !output.is_empty() {
            output.push('/');
        }
        output.push_str(component);
    }
    validate_relative_artifact_path(&output)
        .map_err(OrionCodeArchiveVerificationError::InvalidInput)?;
    Ok(output)
}

fn read_bounded_file(
    path: &Path,
    limit: u64,
) -> Result<Vec<u8>, OrionCodeArchiveVerificationError> {
    let file = File::open(path).map_err(|error| io_error(path, error))?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(path, error))?;
    if bytes.len() as u64 > limit {
        return Err(OrionCodeArchiveVerificationError::FileMismatch {
            path: path.display().to_string(),
            reason: format!("file exceeds {limit} bytes"),
        });
    }
    Ok(bytes)
}

fn sha256_file(path: &Path, max_bytes: u64) -> Result<String, OrionCodeArchiveVerificationError> {
    let file = File::open(path).map_err(|error| io_error(path, error))?;
    let mut reader = BufReader::new(file).take(max_bytes.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| io_error(path, error))?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > max_bytes {
            return Err(OrionCodeArchiveVerificationError::FileMismatch {
                path: path.display().to_string(),
                reason: format!("file exceeds {max_bytes} bytes while hashing"),
            });
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn lowercase_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid_manifest(reason: impl Into<String>) -> OrionCodeArchiveVerificationError {
    OrionCodeArchiveVerificationError::InvalidManifest(reason.into())
}

fn io_error(path: &Path, error: impl std::fmt::Display) -> OrionCodeArchiveVerificationError {
    OrionCodeArchiveVerificationError::Io {
        path: path.display().to_string(),
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use async_zip::{Compression, ZipEntryBuilder, base::write::ZipFileWriter};
    use chrono::{DateTime, Utc};
    use futures::AsyncWriteExt as _;
    use tempfile::TempDir;

    use super::*;
    use crate::OrionCodeArtifactFileV1;

    fn small_limits() -> OrionCodeArchiveLimits {
        OrionCodeArchiveLimits {
            max_entries: 32,
            max_file_bytes: 4096,
            max_uncompressed_bytes: 8192,
        }
    }

    async fn write_zip(
        path: &Path,
        entries: Vec<(&str, Vec<u8>, u16)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut output = async_fs::File::create(path).await?;
        let mut writer = ZipFileWriter::new(&mut output);
        for (name, bytes, mode) in entries {
            let builder =
                ZipEntryBuilder::new(name.into(), Compression::Deflate).unix_permissions(mode);
            writer.write_entry_whole(builder, &bytes).await?;
        }
        writer.close().await?;
        output.flush().await?;
        output.sync_all().await?;
        Ok(())
    }

    #[test]
    fn extraction_rejects_traversal_links_collisions_and_bombs() {
        smol::block_on(async {
            for (name, entries, limits) in [
                (
                    "traversal.zip",
                    vec![("../escape", b"bad".to_vec(), 0o100644)],
                    small_limits(),
                ),
                (
                    "symlink.zip",
                    vec![("link", b"target".to_vec(), 0o120777)],
                    small_limits(),
                ),
                (
                    "collision.zip",
                    vec![
                        ("Runtime/node", b"one".to_vec(), 0o100644),
                        ("runtime/node", b"two".to_vec(), 0o100644),
                    ],
                    small_limits(),
                ),
                (
                    "bomb.zip",
                    vec![("large", vec![0; 1024], 0o100644)],
                    OrionCodeArchiveLimits {
                        max_entries: 2,
                        max_file_bytes: 100,
                        max_uncompressed_bytes: 100,
                    },
                ),
            ] {
                let temporary = TempDir::new().expect("temporary directory");
                let archive = temporary.path().join(name);
                write_zip(&archive, entries)
                    .await
                    .expect("write malicious ZIP");
                let destination = temporary.path().join("staging");
                assert!(
                    extract_orion_code_zip(&archive, &destination, limits)
                        .await
                        .is_err()
                );
                assert!(!destination.exists());
            }
        });
    }

    #[test]
    fn verifies_archive_size_digest_and_manifest_file_set() {
        smol::block_on(async {
            let temporary = TempDir::new().expect("temporary directory");
            let files = [
                ("LICENSE", b"GPL-3.0\n".as_slice(), 0o644),
                (
                    "SBOM.cdx.json",
                    br#"{"bomFormat":"CycloneDX"}"#.as_slice(),
                    0o644,
                ),
                (
                    "THIRD_PARTY_NOTICES",
                    b"dependency notices\n".as_slice(),
                    0o644,
                ),
                ("bin/orion-code-acp", b"binary\n".as_slice(), 0o755),
            ];
            let manifest_files = files
                .iter()
                .map(|(path, bytes, mode)| OrionCodeArtifactFileV1 {
                    path: (*path).to_string(),
                    mode: *mode,
                    bytes: bytes.len() as u64,
                    sha256: lowercase_sha256(bytes),
                })
                .collect::<Vec<_>>();
            let manifest = OrionCodeArtifactManifestV1 {
                schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION,
                version: "0.4.0".to_string(),
                git_sha: "a".repeat(40),
                target: "darwin-aarch64".to_string(),
                built_at: DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
                    .expect("valid date")
                    .with_timezone(&Utc),
                acp_protocol: SUPPORTED_ACP_PROTOCOL,
                studio_version_requirement: ">=0.1.0,<1.0.0".to_string(),
                node_version: "22.22.0".to_string(),
                node_abi: "127".to_string(),
                native_modules: Vec::new(),
                command: "bin/orion-code-acp".to_string(),
                sbom_path: "SBOM.cdx.json".to_string(),
                sbom_sha256: lowercase_sha256(files[1].1),
                notices_path: "THIRD_PARTY_NOTICES".to_string(),
                notices_sha256: lowercase_sha256(files[2].1),
                files: manifest_files,
            };
            let manifest_bytes = serde_json::to_vec(&manifest).expect("serialize manifest");
            let manifest_sha256 = lowercase_sha256(&manifest_bytes);
            let mut zip_entries = files
                .iter()
                .map(|(path, bytes, mode)| (*path, bytes.to_vec(), 0o100000 | *mode as u16))
                .collect::<Vec<_>>();
            zip_entries.push(("manifest.json", manifest_bytes, 0o100644));
            let archive = temporary.path().join("candidate.zip");
            write_zip(&archive, zip_entries)
                .await
                .expect("write valid ZIP");
            let archive_bytes = std::fs::read(&archive).expect("read archive");
            verify_orion_code_archive(
                &archive,
                archive_bytes.len() as u64,
                &lowercase_sha256(&archive_bytes),
            )
            .expect("verify archive bytes");

            let destination = temporary.path().join("staging");
            extract_orion_code_zip(&archive, &destination, small_limits())
                .await
                .expect("extract valid ZIP");
            let verified = verify_orion_code_artifact(
                &destination,
                "0.4.0",
                "darwin-aarch64",
                "bin/orion-code-acp",
                &manifest_sha256,
                &lowercase_sha256(files[1].1),
                small_limits(),
            )
            .expect("verify artifact");
            assert_eq!(verified.file_count, files.len());

            std::fs::write(destination.join("unexpected"), b"tamper")
                .expect("write unexpected file");
            assert!(
                verify_orion_code_artifact(
                    &destination,
                    "0.4.0",
                    "darwin-aarch64",
                    "bin/orion-code-acp",
                    &manifest_sha256,
                    &lowercase_sha256(files[1].1),
                    small_limits(),
                )
                .is_err()
            );
        });
    }
}
