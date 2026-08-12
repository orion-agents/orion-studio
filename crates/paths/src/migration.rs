//! Migration of legacy Zed user-data directories to Orion Studio directories.
//!
//! Implements the safe, idempotent, atomic migration described in
//! `docs/plan/subplans/04-data-migration-and-compatibility.md`.
//!
//! This module is a pure library: nothing in the running application calls
//! these functions yet (the app-startup wiring is a later step), so it cannot
//! affect real user data unless explicitly invoked with real paths.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Name of the marker file written into the new directory on success.
pub const MIGRATION_MARKER_NAME: &str = ".orion_migration_marker";

/// Schema version of the migration marker format. Bump on incompatible changes.
pub const MIGRATION_SCHEMA_VERSION: u32 = 1;

/// Legacy application name (Zed) used to derive legacy directory paths.
const LEGACY_APP_NAME: &str = "Zed";
/// Legacy lowercased slug (`zed`) used on Linux/FreeBSD XDG paths.
const LEGACY_APP_NAME_LOWERCASE: &str = "zed";

/// Observable migration state. Designed so repeated startups can decide
/// whether work remains without using "directory exists" as the sole signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationState {
    /// No legacy directory was found; the new directory was initialized.
    NoLegacyData,
    /// A success marker is already present; nothing was copied this run.
    Completed,
    /// Legacy data was migrated into the new directory this run.
    Migrated,
    /// A legacy marker exists but declares an incompatible schema version.
    /// The migration stops safely and leaves both directories untouched.
    IncompatibleVersion { found: u32, expected: u32 },
}

/// Error surfaced by the migration. On any error the legacy data is guaranteed
/// to remain readable and untouched (equivalent to "failed but old readable").
#[derive(Debug)]
pub enum MigrationError {
    Io {
        source: io::Error,
        path: PathBuf,
    },
    /// The new directory's parent path is missing, so no safe temp location
    /// exists for an atomic rename.
    NoParent(PathBuf),
}

impl MigrationError {
    fn io(source: io::Error, path: PathBuf) -> Self {
        MigrationError::Io { source, path }
    }
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationError::Io { source, path } => {
                write!(f, "migration I/O error at {}: {}", path.display(), source)
            }
            MigrationError::NoParent(path) => {
                write!(
                    f,
                    "migration target has no parent directory: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for MigrationError {}

/// Returns the legacy config directory path (Zed), mirroring `config_dir`'s
/// platform logic but with the legacy app name.
///
/// Provided for the later app-startup wiring; not invoked automatically here.
pub fn legacy_config_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        dirs::config_dir().map(|d| d.join(LEGACY_APP_NAME))
    } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
        if let Ok(p) = std::env::var("FLATPAK_XDG_CONFIG_HOME") {
            Some(PathBuf::from(p))
        } else {
            dirs::config_dir().map(|d| d.join(LEGACY_APP_NAME_LOWERCASE))
        }
    } else {
        Some(
            crate::home_dir()
                .join(".config")
                .join(LEGACY_APP_NAME_LOWERCASE),
        )
    }
}

/// Returns the legacy data directory path (Zed), mirroring `data_dir`'s
/// platform logic but with the legacy app name.
///
/// Provided for the later app-startup wiring; not invoked automatically here.
pub fn legacy_data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(
            crate::home_dir()
                .join("Library/Application Support")
                .join(LEGACY_APP_NAME),
        )
    } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
        if let Ok(p) = std::env::var("FLATPAK_XDG_DATA_HOME") {
            Some(PathBuf::from(p))
        } else {
            dirs::data_local_dir().map(|d| d.join(LEGACY_APP_NAME_LOWERCASE))
        }
    } else if cfg!(target_os = "windows") {
        dirs::data_local_dir().map(|d| d.join(LEGACY_APP_NAME))
    } else {
        legacy_config_dir()
    }
}

