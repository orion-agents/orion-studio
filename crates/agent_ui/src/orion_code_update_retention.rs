use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, DirEntry},
    io,
    path::{Path, PathBuf},
};

use ::orion_code_update::{
    OrionCodeInstallReceiptV2, OrionCodeInstallSource, validate_relative_artifact_path,
    validate_sha256, validate_target_name,
};
use semver::Version;
use thiserror::Error;

pub const ORION_CODE_RETENTION_MAX_COMPLETE_INSTALLS: usize = 3;
const ORION_CODE_RETENTION_MAX_RECEIPT_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OrionCodeManagedInstallIdentity {
    pub version: String,
    pub target: String,
}

impl fmt::Display for OrionCodeManagedInstallIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} for {}", self.version, self.target)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrionCodeRetentionReferences {
    pub current: Option<OrionCodeManagedInstallIdentity>,
    pub previous: Option<OrionCodeManagedInstallIdentity>,
    pub staged: Option<OrionCodeManagedInstallIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrionCodeRetentionRestartGrace {
    pub successful_activation_app_start_sequence: Option<u64>,
    pub current_app_start_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeRetentionInput {
    pub managed_root: PathBuf,
    pub known_targets: BTreeSet<String>,
    pub receipt_owned_installs: BTreeSet<OrionCodeManagedInstallIdentity>,
    pub references: OrionCodeRetentionReferences,
    pub revoked_versions: BTreeSet<String>,
    pub restart_grace: OrionCodeRetentionRestartGrace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrionCodeRetentionDisposition {
    KeepCurrent,
    KeepPrevious,
    KeepStaged,
    KeepUntilSuccessfulRestart,
    KeepWithinLimit,
    DeleteRevoked,
    DeleteOverLimit,
}

impl OrionCodeRetentionDisposition {
    pub fn deletes_binary(self) -> bool {
        matches!(self, Self::DeleteRevoked | Self::DeleteOverLimit)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeRetentionDecision {
    pub install: OrionCodeManagedInstallIdentity,
    pub disposition: OrionCodeRetentionDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeRetentionPlan {
    canonical_managed_root: PathBuf,
    known_targets: BTreeSet<String>,
    receipt_owned_installs: BTreeSet<OrionCodeManagedInstallIdentity>,
    references: OrionCodeRetentionReferences,
    inventory: BTreeSet<OrionCodeManagedInstallIdentity>,
    restart_grace_satisfied: bool,
    decisions: Vec<OrionCodeRetentionDecision>,
}

impl OrionCodeRetentionPlan {
    pub fn decisions(&self) -> &[OrionCodeRetentionDecision] {
        &self.decisions
    }

    pub fn restart_grace_satisfied(&self) -> bool {
        self.restart_grace_satisfied
    }

    pub fn deletion_count(&self) -> usize {
        self.decisions
            .iter()
            .filter(|decision| decision.disposition.deletes_binary())
            .count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeRetentionReport {
    pub deleted_installs: Vec<OrionCodeManagedInstallIdentity>,
}

#[derive(Debug, Error)]
pub enum OrionCodeRetentionError {
    #[error("invalid retention input: {0}")]
    InvalidInput(String),
    #[error("unsafe Orion Code managed path: {0}")]
    UnsafeManagedPath(String),
    #[error("managed install {0} has no verified receipt ownership")]
    UnownedInstall(OrionCodeManagedInstallIdentity),
    #[error("protected managed install {0} has no verified receipt ownership")]
    UnownedProtectedInstall(OrionCodeManagedInstallIdentity),
    #[error("protected managed install {0} is absent from the versions inventory")]
    MissingProtectedInstall(OrionCodeManagedInstallIdentity),
    #[error("managed versions inventory changed after retention planning")]
    InventoryChanged,
    #[error("verified receipt ownership changed after retention planning")]
    ReceiptOwnershipChanged,
    #[error("failed to {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("failed to delete managed install {install}: {source}")]
    Delete {
        install: OrionCodeManagedInstallIdentity,
        #[source]
        source: io::Error,
    },
}

pub fn plan_orion_code_retention(
    input: OrionCodeRetentionInput,
) -> Result<OrionCodeRetentionPlan, OrionCodeRetentionError> {
    validate_known_targets(&input.known_targets)?;
    validate_receipt_ownership(&input.receipt_owned_installs, &input.known_targets)?;
    validate_revoked_versions(&input.revoked_versions)?;
    let protected = validate_references(
        &input.references,
        &input.known_targets,
        &input.receipt_owned_installs,
    )?;
    let restart_grace_satisfied = validate_restart_grace(input.restart_grace)?;
    let canonical_managed_root = canonical_managed_root(&input.managed_root)?;
    let inventory = scan_versions(&canonical_managed_root, &input.known_targets)?;

    for install in &inventory {
        if !input.receipt_owned_installs.contains(install) {
            return Err(OrionCodeRetentionError::UnownedInstall(install.clone()));
        }
    }
    for install in protected.keys() {
        if !inventory.contains(install) {
            return Err(OrionCodeRetentionError::MissingProtectedInstall(
                install.clone(),
            ));
        }
    }

    let ordered_inventory = newest_first(&inventory)?;
    let decisions = decide_retention(
        &ordered_inventory,
        &protected,
        &input.revoked_versions,
        restart_grace_satisfied,
    );

    Ok(OrionCodeRetentionPlan {
        canonical_managed_root,
        known_targets: input.known_targets,
        receipt_owned_installs: input.receipt_owned_installs,
        references: input.references,
        inventory,
        restart_grace_satisfied,
        decisions,
    })
}

pub fn discover_orion_code_receipt_owned_installs(
    managed_root: &Path,
) -> Result<BTreeSet<OrionCodeManagedInstallIdentity>, OrionCodeRetentionError> {
    let canonical_managed_root = canonical_managed_root(managed_root)?;
    let receipts_root = canonical_managed_root.join("receipts");
    match fs::symlink_metadata(&receipts_root) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(source) => return Err(io_error("inspect the receipts directory", source)),
    }
    validate_real_directory(
        &canonical_managed_root,
        &receipts_root,
        "receipts directory",
    )?;

    let mut owned_installs = BTreeSet::new();
    for entry in read_directory(&receipts_root, "read the receipts directory")? {
        let filename = utf8_entry_name(&entry, "receipt")?;
        if filename.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|source| io_error("inspect an install receipt", source))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(OrionCodeRetentionError::UnsafeManagedPath(
                "receipt entry is not a real file".to_string(),
            ));
        }
        if metadata.len() > ORION_CODE_RETENTION_MAX_RECEIPT_BYTES {
            return Err(OrionCodeRetentionError::InvalidInput(
                "install receipt exceeds its byte limit".to_string(),
            ));
        }
        let bytes =
            fs::read(&path).map_err(|source| io_error("read an install receipt", source))?;
        let receipt: OrionCodeInstallReceiptV2 =
            serde_json::from_slice(&bytes).map_err(|error| {
                OrionCodeRetentionError::InvalidInput(format!(
                    "install receipt is not valid v2 JSON: {error}"
                ))
            })?;
        let identity = validate_owned_receipt(&receipt, &filename)?;
        if !owned_installs.insert(identity) {
            return Err(OrionCodeRetentionError::InvalidInput(
                "multiple receipts claim the same managed install".to_string(),
            ));
        }
    }
    Ok(owned_installs)
}

pub fn apply_orion_code_retention_plan(
    plan: &OrionCodeRetentionPlan,
) -> Result<OrionCodeRetentionReport, OrionCodeRetentionError> {
    let deletions = plan
        .decisions
        .iter()
        .filter(|decision| decision.disposition.deletes_binary())
        .map(|decision| decision.install.clone())
        .collect::<Vec<_>>();
    if deletions.is_empty() {
        return Ok(OrionCodeRetentionReport {
            deleted_installs: Vec::new(),
        });
    }

    if !plan.restart_grace_satisfied {
        return Err(OrionCodeRetentionError::InvalidInput(
            "a deletion plan requires a successful activation followed by an App restart"
                .to_string(),
        ));
    }

    let canonical_managed_root = canonical_managed_root(&plan.canonical_managed_root)?;
    if canonical_managed_root != plan.canonical_managed_root {
        return Err(OrionCodeRetentionError::UnsafeManagedPath(
            "managed root identity changed after planning".to_string(),
        ));
    }
    let current_inventory = scan_versions(&canonical_managed_root, &plan.known_targets)?;
    if current_inventory != plan.inventory {
        return Err(OrionCodeRetentionError::InventoryChanged);
    }
    let current_receipt_owned_installs =
        discover_orion_code_receipt_owned_installs(&canonical_managed_root)?;
    if current_receipt_owned_installs != plan.receipt_owned_installs {
        return Err(OrionCodeRetentionError::ReceiptOwnershipChanged);
    }

    let protected = protected_decisions(&plan.references);
    let mut deletion_paths = Vec::with_capacity(deletions.len());
    for install in &deletions {
        validate_install_identity(install, &plan.known_targets)?;
        if protected.contains_key(install)
            || !plan.receipt_owned_installs.contains(install)
            || !current_inventory.contains(install)
        {
            return Err(OrionCodeRetentionError::InvalidInput(format!(
                "retention plan no longer owns unprotected install {install}"
            )));
        }
        let path = install_path(&canonical_managed_root, install);
        validate_install_tree(&canonical_managed_root, &path)?;
        deletion_paths.push((install.clone(), path));
    }

    let mut deleted_installs = Vec::with_capacity(deletion_paths.len());
    for (install, path) in deletion_paths {
        validate_install_tree(&canonical_managed_root, &path)?;
        fs::remove_dir_all(&path).map_err(|source| OrionCodeRetentionError::Delete {
            install: install.clone(),
            source,
        })?;
        deleted_installs.push(install);
    }

    Ok(OrionCodeRetentionReport { deleted_installs })
}

fn validate_known_targets(known_targets: &BTreeSet<String>) -> Result<(), OrionCodeRetentionError> {
    if known_targets.is_empty() {
        return Err(OrionCodeRetentionError::InvalidInput(
            "at least one platform target must be allowlisted".to_string(),
        ));
    }
    for target in known_targets {
        validate_target_name(target).map_err(OrionCodeRetentionError::InvalidInput)?;
    }
    Ok(())
}

fn validate_owned_receipt(
    receipt: &OrionCodeInstallReceiptV2,
    filename: &str,
) -> Result<OrionCodeManagedInstallIdentity, OrionCodeRetentionError> {
    validate_exact_semver(&receipt.version)?;
    let target = receipt.target.as_deref().ok_or_else(|| {
        OrionCodeRetentionError::InvalidInput("archive receipt omitted target".to_string())
    })?;
    validate_target_name(target).map_err(OrionCodeRetentionError::InvalidInput)?;
    if receipt.source != OrionCodeInstallSource::Archive
        || receipt.legacy_imported
        || receipt.installed_at.is_none()
        || receipt.index_sequence.is_none_or(|sequence| sequence == 0)
    {
        return Err(OrionCodeRetentionError::InvalidInput(
            "receipt does not own a completed managed archive install".to_string(),
        ));
    }
    let archive_sha256 = receipt.archive_sha256.as_deref().ok_or_else(|| {
        OrionCodeRetentionError::InvalidInput("archive receipt omitted SHA-256".to_string())
    })?;
    validate_sha256(archive_sha256).map_err(OrionCodeRetentionError::InvalidInput)?;
    let command = receipt.command.as_deref().ok_or_else(|| {
        OrionCodeRetentionError::InvalidInput("archive receipt omitted command".to_string())
    })?;
    validate_relative_artifact_path(command).map_err(OrionCodeRetentionError::InvalidInput)?;
    let expected_filename = format!("{}-{target}.json", receipt.version);
    if filename != expected_filename {
        return Err(OrionCodeRetentionError::InvalidInput(
            "receipt filename does not match its managed install identity".to_string(),
        ));
    }
    Ok(OrionCodeManagedInstallIdentity {
        version: receipt.version.clone(),
        target: target.to_string(),
    })
}

fn validate_receipt_ownership(
    receipt_owned_installs: &BTreeSet<OrionCodeManagedInstallIdentity>,
    known_targets: &BTreeSet<String>,
) -> Result<(), OrionCodeRetentionError> {
    for install in receipt_owned_installs {
        validate_install_identity(install, known_targets)?;
    }
    Ok(())
}

fn validate_references(
    references: &OrionCodeRetentionReferences,
    known_targets: &BTreeSet<String>,
    receipt_owned_installs: &BTreeSet<OrionCodeManagedInstallIdentity>,
) -> Result<
    BTreeMap<OrionCodeManagedInstallIdentity, OrionCodeRetentionDisposition>,
    OrionCodeRetentionError,
> {
    let protected = protected_decisions(references);
    for install in protected.keys() {
        validate_install_identity(install, known_targets)?;
        if !receipt_owned_installs.contains(install) {
            return Err(OrionCodeRetentionError::UnownedProtectedInstall(
                install.clone(),
            ));
        }
    }
    Ok(protected)
}

fn protected_decisions(
    references: &OrionCodeRetentionReferences,
) -> BTreeMap<OrionCodeManagedInstallIdentity, OrionCodeRetentionDisposition> {
    let mut protected = BTreeMap::new();
    if let Some(current) = &references.current {
        protected.insert(current.clone(), OrionCodeRetentionDisposition::KeepCurrent);
    }
    if let Some(previous) = &references.previous {
        protected
            .entry(previous.clone())
            .or_insert(OrionCodeRetentionDisposition::KeepPrevious);
    }
    if let Some(staged) = &references.staged {
        protected
            .entry(staged.clone())
            .or_insert(OrionCodeRetentionDisposition::KeepStaged);
    }
    protected
}

fn validate_restart_grace(
    grace: OrionCodeRetentionRestartGrace,
) -> Result<bool, OrionCodeRetentionError> {
    if grace.current_app_start_sequence == 0 {
        return Err(OrionCodeRetentionError::InvalidInput(
            "current App start sequence must be non-zero".to_string(),
        ));
    }
    let Some(activation_sequence) = grace.successful_activation_app_start_sequence else {
        return Ok(false);
    };
    if activation_sequence == 0 {
        return Err(OrionCodeRetentionError::InvalidInput(
            "successful activation App start sequence must be non-zero".to_string(),
        ));
    }
    if activation_sequence > grace.current_app_start_sequence {
        return Err(OrionCodeRetentionError::InvalidInput(
            "successful activation App start sequence is ahead of the current App start"
                .to_string(),
        ));
    }
    Ok(grace.current_app_start_sequence > activation_sequence)
}

fn validate_revoked_versions(
    revoked_versions: &BTreeSet<String>,
) -> Result<(), OrionCodeRetentionError> {
    for version in revoked_versions {
        validate_exact_semver(version)?;
    }
    Ok(())
}

fn validate_install_identity(
    install: &OrionCodeManagedInstallIdentity,
    known_targets: &BTreeSet<String>,
) -> Result<(), OrionCodeRetentionError> {
    validate_exact_semver(&install.version)?;
    validate_target_name(&install.target).map_err(OrionCodeRetentionError::InvalidInput)?;
    if !known_targets.contains(&install.target) {
        return Err(OrionCodeRetentionError::InvalidInput(format!(
            "unknown platform target {:?}",
            install.target
        )));
    }
    Ok(())
}

fn validate_exact_semver(value: &str) -> Result<Version, OrionCodeRetentionError> {
    let version = Version::parse(value).map_err(|error| {
        OrionCodeRetentionError::InvalidInput(format!(
            "managed version {value:?} is not exact semver: {error}"
        ))
    })?;
    if version.to_string() != value {
        return Err(OrionCodeRetentionError::InvalidInput(format!(
            "managed version {value:?} is not canonical exact semver"
        )));
    }
    Ok(version)
}

fn canonical_managed_root(path: &Path) -> Result<PathBuf, OrionCodeRetentionError> {
    if path.as_os_str().is_empty() {
        return Err(OrionCodeRetentionError::InvalidInput(
            "managed root must not be empty".to_string(),
        ));
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect the managed root", source))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(OrionCodeRetentionError::UnsafeManagedPath(
            "managed root must be a real directory".to_string(),
        ));
    }
    path.canonicalize()
        .map_err(|source| io_error("canonicalize the managed root", source))
}

fn scan_versions(
    canonical_managed_root: &Path,
    known_targets: &BTreeSet<String>,
) -> Result<BTreeSet<OrionCodeManagedInstallIdentity>, OrionCodeRetentionError> {
    validate_real_directory(
        canonical_managed_root,
        canonical_managed_root,
        "managed root",
    )?;
    let versions_root = canonical_managed_root.join("versions");
    match fs::symlink_metadata(&versions_root) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(source) => return Err(io_error("inspect the versions directory", source)),
    }
    validate_real_directory(canonical_managed_root, &versions_root, "versions directory")?;

    let mut inventory = BTreeSet::new();
    for version_entry in read_directory(&versions_root, "read the versions directory")? {
        let version_name = utf8_entry_name(&version_entry, "version")?;
        validate_exact_semver(&version_name)?;
        let version_path = version_entry.path();
        validate_real_directory(canonical_managed_root, &version_path, "version directory")?;

        for target_entry in read_directory(&version_path, "read a version directory")? {
            let target_name = utf8_entry_name(&target_entry, "target")?;
            validate_target_name(&target_name).map_err(OrionCodeRetentionError::InvalidInput)?;
            if !known_targets.contains(&target_name) {
                return Err(OrionCodeRetentionError::InvalidInput(format!(
                    "unknown platform target {target_name:?} in versions inventory"
                )));
            }
            let target_path = target_entry.path();
            validate_install_tree(canonical_managed_root, &target_path)?;
            inventory.insert(OrionCodeManagedInstallIdentity {
                version: version_name.clone(),
                target: target_name,
            });
        }
    }
    Ok(inventory)
}

fn validate_install_tree(
    canonical_managed_root: &Path,
    install_path: &Path,
) -> Result<(), OrionCodeRetentionError> {
    let versions_root = canonical_managed_root.join("versions");
    validate_real_directory(canonical_managed_root, &versions_root, "versions directory")?;
    if install_path == versions_root || !install_path.starts_with(&versions_root) {
        return Err(OrionCodeRetentionError::UnsafeManagedPath(
            "install path escaped the versions directory".to_string(),
        ));
    }
    validate_real_directory(canonical_managed_root, install_path, "install directory")?;

    let mut pending_directories = vec![install_path.to_path_buf()];
    while let Some(directory) = pending_directories.pop() {
        for entry in read_directory(&directory, "inspect an install tree")? {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|source| io_error("inspect an install tree entry", source))?;
            if metadata.file_type().is_symlink() {
                return Err(OrionCodeRetentionError::UnsafeManagedPath(
                    "install tree contains a symbolic link".to_string(),
                ));
            }
            if metadata.is_dir() {
                validate_real_directory(canonical_managed_root, &path, "install subdirectory")?;
                if !path.starts_with(install_path) {
                    return Err(OrionCodeRetentionError::UnsafeManagedPath(
                        "install subdirectory escaped its owned install".to_string(),
                    ));
                }
                pending_directories.push(path);
            } else if !metadata.is_file() {
                return Err(OrionCodeRetentionError::UnsafeManagedPath(
                    "install tree contains a non-file entry".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_real_directory(
    canonical_managed_root: &Path,
    path: &Path,
    label: &'static str,
) -> Result<(), OrionCodeRetentionError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect a managed directory", source))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(OrionCodeRetentionError::UnsafeManagedPath(format!(
            "{label} is not a real directory"
        )));
    }
    let canonical_path = path
        .canonicalize()
        .map_err(|source| io_error("canonicalize a managed directory", source))?;
    if canonical_path != path || !canonical_path.starts_with(canonical_managed_root) {
        return Err(OrionCodeRetentionError::UnsafeManagedPath(format!(
            "{label} escaped the canonical managed root"
        )));
    }
    Ok(())
}

fn read_directory(
    path: &Path,
    operation: &'static str,
) -> Result<Vec<DirEntry>, OrionCodeRetentionError> {
    let mut entries = fs::read_dir(path)
        .map_err(|source| io_error(operation, source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| io_error(operation, source))?;
    entries.sort_by_key(DirEntry::file_name);
    Ok(entries)
}

fn utf8_entry_name(
    entry: &DirEntry,
    label: &'static str,
) -> Result<String, OrionCodeRetentionError> {
    entry.file_name().into_string().map_err(|_| {
        OrionCodeRetentionError::UnsafeManagedPath(format!("{label} directory name is not UTF-8"))
    })
}

fn newest_first(
    inventory: &BTreeSet<OrionCodeManagedInstallIdentity>,
) -> Result<Vec<OrionCodeManagedInstallIdentity>, OrionCodeRetentionError> {
    let mut parsed = inventory
        .iter()
        .map(|install| Ok((install.clone(), validate_exact_semver(&install.version)?)))
        .collect::<Result<Vec<_>, OrionCodeRetentionError>>()?;
    parsed.sort_by(
        |(left_install, left_version), (right_install, right_version)| {
            right_version
                .cmp(left_version)
                .then_with(|| right_install.version.cmp(&left_install.version))
                .then_with(|| left_install.target.cmp(&right_install.target))
        },
    );
    Ok(parsed
        .into_iter()
        .map(|(install, _version)| install)
        .collect())
}

fn decide_retention(
    ordered_inventory: &[OrionCodeManagedInstallIdentity],
    protected: &BTreeMap<OrionCodeManagedInstallIdentity, OrionCodeRetentionDisposition>,
    revoked_versions: &BTreeSet<String>,
    restart_grace_satisfied: bool,
) -> Vec<OrionCodeRetentionDecision> {
    let available_unprotected_slots =
        ORION_CODE_RETENTION_MAX_COMPLETE_INSTALLS.saturating_sub(protected.len());
    let mut retained_unprotected = 0;

    ordered_inventory
        .iter()
        .map(|install| {
            let disposition = if let Some(disposition) = protected.get(install) {
                *disposition
            } else if !restart_grace_satisfied {
                OrionCodeRetentionDisposition::KeepUntilSuccessfulRestart
            } else if revoked_versions.contains(&install.version) {
                OrionCodeRetentionDisposition::DeleteRevoked
            } else if retained_unprotected < available_unprotected_slots {
                retained_unprotected += 1;
                OrionCodeRetentionDisposition::KeepWithinLimit
            } else {
                OrionCodeRetentionDisposition::DeleteOverLimit
            };
            OrionCodeRetentionDecision {
                install: install.clone(),
                disposition,
            }
        })
        .collect()
}

fn install_path(
    canonical_managed_root: &Path,
    install: &OrionCodeManagedInstallIdentity,
) -> PathBuf {
    canonical_managed_root
        .join("versions")
        .join(&install.version)
        .join(&install.target)
}

fn io_error(operation: &'static str, source: io::Error) -> OrionCodeRetentionError {
    OrionCodeRetentionError::Io { operation, source }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, error::Error, fs};

    use tempfile::TempDir;

    use super::*;

    const TARGET: &str = "darwin-aarch64";

    fn install(version: &str) -> OrionCodeManagedInstallIdentity {
        OrionCodeManagedInstallIdentity {
            version: version.to_string(),
            target: TARGET.to_string(),
        }
    }

    fn setup_managed_root() -> Result<TempDir, Box<dyn Error>> {
        let managed_root = tempfile::tempdir()?;
        fs::create_dir(managed_root.path().join("versions"))?;
        fs::create_dir(managed_root.path().join("receipts"))?;
        fs::create_dir(managed_root.path().join("downloads"))?;
        fs::create_dir(managed_root.path().join("user-data"))?;
        Ok(managed_root)
    }

    fn create_install(
        managed_root: &Path,
        version: &str,
    ) -> Result<OrionCodeManagedInstallIdentity, Box<dyn Error>> {
        let identity = install(version);
        let artifact_root = managed_root
            .join("versions")
            .join(&identity.version)
            .join(&identity.target);
        fs::create_dir_all(artifact_root.join("bin"))?;
        fs::write(artifact_root.join("bin/orion-code-acp"), version)?;
        let receipt = OrionCodeInstallReceiptV2 {
            version: identity.version.clone(),
            source: OrionCodeInstallSource::Archive,
            legacy_imported: false,
            target: Some(identity.target.clone()),
            archive_sha256: Some("a".repeat(64)),
            command: Some("OrionCodeSidecar.app/Contents/MacOS/orion-code-acp".to_string()),
            installed_at: Some(chrono::Utc::now()),
            index_sequence: Some(1),
        };
        fs::write(
            managed_root
                .join("receipts")
                .join(format!("{}-{}.json", identity.version, identity.target)),
            serde_json::to_vec(&receipt)?,
        )?;
        Ok(identity)
    }

    fn retention_input(
        managed_root: &Path,
        receipt_owned_installs: BTreeSet<OrionCodeManagedInstallIdentity>,
        references: OrionCodeRetentionReferences,
        revoked_versions: BTreeSet<String>,
        activation_sequence: Option<u64>,
        current_sequence: u64,
    ) -> OrionCodeRetentionInput {
        OrionCodeRetentionInput {
            managed_root: managed_root.to_path_buf(),
            known_targets: BTreeSet::from([TARGET.to_string()]),
            receipt_owned_installs,
            references,
            revoked_versions,
            restart_grace: OrionCodeRetentionRestartGrace {
                successful_activation_app_start_sequence: activation_sequence,
                current_app_start_sequence: current_sequence,
            },
        }
    }

    fn install_exists(managed_root: &Path, identity: &OrionCodeManagedInstallIdentity) -> bool {
        managed_root
            .join("versions")
            .join(&identity.version)
            .join(&identity.target)
            .is_dir()
    }

    #[test]
    fn protects_current_previous_and_staged_even_when_revoked_or_over_limit()
    -> Result<(), Box<dyn Error>> {
        let managed_root = setup_managed_root()?;
        let installs = ["1.0.0", "2.0.0", "3.0.0", "4.0.0", "5.0.0"]
            .into_iter()
            .map(|version| create_install(managed_root.path(), version))
            .collect::<Result<Vec<_>, _>>()?;
        let owned = installs.iter().cloned().collect();
        let references = OrionCodeRetentionReferences {
            current: Some(installs[0].clone()),
            previous: Some(installs[1].clone()),
            staged: Some(installs[2].clone()),
        };
        let plan = plan_orion_code_retention(retention_input(
            managed_root.path(),
            owned,
            references,
            BTreeSet::from(["1.0.0".to_string(), "4.0.0".to_string()]),
            Some(7),
            8,
        ))?;

        assert_eq!(plan.deletion_count(), 2);
        apply_orion_code_retention_plan(&plan)?;

        assert!(install_exists(managed_root.path(), &installs[0]));
        assert!(install_exists(managed_root.path(), &installs[1]));
        assert!(install_exists(managed_root.path(), &installs[2]));
        assert!(!install_exists(managed_root.path(), &installs[3]));
        assert!(!install_exists(managed_root.path(), &installs[4]));
        Ok(())
    }

    #[test]
    fn waits_for_an_app_restart_after_successful_activation_before_pruning()
    -> Result<(), Box<dyn Error>> {
        let managed_root = setup_managed_root()?;
        let installs = ["1.0.0", "2.0.0", "3.0.0", "4.0.0"]
            .into_iter()
            .map(|version| create_install(managed_root.path(), version))
            .collect::<Result<Vec<_>, _>>()?;
        let owned = installs.iter().cloned().collect::<BTreeSet<_>>();
        let references = OrionCodeRetentionReferences {
            current: Some(installs[3].clone()),
            previous: Some(installs[2].clone()),
            staged: None,
        };
        let before_restart = plan_orion_code_retention(retention_input(
            managed_root.path(),
            owned.clone(),
            references.clone(),
            BTreeSet::new(),
            Some(11),
            11,
        ))?;

        assert!(!before_restart.restart_grace_satisfied());
        assert_eq!(before_restart.deletion_count(), 0);
        apply_orion_code_retention_plan(&before_restart)?;
        assert!(
            installs
                .iter()
                .all(|identity| install_exists(managed_root.path(), identity))
        );

        let after_restart = plan_orion_code_retention(retention_input(
            managed_root.path(),
            owned,
            references,
            BTreeSet::new(),
            Some(11),
            12,
        ))?;
        assert!(after_restart.restart_grace_satisfied());
        assert_eq!(after_restart.deletion_count(), 1);
        apply_orion_code_retention_plan(&after_restart)?;
        assert!(!install_exists(managed_root.path(), &installs[0]));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_before_any_deletion() -> Result<(), Box<dyn Error>> {
        use std::os::unix::fs::symlink;

        let managed_root = setup_managed_root()?;
        let installs = ["1.0.0", "2.0.0", "3.0.0", "4.0.0"]
            .into_iter()
            .map(|version| create_install(managed_root.path(), version))
            .collect::<Result<Vec<_>, _>>()?;
        let outside = tempfile::tempdir()?;
        let outside_sentinel = outside.path().join("must-remain.txt");
        fs::write(&outside_sentinel, "outside")?;
        let escaped_target = managed_root.path().join("versions/1.0.0").join(TARGET);
        fs::remove_dir_all(&escaped_target)?;
        symlink(outside.path(), &escaped_target)?;

        let result = plan_orion_code_retention(retention_input(
            managed_root.path(),
            installs.iter().cloned().collect(),
            OrionCodeRetentionReferences {
                current: Some(installs[3].clone()),
                previous: Some(installs[2].clone()),
                staged: None,
            },
            BTreeSet::new(),
            Some(3),
            4,
        ));

        assert!(matches!(
            result,
            Err(OrionCodeRetentionError::UnsafeManagedPath(_))
        ));
        assert!(outside_sentinel.is_file());
        assert!(install_exists(managed_root.path(), &installs[1]));
        Ok(())
    }

    #[test]
    fn rejects_malformed_version_unknown_target_and_unowned_install_without_deleting()
    -> Result<(), Box<dyn Error>> {
        let malformed_root = setup_managed_root()?;
        let safe_installs = ["1.0.0", "2.0.0", "3.0.0", "4.0.0"]
            .into_iter()
            .map(|version| create_install(malformed_root.path(), version))
            .collect::<Result<Vec<_>, _>>()?;
        fs::create_dir_all(malformed_root.path().join("versions/v9.0.0").join(TARGET))?;
        let malformed_result = plan_orion_code_retention(retention_input(
            malformed_root.path(),
            safe_installs.iter().cloned().collect(),
            OrionCodeRetentionReferences {
                current: Some(safe_installs[3].clone()),
                previous: Some(safe_installs[2].clone()),
                staged: None,
            },
            BTreeSet::new(),
            Some(1),
            2,
        ));
        assert!(matches!(
            malformed_result,
            Err(OrionCodeRetentionError::InvalidInput(_))
        ));
        assert!(install_exists(malformed_root.path(), &safe_installs[0]));

        let unknown_root = setup_managed_root()?;
        let known = create_install(unknown_root.path(), "1.0.0")?;
        fs::create_dir_all(unknown_root.path().join("versions/2.0.0/unknown-target"))?;
        let unknown_result = plan_orion_code_retention(retention_input(
            unknown_root.path(),
            BTreeSet::from([known.clone()]),
            OrionCodeRetentionReferences {
                current: Some(known.clone()),
                ..OrionCodeRetentionReferences::default()
            },
            BTreeSet::new(),
            Some(1),
            2,
        ));
        assert!(matches!(
            unknown_result,
            Err(OrionCodeRetentionError::InvalidInput(_))
        ));
        assert!(install_exists(unknown_root.path(), &known));

        let unowned_root = setup_managed_root()?;
        let owned = create_install(unowned_root.path(), "1.0.0")?;
        let unowned = create_install(unowned_root.path(), "2.0.0")?;
        let unowned_result = plan_orion_code_retention(retention_input(
            unowned_root.path(),
            BTreeSet::from([owned.clone()]),
            OrionCodeRetentionReferences {
                current: Some(owned.clone()),
                ..OrionCodeRetentionReferences::default()
            },
            BTreeSet::new(),
            Some(1),
            2,
        ));
        assert!(matches!(
            unowned_result,
            Err(OrionCodeRetentionError::UnownedInstall(identity)) if identity == unowned
        ));
        assert!(install_exists(unowned_root.path(), &owned));
        assert!(install_exists(unowned_root.path(), &unowned));
        Ok(())
    }

    #[test]
    fn retains_only_three_newest_complete_installs() -> Result<(), Box<dyn Error>> {
        let managed_root = setup_managed_root()?;
        let installs = ["1.0.0", "2.0.0", "3.0.0", "4.0.0", "5.0.0"]
            .into_iter()
            .map(|version| create_install(managed_root.path(), version))
            .collect::<Result<Vec<_>, _>>()?;
        let plan = plan_orion_code_retention(retention_input(
            managed_root.path(),
            installs.iter().cloned().collect(),
            OrionCodeRetentionReferences {
                current: Some(installs[4].clone()),
                previous: Some(installs[3].clone()),
                staged: None,
            },
            BTreeSet::new(),
            Some(5),
            6,
        ))?;

        apply_orion_code_retention_plan(&plan)?;
        assert!(!install_exists(managed_root.path(), &installs[0]));
        assert!(!install_exists(managed_root.path(), &installs[1]));
        assert!(install_exists(managed_root.path(), &installs[2]));
        assert!(install_exists(managed_root.path(), &installs[3]));
        assert!(install_exists(managed_root.path(), &installs[4]));
        Ok(())
    }

    #[test]
    fn deletes_revoked_binary_but_retains_receipt_and_unrelated_data() -> Result<(), Box<dyn Error>>
    {
        let managed_root = setup_managed_root()?;
        let revoked = create_install(managed_root.path(), "1.0.0")?;
        let current = create_install(managed_root.path(), "2.0.0")?;
        let receipt = managed_root
            .path()
            .join("receipts/1.0.0-darwin-aarch64.json");
        let download = managed_root.path().join("downloads/user-download.partial");
        let user_data = managed_root.path().join("user-data/session.json");
        fs::write(&download, "partial")?;
        fs::write(&user_data, "session")?;
        let plan = plan_orion_code_retention(retention_input(
            managed_root.path(),
            BTreeSet::from([revoked.clone(), current.clone()]),
            OrionCodeRetentionReferences {
                current: Some(current.clone()),
                ..OrionCodeRetentionReferences::default()
            },
            BTreeSet::from([revoked.version.clone()]),
            Some(20),
            21,
        ))?;

        assert!(plan.decisions().iter().any(|decision| {
            decision.install == revoked
                && decision.disposition == OrionCodeRetentionDisposition::DeleteRevoked
        }));
        let report = apply_orion_code_retention_plan(&plan)?;

        assert_eq!(report.deleted_installs, vec![revoked.clone()]);
        assert!(!install_exists(managed_root.path(), &revoked));
        assert!(install_exists(managed_root.path(), &current));
        assert!(receipt.is_file());
        assert!(download.is_file());
        assert!(user_data.is_file());
        Ok(())
    }
}