/// Migrate a single legacy root directory into its new Orion root.
///
/// Behaviour:
/// * If `new` already carries a success marker, returns `Completed` (idempotent;
///   no recopy).
/// * If `new` carries a marker with an incompatible schema version, returns
///   `IncompatibleVersion` and leaves both directories untouched.
/// * If `old` does not exist, the new directory is initialized and a success
///   marker is written; returns `NoLegacyData`.
/// * If `old` exists and `new` does not, the tree is copied into a sibling temp
///   directory, a marker is written there, and the temp directory is renamed
///   onto `new` atomically. The legacy directory is retained (never deleted).
/// * If `old` exists and `new` already exists (partial install), the trees are
///   merged without overwriting files already present in `new`.
///
/// Any failure returns `Err` with the legacy data intact. Symlinks and special
/// files (sockets, fifos, devices) are never followed or copied.
pub fn migrate_root(old: &Path, new: &Path) -> Result<MigrationState, MigrationError> {
    if let Some(marker) = read_marker(new)? {
        if marker.schema != MIGRATION_SCHEMA_VERSION {
            return Ok(MigrationState::IncompatibleVersion {
                found: marker.schema,
                expected: MIGRATION_SCHEMA_VERSION,
            });
        }
        if marker.result == "success" {
            return Ok(MigrationState::Completed);
        }
        // A non-success marker (e.g. from an interrupted run) falls through to
        // re-attempt migration below.
    }

    if !old.exists() {
        fs::create_dir_all(new).map_err(|e| MigrationError::io(e, new.to_path_buf()))?;
        write_marker(new, old, "success")?;
        return Ok(MigrationState::NoLegacyData);
    }

    if new.exists() {
        copy_tree_merge(old, new)?;
        write_marker(new, old, "success")?;
        return Ok(MigrationState::Migrated);
    }

    let parent = new
        .parent()
        .ok_or_else(|| MigrationError::NoParent(new.to_path_buf()))?;
    fs::create_dir_all(parent).map_err(|e| MigrationError::io(e, parent.to_path_buf()))?;

    let temp = parent.join(format!(
        "{}.migrating-{}-{}",
        new.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id(),
        now_nanos(),
    ));
    if temp.exists() {
        fs::remove_dir_all(&temp).map_err(|e| MigrationError::io(e, temp.clone()))?;
    }

    if let Err(e) = copy_tree(old, &temp) {
        let _ = fs::remove_dir_all(&temp);
        return Err(e);
    }
    if let Err(e) = write_marker(&temp, old, "success") {
        let _ = fs::remove_dir_all(&temp);
        return Err(e);
    }

    match fs::rename(&temp, new) {
        Ok(()) => Ok(MigrationState::Migrated),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            // A concurrent process created `new` between our check and rename.
            // Merge into it instead and discard the temp copy.
            let _ = fs::remove_dir_all(&temp);
            copy_tree_merge(old, new)?;
            write_marker(new, old, "success")?;
            Ok(MigrationState::Migrated)
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&temp);
            Err(MigrationError::io(e, new.to_path_buf()))
        }
    }
}

/// Recursively copy `from` into `to`, creating `to` if needed.
/// Symlinks and special files are skipped.
fn copy_tree(from: &Path, to: &Path) -> Result<(), MigrationError> {
    fs::create_dir_all(to).map_err(|e| MigrationError::io(e, to.to_path_buf()))?;
    for entry in fs::read_dir(from).map_err(|e| MigrationError::io(e, from.to_path_buf()))? {
        let entry = entry.map_err(|e| MigrationError::io(e, from.to_path_buf()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| MigrationError::io(e, path.clone()))?;
        let dest = to.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        } else if file_type.is_dir() {
            copy_tree(&path, &dest)?;
        } else if file_type.is_file() {
            fs::copy(&path, &dest).map_err(|e| MigrationError::io(e, path.clone()))?;
        } else {
            // Sockets, fifos, devices: unsupported, do not migrate.
            continue;
        }
    }
    Ok(())
}

/// Merge `from` into `to` without overwriting files already present in `to`.
/// Used when a partial new directory already exists.
fn copy_tree_merge(from: &Path, to: &Path) -> Result<(), MigrationError> {
    fs::create_dir_all(to).map_err(|e| MigrationError::io(e, to.to_path_buf()))?;
    for entry in fs::read_dir(from).map_err(|e| MigrationError::io(e, from.to_path_buf()))? {
        let entry = entry.map_err(|e| MigrationError::io(e, from.to_path_buf()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| MigrationError::io(e, path.clone()))?;
        let dest = to.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        } else if file_type.is_dir() {
            copy_tree_merge(&path, &dest)?;
        } else if file_type.is_file() {
            if !dest.exists() {
                fs::copy(&path, &dest).map_err(|e| MigrationError::io(e, path.clone()))?;
            }
        } else {
            continue;
        }
    }
    Ok(())
}

fn write_marker(dir: &Path, source: &Path, result: &str) -> Result<(), MigrationError> {
    fs::create_dir_all(dir).map_err(|e| MigrationError::io(e, dir.to_path_buf()))?;
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let content = format!(
        "orion-migration-marker v1\nschema={}\nsource={}\ntarget={}\ntime={}\nresult={}\n",
        MIGRATION_SCHEMA_VERSION,
        source.display(),
        dir.display(),
        time,
        result
    );
    let marker_path = dir.join(MIGRATION_MARKER_NAME);
    fs::write(&marker_path, content).map_err(|e| MigrationError::io(e, marker_path))?;
    Ok(())
}

struct Marker {
    schema: u32,
    result: String,
}

fn read_marker(dir: &Path) -> Result<Option<Marker>, MigrationError> {
    let marker_path = dir.join(MIGRATION_MARKER_NAME);
    if !marker_path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&marker_path).map_err(|e| MigrationError::io(e, marker_path.clone()))?;
    let mut schema = None;
    let mut result = None;
    let mut first = true;
    for line in content.lines() {
        if first {
            first = false;
            if !line.starts_with("orion-migration-marker") {
                return Ok(None);
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "schema" => schema = v.parse().ok(),
                "result" => result = Some(v.to_string()),
                // `source`, `target`, and `time` are written to the marker file
                // as a human-readable audit trail but are not needed to decide
                // migration state, so they are parsed and discarded here.
                _ => {}
            }
        }
    }
    Ok(Some(Marker {
        schema: schema.unwrap_or(0),
        result: result.unwrap_or_default(),
    }))
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "orion-migrate-test-{}-{}",
            std::process::id(),
            now_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fresh_install_without_legacy() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        assert!(!old.exists());

        let state = migrate_root(&old, &new).unwrap();
        assert_eq!(state, MigrationState::NoLegacyData);
        assert!(new.exists());
        assert!(new.join(MIGRATION_MARKER_NAME).exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn upgrade_only_legacy_is_migrated_and_old_retained() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(old.join("db")).unwrap();
        fs::write(old.join("db").join("settings.db"), b"data").unwrap();
        fs::write(old.join("settings.json"), b"{}").unwrap();

        let state = migrate_root(&old, &new).unwrap();
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("settings.json").exists());
        assert!(new.join("db").join("settings.db").exists());

        // Old directory is retained as backup; never deleted in the first round.
        assert!(old.join("settings.json").exists());
        assert!(old.join("db").join("settings.db").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn partial_new_directory_is_merged_without_overwrite() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("from_old.json"), b"old").unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("from_new.json"), b"new").unwrap();

        let state = migrate_root(&old, &new).unwrap();
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("from_old.json").exists());
        assert!(new.join("from_new.json").exists());
        // Pre-existing new file is not overwritten.
        assert_eq!(
            fs::read_to_string(new.join("from_new.json")).unwrap(),
            "new"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repeated_migration_is_idempotent() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("settings.json"), b"{}").unwrap();

        assert_eq!(migrate_root(&old, &new).unwrap(), MigrationState::Migrated);
        // Second run sees the success marker and does not recopy.
        assert_eq!(migrate_root(&old, &new).unwrap(), MigrationState::Completed);
        assert_eq!(fs::read_dir(&new).unwrap().count(), 2); // settings.json + marker

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn interrupted_partial_new_retries_successfully() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap(); // represents a failed previous run
        fs::write(old.join("settings.json"), b"{}").unwrap();

        let state = migrate_root(&old, &new).unwrap();
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("settings.json").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_failure_preserves_legacy_data() {
        let root = temp_root();
        let old = root.join("zed");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("settings.json"), b"secret-config").unwrap();

        // Make the new target's parent a regular file so no safe temp location
        // exists; migration must error rather than destroy legacy data.
        let parent_file = root.join("blocker");
        fs::write(&parent_file, b"not a directory").unwrap();
        let new = parent_file.join("orion-studio");

        let result = migrate_root(&old, &new);
        assert!(result.is_err());

        // Legacy data is intact and untouched.
        assert!(old.join("settings.json").exists());
        assert_eq!(
            fs::read_to_string(old.join("settings.json")).unwrap(),
            "secret-config"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_copied() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old).unwrap();
        let target = root.join("somewhere");
        fs::write(&target, b"x").unwrap();
        std::os::unix::fs::symlink(&target, old.join("link")).unwrap();
        fs::write(old.join("real.json"), b"{}").unwrap();

        let state = migrate_root(&old, &new).unwrap();
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("real.json").exists());
        assert!(!new.join("link").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn secrets_are_copied_and_legacy_retained() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(old.join("auth")).unwrap();
        fs::write(old.join("auth").join("token.json"), b"credential").unwrap();

        let state = migrate_root(&old, &new).unwrap();
        assert_eq!(state, MigrationState::Migrated);
        // The credential is present in the new location...
        assert!(new.join("auth").join("token.json").exists());
        // ...and the legacy directory is retained as backup (no deletion).
        assert!(old.join("auth").join("token.json").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn incompatible_marker_stops_safely() {
        let root = temp_root();
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("settings.json"), b"{}").unwrap();
        fs::create_dir_all(&new).unwrap();
        // Pre-seed an incompatible-version success marker.
        fs::write(
            new.join(MIGRATION_MARKER_NAME),
            "orion-migration-marker v1\nschema=999\nsource=/old\ntarget=/new\ntime=0\nresult=success\n",
        )
        .unwrap();

        let state = migrate_root(&old, &new).unwrap();
        assert_eq!(
            state,
            MigrationState::IncompatibleVersion {
                found: 999,
                expected: MIGRATION_SCHEMA_VERSION
            }
        );
        // Old data was not copied into new (new only has the marker).
        assert_eq!(fs::read_dir(&new).unwrap().count(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn legacy_dirs_use_zed_naming() {
        let legacy_config = legacy_config_dir().expect("legacy config dir resolvable");
        let legacy_data = legacy_data_dir().expect("legacy data dir resolvable");
        if cfg!(target_os = "macos") {
            assert!(
                legacy_data.ends_with("Application Support/Zed"),
                "unexpected legacy data dir: {legacy_data:?}"
            );
            assert!(
                legacy_config.ends_with(".config/zed"),
                "unexpected legacy config dir: {legacy_config:?}"
            );
        } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            assert!(
                legacy_data.to_string_lossy().ends_with("zed"),
                "unexpected legacy data dir: {legacy_data:?}"
            );
            assert!(
                legacy_config.to_string_lossy().ends_with("zed"),
                "unexpected legacy config dir: {legacy_config:?}"
            );
        } else if cfg!(target_os = "windows") {
            assert!(
                legacy_data.ends_with("Zed"),
                "unexpected legacy data dir: {legacy_data:?}"
            );
            assert!(
                legacy_config.ends_with("Zed"),
                "unexpected legacy config dir: {legacy_config:?}"
            );
        }
    }
}
