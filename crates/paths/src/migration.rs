//! Migration of legacy Zed user-data directories to Orion Studio directories.
//!
//! Implements the safe, idempotent, atomic migration described in
//! `docs/plan/subplans/04-data-migration-and-compatibility.md`.
//!
//! [`migrate_legacy_user_data`] is the startup entry point. The application
//! calls it before creating or opening Orion Studio persistence so legacy
//! databases cannot be shadowed by newly initialized destination files (see
//! `crates/zed/src/main.rs`). The lower-level [`migrate_root`] operates on a
//! single root directory and is kept public for tests and future callers.
//!
//! Safety contract:
//! * Any failure leaves the legacy directory intact and readable.
//! * A marker file that exists but is corrupted, malformed, or records a
//!   different legacy source fails closed (returns an error) instead of being
//!   treated as "no migration state".
//! * The legacy directory is retained after migration (never deleted).
//! * Symlinks and special files (sockets, fifos, devices) are never copied.
//! * No error is silently discarded: every fallible operation either
//!   propagates an error with source/target path and stage context, or is an
//!   explicitly reported cleanup failure.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(unix))]
use std::fs::OpenOptions;

/// Name of the marker file written into the new directory on success.
pub const MIGRATION_MARKER_NAME: &str = ".orion_migration_marker";

/// Schema version of the migration marker format. Bump on incompatible changes.
pub const MIGRATION_SCHEMA_VERSION: u32 = 3;

const LEGACY_UNTRUSTED_MIGRATION_SCHEMA_VERSION: u32 = 2;

const MIGRATION_LOCK_NAME: &str = ".orion-studio-migration.lock";
const MARKER_TEMP_PREFIX: &str = ".orion-migration-marker.tmp";
const FILE_TEMP_PREFIX: &str = ".orion-migration-file.tmp";
const DIRECTORY_TEMP_PREFIX: &str = ".orion-migrating";
const MAX_SOURCE_STABILITY_ATTEMPTS: usize = 3;

#[cfg(unix)]
mod unix_ffi {
    use std::ffi::{CString, OsStr};
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::raw::{c_char, c_int, c_uint};
    use std::os::unix::ffi::OsStrExt;

    const O_RDONLY: c_int = 0;
    const O_RDWR: c_int = 2;

    #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
    const O_CREAT: c_int = 0x0000_0200;
    #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
    const O_EXCL: c_int = 0x0000_0800;
    #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
    const O_NONBLOCK: c_int = 0x0000_0004;
    #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
    const O_NOFOLLOW: c_int = 0x0000_0100;

    #[cfg(target_vendor = "apple")]
    const O_DIRECTORY: c_int = 0x0010_0000;
    #[cfg(target_vendor = "apple")]
    const O_CLOEXEC: c_int = 0x0100_0000;
    #[cfg(target_vendor = "apple")]
    const AT_REMOVEDIR: c_int = 0x0080;

    #[cfg(target_os = "freebsd")]
    const O_DIRECTORY: c_int = 0x0002_0000;
    #[cfg(target_os = "freebsd")]
    const O_CLOEXEC: c_int = 0x0010_0000;
    #[cfg(target_os = "freebsd")]
    const AT_REMOVEDIR: c_int = 0x0800;

    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6"
        )
    ))]
    const O_CREAT: c_int = 0x0100;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6"
        )
    ))]
    const O_EXCL: c_int = 0x0400;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6"
        )
    ))]
    const O_NONBLOCK: c_int = 0x0080;

    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(target_arch = "sparc", target_arch = "sparc64")
    ))]
    const O_CREAT: c_int = 0x0200;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(target_arch = "sparc", target_arch = "sparc64")
    ))]
    const O_EXCL: c_int = 0x0800;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(target_arch = "sparc", target_arch = "sparc64")
    ))]
    const O_NONBLOCK: c_int = 0x4000;

    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6",
            target_arch = "sparc",
            target_arch = "sparc64"
        ))
    ))]
    const O_CREAT: c_int = 0x0040;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6",
            target_arch = "sparc",
            target_arch = "sparc64"
        ))
    ))]
    const O_EXCL: c_int = 0x0080;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(any(
            target_arch = "mips",
            target_arch = "mips32r6",
            target_arch = "mips64",
            target_arch = "mips64r6",
            target_arch = "sparc",
            target_arch = "sparc64"
        ))
    ))]
    const O_NONBLOCK: c_int = 0x0800;

    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "powerpc",
            target_arch = "powerpc64"
        )
    ))]
    const O_DIRECTORY: c_int = 0x4000;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(any(
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "powerpc",
            target_arch = "powerpc64"
        ))
    ))]
    const O_DIRECTORY: c_int = 0x1_0000;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const O_NOFOLLOW: c_int = 0x2_0000;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "powerpc",
            target_arch = "powerpc64"
        )
    ))]
    const O_NOFOLLOW_ARCH: c_int = 0x8000;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "powerpc",
            target_arch = "powerpc64"
        )
    ))]
    const EFFECTIVE_O_NOFOLLOW: c_int = O_NOFOLLOW_ARCH;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(any(
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "powerpc",
            target_arch = "powerpc64"
        ))
    ))]
    const EFFECTIVE_O_NOFOLLOW: c_int = O_NOFOLLOW;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        any(target_arch = "sparc", target_arch = "sparc64")
    ))]
    const O_CLOEXEC: c_int = 0x0040_0000;
    #[cfg(all(
        any(target_os = "linux", target_os = "android"),
        not(any(target_arch = "sparc", target_arch = "sparc64"))
    ))]
    const O_CLOEXEC: c_int = 0x0008_0000;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    const AT_REMOVEDIR: c_int = 0x0200;

    #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
    const EFFECTIVE_O_NOFOLLOW: c_int = O_NOFOLLOW;

    #[cfg(any(
        target_vendor = "apple",
        target_os = "freebsd",
        all(target_os = "android", target_pointer_width = "32")
    ))]
    type Mode = u16;
    #[cfg(all(
        unix,
        not(any(
            target_vendor = "apple",
            target_os = "freebsd",
            all(target_os = "android", target_pointer_width = "32")
        ))
    ))]
    type Mode = u32;

    pub const DIRECTORY_OPEN_FLAGS: c_int =
        O_RDONLY | O_DIRECTORY | EFFECTIVE_O_NOFOLLOW | O_CLOEXEC;
    pub const INSPECT_OPEN_FLAGS: c_int = O_RDONLY | O_NONBLOCK | EFFECTIVE_O_NOFOLLOW | O_CLOEXEC;
    pub const FILE_READ_OPEN_FLAGS: c_int = INSPECT_OPEN_FLAGS;
    pub const FILE_READ_WRITE_OPEN_FLAGS: c_int =
        O_RDWR | O_NONBLOCK | EFFECTIVE_O_NOFOLLOW | O_CLOEXEC;
    pub const FILE_CREATE_FLAGS: c_int =
        O_RDWR | O_CREAT | O_EXCL | EFFECTIVE_O_NOFOLLOW | O_CLOEXEC;

    unsafe extern "C" {
        fn openat(directory: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
        fn mkdirat(directory: c_int, path: *const c_char, mode: Mode) -> c_int;
        fn renameat(
            source_directory: c_int,
            source_path: *const c_char,
            destination_directory: c_int,
            destination_path: *const c_char,
        ) -> c_int;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        fn renameat2(
            source_directory: c_int,
            source_path: *const c_char,
            destination_directory: c_int,
            destination_path: *const c_char,
            flags: c_uint,
        ) -> c_int;
        #[cfg(target_vendor = "apple")]
        fn renameatx_np(
            source_directory: c_int,
            source_path: *const c_char,
            destination_directory: c_int,
            destination_path: *const c_char,
            flags: c_uint,
        ) -> c_int;
        fn linkat(
            source_directory: c_int,
            source_path: *const c_char,
            destination_directory: c_int,
            destination_path: *const c_char,
            flags: c_int,
        ) -> c_int;
        fn unlinkat(directory: c_int, path: *const c_char, flags: c_int) -> c_int;
    }

    fn component(name: &OsStr) -> io::Result<CString> {
        CString::new(name.as_bytes()).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path component contains a NUL byte: {error}"),
            )
        })
    }

    pub fn open_child(
        parent: &File,
        name: &OsStr,
        flags: c_int,
        mode: Option<u32>,
    ) -> io::Result<File> {
        let name = component(name)?;
        let descriptor = match mode {
            Some(mode) => unsafe {
                openat(parent.as_raw_fd(), name.as_ptr(), flags, mode as c_uint)
            },
            None => unsafe { openat(parent.as_raw_fd(), name.as_ptr(), flags) },
        };
        if descriptor < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(descriptor) })
        }
    }

    pub fn create_directory(parent: &File, name: &OsStr, mode: u32) -> io::Result<()> {
        let name = component(name)?;
        let mode = Mode::try_from(mode).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("directory mode is out of range: {error}"),
            )
        })?;
        if unsafe { mkdirat(parent.as_raw_fd(), name.as_ptr(), mode) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn rename_child(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        let source_name = component(source_name)?;
        let destination_name = component(destination_name)?;
        if unsafe {
            renameat(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub fn rename_child_no_replace(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        const RENAME_NOREPLACE: c_uint = 1;
        let source_name = component(source_name)?;
        let destination_name = component(destination_name)?;
        if unsafe {
            renameat2(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                RENAME_NOREPLACE,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(target_vendor = "apple")]
    pub fn rename_child_no_replace(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        const RENAME_EXCL: c_uint = 4;
        let source_name = component(source_name)?;
        let destination_name = component(destination_name)?;
        if unsafe {
            renameatx_np(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                RENAME_EXCL,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[cfg(all(
        unix,
        not(any(target_os = "linux", target_os = "android", target_vendor = "apple"))
    ))]
    pub fn rename_child_no_replace(
        _source_parent: &File,
        _source_name: &OsStr,
        _destination_parent: &File,
        _destination_name: &OsStr,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "atomic no-replace directory rename is unavailable",
        ))
    }

    pub fn hard_link_child(
        source_parent: &File,
        source_name: &OsStr,
        destination_parent: &File,
        destination_name: &OsStr,
    ) -> io::Result<()> {
        let source_name = component(source_name)?;
        let destination_name = component(destination_name)?;
        if unsafe {
            linkat(
                source_parent.as_raw_fd(),
                source_name.as_ptr(),
                destination_parent.as_raw_fd(),
                destination_name.as_ptr(),
                0,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn remove_child(parent: &File, name: &OsStr, directory: bool) -> io::Result<()> {
        let name = component(name)?;
        let flags = if directory { AT_REMOVEDIR } else { 0 };
        if unsafe { unlinkat(parent.as_raw_fd(), name.as_ptr(), flags) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn error_means_symlink(error: &io::Error) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const ELOOP: i32 = 40;
        #[cfg(any(target_vendor = "apple", target_os = "freebsd"))]
        const ELOOP: i32 = 62;

        error.raw_os_error() == Some(ELOOP)
    }
}

#[cfg(target_os = "windows")]
mod windows_ffi {
    use std::ffi::c_void;
    use std::fs::File;
    use std::io;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            file: *mut c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    pub fn file_identity(file: &File) -> io::Result<(u64, u128)> {
        let mut information = ByHandleFileInformation {
            file_attributes: 0,
            creation_time: FileTime { low: 0, high: 0 },
            last_access_time: FileTime { low: 0, high: 0 },
            last_write_time: FileTime { low: 0, high: 0 },
            volume_serial_number: 0,
            file_size_high: 0,
            file_size_low: 0,
            number_of_links: 0,
            file_index_high: 0,
            file_index_low: 0,
        };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((
            u64::from(information.volume_serial_number),
            u128::from(
                (u64::from(information.file_index_high) << 32)
                    | u64::from(information.file_index_low),
            ),
        ))
    }

    pub fn move_file(existing: &[u16], new: &[u16], replace: bool) -> io::Result<()> {
        const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
        const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
        let flags = if replace {
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
        } else {
            0
        };
        if unsafe { MoveFileExW(existing.as_ptr(), new.as_ptr(), flags) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

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
}

/// Outcome of migrating the config and data roots during startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOutcome {
    /// State of the config-root migration, or `None` when no legacy config
    /// data was found and the config root was left untouched.
    pub config: Option<MigrationState>,
    /// State of the data-root migration, or `None` when no legacy data was
    /// found and the data root was left untouched.
    pub data: Option<MigrationState>,
}

/// Error surfaced by the migration. On any error the legacy data is guaranteed
/// to remain readable and untouched (equivalent to "failed but old readable").
///
/// Every variant that wraps an I/O error also carries the affected path and a
/// short `context` string naming the migration stage, so failures can be
/// reported with source, target, and stage information.
#[derive(Debug)]
pub enum MigrationError {
    /// An I/O operation failed. `path` identifies the path involved and
    /// `context` names the migration stage that failed.
    Io {
        source: io::Error,
        path: PathBuf,
        context: &'static str,
    },
    /// The new directory's parent path is missing, so no safe temp location
    /// exists for an atomic rename.
    NoParent(PathBuf),
    /// A marker file exists but is malformed: empty, wrong header, duplicate
    /// fields, missing required fields, or a non-numeric schema. Migration
    /// fails closed because the state is ambiguous.
    MarkerCorrupted { path: PathBuf, reason: String },
    /// A valid marker records a different legacy source directory than the
    /// one being migrated now. Merging could mix data from two sources, so
    /// migration fails closed.
    MarkerSourceMismatch {
        marker_path: PathBuf,
        recorded_source: PathBuf,
        current_source: PathBuf,
    },
    /// A marker was written by a migration schema this binary cannot safely
    /// interpret. Startup must stop instead of continuing with ambiguous data.
    IncompatibleVersion {
        marker_path: PathBuf,
        found: u32,
        expected: u32,
    },
    /// A marker cannot be trusted as complete, but its recorded legacy source
    /// is unavailable, so the destination cannot be verified or upgraded.
    MarkerVerificationSourceMissing {
        marker_path: PathBuf,
        source: PathBuf,
        schema: u32,
    },
    /// Source and destination resolve to the same directory or one contains
    /// the other, which would recurse or merge a tree into itself.
    OverlappingRoots {
        source: PathBuf,
        destination: PathBuf,
    },
    /// A path that migration may read from or write through is a symbolic link.
    UnsafeSymlink {
        path: PathBuf,
        context: &'static str,
    },
    /// A pathname no longer identifies the object that was opened and pinned.
    /// This is treated as an attack or concurrent replacement and fails closed.
    ObjectIdentityChanged {
        path: PathBuf,
        context: &'static str,
    },
    /// The legacy tree changed while it was being snapshotted or copied.
    /// No success marker is written; a later startup can retry safely.
    SourceChanged { path: PathBuf, attempts: usize },
    /// A destination entry already exists but is not identical to the legacy
    /// entry. Neither entry is replaced, and no success marker is written.
    DestinationConflict {
        source: PathBuf,
        destination: PathBuf,
    },
    /// The migration's data steps succeeded, but a temporary path could not be
    /// removed afterwards. The migrated data is valid; this error reports the
    /// leftover path so it is not silently discarded.
    TempCleanupFailed { source: io::Error, path: PathBuf },
    /// A migration step failed, and cleaning up its temporary path also failed;
    /// both errors are surfaced.
    CleanupFailed {
        original: Box<MigrationError>,
        source: io::Error,
        path: PathBuf,
    },
}

impl MigrationError {
    fn io(source: io::Error, path: PathBuf, context: &'static str) -> Self {
        MigrationError::Io {
            source,
            path,
            context,
        }
    }
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationError::Io {
                source,
                path,
                context,
            } => write!(
                f,
                "migration failed while {context} at {}: {source}",
                path.display()
            ),
            MigrationError::NoParent(path) => write!(
                f,
                "migration target has no parent directory: {}",
                path.display()
            ),
            MigrationError::MarkerCorrupted { path, reason } => {
                write!(
                    f,
                    "migration marker is corrupted at {}: {reason}",
                    path.display()
                )
            }
            MigrationError::MarkerSourceMismatch {
                marker_path,
                recorded_source,
                current_source,
            } => write!(
                f,
                "migration marker at {} records source {recorded_source:?} but the current legacy source is {current_source:?}",
                marker_path.display()
            ),
            MigrationError::IncompatibleVersion {
                marker_path,
                found,
                expected,
            } => write!(
                f,
                "migration marker at {} uses schema {found}, but this build requires schema {expected}",
                marker_path.display()
            ),
            MigrationError::MarkerVerificationSourceMissing {
                marker_path,
                source,
                schema,
            } => write!(
                f,
                "migration marker at {} uses schema {schema} and requires verification, but legacy source {} is unavailable",
                marker_path.display(),
                source.display()
            ),
            MigrationError::OverlappingRoots {
                source,
                destination,
            } => write!(
                f,
                "migration source {} and destination {} overlap",
                source.display(),
                destination.display()
            ),
            MigrationError::UnsafeSymlink { path, context } => write!(
                f,
                "migration refused a symbolic link while {context} at {}",
                path.display()
            ),
            MigrationError::ObjectIdentityChanged { path, context } => write!(
                f,
                "migration detected an object replacement while {context} at {}",
                path.display()
            ),
            MigrationError::SourceChanged { path, attempts } => write!(
                f,
                "migration source changed during {attempts} stability attempts at {}",
                path.display()
            ),
            MigrationError::DestinationConflict {
                source,
                destination,
            } => write!(
                f,
                "migration cannot copy {} because a different entry already exists at {}",
                source.display(),
                destination.display()
            ),
            MigrationError::TempCleanupFailed { source, path } => write!(
                f,
                "migration succeeded but failed to remove temporary path {}: {source}",
                path.display()
            ),
            MigrationError::CleanupFailed {
                original,
                source,
                path,
            } => write!(
                f,
                "{original}; additionally failed to clean up temporary path {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MigrationError::Io { source, .. } => Some(source),
            MigrationError::TempCleanupFailed { source, .. } => Some(source),
            MigrationError::CleanupFailed { original, .. } => Some(original),
            _ => None,
        }
    }
}

/// Returns the legacy config directory path (Zed), mirroring `config_dir`'s
/// platform logic but with the legacy app name.
///
/// Used by the startup migration before Orion initializes its own paths.
pub fn legacy_config_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        dirs::config_dir().map(|d| d.join(LEGACY_APP_NAME))
    } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
        if let Some(path) = std::env::var_os("FLATPAK_XDG_CONFIG_HOME") {
            Some(flatpak_legacy_root(path))
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
/// Used by the startup migration before Orion initializes its own paths.
pub fn legacy_data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        Some(
            crate::home_dir()
                .join("Library/Application Support")
                .join(LEGACY_APP_NAME),
        )
    } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
        if let Some(path) = std::env::var_os("FLATPAK_XDG_DATA_HOME") {
            Some(flatpak_legacy_root(path))
        } else {
            dirs::data_local_dir().map(|d| d.join(LEGACY_APP_NAME_LOWERCASE))
        }
    } else if cfg!(target_os = "windows") {
        dirs::data_local_dir().map(|d| d.join(LEGACY_APP_NAME))
    } else {
        legacy_config_dir()
    }
}

fn flatpak_legacy_root(path: OsString) -> PathBuf {
    PathBuf::from(path).join(LEGACY_APP_NAME_LOWERCASE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FileIdentity {
    volume: u64,
    file: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Directory,
    RegularFile,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct NamedMetadata {
    kind: EntryKind,
    identity: Option<FileIdentity>,
}

struct SecureDirectory {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
}

struct SecureFile {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
}

impl SecureDirectory {
    fn open(path: &Path, context: &'static str) -> Result<Self, MigrationError> {
        secure_open_directory_path(path, context)
    }

    fn open_optional(path: &Path, context: &'static str) -> Result<Option<Self>, MigrationError> {
        match Self::open(path, context) {
            Ok(directory) => Ok(Some(directory)),
            Err(error) if migration_error_is_not_found(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn open_or_create(path: &Path, context: &'static str) -> Result<Self, MigrationError> {
        match Self::open(path, context) {
            Ok(directory) => Ok(directory),
            Err(error) if migration_error_is_not_found(&error) => {
                let parent = path
                    .parent()
                    .ok_or_else(|| MigrationError::NoParent(path.to_path_buf()))?;
                if parent == path {
                    return Err(error);
                }
                let parent = Self::open_or_create(parent, context)?;
                let name = validated_child_name(path)?;
                match parent.create_child_directory(name, context) {
                    Ok(directory) => Ok(directory),
                    Err(create_error) if migration_error_is_already_exists(&create_error) => {
                        parent.open_child_directory(name, context)
                    }
                    Err(create_error) => Err(create_error),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn from_file(path: PathBuf, file: File, context: &'static str) -> Result<Self, MigrationError> {
        let metadata = file
            .metadata()
            .map_err(|error| MigrationError::io(error, path.clone(), context))?;
        ensure_opened_metadata(&path, &metadata, EntryKind::Directory, context)?;
        let identity = file_identity(&file, &path, context)?;
        Ok(Self {
            path,
            file,
            identity,
        })
    }

    fn named_metadata(
        &self,
        name: &OsStr,
        context: &'static str,
    ) -> Result<Option<NamedMetadata>, MigrationError> {
        validate_single_component(name, &self.path)?;
        platform_named_metadata(self, name, context)
    }

    fn open_child_directory(
        &self,
        name: &OsStr,
        context: &'static str,
    ) -> Result<Self, MigrationError> {
        let expected = self
            .named_metadata(name, context)?
            .ok_or_else(|| not_found_error(self.path.join(name), context))?;
        if expected.kind == EntryKind::Symlink {
            return Err(MigrationError::UnsafeSymlink {
                path: self.path.join(name),
                context,
            });
        }
        if expected.kind != EntryKind::Directory {
            return Err(MigrationError::io(
                io::Error::new(io::ErrorKind::NotADirectory, "expected a directory"),
                self.path.join(name),
                context,
            ));
        }
        let path = self.path.join(name);
        let file = platform_open_child_directory(self, name, context)?;
        let directory = Self::from_file(path, file, context)?;
        verify_expected_identity(
            expected.identity,
            directory.identity,
            &directory.path,
            context,
        )?;
        self.verify_child_identity(name, directory.identity, context)?;
        Ok(directory)
    }

    fn create_child_directory(
        &self,
        name: &OsStr,
        context: &'static str,
    ) -> Result<Self, MigrationError> {
        validate_single_component(name, &self.path)?;
        platform_create_child_directory(self, name, context)?;
        self.sync("sync parent after creating directory")?;
        self.open_child_directory(name, context)
    }

    fn open_or_create_child_directory(
        &self,
        name: &OsStr,
        context: &'static str,
    ) -> Result<(Self, bool), MigrationError> {
        match self.named_metadata(name, context)? {
            Some(metadata) if metadata.kind == EntryKind::Symlink => {
                Err(MigrationError::UnsafeSymlink {
                    path: self.path.join(name),
                    context,
                })
            }
            Some(metadata) if metadata.kind == EntryKind::Directory => self
                .open_child_directory(name, context)
                .map(|directory| (directory, false)),
            Some(_) => Err(MigrationError::io(
                io::Error::new(io::ErrorKind::AlreadyExists, "non-directory entry exists"),
                self.path.join(name),
                context,
            )),
            None => match self.create_child_directory(name, context) {
                Ok(directory) => Ok((directory, true)),
                Err(error) if migration_error_is_already_exists(&error) => self
                    .open_child_directory(name, context)
                    .map(|directory| (directory, false)),
                Err(error) => Err(error),
            },
        }
    }

    fn open_child_file(
        &self,
        name: &OsStr,
        read_write: bool,
        context: &'static str,
    ) -> Result<SecureFile, MigrationError> {
        let expected = self
            .named_metadata(name, context)?
            .ok_or_else(|| not_found_error(self.path.join(name), context))?;
        if expected.kind == EntryKind::Symlink {
            return Err(MigrationError::UnsafeSymlink {
                path: self.path.join(name),
                context,
            });
        }
        if expected.kind != EntryKind::RegularFile {
            return Err(MigrationError::io(
                io::Error::new(io::ErrorKind::InvalidData, "expected a regular file"),
                self.path.join(name),
                context,
            ));
        }
        let path = self.path.join(name);
        let file = platform_open_child_file(self, name, read_write, context)?;
        let file = SecureFile::from_file(path, file, context)?;
        verify_expected_identity(expected.identity, file.identity, &file.path, context)?;
        self.verify_child_identity(name, file.identity, context)?;
        Ok(file)
    }

    fn create_child_file(
        &self,
        name: &OsStr,
        unix_mode: u32,
        context: &'static str,
    ) -> Result<SecureFile, MigrationError> {
        validate_single_component(name, &self.path)?;
        let path = self.path.join(name);
        let file = platform_create_child_file(self, name, unix_mode, context)?;
        let file = SecureFile::from_file(path, file, context)?;
        self.sync("sync parent after creating file")?;
        self.verify_child_identity(name, file.identity, context)?;
        Ok(file)
    }

    fn verify_child_identity(
        &self,
        name: &OsStr,
        identity: FileIdentity,
        context: &'static str,
    ) -> Result<(), MigrationError> {
        let current = self.named_metadata(name, context)?.ok_or_else(|| {
            MigrationError::ObjectIdentityChanged {
                path: self.path.join(name),
                context,
            }
        })?;
        if current.kind == EntryKind::Symlink {
            return Err(MigrationError::UnsafeSymlink {
                path: self.path.join(name),
                context,
            });
        }
        verify_expected_identity(current.identity, identity, &self.path.join(name), context)
    }

    fn verify_path_identity(&self, context: &'static str) -> Result<(), MigrationError> {
        let reopened = match Self::open(&self.path, context) {
            Ok(directory) => directory,
            Err(MigrationError::UnsafeSymlink { .. }) => {
                return Err(MigrationError::UnsafeSymlink {
                    path: self.path.clone(),
                    context,
                });
            }
            Err(error) if migration_error_is_not_found(&error) => {
                return Err(MigrationError::ObjectIdentityChanged {
                    path: self.path.clone(),
                    context,
                });
            }
            Err(error) => return Err(error),
        };
        verify_expected_identity(Some(self.identity), reopened.identity, &self.path, context)
    }

    fn entry_names(&self, context: &'static str) -> Result<Vec<OsString>, MigrationError> {
        platform_entry_names(self, context)
    }

    fn rename_child_to(
        &self,
        source_name: &OsStr,
        destination: &SecureDirectory,
        destination_name: &OsStr,
        no_replace: bool,
        context: &'static str,
    ) -> Result<(), MigrationError> {
        validate_single_component(source_name, &self.path)?;
        validate_single_component(destination_name, &destination.path)?;
        platform_rename_child(
            self,
            source_name,
            destination,
            destination_name,
            no_replace,
            context,
        )?;
        self.sync("sync source parent after rename")?;
        if self.identity != destination.identity {
            destination.sync("sync destination parent after rename")?;
        }
        Ok(())
    }

    fn hard_link_child_to(
        &self,
        source_name: &OsStr,
        destination: &SecureDirectory,
        destination_name: &OsStr,
        context: &'static str,
    ) -> Result<(), MigrationError> {
        validate_single_component(source_name, &self.path)?;
        validate_single_component(destination_name, &destination.path)?;
        platform_hard_link_child(self, source_name, destination, destination_name, context)?;
        destination.sync("sync destination parent after hard link")
    }

    fn remove_child_file(&self, name: &OsStr, context: &'static str) -> Result<(), MigrationError> {
        platform_remove_child(self, name, false, context)?;
        self.sync("sync parent after removing file")
    }

    fn remove_child_tree(&self, name: &OsStr, context: &'static str) -> Result<(), MigrationError> {
        let metadata = match self.named_metadata(name, context)? {
            Some(metadata) => metadata,
            None => return Ok(()),
        };
        if metadata.kind != EntryKind::Directory {
            return self.remove_child_file(name, context);
        }
        let child = self.open_child_directory(name, context)?;
        for entry_name in child.entry_names(context)? {
            child.remove_child_tree(&entry_name, context)?;
        }
        child.sync("sync temporary directory before removal")?;
        platform_remove_child(self, name, true, context)?;
        self.sync("sync parent after removing directory")
    }

    fn set_permissions(
        &self,
        permissions: fs::Permissions,
        context: &'static str,
    ) -> Result<(), MigrationError> {
        self.file
            .set_permissions(permissions)
            .map_err(|error| MigrationError::io(error, self.path.clone(), context))?;
        self.sync(context)
    }

    fn sync(&self, context: &'static str) -> Result<(), MigrationError> {
        self.file
            .sync_all()
            .map_err(|error| MigrationError::io(error, self.path.clone(), context))
    }
}

impl SecureFile {
    fn from_file(path: PathBuf, file: File, context: &'static str) -> Result<Self, MigrationError> {
        let metadata = file
            .metadata()
            .map_err(|error| MigrationError::io(error, path.clone(), context))?;
        ensure_opened_metadata(&path, &metadata, EntryKind::RegularFile, context)?;
        let identity = file_identity(&file, &path, context)?;
        Ok(Self {
            path,
            file,
            identity,
        })
    }

    fn metadata(&self, context: &'static str) -> Result<fs::Metadata, MigrationError> {
        self.file
            .metadata()
            .map_err(|error| MigrationError::io(error, self.path.clone(), context))
    }

    fn sync(&self, context: &'static str) -> Result<(), MigrationError> {
        self.file
            .sync_all()
            .map_err(|error| MigrationError::io(error, self.path.clone(), context))
    }
}

fn validated_child_name(path: &Path) -> Result<&OsStr, MigrationError> {
    let name = path
        .file_name()
        .ok_or_else(|| MigrationError::NoParent(path.to_path_buf()))?;
    validate_single_component(name, path)?;
    Ok(name)
}

fn validate_single_component(name: &OsStr, parent: &Path) -> Result<(), MigrationError> {
    let mut components = Path::new(name).components();
    let valid = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !name.is_empty();
    if valid {
        Ok(())
    } else {
        Err(MigrationError::io(
            io::Error::new(io::ErrorKind::InvalidInput, "expected one path component"),
            parent.join(name),
            "validate child path",
        ))
    }
}

fn not_found_error(path: PathBuf, context: &'static str) -> MigrationError {
    MigrationError::io(
        io::Error::new(io::ErrorKind::NotFound, "path does not exist"),
        path,
        context,
    )
}

fn migration_error_is_not_found(error: &MigrationError) -> bool {
    matches!(
        error,
        MigrationError::Io { source, .. } if source.kind() == io::ErrorKind::NotFound
    )
}

fn migration_error_is_already_exists(error: &MigrationError) -> bool {
    matches!(
        error,
        MigrationError::Io { source, .. } if source.kind() == io::ErrorKind::AlreadyExists
    )
}

fn verify_expected_identity(
    expected: Option<FileIdentity>,
    actual: FileIdentity,
    path: &Path,
    context: &'static str,
) -> Result<(), MigrationError> {
    if expected.is_some_and(|expected| expected != actual) {
        Err(MigrationError::ObjectIdentityChanged {
            path: path.to_path_buf(),
            context,
        })
    } else {
        Ok(())
    }
}

fn ensure_opened_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    expected: EntryKind,
    context: &'static str,
) -> Result<(), MigrationError> {
    if metadata_is_reparse_or_symlink(metadata) {
        return Err(MigrationError::UnsafeSymlink {
            path: path.to_path_buf(),
            context,
        });
    }
    let actual = if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::RegularFile
    } else {
        EntryKind::Other
    };
    if actual != expected {
        let message = match expected {
            EntryKind::Directory => "expected a directory",
            EntryKind::RegularFile => "expected a regular file",
            EntryKind::Symlink | EntryKind::Other => "unexpected filesystem object",
        };
        return Err(MigrationError::io(
            io::Error::new(io::ErrorKind::InvalidData, message),
            path.to_path_buf(),
            context,
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn metadata_is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(target_os = "windows")]
fn metadata_is_reparse_or_symlink(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(unix)]
fn file_identity(
    file: &File,
    path: &Path,
    context: &'static str,
) -> Result<FileIdentity, MigrationError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = file
        .metadata()
        .map_err(|error| MigrationError::io(error, path.to_path_buf(), context))?;
    Ok(FileIdentity {
        volume: metadata.dev(),
        file: u128::from(metadata.ino()),
    })
}

#[cfg(target_os = "windows")]
fn file_identity(
    file: &File,
    path: &Path,
    context: &'static str,
) -> Result<FileIdentity, MigrationError> {
    let (volume, file) = windows_ffi::file_identity(file)
        .map_err(|error| MigrationError::io(error, path.to_path_buf(), context))?;
    Ok(FileIdentity { volume, file })
}

#[cfg(not(any(unix, target_os = "windows")))]
fn file_identity(
    file: &File,
    path: &Path,
    context: &'static str,
) -> Result<FileIdentity, MigrationError> {
    let metadata = file
        .metadata()
        .map_err(|error| MigrationError::io(error, path.to_path_buf(), context))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    Ok(FileIdentity {
        volume: metadata.len(),
        file: modified,
    })
}

#[cfg(unix)]
fn secure_open_directory_path(
    path: &Path,
    context: &'static str,
) -> Result<SecureDirectory, MigrationError> {
    let normalized = normalize_absolute_path(path, context)?;
    let mut current_file = File::open("/")
        .map_err(|error| MigrationError::io(error, PathBuf::from("/"), "open filesystem root"))?;
    let mut current_path = PathBuf::from("/");
    for component in normalized.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        current_path.push(name);
        let opened =
            unix_ffi::open_child(&current_file, name, unix_ffi::DIRECTORY_OPEN_FLAGS, None)
                .map_err(|error| {
                    unix_open_error(&current_file, name, current_path.clone(), context, error)
                })?;
        let opened_identity = file_identity(&opened, &current_path, context)?;
        let named =
            unix_named_metadata(&current_file, name, &current_path, context)?.ok_or_else(|| {
                MigrationError::ObjectIdentityChanged {
                    path: current_path.clone(),
                    context,
                }
            })?;
        verify_expected_identity(named.identity, opened_identity, &current_path, context)?;
        current_file = opened;
    }
    SecureDirectory::from_file(normalized, current_file, context)
}

#[cfg(target_os = "windows")]
fn secure_open_directory_path(
    path: &Path,
    context: &'static str,
) -> Result<SecureDirectory, MigrationError> {
    let normalized = normalize_absolute_path(path, context)?;
    verify_windows_directory_chain(&normalized, context)?;
    let file = windows_open_directory(&normalized)
        .map_err(|error| MigrationError::io(error, normalized.clone(), context))?;
    SecureDirectory::from_file(normalized, file, context)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn secure_open_directory_path(
    path: &Path,
    context: &'static str,
) -> Result<SecureDirectory, MigrationError> {
    let normalized = normalize_absolute_path(path, context)?;
    let file = File::open(&normalized)
        .map_err(|error| MigrationError::io(error, normalized.clone(), context))?;
    SecureDirectory::from_file(normalized, file, context)
}

#[cfg(unix)]
fn unix_open_error(
    parent: &File,
    name: &OsStr,
    path: PathBuf,
    context: &'static str,
    error: io::Error,
) -> MigrationError {
    match unix_named_metadata(parent, name, &path, context) {
        Ok(Some(metadata)) if metadata.kind == EntryKind::Symlink => {
            MigrationError::UnsafeSymlink { path, context }
        }
        _ => MigrationError::io(error, path, context),
    }
}

#[cfg(unix)]
fn unix_named_metadata(
    parent: &File,
    name: &OsStr,
    path: &Path,
    context: &'static str,
) -> Result<Option<NamedMetadata>, MigrationError> {
    let file = match unix_ffi::open_child(parent, name, unix_ffi::INSPECT_OPEN_FLAGS, None) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) if unix_ffi::error_means_symlink(&error) => {
            return Ok(Some(NamedMetadata {
                kind: EntryKind::Symlink,
                identity: None,
            }));
        }
        Err(error) => return unix_named_metadata_fallback(path, context, error),
    };
    let metadata = file
        .metadata()
        .map_err(|error| MigrationError::io(error, path.to_path_buf(), context))?;
    let kind = if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::RegularFile
    } else {
        EntryKind::Other
    };
    Ok(Some(NamedMetadata {
        kind,
        identity: Some(file_identity(&file, path, context)?),
    }))
}

#[cfg(unix)]
fn unix_named_metadata_fallback(
    path: &Path,
    context: &'static str,
    open_error: io::Error,
) -> Result<Option<NamedMetadata>, MigrationError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(MigrationError::io(open_error, path.to_path_buf(), context)),
    };
    let kind = if metadata.file_type().is_symlink() {
        EntryKind::Symlink
    } else if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::RegularFile
    } else {
        EntryKind::Other
    };
    Ok(Some(NamedMetadata {
        kind,
        identity: Some(FileIdentity {
            volume: metadata.dev(),
            file: u128::from(metadata.ino()),
        }),
    }))
}

#[cfg(unix)]
fn platform_named_metadata(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<Option<NamedMetadata>, MigrationError> {
    unix_named_metadata(&parent.file, name, &parent.path.join(name), context)
}

#[cfg(not(unix))]
fn platform_named_metadata(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<Option<NamedMetadata>, MigrationError> {
    parent.verify_path_identity("verify parent before inspecting child")?;
    let path = parent.path.join(name);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(MigrationError::io(error, path, context)),
    };
    let kind = if metadata_is_reparse_or_symlink(&metadata) {
        EntryKind::Symlink
    } else if metadata.is_dir() {
        EntryKind::Directory
    } else if metadata.is_file() {
        EntryKind::RegularFile
    } else {
        EntryKind::Other
    };
    parent.verify_path_identity("verify parent after inspecting child")?;
    Ok(Some(NamedMetadata {
        kind,
        identity: None,
    }))
}

#[cfg(unix)]
fn platform_open_child_directory(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<File, MigrationError> {
    unix_ffi::open_child(&parent.file, name, unix_ffi::DIRECTORY_OPEN_FLAGS, None).map_err(
        |error| unix_open_error(&parent.file, name, parent.path.join(name), context, error),
    )
}

#[cfg(target_os = "windows")]
fn platform_open_child_directory(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<File, MigrationError> {
    parent.verify_path_identity("verify parent before opening child directory")?;
    let path = parent.path.join(name);
    let file = windows_open_directory(&path)
        .map_err(|error| MigrationError::io(error, path.clone(), context))?;
    parent.verify_path_identity("verify parent after opening child directory")?;
    Ok(file)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn platform_open_child_directory(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<File, MigrationError> {
    let path = parent.path.join(name);
    File::open(&path).map_err(|error| MigrationError::io(error, path, context))
}

#[cfg(unix)]
fn platform_create_child_directory(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<(), MigrationError> {
    unix_ffi::create_directory(&parent.file, name, 0o700)
        .map_err(|error| MigrationError::io(error, parent.path.join(name), context))
}

#[cfg(not(unix))]
fn platform_create_child_directory(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<(), MigrationError> {
    parent.verify_path_identity("verify parent before creating child directory")?;
    let path = parent.path.join(name);
    fs::create_dir(&path).map_err(|error| MigrationError::io(error, path.clone(), context))?;
    parent.verify_path_identity("verify parent after creating child directory")
}

#[cfg(unix)]
fn platform_open_child_file(
    parent: &SecureDirectory,
    name: &OsStr,
    read_write: bool,
    context: &'static str,
) -> Result<File, MigrationError> {
    let flags = if read_write {
        unix_ffi::FILE_READ_WRITE_OPEN_FLAGS
    } else {
        unix_ffi::FILE_READ_OPEN_FLAGS
    };
    unix_ffi::open_child(&parent.file, name, flags, None).map_err(|error| {
        unix_open_error(&parent.file, name, parent.path.join(name), context, error)
    })
}

#[cfg(target_os = "windows")]
fn platform_open_child_file(
    parent: &SecureDirectory,
    name: &OsStr,
    read_write: bool,
    context: &'static str,
) -> Result<File, MigrationError> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    parent.verify_path_identity("verify parent before opening child file")?;
    let path = parent.path.join(name);
    let mut options = OpenOptions::new();
    options.read(true).write(read_write);
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options
        .open(&path)
        .map_err(|error| MigrationError::io(error, path.clone(), context))?;
    parent.verify_path_identity("verify parent after opening child file")?;
    Ok(file)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn platform_open_child_file(
    parent: &SecureDirectory,
    name: &OsStr,
    read_write: bool,
    context: &'static str,
) -> Result<File, MigrationError> {
    let path = parent.path.join(name);
    OpenOptions::new()
        .read(true)
        .write(read_write)
        .open(&path)
        .map_err(|error| MigrationError::io(error, path, context))
}

#[cfg(unix)]
fn platform_create_child_file(
    parent: &SecureDirectory,
    name: &OsStr,
    unix_mode: u32,
    context: &'static str,
) -> Result<File, MigrationError> {
    unix_ffi::open_child(
        &parent.file,
        name,
        unix_ffi::FILE_CREATE_FLAGS,
        Some(unix_mode),
    )
    .map_err(|error| MigrationError::io(error, parent.path.join(name), context))
}

#[cfg(target_os = "windows")]
fn platform_create_child_file(
    parent: &SecureDirectory,
    name: &OsStr,
    _unix_mode: u32,
    context: &'static str,
) -> Result<File, MigrationError> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    parent.verify_path_identity("verify parent before creating child file")?;
    let path = parent.path.join(name);
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options
        .open(&path)
        .map_err(|error| MigrationError::io(error, path.clone(), context))?;
    parent.verify_path_identity("verify parent after creating child file")?;
    Ok(file)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn platform_create_child_file(
    parent: &SecureDirectory,
    name: &OsStr,
    _unix_mode: u32,
    context: &'static str,
) -> Result<File, MigrationError> {
    let path = parent.path.join(name);
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| MigrationError::io(error, path, context))
}

#[cfg(unix)]
fn platform_entry_names(
    directory: &SecureDirectory,
    context: &'static str,
) -> Result<Vec<OsString>, MigrationError> {
    directory.verify_path_identity("verify directory before listing entries")?;
    let mut names = Vec::new();
    for entry in fs::read_dir(&directory.path)
        .map_err(|error| MigrationError::io(error, directory.path.clone(), context))?
    {
        let entry =
            entry.map_err(|error| MigrationError::io(error, directory.path.clone(), context))?;
        names.push(entry.file_name());
    }
    directory.verify_path_identity("verify directory after listing entries")?;
    names.sort();
    Ok(names)
}

#[cfg(not(unix))]
fn platform_entry_names(
    directory: &SecureDirectory,
    context: &'static str,
) -> Result<Vec<OsString>, MigrationError> {
    directory.verify_path_identity("verify directory before listing entries")?;
    let mut names = Vec::new();
    for entry in fs::read_dir(&directory.path)
        .map_err(|error| MigrationError::io(error, directory.path.clone(), context))?
    {
        let entry =
            entry.map_err(|error| MigrationError::io(error, directory.path.clone(), context))?;
        names.push(entry.file_name());
    }
    directory.verify_path_identity("verify directory after listing entries")?;
    names.sort();
    Ok(names)
}

#[cfg(unix)]
fn platform_rename_child(
    source: &SecureDirectory,
    source_name: &OsStr,
    destination: &SecureDirectory,
    destination_name: &OsStr,
    no_replace: bool,
    context: &'static str,
) -> Result<(), MigrationError> {
    let result = if no_replace {
        unix_ffi::rename_child_no_replace(
            &source.file,
            source_name,
            &destination.file,
            destination_name,
        )
    } else {
        unix_ffi::rename_child(
            &source.file,
            source_name,
            &destination.file,
            destination_name,
        )
    };
    result.map_err(|error| {
        MigrationError::io(error, destination.path.join(destination_name), context)
    })
}

#[cfg(target_os = "windows")]
fn platform_rename_child(
    source: &SecureDirectory,
    source_name: &OsStr,
    destination: &SecureDirectory,
    destination_name: &OsStr,
    no_replace: bool,
    context: &'static str,
) -> Result<(), MigrationError> {
    use std::os::windows::ffi::OsStrExt;

    source.verify_path_identity("verify source parent before rename")?;
    destination.verify_path_identity("verify destination parent before rename")?;
    let source_path = source.path.join(source_name);
    let destination_path = destination.path.join(destination_name);
    let source_wide = source_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination_wide = destination_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    windows_ffi::move_file(&source_wide, &destination_wide, !no_replace)
        .map_err(|error| MigrationError::io(error, destination_path, context))?;
    source.verify_path_identity("verify source parent after rename")?;
    destination.verify_path_identity("verify destination parent after rename")
}

#[cfg(not(any(unix, target_os = "windows")))]
fn platform_rename_child(
    source: &SecureDirectory,
    source_name: &OsStr,
    destination: &SecureDirectory,
    destination_name: &OsStr,
    no_replace: bool,
    context: &'static str,
) -> Result<(), MigrationError> {
    source.verify_path_identity("verify source parent before rename")?;
    destination.verify_path_identity("verify destination parent before rename")?;
    let destination_path = destination.path.join(destination_name);
    if no_replace && destination_path.exists() {
        return Err(MigrationError::io(
            io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists"),
            destination_path,
            context,
        ));
    }
    fs::rename(source.path.join(source_name), &destination_path)
        .map_err(|error| MigrationError::io(error, destination_path, context))?;
    source.verify_path_identity("verify source parent after rename")?;
    destination.verify_path_identity("verify destination parent after rename")
}

#[cfg(unix)]
fn platform_hard_link_child(
    source: &SecureDirectory,
    source_name: &OsStr,
    destination: &SecureDirectory,
    destination_name: &OsStr,
    context: &'static str,
) -> Result<(), MigrationError> {
    unix_ffi::hard_link_child(
        &source.file,
        source_name,
        &destination.file,
        destination_name,
    )
    .map_err(|error| MigrationError::io(error, destination.path.join(destination_name), context))
}

#[cfg(not(unix))]
fn platform_hard_link_child(
    source: &SecureDirectory,
    source_name: &OsStr,
    destination: &SecureDirectory,
    destination_name: &OsStr,
    context: &'static str,
) -> Result<(), MigrationError> {
    source.verify_path_identity("verify source parent before hard link")?;
    destination.verify_path_identity("verify destination parent before hard link")?;
    let destination_path = destination.path.join(destination_name);
    fs::hard_link(source.path.join(source_name), &destination_path)
        .map_err(|error| MigrationError::io(error, destination_path, context))?;
    source.verify_path_identity("verify source parent after hard link")?;
    destination.verify_path_identity("verify destination parent after hard link")
}

#[cfg(unix)]
fn platform_remove_child(
    parent: &SecureDirectory,
    name: &OsStr,
    directory: bool,
    context: &'static str,
) -> Result<(), MigrationError> {
    unix_ffi::remove_child(&parent.file, name, directory)
        .map_err(|error| MigrationError::io(error, parent.path.join(name), context))
}

#[cfg(not(unix))]
fn platform_remove_child(
    parent: &SecureDirectory,
    name: &OsStr,
    directory: bool,
    context: &'static str,
) -> Result<(), MigrationError> {
    parent.verify_path_identity("verify parent before removing child")?;
    let path = parent.path.join(name);
    let result = if directory {
        fs::remove_dir(&path)
    } else {
        fs::remove_file(&path)
    };
    result.map_err(|error| MigrationError::io(error, path, context))?;
    parent.verify_path_identity("verify parent after removing child")
}

#[cfg(target_os = "windows")]
fn windows_open_directory(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path)
}

#[cfg(target_os = "windows")]
fn verify_windows_directory_chain(
    path: &Path,
    context: &'static str,
) -> Result<(), MigrationError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if !matches!(component, Component::Normal(_)) {
            continue;
        }
        let file = windows_open_directory(&current)
            .map_err(|error| MigrationError::io(error, current.clone(), context))?;
        let metadata = file
            .metadata()
            .map_err(|error| MigrationError::io(error, current.clone(), context))?;
        ensure_opened_metadata(&current, &metadata, EntryKind::Directory, context)?;
    }
    Ok(())
}

/// Migrate a single legacy root directory into its new Orion root.
///
/// Behaviour:
/// * If `new` already carries a success marker, returns `Completed` (idempotent;
///   no recopy).
/// * If `new` carries a marker with an incompatible schema version, returns an
///   error and leaves both directories untouched.
/// * If `new` carries a marker recording a different legacy source than `old`,
///   returns `MarkerSourceMismatch` (fail closed).
/// * If `new` carries a marker that is corrupted or malformed, returns
///   `MarkerCorrupted` (fail closed) instead of guessing at the state.
/// * If `old` does not exist, the new directory is initialized and a success
///   marker is written; returns `NoLegacyData`.
/// * If `old` exists and `new` does not, the tree is copied into a sibling temp
///   directory and installed without replacing a concurrent destination. The
///   durable success marker is written only after the final tree is synced.
/// * If `old` exists and `new` already exists (partial install), the trees are
///   merged without overwriting files already present in `new`. An identical
///   file is accepted as an idempotent retry; a different file or entry type is
///   reported as a conflict and prevents the success marker from being written.
///
/// Any failure returns `Err` with the legacy data intact. Symlinks and special
/// files (sockets, fifos, devices) are never followed or copied. Temporary
/// directories are removed on both success and failure; a cleanup failure is
/// reported instead of being silently discarded.
pub fn migrate_root(old: &Path, new: &Path) -> Result<MigrationState, MigrationError> {
    validate_root_relationship(old, new)?;
    let _migration_lock = acquire_migration_lock(new)?;
    validate_root_relationship(old, new)?;
    migrate_root_unlocked(old, new)
}

fn migrate_root_unlocked(old: &Path, new: &Path) -> Result<MigrationState, MigrationError> {
    migrate_root_unlocked_with_after_copy(old, new, |_| Ok(()))
}

fn migrate_root_unlocked_with_after_copy<F>(
    old: &Path,
    new: &Path,
    after_copy: F,
) -> Result<MigrationState, MigrationError>
where
    F: FnMut(usize) -> Result<(), MigrationError>,
{
    let marker_requiring_verification = match read_marker(new)? {
        Some(marker) => {
            validate_marker(&marker, old, new)?;
            if marker.version == MarkerVersion::CurrentV3 && marker.result == "success" {
                return Ok(MigrationState::Completed);
            }
            Some(marker)
        }
        None => None,
    };

    let parent_path = new
        .parent()
        .ok_or_else(|| MigrationError::NoParent(new.to_path_buf()))?;
    let destination_name = validated_child_name(new)?;
    let destination_parent =
        SecureDirectory::open_or_create(parent_path, "open migration destination parent")?;
    let source = SecureDirectory::open_optional(old, "open legacy source root")?;
    if source.is_none() {
        if let Some(marker) = marker_requiring_verification {
            return Err(MigrationError::MarkerVerificationSourceMissing {
                marker_path: new.join(MIGRATION_MARKER_NAME),
                source: old.to_path_buf(),
                schema: marker.schema,
            });
        }
        let (destination, _) = destination_parent
            .open_or_create_child_directory(destination_name, "initialize new directory")?;
        if SecureDirectory::open_optional(old, "recheck absent legacy source")?.is_some() {
            return Err(MigrationError::SourceChanged {
                path: old.to_path_buf(),
                attempts: 1,
            });
        }
        sync_tree(&destination)?;
        destination_parent.sync("sync destination parent before marker")?;
        destination.verify_path_identity("verify destination before marker")?;
        write_marker_in_directory(&destination, old, new, "success")?;
        return Ok(MigrationState::NoLegacyData);
    }
    drop(source);

    let staged = create_stable_staging(old, &destination_parent, after_copy)?;
    install_staged_tree(staged, &destination_parent, destination_name, old, new)
}

struct StagedTree {
    name: OsString,
    directory: SecureDirectory,
    snapshot: TreeSnapshot,
}

fn install_staged_tree(
    staged: StagedTree,
    destination_parent: &SecureDirectory,
    destination_name: &OsStr,
    old: &Path,
    new: &Path,
) -> Result<MigrationState, MigrationError> {
    match destination_parent.named_metadata(destination_name, "inspect migration destination")? {
        Some(metadata) if metadata.kind == EntryKind::Symlink => {
            let original = MigrationError::UnsafeSymlink {
                path: destination_parent.path.join(destination_name),
                context: "inspect migration destination",
            };
            return Err(cleanup_staging_after_failure(
                destination_parent,
                &staged.name,
                original,
            ));
        }
        Some(metadata) if metadata.kind != EntryKind::Directory => {
            let original = MigrationError::io(
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "destination is not a directory",
                ),
                destination_parent.path.join(destination_name),
                "inspect migration destination",
            );
            return Err(cleanup_staging_after_failure(
                destination_parent,
                &staged.name,
                original,
            ));
        }
        Some(_) => {
            return merge_staged_tree(staged, destination_parent, destination_name, old, new);
        }
        None => {}
    }

    match destination_parent.rename_child_to(
        &staged.name,
        destination_parent,
        destination_name,
        true,
        "rename stable staging directory into place",
    ) {
        Ok(()) => {
            let destination = destination_parent
                .open_child_directory(destination_name, "open installed migration destination")?;
            verify_expected_identity(
                Some(staged.directory.identity),
                destination.identity,
                &destination.path,
                "verify renamed staging directory identity",
            )?;
            sync_tree(&destination)?;
            apply_directory_permissions(&destination, &staged.snapshot)?;
            verify_destination_content(&destination, &staged.snapshot)?;
            destination_parent.sync("sync destination parent before marker")?;
            destination.verify_path_identity("verify installed destination before marker")?;
            write_marker_in_directory(&destination, old, new, "success")?;
            Ok(MigrationState::Migrated)
        }
        Err(error)
            if migration_error_is_already_exists(&error)
                || matches!(
                    &error,
                    MigrationError::Io { source, .. }
                        if source.kind() == io::ErrorKind::DirectoryNotEmpty
                            || source.kind() == io::ErrorKind::Unsupported
                ) =>
        {
            merge_staged_tree(staged, destination_parent, destination_name, old, new)
        }
        Err(original) => Err(cleanup_staging_after_failure(
            destination_parent,
            &staged.name,
            original,
        )),
    }
}

fn merge_staged_tree(
    staged: StagedTree,
    destination_parent: &SecureDirectory,
    destination_name: &OsStr,
    old: &Path,
    new: &Path,
) -> Result<MigrationState, MigrationError> {
    let (destination, destination_created) = destination_parent
        .open_or_create_child_directory(destination_name, "open merge destination")
        .map_err(|original| {
            cleanup_staging_after_failure(destination_parent, &staged.name, original)
        })?;
    if let Err(original) = merge_tree(
        &staged.directory,
        &destination,
        Path::new(""),
        &staged.snapshot,
    ) {
        return Err(cleanup_staging_after_failure(
            destination_parent,
            &staged.name,
            original,
        ));
    }
    if let Err(original) = sync_tree(&destination) {
        return Err(cleanup_staging_after_failure(
            destination_parent,
            &staged.name,
            original,
        ));
    }
    destination_parent
        .remove_child_tree(&staged.name, "remove stable staging directory")
        .map_err(|source| MigrationError::TempCleanupFailed {
            source: io::Error::other(source),
            path: destination_parent.path.join(&staged.name),
        })?;
    if destination_created {
        apply_directory_permissions(&destination, &staged.snapshot)?;
        verify_destination_content(&destination, &staged.snapshot)?;
    }
    destination_parent.sync("sync destination parent before marker")?;
    destination.verify_path_identity("verify merged destination before marker")?;
    write_marker_in_directory(&destination, old, new, "success")?;
    Ok(MigrationState::Migrated)
}

fn cleanup_staging_after_failure(
    parent: &SecureDirectory,
    name: &OsStr,
    original: MigrationError,
) -> MigrationError {
    match parent.remove_child_tree(name, "clean up temporary migration directory") {
        Ok(()) => original,
        Err(source) => MigrationError::CleanupFailed {
            original: Box::new(original),
            source: io::Error::other(source),
            path: parent.path.join(name),
        },
    }
}

/// Migrate the config and data roots from their legacy Zed locations into the
/// Orion Studio locations.
///
/// Intended to be called exactly once during startup before Orion Studio opens
/// or initializes persistence. This function performs blocking filesystem I/O
/// and may copy entire directory trees. The destination can already exist:
/// legacy entries are merged without overwriting files created by Orion Studio.
/// A root is only touched when its legacy directory exists, so a fresh install
/// (no legacy data) performs no work and startup behavior is unchanged. Repeated
/// calls are idempotent: the success marker written by [`migrate_root`] makes
/// later calls return `Completed` without recopying.
///
/// The config root is migrated first; if it succeeds but the data root fails,
/// the error is returned and the config root remains migrated (its marker
/// makes the next run skip it), so a retry only does the remaining work.
pub fn migrate_legacy_user_data() -> Result<MigrationOutcome, MigrationError> {
    migrate_legacy_dirs(
        legacy_config_dir().as_deref(),
        crate::config_dir(),
        legacy_data_dir().as_deref(),
        crate::data_dir(),
    )
}

/// Orchestrate migration of the config and data roots.
///
/// Each root is migrated independently. A root is skipped only when its legacy
/// path and migration marker are both absent. Existing markers are always
/// validated, even if the legacy directory has since been removed. A failure
/// in either root aborts with an error; legacy directories are never deleted,
/// so a failed run can be retried.
pub fn migrate_legacy_dirs(
    legacy_config: Option<&Path>,
    new_config: &Path,
    legacy_data: Option<&Path>,
    new_data: &Path,
) -> Result<MigrationOutcome, MigrationError> {
    let config_exists = legacy_root_exists(legacy_config, "check legacy config directory")?;
    let data_exists = legacy_root_exists(legacy_data, "check legacy data directory")?;
    let config_marker_exists = migration_marker_exists(new_config, "inspect config marker")?;
    ensure_marker_has_legacy_source(legacy_config, new_config, config_marker_exists)?;
    let config_required = config_exists || config_marker_exists;
    let mut data_marker_exists = false;
    if !config_required && !data_exists {
        data_marker_exists = migration_marker_exists(new_data, "inspect data marker")?;
        ensure_marker_has_legacy_source(legacy_data, new_data, data_marker_exists)?;
    }
    let mut data_required = data_exists || data_marker_exists;
    if !config_required && !data_required {
        return Ok(MigrationOutcome {
            config: None,
            data: None,
        });
    }

    if let Some(legacy) = legacy_config.filter(|_| config_required) {
        validate_root_relationship(legacy, new_config)?;
    } else {
        resolve_root_path(new_config, "validate migration lock destination")?;
    }
    if !config_required {
        if let Some(legacy) = legacy_data.filter(|_| data_required) {
            validate_root_relationship(legacy, new_data)?;
        }
    }

    let _migration_lock = acquire_migration_lock(new_config)?;

    let config = match legacy_config.filter(|_| config_required) {
        Some(legacy) => {
            validate_root_relationship(legacy, new_config)?;
            Some(migrate_root_unlocked(legacy, new_config)?)
        }
        None => None,
    };
    if config_required && !data_exists {
        data_marker_exists = migration_marker_exists(new_data, "inspect data marker")?;
        ensure_marker_has_legacy_source(legacy_data, new_data, data_marker_exists)?;
        data_required = data_marker_exists;
    }
    let data = match legacy_data.filter(|_| data_required) {
        Some(legacy) => {
            validate_root_relationship(legacy, new_data)?;
            Some(migrate_root_unlocked(legacy, new_data)?)
        }
        None => None,
    };
    Ok(MigrationOutcome { config, data })
}

fn legacy_root_exists(
    legacy: Option<&Path>,
    check_context: &'static str,
) -> Result<bool, MigrationError> {
    match legacy {
        Some(legacy) => SecureDirectory::open_optional(legacy, check_context)
            .map(|directory| directory.is_some()),
        None => Ok(false),
    }
}

fn migration_marker_exists(new: &Path, context: &'static str) -> Result<bool, MigrationError> {
    let Some(directory) = SecureDirectory::open_optional(new, context)? else {
        return Ok(false);
    };
    match directory.named_metadata(OsStr::new(MIGRATION_MARKER_NAME), context)? {
        Some(metadata) if metadata.kind == EntryKind::Symlink => {
            Err(MigrationError::UnsafeSymlink {
                path: new.join(MIGRATION_MARKER_NAME),
                context,
            })
        }
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

fn ensure_marker_has_legacy_source(
    legacy: Option<&Path>,
    new: &Path,
    marker_exists: bool,
) -> Result<(), MigrationError> {
    if marker_exists && legacy.is_none() {
        return Err(MigrationError::MarkerCorrupted {
            path: new.join(MIGRATION_MARKER_NAME),
            reason: "legacy source path is unavailable".into(),
        });
    }
    Ok(())
}

struct MigrationLock {
    _file: SecureFile,
}

fn acquire_migration_lock(new: &Path) -> Result<MigrationLock, MigrationError> {
    let parent = new
        .parent()
        .ok_or_else(|| MigrationError::NoParent(new.to_path_buf()))?;
    let parent = SecureDirectory::open_or_create(parent, "create migration lock parent")?;
    let lock_name = OsStr::new(MIGRATION_LOCK_NAME);
    let file = match parent.named_metadata(lock_name, "inspect migration lock")? {
        Some(metadata) if metadata.kind == EntryKind::Symlink => {
            return Err(MigrationError::UnsafeSymlink {
                path: parent.path.join(lock_name),
                context: "open migration lock",
            });
        }
        Some(metadata) if metadata.kind == EntryKind::RegularFile => {
            parent.open_child_file(lock_name, true, "open migration lock")?
        }
        Some(_) => {
            return Err(MigrationError::io(
                io::Error::new(io::ErrorKind::InvalidData, "lock is not a regular file"),
                parent.path.join(lock_name),
                "open migration lock",
            ));
        }
        None => match parent.create_child_file(lock_name, 0o600, "create migration lock") {
            Ok(file) => file,
            Err(error) if migration_error_is_already_exists(&error) => {
                parent.open_child_file(lock_name, true, "open concurrent migration lock")?
            }
            Err(error) => return Err(error),
        },
    };
    file.file
        .lock()
        .map_err(|error| MigrationError::io(error, file.path.clone(), "acquire migration lock"))?;
    parent.verify_child_identity(lock_name, file.identity, "validate locked migration lock")?;
    parent.verify_path_identity("validate migration lock parent")?;
    Ok(MigrationLock { _file: file })
}

fn validate_root_relationship(old: &Path, new: &Path) -> Result<(), MigrationError> {
    let source = resolve_root_path(old, "validate legacy source root")?;
    let destination = resolve_root_path(new, "validate migration destination root")?;
    if source == destination || source.starts_with(&destination) || destination.starts_with(&source)
    {
        return Err(MigrationError::OverlappingRoots {
            source,
            destination,
        });
    }
    Ok(())
}

fn resolve_root_path(path: &Path, context: &'static str) -> Result<PathBuf, MigrationError> {
    let normalized = normalize_absolute_path(path, context)?;
    match fs::symlink_metadata(&normalized) {
        Ok(metadata) => {
            ensure_directory_metadata(&normalized, &metadata, context)?;
            fs::canonicalize(&normalized)
                .map_err(|error| MigrationError::io(error, normalized, context))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            resolve_nonexistent_path(&normalized, context)
        }
        Err(error) => Err(MigrationError::io(error, normalized, context)),
    }
}

fn resolve_nonexistent_path(path: &Path, context: &'static str) -> Result<PathBuf, MigrationError> {
    let mut existing = path.to_path_buf();
    let mut missing_components = Vec::new();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(metadata) => {
                ensure_directory_metadata(&existing, &metadata, context)?;
                let mut resolved = fs::canonicalize(&existing)
                    .map_err(|error| MigrationError::io(error, existing.clone(), context))?;
                for component in missing_components.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let component = existing
                    .file_name()
                    .ok_or_else(|| MigrationError::NoParent(path.to_path_buf()))?;
                missing_components.push(component.to_os_string());
                existing = existing
                    .parent()
                    .ok_or_else(|| MigrationError::NoParent(path.to_path_buf()))?
                    .to_path_buf();
            }
            Err(error) => {
                return Err(MigrationError::io(error, existing, context));
            }
        }
    }
}

fn normalize_absolute_path(path: &Path, context: &'static str) -> Result<PathBuf, MigrationError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| MigrationError::io(error, path.to_path_buf(), context))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return Err(MigrationError::io(
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "path escapes the filesystem root",
                        ),
                        path.to_path_buf(),
                        context,
                    ));
                }
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

fn ensure_directory_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    context: &'static str,
) -> Result<(), MigrationError> {
    if metadata.file_type().is_symlink() {
        return Err(MigrationError::UnsafeSymlink {
            path: path.to_path_buf(),
            context,
        });
    }
    if !metadata.is_dir() {
        return Err(MigrationError::io(
            io::Error::new(io::ErrorKind::NotADirectory, "expected a directory"),
            path.to_path_buf(),
            context,
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeSnapshot {
    stability: Vec<StabilityEntry>,
    content: Vec<ContentEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StabilityEntry {
    path: PathBuf,
    kind: EntryKind,
    identity: Option<FileIdentity>,
    length: u64,
    permissions: u32,
    modified: i128,
    changed: i128,
    digest: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContentEntry {
    path: PathBuf,
    kind: EntryKind,
    length: u64,
    permissions: u32,
    digest: Option<[u8; 32]>,
}

fn create_stable_staging<F>(
    old: &Path,
    destination_parent: &SecureDirectory,
    mut after_copy: F,
) -> Result<StagedTree, MigrationError>
where
    F: FnMut(usize) -> Result<(), MigrationError>,
{
    for attempt in 1..=MAX_SOURCE_STABILITY_ATTEMPTS {
        let source = match SecureDirectory::open(old, "open legacy source for snapshot") {
            Ok(source) => source,
            Err(error) if source_change_error(&error) => {
                if attempt == MAX_SOURCE_STABILITY_ATTEMPTS {
                    break;
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let before = match capture_tree_snapshot(&source) {
            Ok(snapshot) => snapshot,
            Err(error) if source_change_error(&error) => {
                if attempt == MAX_SOURCE_STABILITY_ATTEMPTS {
                    break;
                }
                continue;
            }
            Err(error) => return Err(error),
        };
        let staging_name = OsString::from(format!(
            "{DIRECTORY_TEMP_PREFIX}-{}-{}-{attempt}",
            std::process::id(),
            now_nanos()
        ));
        let staging = destination_parent
            .create_child_directory(&staging_name, "create stable migration staging directory")?;
        let attempt_result = (|| {
            copy_tree_to_staging(&source, &staging)?;
            after_copy(attempt)?;
            let after = capture_tree_snapshot(&source)?;
            source.verify_path_identity("verify legacy source after snapshot")?;
            let staged_snapshot = capture_tree_snapshot(&staging)?;
            if before != after || !content_matches_staging(&before, &staged_snapshot) {
                return Err(MigrationError::SourceChanged {
                    path: old.to_path_buf(),
                    attempts: attempt,
                });
            }
            staging.sync("sync stable staging root")?;
            destination_parent.sync("sync staging parent")?;
            Ok(before)
        })();
        match attempt_result {
            Ok(snapshot) => {
                return Ok(StagedTree {
                    name: staging_name,
                    directory: staging,
                    snapshot,
                });
            }
            Err(error) if source_change_error(&error) => {
                let source_changed = MigrationError::SourceChanged {
                    path: old.to_path_buf(),
                    attempts: attempt,
                };
                let cleanup = destination_parent.remove_child_tree(
                    &staging_name,
                    "remove unstable migration staging directory",
                );
                if let Err(cleanup_error) = cleanup {
                    return Err(MigrationError::CleanupFailed {
                        original: Box::new(source_changed),
                        source: io::Error::other(cleanup_error),
                        path: destination_parent.path.join(staging_name),
                    });
                }
                if attempt == MAX_SOURCE_STABILITY_ATTEMPTS {
                    break;
                }
            }
            Err(original) => {
                return Err(cleanup_staging_after_failure(
                    destination_parent,
                    &staging_name,
                    original,
                ));
            }
        }
    }
    Err(MigrationError::SourceChanged {
        path: old.to_path_buf(),
        attempts: MAX_SOURCE_STABILITY_ATTEMPTS,
    })
}

fn source_change_error(error: &MigrationError) -> bool {
    matches!(
        error,
        MigrationError::SourceChanged { .. } | MigrationError::ObjectIdentityChanged { .. }
    )
}

fn capture_tree_snapshot(root: &SecureDirectory) -> Result<TreeSnapshot, MigrationError> {
    let mut snapshot = TreeSnapshot {
        stability: Vec::new(),
        content: Vec::new(),
    };
    capture_directory_snapshot(root, Path::new(""), &mut snapshot)?;
    snapshot
        .stability
        .sort_by(|left, right| left.path.cmp(&right.path));
    snapshot
        .content
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(snapshot)
}

fn capture_directory_snapshot(
    directory: &SecureDirectory,
    relative: &Path,
    snapshot: &mut TreeSnapshot,
) -> Result<(), MigrationError> {
    let before = directory
        .file
        .metadata()
        .map_err(|error| MigrationError::io(error, directory.path.clone(), "snapshot directory"))?;
    let before_entry = stability_entry(
        relative,
        EntryKind::Directory,
        Some(directory.identity),
        &before,
        None,
    );
    snapshot.content.push(content_entry(&before_entry));
    for name in directory.entry_names("list source directory for snapshot")? {
        let child_relative = relative.join(&name);
        let Some(named) = directory.named_metadata(&name, "inspect source snapshot entry")? else {
            return Err(MigrationError::SourceChanged {
                path: directory.path.join(&name),
                attempts: 1,
            });
        };
        match named.kind {
            EntryKind::Directory => {
                let child =
                    directory.open_child_directory(&name, "open source snapshot directory")?;
                capture_directory_snapshot(&child, &child_relative, snapshot)?;
                directory.verify_child_identity(
                    &name,
                    child.identity,
                    "verify source snapshot directory",
                )?;
            }
            EntryKind::RegularFile => {
                let mut child =
                    directory.open_child_file(&name, false, "open source snapshot file")?;
                let metadata_before = child.metadata("inspect source snapshot file")?;
                let digest = hash_file(&mut child)?;
                let metadata_after = child.metadata("reinspect source snapshot file")?;
                let before_file = stability_entry(
                    &child_relative,
                    EntryKind::RegularFile,
                    Some(child.identity),
                    &metadata_before,
                    Some(digest),
                );
                let after_file = stability_entry(
                    &child_relative,
                    EntryKind::RegularFile,
                    Some(child.identity),
                    &metadata_after,
                    Some(digest),
                );
                if before_file != after_file {
                    return Err(MigrationError::SourceChanged {
                        path: child.path,
                        attempts: 1,
                    });
                }
                directory.verify_child_identity(
                    &name,
                    child.identity,
                    "verify source snapshot file",
                )?;
                snapshot.content.push(content_entry(&before_file));
                snapshot.stability.push(before_file);
            }
            EntryKind::Symlink | EntryKind::Other => {
                snapshot.stability.push(StabilityEntry {
                    path: child_relative,
                    kind: named.kind,
                    identity: named.identity,
                    length: 0,
                    permissions: 0,
                    modified: 0,
                    changed: 0,
                    digest: None,
                });
            }
        }
    }
    let after = directory.file.metadata().map_err(|error| {
        MigrationError::io(
            error,
            directory.path.clone(),
            "reinspect snapshot directory",
        )
    })?;
    let after_entry = stability_entry(
        relative,
        EntryKind::Directory,
        Some(directory.identity),
        &after,
        None,
    );
    if before_entry != after_entry {
        return Err(MigrationError::SourceChanged {
            path: directory.path.clone(),
            attempts: 1,
        });
    }
    directory.verify_path_identity("verify snapshotted directory identity")?;
    snapshot.stability.push(before_entry);
    Ok(())
}

struct Sha256State {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_length: usize,
    length_bytes: u64,
}

impl Sha256State {
    fn new() -> Self {
        Self {
            state: [
                0x6a09_e667,
                0xbb67_ae85,
                0x3c6e_f372,
                0xa54f_f53a,
                0x510e_527f,
                0x9b05_688c,
                0x1f83_d9ab,
                0x5be0_cd19,
            ],
            buffer: [0; 64],
            buffer_length: 0,
            length_bytes: 0,
        }
    }

    fn update(&mut self, mut bytes: &[u8]) {
        self.length_bytes = self.length_bytes.wrapping_add(bytes.len() as u64);
        if self.buffer_length != 0 {
            let copy_length = (64 - self.buffer_length).min(bytes.len());
            let buffer_end = self.buffer_length + copy_length;
            self.buffer[self.buffer_length..buffer_end].copy_from_slice(&bytes[..copy_length]);
            self.buffer_length = buffer_end;
            bytes = &bytes[copy_length..];
            if self.buffer_length == 64 {
                let block = self.buffer;
                self.process_block(&block);
                self.buffer_length = 0;
            }
        }
        while bytes.len() >= 64 {
            let mut block = [0; 64];
            block.copy_from_slice(&bytes[..64]);
            self.process_block(&block);
            bytes = &bytes[64..];
        }
        if !bytes.is_empty() {
            self.buffer[..bytes.len()].copy_from_slice(bytes);
            self.buffer_length = bytes.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_length = self.length_bytes.wrapping_mul(8);
        self.update(&[0x80]);
        let zeroes = [0; 64];
        let padding_length = if self.buffer_length <= 56 {
            56 - self.buffer_length
        } else {
            64 + 56 - self.buffer_length
        };
        self.update(&zeroes[..padding_length]);
        self.update(&bit_length.to_be_bytes());

        let mut result = [0; 32];
        for (target, value) in result.chunks_exact_mut(4).zip(self.state) {
            target.copy_from_slice(&value.to_be_bytes());
        }
        result
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        const ROUND_CONSTANTS: [u32; 64] = [
            0x428a_2f98,
            0x7137_4491,
            0xb5c0_fbcf,
            0xe9b5_dba5,
            0x3956_c25b,
            0x59f1_11f1,
            0x923f_82a4,
            0xab1c_5ed5,
            0xd807_aa98,
            0x1283_5b01,
            0x2431_85be,
            0x550c_7dc3,
            0x72be_5d74,
            0x80de_b1fe,
            0x9bdc_06a7,
            0xc19b_f174,
            0xe49b_69c1,
            0xefbe_4786,
            0x0fc1_9dc6,
            0x240c_a1cc,
            0x2de9_2c6f,
            0x4a74_84aa,
            0x5cb0_a9dc,
            0x76f9_88da,
            0x983e_5152,
            0xa831_c66d,
            0xb003_27c8,
            0xbf59_7fc7,
            0xc6e0_0bf3,
            0xd5a7_9147,
            0x06ca_6351,
            0x1429_2967,
            0x27b7_0a85,
            0x2e1b_2138,
            0x4d2c_6dfc,
            0x5338_0d13,
            0x650a_7354,
            0x766a_0abb,
            0x81c2_c92e,
            0x9272_2c85,
            0xa2bf_e8a1,
            0xa81a_664b,
            0xc24b_8b70,
            0xc76c_51a3,
            0xd192_e819,
            0xd699_0624,
            0xf40e_3585,
            0x106a_a070,
            0x19a4_c116,
            0x1e37_6c08,
            0x2748_774c,
            0x34b0_bcb5,
            0x391c_0cb3,
            0x4ed8_aa4a,
            0x5b9c_ca4f,
            0x682e_6ff3,
            0x748f_82ee,
            0x78a5_636f,
            0x84c8_7814,
            0x8cc7_0208,
            0x90be_fffa,
            0xa450_6ceb,
            0xbef9_a3f7,
            0xc671_78f2,
        ];

        let mut schedule = [0_u32; 64];
        for (word, bytes) in schedule[..16].iter_mut().zip(block.chunks_exact(4)) {
            let mut word_bytes = [0; 4];
            word_bytes.copy_from_slice(bytes);
            *word = u32::from_be_bytes(word_bytes);
        }
        for index in 16..64 {
            let small_sigma_zero = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let small_sigma_one = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(small_sigma_zero)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(small_sigma_one);
        }

        let [
            initial_a,
            initial_b,
            initial_c,
            initial_d,
            initial_e,
            initial_f,
            initial_g,
            initial_h,
        ] = self.state;
        let (mut a, mut b, mut c, mut d) = (initial_a, initial_b, initial_c, initial_d);
        let (mut e, mut f, mut g, mut h) = (initial_e, initial_f, initial_g, initial_h);
        for (constant, word) in ROUND_CONSTANTS.into_iter().zip(schedule) {
            let big_sigma_one = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temporary_one = h
                .wrapping_add(big_sigma_one)
                .wrapping_add(choice)
                .wrapping_add(constant)
                .wrapping_add(word);
            let big_sigma_zero = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temporary_two = big_sigma_zero.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temporary_one);
            d = c;
            c = b;
            b = a;
            a = temporary_one.wrapping_add(temporary_two);
        }
        self.state = [
            initial_a.wrapping_add(a),
            initial_b.wrapping_add(b),
            initial_c.wrapping_add(c),
            initial_d.wrapping_add(d),
            initial_e.wrapping_add(e),
            initial_f.wrapping_add(f),
            initial_g.wrapping_add(g),
            initial_h.wrapping_add(h),
        ];
    }
}

fn hash_file(file: &mut SecureFile) -> Result<[u8; 32], MigrationError> {
    hash_file_with_context(file, "hash source file for snapshot")
}

fn hash_file_with_context(
    file: &mut SecureFile,
    context: &'static str,
) -> Result<[u8; 32], MigrationError> {
    let mut digest = Sha256State::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .file
            .read(&mut buffer)
            .map_err(|error| MigrationError::io(error, file.path.clone(), context))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest.finalize())
}

fn stability_entry(
    path: &Path,
    kind: EntryKind,
    identity: Option<FileIdentity>,
    metadata: &fs::Metadata,
    digest: Option<[u8; 32]>,
) -> StabilityEntry {
    let (permissions, modified, changed) = metadata_stability_fields(metadata);
    StabilityEntry {
        path: path.to_path_buf(),
        kind,
        identity,
        length: metadata.len(),
        permissions,
        modified,
        changed,
        digest,
    }
}

fn content_entry(stability: &StabilityEntry) -> ContentEntry {
    ContentEntry {
        path: stability.path.clone(),
        kind: stability.kind,
        length: stability.length,
        permissions: stability.permissions,
        digest: stability.digest,
    }
}

#[cfg(unix)]
fn metadata_stability_fields(metadata: &fs::Metadata) -> (u32, i128, i128) {
    use std::os::unix::fs::MetadataExt;

    let modified = i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec());
    let changed = i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec());
    (metadata.mode(), modified, changed)
}

#[cfg(target_os = "windows")]
fn metadata_stability_fields(metadata: &fs::Metadata) -> (u32, i128, i128) {
    use std::os::windows::fs::MetadataExt;

    (
        metadata.file_attributes(),
        i128::from(metadata.last_write_time()),
        i128::from(metadata.creation_time()),
    )
}

#[cfg(not(any(unix, target_os = "windows")))]
fn metadata_stability_fields(metadata: &fs::Metadata) -> (u32, i128, i128) {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos() as i128);
    (
        u32::from(metadata.permissions().readonly()),
        modified,
        modified,
    )
}

fn content_matches_staging(source: &TreeSnapshot, staging: &TreeSnapshot) -> bool {
    if source.content.len() != staging.content.len() {
        return false;
    }
    source
        .content
        .iter()
        .zip(&staging.content)
        .all(|(source, staging)| {
            source.path == staging.path
                && source.kind == staging.kind
                && (source.kind == EntryKind::Directory
                    || (source.length == staging.length
                        && source.digest == staging.digest
                        && source.permissions == staging.permissions))
        })
}

fn verify_destination_content(
    destination: &SecureDirectory,
    source_snapshot: &TreeSnapshot,
) -> Result<(), MigrationError> {
    let destination_snapshot = capture_tree_snapshot(destination)?;
    if content_matches_staging(source_snapshot, &destination_snapshot) {
        Ok(())
    } else {
        Err(MigrationError::ObjectIdentityChanged {
            path: destination.path.clone(),
            context: "verify installed destination contents before marker",
        })
    }
}

fn copy_tree_to_staging(
    source: &SecureDirectory,
    destination: &SecureDirectory,
) -> Result<(), MigrationError> {
    for name in source.entry_names("list legacy directory for copy")? {
        let Some(metadata) = source.named_metadata(&name, "inspect legacy copy entry")? else {
            return Err(MigrationError::SourceChanged {
                path: source.path.join(&name),
                attempts: 1,
            });
        };
        match metadata.kind {
            EntryKind::Directory => {
                let source_child =
                    source.open_child_directory(&name, "open legacy directory for copy")?;
                let destination_child =
                    destination.create_child_directory(&name, "create staging directory")?;
                copy_tree_to_staging(&source_child, &destination_child)?;
                source.verify_child_identity(
                    &name,
                    source_child.identity,
                    "verify copied legacy directory",
                )?;
                destination_child.sync("sync copied staging directory")?;
            }
            EntryKind::RegularFile => {
                copy_file_to_directory(
                    source,
                    &name,
                    destination,
                    &name,
                    "copy legacy file to staging",
                )?;
            }
            EntryKind::Symlink | EntryKind::Other => {}
        }
    }
    destination.sync("sync staging directory after copy")
}

fn copy_file_to_directory(
    source_parent: &SecureDirectory,
    source_name: &OsStr,
    destination_parent: &SecureDirectory,
    destination_name: &OsStr,
    context: &'static str,
) -> Result<FileIdentity, MigrationError> {
    let mut source = source_parent.open_child_file(source_name, false, context)?;
    let source_before = source.metadata("inspect source file before copy")?;
    let mut destination = destination_parent.create_child_file(
        destination_name,
        0o600,
        "create destination copy file",
    )?;
    let copy_result = (|| {
        io::copy(&mut source.file, &mut destination.file).map_err(|error| {
            MigrationError::io(error, destination.path.clone(), "copy file contents")
        })?;
        destination
            .file
            .set_permissions(source_before.permissions())
            .map_err(|error| {
                MigrationError::io(
                    error,
                    destination.path.clone(),
                    "preserve copied file permissions",
                )
            })?;
        destination.sync("sync copied destination file")?;
        Ok(())
    })();
    if let Err(original) = copy_result {
        return Err(cleanup_file_in_directory_after_failure(
            destination_parent,
            destination_name,
            original,
        ));
    }
    let source_after = source.metadata("inspect source file after copy")?;
    let before = stability_entry(
        Path::new(source_name),
        EntryKind::RegularFile,
        Some(source.identity),
        &source_before,
        None,
    );
    let after = stability_entry(
        Path::new(source_name),
        EntryKind::RegularFile,
        Some(source.identity),
        &source_after,
        None,
    );
    if before != after {
        let original = MigrationError::SourceChanged {
            path: source.path,
            attempts: 1,
        };
        return Err(cleanup_file_in_directory_after_failure(
            destination_parent,
            destination_name,
            original,
        ));
    }
    source_parent.verify_child_identity(
        source_name,
        source.identity,
        "verify source file after copy",
    )?;
    destination_parent.verify_child_identity(
        destination_name,
        destination.identity,
        "verify destination file after copy",
    )?;
    Ok(destination.identity)
}

fn cleanup_file_in_directory_after_failure(
    parent: &SecureDirectory,
    name: &OsStr,
    original: MigrationError,
) -> MigrationError {
    match parent.remove_child_file(name, "clean up temporary migration file") {
        Ok(()) => original,
        Err(source) => MigrationError::CleanupFailed {
            original: Box::new(original),
            source: io::Error::other(source),
            path: parent.path.join(name),
        },
    }
}

fn merge_tree(
    source: &SecureDirectory,
    destination: &SecureDirectory,
    relative: &Path,
    snapshot: &TreeSnapshot,
) -> Result<(), MigrationError> {
    for name in source.entry_names("list stable staging directory for merge")? {
        let Some(source_metadata) = source.named_metadata(&name, "inspect staged merge entry")?
        else {
            return Err(MigrationError::ObjectIdentityChanged {
                path: source.path.join(&name),
                context: "inspect staged merge entry",
            });
        };
        let child_relative = relative.join(&name);
        match source_metadata.kind {
            EntryKind::Directory => {
                let source_child =
                    source.open_child_directory(&name, "open staged merge directory")?;
                let destination_metadata =
                    destination.named_metadata(&name, "inspect merge destination directory")?;
                let (destination_child, created) = match destination_metadata {
                    Some(metadata) if metadata.kind == EntryKind::Symlink => {
                        return Err(MigrationError::UnsafeSymlink {
                            path: destination.path.join(&name),
                            context: "inspect merge destination directory",
                        });
                    }
                    Some(metadata) if metadata.kind == EntryKind::Directory => (
                        destination.open_child_directory(
                            &name,
                            "open existing merge destination directory",
                        )?,
                        false,
                    ),
                    Some(_) => {
                        return Err(MigrationError::DestinationConflict {
                            source: source.path.join(&name),
                            destination: destination.path.join(&name),
                        });
                    }
                    None => (
                        destination
                            .create_child_directory(&name, "create merge destination directory")?,
                        true,
                    ),
                };
                merge_tree(&source_child, &destination_child, &child_relative, snapshot)?;
                if created {
                    let permissions = snapshot_directory_permissions(snapshot, &child_relative)?;
                    set_directory_permission_bits(
                        &destination_child,
                        permissions,
                        "preserve merged directory permissions",
                    )?;
                }
                destination_child.sync("sync merged destination directory")?;
                destination.verify_child_identity(
                    &name,
                    destination_child.identity,
                    "verify merged destination directory",
                )?;
                destination_child
                    .verify_path_identity("verify merged destination directory path")?;
            }
            EntryKind::RegularFile => {
                merge_file_without_overwrite(source, &name, destination, &name)?
            }
            EntryKind::Symlink | EntryKind::Other => {
                return Err(MigrationError::ObjectIdentityChanged {
                    path: source.path.join(&name),
                    context: "validate private staging tree",
                });
            }
        }
    }
    destination.sync("sync merged directory entries")
}

fn merge_file_without_overwrite(
    source: &SecureDirectory,
    source_name: &OsStr,
    destination: &SecureDirectory,
    destination_name: &OsStr,
) -> Result<(), MigrationError> {
    match destination.named_metadata(destination_name, "inspect merge destination file")? {
        Some(metadata) if metadata.kind == EntryKind::Symlink => {
            return Err(MigrationError::UnsafeSymlink {
                path: destination.path.join(destination_name),
                context: "inspect merge destination file",
            });
        }
        Some(metadata) if metadata.kind == EntryKind::RegularFile => {
            return ensure_existing_file_matches(
                source,
                source_name,
                destination,
                destination_name,
            );
        }
        Some(_) => {
            return Err(MigrationError::DestinationConflict {
                source: source.path.join(source_name),
                destination: destination.path.join(destination_name),
            });
        }
        None => {}
    }
    let temp_name = OsString::from(format!(
        "{FILE_TEMP_PREFIX}-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    let temp_identity = copy_file_to_directory(
        source,
        source_name,
        destination,
        &temp_name,
        "copy stable file for merge",
    )?;
    let link_result = destination.hard_link_child_to(
        &temp_name,
        destination,
        destination_name,
        "install merged file without overwrite",
    );
    match link_result {
        Ok(()) => {
            destination.verify_child_identity(
                destination_name,
                temp_identity,
                "verify installed merged file",
            )?;
        }
        Err(error) if migration_error_is_already_exists(&error) => {
            match destination
                .named_metadata(destination_name, "inspect concurrent merge destination")?
            {
                Some(metadata) if metadata.kind == EntryKind::Symlink => {
                    let original = MigrationError::UnsafeSymlink {
                        path: destination.path.join(destination_name),
                        context: "inspect concurrent merge destination",
                    };
                    return Err(cleanup_file_in_directory_after_failure(
                        destination,
                        &temp_name,
                        original,
                    ));
                }
                Some(metadata) if metadata.kind == EntryKind::RegularFile => {
                    if let Err(original) = ensure_existing_file_matches(
                        source,
                        source_name,
                        destination,
                        destination_name,
                    ) {
                        return Err(cleanup_file_in_directory_after_failure(
                            destination,
                            &temp_name,
                            original,
                        ));
                    }
                }
                Some(_) => {
                    let original = MigrationError::DestinationConflict {
                        source: source.path.join(source_name),
                        destination: destination.path.join(destination_name),
                    };
                    return Err(cleanup_file_in_directory_after_failure(
                        destination,
                        &temp_name,
                        original,
                    ));
                }
                None => {
                    return Err(cleanup_file_in_directory_after_failure(
                        destination,
                        &temp_name,
                        error,
                    ));
                }
            }
        }
        Err(original) => {
            return Err(cleanup_file_in_directory_after_failure(
                destination,
                &temp_name,
                original,
            ));
        }
    }
    destination.remove_child_file(&temp_name, "remove merged file staging link")
}

fn ensure_existing_file_matches(
    source: &SecureDirectory,
    source_name: &OsStr,
    destination: &SecureDirectory,
    destination_name: &OsStr,
) -> Result<(), MigrationError> {
    let source_content = capture_file_content(
        source,
        source_name,
        "inspect staged file for destination conflict",
    )?;
    let destination_content = capture_file_content(
        destination,
        destination_name,
        "inspect existing destination file for conflict",
    )?;
    if source_content.kind == destination_content.kind
        && source_content.length == destination_content.length
        && source_content.permissions == destination_content.permissions
        && source_content.digest == destination_content.digest
    {
        Ok(())
    } else {
        Err(MigrationError::DestinationConflict {
            source: source.path.join(source_name),
            destination: destination.path.join(destination_name),
        })
    }
}

fn capture_file_content(
    parent: &SecureDirectory,
    name: &OsStr,
    context: &'static str,
) -> Result<ContentEntry, MigrationError> {
    let mut file = parent.open_child_file(name, false, context)?;
    let metadata_before = file.metadata(context)?;
    let digest = hash_file_with_context(&mut file, context)?;
    let metadata_after = file.metadata(context)?;
    let before = stability_entry(
        Path::new(name),
        EntryKind::RegularFile,
        Some(file.identity),
        &metadata_before,
        Some(digest),
    );
    let after = stability_entry(
        Path::new(name),
        EntryKind::RegularFile,
        Some(file.identity),
        &metadata_after,
        Some(digest),
    );
    if before != after {
        return Err(MigrationError::ObjectIdentityChanged {
            path: file.path,
            context,
        });
    }
    parent.verify_child_identity(name, file.identity, context)?;
    Ok(content_entry(&before))
}

fn snapshot_directory_permissions(
    snapshot: &TreeSnapshot,
    relative: &Path,
) -> Result<u32, MigrationError> {
    snapshot
        .content
        .iter()
        .find(|entry| entry.path == relative && entry.kind == EntryKind::Directory)
        .map(|entry| entry.permissions)
        .ok_or_else(|| {
            MigrationError::io(
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "missing directory snapshot metadata",
                ),
                relative.to_path_buf(),
                "restore directory permissions",
            )
        })
}

fn apply_directory_permissions(
    root: &SecureDirectory,
    snapshot: &TreeSnapshot,
) -> Result<(), MigrationError> {
    let mut directories: Vec<_> = snapshot
        .content
        .iter()
        .filter(|entry| entry.kind == EntryKind::Directory)
        .collect();
    directories.sort_by(|left, right| {
        right
            .path
            .components()
            .count()
            .cmp(&left.path.components().count())
            .then_with(|| left.path.cmp(&right.path))
    });
    for entry in directories {
        if entry.path.as_os_str().is_empty() {
            set_directory_permission_bits(
                root,
                entry.permissions,
                "preserve migrated root directory permissions",
            )?;
        } else {
            let directory = open_descendant_directory(root, &entry.path)?;
            set_directory_permission_bits(
                &directory,
                entry.permissions,
                "preserve migrated directory permissions",
            )?;
        }
    }
    Ok(())
}

fn open_descendant_directory(
    root: &SecureDirectory,
    relative: &Path,
) -> Result<SecureDirectory, MigrationError> {
    let mut current: Option<SecureDirectory> = None;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(MigrationError::io(
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid relative directory path",
                ),
                root.path.join(relative),
                "open migrated descendant directory",
            ));
        };
        let parent = match current.as_ref() {
            Some(current) => current,
            None => root,
        };
        current = Some(parent.open_child_directory(name, "open migrated descendant directory")?);
    }
    current.ok_or_else(|| {
        MigrationError::io(
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty descendant directory path",
            ),
            root.path.clone(),
            "open migrated descendant directory",
        )
    })
}

#[cfg(unix)]
fn set_directory_permission_bits(
    directory: &SecureDirectory,
    permissions: u32,
    context: &'static str,
) -> Result<(), MigrationError> {
    use std::os::unix::fs::PermissionsExt;

    directory.set_permissions(fs::Permissions::from_mode(permissions), context)
}

#[cfg(not(unix))]
fn set_directory_permission_bits(
    directory: &SecureDirectory,
    permissions: u32,
    context: &'static str,
) -> Result<(), MigrationError> {
    let mut current = directory
        .file
        .metadata()
        .map_err(|error| MigrationError::io(error, directory.path.clone(), context))?
        .permissions();
    current.set_readonly(permissions & 1 != 0);
    directory.set_permissions(current, context)
}

fn sync_tree(directory: &SecureDirectory) -> Result<(), MigrationError> {
    for name in directory.entry_names("list destination tree for sync")? {
        let Some(metadata) = directory.named_metadata(&name, "inspect destination sync entry")?
        else {
            return Err(MigrationError::ObjectIdentityChanged {
                path: directory.path.join(&name),
                context: "inspect destination sync entry",
            });
        };
        match metadata.kind {
            EntryKind::Directory => {
                let child =
                    directory.open_child_directory(&name, "open destination directory for sync")?;
                sync_tree(&child)?;
            }
            EntryKind::Symlink => {
                return Err(MigrationError::UnsafeSymlink {
                    path: directory.path.join(&name),
                    context: "sync destination tree",
                });
            }
            EntryKind::RegularFile | EntryKind::Other => {}
        }
    }
    directory.sync("sync destination directory")
}

#[cfg(test)]
fn open_new_file(path: &Path, unix_mode: u32) -> io::Result<File> {
    let parent_path = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "file has no parent"))?;
    let parent =
        SecureDirectory::open(parent_path, "open new file parent").map_err(io::Error::other)?;
    let name = validated_child_name(path).map_err(io::Error::other)?;
    parent
        .create_child_file(name, unix_mode, "create new file")
        .map(|file| file.file)
        .map_err(io::Error::other)
}

#[cfg(test)]
fn cleanup_after_failure(temp: &Path, original: MigrationError) -> MigrationError {
    let Some(parent_path) = temp.parent() else {
        return MigrationError::CleanupFailed {
            original: Box::new(original),
            source: io::Error::new(io::ErrorKind::InvalidInput, "temporary path has no parent"),
            path: temp.to_path_buf(),
        };
    };
    let parent = match SecureDirectory::open(parent_path, "open cleanup parent") {
        Ok(parent) => parent,
        Err(source) => {
            return MigrationError::CleanupFailed {
                original: Box::new(original),
                source: io::Error::other(source),
                path: temp.to_path_buf(),
            };
        }
    };
    match validated_child_name(temp)
        .and_then(|name| parent.remove_child_tree(name, "clean up temporary directory"))
    {
        Ok(()) => original,
        Err(source) => MigrationError::CleanupFailed {
            original: Box::new(original),
            source: io::Error::other(source),
            path: temp.to_path_buf(),
        },
    }
}

#[cfg(test)]
fn cleanup_after_success(temp: &Path) -> Result<(), MigrationError> {
    let cleanup = (|| {
        let parent_path = temp
            .parent()
            .ok_or_else(|| MigrationError::NoParent(temp.to_path_buf()))?;
        let parent = SecureDirectory::open(parent_path, "open cleanup parent")?;
        let name = validated_child_name(temp)?;
        parent.remove_child_tree(name, "clean up temporary directory")
    })();
    cleanup.map_err(|source| MigrationError::TempCleanupFailed {
        source: io::Error::other(source),
        path: temp.to_path_buf(),
    })
}

#[cfg(test)]
fn sync_directory(path: &Path, context: &'static str) -> Result<(), MigrationError> {
    SecureDirectory::open(path, context)?.sync(context)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerVersion {
    LegacyUntrustedV2,
    CurrentV3,
}

impl MarkerVersion {
    fn schema(self) -> u32 {
        match self {
            MarkerVersion::LegacyUntrustedV2 => LEGACY_UNTRUSTED_MIGRATION_SCHEMA_VERSION,
            MarkerVersion::CurrentV3 => MIGRATION_SCHEMA_VERSION,
        }
    }
}

struct Marker {
    version: MarkerVersion,
    schema: u32,
    result: String,
    source: PathBuf,
    target: PathBuf,
}

fn validate_marker(marker: &Marker, old: &Path, new: &Path) -> Result<(), MigrationError> {
    let marker_path = new.join(MIGRATION_MARKER_NAME);
    if marker.schema != marker.version.schema() {
        return Err(MigrationError::MarkerCorrupted {
            path: marker_path,
            reason: format!(
                "marker version {:?} does not match schema {}",
                marker.version, marker.schema
            ),
        });
    }
    if marker.source != old {
        return Err(MigrationError::MarkerSourceMismatch {
            marker_path,
            recorded_source: marker.source.clone(),
            current_source: old.to_path_buf(),
        });
    }
    if marker.target != new {
        return Err(MigrationError::MarkerCorrupted {
            path: marker_path,
            reason: format!(
                "marker target {:?} does not match current target {:?}",
                marker.target, new
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
fn write_marker(dir: &Path, source: &Path, result: &str) -> Result<(), MigrationError> {
    let directory = SecureDirectory::open_or_create(dir, "open marker directory")?;
    write_marker_in_directory(&directory, source, dir, result)
}

fn write_marker_in_directory(
    directory: &SecureDirectory,
    source: &Path,
    target: &Path,
    result: &str,
) -> Result<(), MigrationError> {
    if !matches!(result, "success" | "incomplete") {
        return Err(MigrationError::MarkerCorrupted {
            path: directory.path.join(MIGRATION_MARKER_NAME),
            reason: format!("invalid marker result: {result:?}"),
        });
    }
    let marker_name = OsStr::new(MIGRATION_MARKER_NAME);
    match directory.named_metadata(marker_name, "inspect migration marker")? {
        Some(metadata) if metadata.kind == EntryKind::Symlink => {
            return Err(MigrationError::UnsafeSymlink {
                path: directory.path.join(marker_name),
                context: "replace migration marker",
            });
        }
        Some(metadata) if metadata.kind != EntryKind::RegularFile => {
            return Err(MigrationError::io(
                io::Error::new(io::ErrorKind::InvalidData, "marker is not a regular file"),
                directory.path.join(marker_name),
                "replace migration marker",
            ));
        }
        Some(_) | None => {}
    }
    let content = serialize_marker(source, target, result)?;
    let temp_name = OsString::from(format!(
        "{MARKER_TEMP_PREFIX}-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    let mut marker_file =
        directory.create_child_file(&temp_name, 0o600, "create temporary migration marker")?;
    let write_result = (|| {
        marker_file
            .file
            .write_all(content.as_bytes())
            .map_err(|error| {
                MigrationError::io(
                    error,
                    marker_file.path.clone(),
                    "write temporary migration marker",
                )
            })?;
        marker_file.sync("sync temporary migration marker")?;
        directory.verify_child_identity(
            &temp_name,
            marker_file.identity,
            "verify temporary migration marker",
        )?;
        Ok(())
    })();
    if let Err(original) = write_result {
        return Err(cleanup_file_in_directory_after_failure(
            directory, &temp_name, original,
        ));
    }
    if let Err(original) = directory.rename_child_to(
        &temp_name,
        directory,
        marker_name,
        false,
        "atomically replace migration marker",
    ) {
        return Err(cleanup_file_in_directory_after_failure(
            directory, &temp_name, original,
        ));
    }
    let marker = directory.open_child_file(marker_name, false, "verify committed marker")?;
    verify_expected_identity(
        Some(marker_file.identity),
        marker.identity,
        &marker.path,
        "verify renamed migration marker identity",
    )?;
    directory.verify_child_identity(
        marker_name,
        marker.identity,
        "verify committed migration marker",
    )?;
    directory.sync("sync migration marker parent directory")?;
    directory.verify_path_identity("verify migration marker directory after commit")
}

fn serialize_marker(source: &Path, target: &Path, result: &str) -> Result<String, MigrationError> {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    Ok(format!(
        "orion-migration-marker v3\nschema={MIGRATION_SCHEMA_VERSION}\npath_encoding={}\nsource={}\ntarget={}\ntime={time}\nresult={result}\n",
        marker_path_encoding(),
        encode_marker_path(source)?,
        encode_marker_path(target)?,
    ))
}

fn read_marker(dir: &Path) -> Result<Option<Marker>, MigrationError> {
    let Some(directory) = SecureDirectory::open_optional(dir, "open migration marker directory")?
    else {
        return Ok(None);
    };
    read_marker_in_directory(&directory)
}

fn read_marker_in_directory(directory: &SecureDirectory) -> Result<Option<Marker>, MigrationError> {
    let marker_name = OsStr::new(MIGRATION_MARKER_NAME);
    match directory.named_metadata(marker_name, "inspect migration marker")? {
        None => return Ok(None),
        Some(metadata) if metadata.kind == EntryKind::Symlink => {
            return Err(MigrationError::UnsafeSymlink {
                path: directory.path.join(marker_name),
                context: "read migration marker",
            });
        }
        Some(metadata) if metadata.kind != EntryKind::RegularFile => {
            return Err(MigrationError::MarkerCorrupted {
                path: directory.path.join(marker_name),
                reason: "marker is not a regular file".into(),
            });
        }
        Some(_) => {}
    }
    let mut marker_file = directory.open_child_file(marker_name, false, "open migration marker")?;
    let mut content = Vec::new();
    marker_file
        .file
        .read_to_end(&mut content)
        .map_err(|error| {
            MigrationError::io(error, marker_file.path.clone(), "read migration marker")
        })?;
    directory.verify_child_identity(
        marker_name,
        marker_file.identity,
        "verify read migration marker",
    )?;
    parse_marker(&content, &marker_file.path).map(Some)
}

fn parse_marker(content: &[u8], marker_path: &Path) -> Result<Marker, MigrationError> {
    let corrupted = |reason: String| MigrationError::MarkerCorrupted {
        path: marker_path.to_path_buf(),
        reason,
    };
    let content = std::str::from_utf8(content)
        .map_err(|error| corrupted(format!("marker is not UTF-8: {error}")))?;
    if !content.ends_with('\n') {
        return Err(corrupted("marker is missing its final newline".into()));
    }
    let mut lines = content.split_terminator('\n');
    let header = lines
        .next()
        .ok_or_else(|| corrupted("empty marker file".into()))?;
    let version = match header {
        "orion-migration-marker v2" => MarkerVersion::LegacyUntrustedV2,
        "orion-migration-marker v3" => MarkerVersion::CurrentV3,
        _ => {
            if let Some(found) = header
                .strip_prefix("orion-migration-marker v")
                .and_then(|value| value.parse::<u32>().ok())
            {
                return Err(MigrationError::IncompatibleVersion {
                    marker_path: marker_path.to_path_buf(),
                    found,
                    expected: MIGRATION_SCHEMA_VERSION,
                });
            }
            return Err(corrupted(format!("unrecognized marker header: {header:?}")));
        }
    };
    let mut schema = None;
    let mut path_encoding = None;
    let mut source = None;
    let mut target = None;
    let mut time = None;
    let mut result = None;
    for line in lines {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| corrupted(format!("malformed marker line: {line:?}")))?;
        let duplicate = |field: &str| corrupted(format!("duplicate {field} field"));
        match key {
            "schema" => {
                if schema.is_some() {
                    return Err(duplicate("schema"));
                }
                schema = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| corrupted(format!("non-numeric schema: {value:?}")))?,
                );
            }
            "path_encoding" => {
                if path_encoding.is_some() {
                    return Err(duplicate("path_encoding"));
                }
                path_encoding = Some(value);
            }
            "source" => {
                if source.is_some() {
                    return Err(duplicate("source"));
                }
                source = Some(value);
            }
            "target" => {
                if target.is_some() {
                    return Err(duplicate("target"));
                }
                target = Some(value);
            }
            "time" => {
                if time.is_some() {
                    return Err(duplicate("time"));
                }
                time = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| corrupted(format!("non-numeric time: {value:?}")))?,
                );
            }
            "result" => {
                if result.is_some() {
                    return Err(duplicate("result"));
                }
                if !matches!(value, "success" | "incomplete") {
                    return Err(corrupted(format!("invalid result: {value:?}")));
                }
                result = Some(value.to_string());
            }
            _ => return Err(corrupted(format!("unknown marker field: {key:?}"))),
        }
    }
    let schema = schema.ok_or_else(|| corrupted("missing schema field".into()))?;
    if schema != version.schema() {
        return Err(MigrationError::IncompatibleVersion {
            marker_path: marker_path.to_path_buf(),
            found: schema,
            expected: version.schema(),
        });
    }
    let encoding = path_encoding.ok_or_else(|| corrupted("missing path_encoding field".into()))?;
    if encoding != marker_path_encoding() {
        return Err(corrupted(format!(
            "marker path encoding {encoding:?} is not supported on this platform"
        )));
    }
    let source = source.ok_or_else(|| corrupted("missing source field".into()))?;
    let target = target.ok_or_else(|| corrupted("missing target field".into()))?;
    time.ok_or_else(|| corrupted("missing time field".into()))?;
    Ok(Marker {
        version,
        schema,
        result: result.ok_or_else(|| corrupted("missing result field".into()))?,
        source: decode_marker_path(source)
            .map_err(|reason| corrupted(format!("invalid source path: {reason}")))?,
        target: decode_marker_path(target)
            .map_err(|reason| corrupted(format!("invalid target path: {reason}")))?,
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex value has odd length".into());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut characters = value.as_bytes().chunks_exact(2);
    for pair in &mut characters {
        let high = decode_hex_digit(pair[0])?;
        let low = decode_hex_digit(pair[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn decode_hex_digit(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(format!("invalid lowercase hex digit: {value:#04x}")),
    }
}

#[cfg(unix)]
fn marker_path_encoding() -> &'static str {
    "unix-bytes-hex"
}

#[cfg(target_os = "windows")]
fn marker_path_encoding() -> &'static str {
    "windows-wtf16le-hex"
}

#[cfg(not(any(unix, target_os = "windows")))]
fn marker_path_encoding() -> &'static str {
    "utf8-hex"
}

#[cfg(unix)]
fn encode_marker_path(path: &Path) -> Result<String, MigrationError> {
    use std::os::unix::ffi::OsStrExt;

    Ok(encode_hex(path.as_os_str().as_bytes()))
}

#[cfg(target_os = "windows")]
fn encode_marker_path(path: &Path) -> Result<String, MigrationError> {
    use std::os::windows::ffi::OsStrExt;

    let mut bytes = Vec::new();
    for unit in path.as_os_str().encode_wide() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(encode_hex(&bytes))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn encode_marker_path(path: &Path) -> Result<String, MigrationError> {
    path.to_str()
        .map(|value| encode_hex(value.as_bytes()))
        .ok_or_else(|| MigrationError::MarkerCorrupted {
            path: path.to_path_buf(),
            reason: "platform path is not valid UTF-8".into(),
        })
}

#[cfg(unix)]
fn decode_marker_path(value: &str) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStringExt;

    decode_hex(value).map(|bytes| PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(target_os = "windows")]
fn decode_marker_path(value: &str) -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;

    let bytes = decode_hex(value)?;
    if bytes.len() % 2 != 0 {
        return Err("WTF-16LE path has an odd byte length".into());
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    Ok(PathBuf::from(OsString::from_wide(&units)))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn decode_marker_path(value: &str) -> Result<PathBuf, String> {
    let bytes = decode_hex(value)?;
    String::from_utf8(bytes)
        .map(PathBuf::from)
        .map_err(|error| format!("path is not UTF-8: {error}"))
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .map_or(0, |nanos| nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
    static NEXT_TEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn temp_root() -> TestResult<PathBuf> {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "orion-migrate-test-{}-{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&dir)?;
        Ok(fs::canonicalize(dir)?)
    }

    /// Returns true when `dir` contains no leftover `.migrating-*` entries.
    fn no_migration_temps(dir: &Path) -> TestResult<bool> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.contains(".migrating-") || name.starts_with(DIRECTORY_TEMP_PREFIX) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn write_marker_file(dir: &Path, content: &str) -> io::Result<()> {
        fs::write(dir.join(MIGRATION_MARKER_NAME), content)
    }

    fn marker_content(
        source: &Path,
        target: &Path,
        schema: u32,
        result: &str,
    ) -> TestResult<String> {
        marker_content_for_version(source, target, MIGRATION_SCHEMA_VERSION, schema, result)
    }

    fn legacy_v2_marker_content(source: &Path, target: &Path, result: &str) -> TestResult<String> {
        marker_content_for_version(
            source,
            target,
            LEGACY_UNTRUSTED_MIGRATION_SCHEMA_VERSION,
            LEGACY_UNTRUSTED_MIGRATION_SCHEMA_VERSION,
            result,
        )
    }

    fn marker_content_for_version(
        source: &Path,
        target: &Path,
        header_version: u32,
        schema: u32,
        result: &str,
    ) -> TestResult<String> {
        Ok(format!(
            "orion-migration-marker v{header_version}\nschema={schema}\npath_encoding={}\nsource={}\ntarget={}\ntime=0\nresult={result}\n",
            marker_path_encoding(),
            encode_marker_path(source)?,
            encode_marker_path(target)?,
        ))
    }

    fn entry_count(dir: &Path) -> io::Result<usize> {
        let mut count = 0;
        for entry in fs::read_dir(dir)? {
            entry?;
            count += 1;
        }
        Ok(count)
    }

    #[test]
    fn fresh_install_without_legacy() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        assert!(!old.exists());

        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::NoLegacyData);
        assert!(new.exists());
        assert!(new.join(MIGRATION_MARKER_NAME).exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn legacy_database_migrates_before_destination_creation() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        let legacy_database = old.join("db").join("0-stable").join("db.sqlite");
        fs::create_dir_all(
            legacy_database
                .parent()
                .ok_or_else(|| io::Error::other("legacy database path has no parent"))?,
        )?;
        fs::write(&legacy_database, b"legacy-database")?;
        fs::write(old.join("settings.json"), b"{}")?;
        assert!(!new.exists());

        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("settings.json").exists());
        assert_eq!(
            fs::read(new.join("db").join("0-stable").join("db.sqlite"))?,
            b"legacy-database"
        );
        assert!(new.join(MIGRATION_MARKER_NAME).exists());

        // Old directory is retained as backup; never deleted in the first round.
        assert!(old.join("settings.json").exists());
        assert_eq!(fs::read(legacy_database)?, b"legacy-database");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn partial_new_directory_is_merged_without_overwrite() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("from_old.json"), b"old")?;
        fs::create_dir_all(&new)?;
        fs::write(new.join("from_new.json"), b"new")?;

        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("from_old.json").exists());
        assert!(new.join("from_new.json").exists());
        // Pre-existing new file is not overwritten.
        assert_eq!(fs::read_to_string(new.join("from_new.json"))?, "new");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn destination_database_conflict_is_unmarked_and_retryable() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        let legacy_database = old.join("db").join("0-stable").join("db.sqlite");
        let destination_database = new.join("db").join("0-stable").join("db.sqlite");
        fs::create_dir_all(
            legacy_database
                .parent()
                .ok_or_else(|| io::Error::other("legacy database path has no parent"))?,
        )?;
        fs::write(&legacy_database, b"legacy-state")?;
        fs::write(
            old.join("db").join("0-stable").join("a-legacy-only.db"),
            b"legacy-only",
        )?;
        fs::create_dir_all(
            destination_database
                .parent()
                .ok_or_else(|| io::Error::other("destination database path has no parent"))?,
        )?;
        fs::write(&destination_database, b"orion-state")?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::DestinationConflict {
                ref destination,
                ..
            }) if destination == &destination_database
        ));
        assert_eq!(fs::read(&destination_database)?, b"orion-state");
        assert!(!new.join(MIGRATION_MARKER_NAME).exists());
        assert_eq!(fs::read(&legacy_database)?, b"legacy-state");

        // The first attempt may have installed non-conflicting files. Their
        // identical content must be accepted on retry instead of becoming a
        // new conflict.
        assert_eq!(
            fs::read(new.join("db").join("0-stable").join("a-legacy-only.db"),)?,
            b"legacy-only"
        );

        fs::remove_file(&destination_database)?;
        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::Migrated);
        assert_eq!(fs::read(&destination_database)?, b"legacy-state");
        assert!(new.join(MIGRATION_MARKER_NAME).exists());
        assert_eq!(
            fs::read(old.join("db").join("0-stable").join("a-legacy-only.db"),)?,
            b"legacy-only"
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn legacy_v2_success_marker_does_not_hide_database_conflict() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        let legacy_database = old.join("db").join("0-stable").join("db.sqlite");
        let destination_database = new.join("db").join("0-stable").join("db.sqlite");
        fs::create_dir_all(
            legacy_database
                .parent()
                .ok_or_else(|| io::Error::other("legacy database path has no parent"))?,
        )?;
        fs::create_dir_all(
            destination_database
                .parent()
                .ok_or_else(|| io::Error::other("destination database path has no parent"))?,
        )?;
        fs::write(&legacy_database, b"legacy-database")?;
        fs::write(&destination_database, b"post-init-orion-database")?;
        write_marker_file(&new, &legacy_v2_marker_content(&old, &new, "success")?)?;
        let original_marker = fs::read(new.join(MIGRATION_MARKER_NAME))?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::DestinationConflict {
                ref destination,
                ..
            }) if destination == &destination_database
        ));
        assert_eq!(fs::read(&legacy_database)?, b"legacy-database");
        assert_eq!(
            fs::read(&destination_database)?,
            b"post-init-orion-database"
        );
        let marker_after_conflict = fs::read(new.join(MIGRATION_MARKER_NAME))?;
        assert_eq!(marker_after_conflict, original_marker);
        assert!(marker_after_conflict.starts_with(b"orion-migration-marker v2\n"));
        assert!(!marker_after_conflict.starts_with(b"orion-migration-marker v3\n"));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn completable_legacy_v2_tree_upgrades_to_v3_idempotently() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(old.join("db").join("0-stable"))?;
        fs::create_dir_all(&new)?;
        let legacy_settings = old.join("settings.json");
        let destination_settings = new.join("settings.json");
        fs::write(&legacy_settings, b"identical-settings")?;
        fs::copy(&legacy_settings, &destination_settings)?;
        fs::write(
            old.join("db").join("0-stable").join("db.sqlite"),
            b"legacy-database",
        )?;
        write_marker_file(&new, &legacy_v2_marker_content(&old, &new, "success")?)?;

        assert_eq!(migrate_root(&old, &new)?, MigrationState::Migrated);
        assert_eq!(fs::read(&destination_settings)?, b"identical-settings");
        assert_eq!(
            fs::read(new.join("db").join("0-stable").join("db.sqlite"))?,
            b"legacy-database"
        );
        let upgraded_marker = fs::read(new.join(MIGRATION_MARKER_NAME))?;
        assert!(upgraded_marker.starts_with(b"orion-migration-marker v3\n"));
        let marker = read_marker(&new)?
            .ok_or_else(|| io::Error::other("upgraded migration marker should exist"))?;
        assert_eq!(marker.version, MarkerVersion::CurrentV3);
        assert_eq!(marker.schema, MIGRATION_SCHEMA_VERSION);
        assert_eq!(marker.result, "success");

        assert_eq!(migrate_root(&old, &new)?, MigrationState::Completed);
        assert_eq!(fs::read(new.join(MIGRATION_MARKER_NAME))?, upgraded_marker);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn legacy_v2_marker_without_source_is_preserved_until_retry() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&new)?;
        write_marker_file(&new, &legacy_v2_marker_content(&old, &new, "success")?)?;
        let original_marker = fs::read(new.join(MIGRATION_MARKER_NAME))?;

        assert!(matches!(
            migrate_root(&old, &new),
            Err(MigrationError::MarkerVerificationSourceMissing {
                schema: LEGACY_UNTRUSTED_MIGRATION_SCHEMA_VERSION,
                ..
            })
        ));
        assert_eq!(fs::read(new.join(MIGRATION_MARKER_NAME))?, original_marker);

        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"legacy-settings")?;
        assert_eq!(migrate_root(&old, &new)?, MigrationState::Migrated);
        assert!(
            fs::read(new.join(MIGRATION_MARKER_NAME))?.starts_with(b"orion-migration-marker v3\n")
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn repeated_migration_is_idempotent() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;

        assert_eq!(migrate_root(&old, &new)?, MigrationState::Migrated);
        // Second run sees the success marker and does not recopy.
        assert_eq!(migrate_root(&old, &new)?, MigrationState::Completed);
        assert_eq!(entry_count(&new)?, 2); // settings.json + marker

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn interrupted_partial_new_retries_successfully() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?; // represents a failed previous run
        fs::write(old.join("settings.json"), b"{}")?;

        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("settings.json").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn write_failure_preserves_legacy_data() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"secret-config")?;

        // Make the new target's parent a regular file so no safe temp location
        // exists; migration must error rather than destroy legacy data.
        let parent_file = root.join("blocker");
        fs::write(&parent_file, b"not a directory")?;
        let new = parent_file.join("orion-studio");

        let result = migrate_root(&old, &new);
        assert!(result.is_err());

        // Legacy data is intact and untouched.
        assert!(old.join("settings.json").exists());
        assert_eq!(
            fs::read_to_string(old.join("settings.json"))?,
            "secret-config"
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_copied() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        let target = root.join("somewhere");
        fs::write(&target, b"x")?;
        std::os::unix::fs::symlink(&target, old.join("link"))?;
        fs::write(old.join("real.json"), b"{}")?;

        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("real.json").exists());
        assert!(!new.join("link").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn nested_symlink_is_not_copied() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(old.join("sub"))?;
        let target = root.join("outside");
        fs::write(&target, b"x")?;
        std::os::unix::fs::symlink(&target, old.join("sub").join("link"))?;

        migrate_root(&old, &new)?;
        // The directory is migrated, but the nested symlink is not.
        assert!(new.join("sub").exists());
        assert!(!new.join("sub").join("link").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn socket_file_is_not_copied() -> TestResult {
        // Unix sockets have a short path limit (SUN_LEN), so this test uses a
        // compact root instead of the deep `temp_root()` path.
        let root = std::env::temp_dir().join(format!(
            "om-{}-{}",
            std::process::id(),
            now_nanos() % 1_000_000
        ));
        fs::create_dir(&root)?;
        let root = fs::canonicalize(root)?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        let socket_path = old.join("s.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;

        migrate_root(&old, &new)?;
        assert!(!new.join("s.sock").exists());

        drop(listener);
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn secrets_are_copied_and_legacy_retained() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(old.join("auth"))?;
        fs::write(old.join("auth").join("token.json"), b"credential")?;

        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::Migrated);
        // The credential is present in the new location...
        assert!(new.join("auth").join("token.json").exists());
        // ...and the legacy directory is retained as backup (no deletion).
        assert!(old.join("auth").join("token.json").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn copied_files_preserve_permissions() -> TestResult {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(old.join("auth"))?;
        let secret = old.join("auth").join("token.json");
        fs::write(&secret, b"credential")?;
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600))?;

        migrate_root(&old, &new)?;
        let mode = fs::metadata(new.join("auth").join("token.json"))?
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "copied secret must keep its restricted mode");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn incompatible_marker_stops_safely() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;
        fs::create_dir_all(&new)?;
        // Pre-seed an incompatible-version success marker.
        write_marker_file(
            &new,
            "orion-migration-marker v1\nschema=1\nsource=/old\ntarget=/new\ntime=0\nresult=success\n",
        )?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::IncompatibleVersion {
                found: 1,
                expected: MIGRATION_SCHEMA_VERSION,
                ..
            })
        ));
        // Old data was not copied into new (new only has the marker).
        assert_eq!(entry_count(&new)?, 1);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn empty_marker_fails_closed() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;
        fs::create_dir_all(&new)?;
        write_marker_file(&new, "")?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::MarkerCorrupted { .. })
        ));
        // Old data was not copied into new (new only has the marker).
        assert_eq!(entry_count(&new)?, 1);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn unrecognized_marker_header_fails_closed() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;
        fs::create_dir_all(&new)?;
        write_marker_file(
            &new,
            "not-a-marker\nschema=1\nresult=success\nsource=/old\n",
        )?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::MarkerCorrupted { .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn future_marker_version_fails_closed() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;
        fs::create_dir_all(&new)?;
        write_marker_file(
            &new,
            &marker_content_for_version(&old, &new, 4, 4, "success")?,
        )?;

        assert!(matches!(
            migrate_root(&old, &new),
            Err(MigrationError::IncompatibleVersion {
                found: 4,
                expected: MIGRATION_SCHEMA_VERSION,
                ..
            })
        ));
        assert!(!new.join("settings.json").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn duplicate_marker_field_fails_closed() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?;
        let source = encode_marker_path(&old)?;
        let target = encode_marker_path(&new)?;
        write_marker_file(
            &new,
            &format!(
                "orion-migration-marker v3\nschema={MIGRATION_SCHEMA_VERSION}\nschema={MIGRATION_SCHEMA_VERSION}\npath_encoding={}\nsource={source}\ntarget={target}\ntime=0\nresult=success\n",
                marker_path_encoding(),
            ),
        )?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::MarkerCorrupted { .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn marker_missing_required_field_fails_closed() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?;
        // Missing `result` and `source`.
        write_marker_file(
            &new,
            &format!(
                "orion-migration-marker v3\nschema={MIGRATION_SCHEMA_VERSION}\npath_encoding={}\n",
                marker_path_encoding()
            ),
        )?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::MarkerCorrupted { .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn marker_source_mismatch_fails_closed() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;
        fs::create_dir_all(&new)?;
        let other_source = root.join("different-legacy");
        write_marker_file(
            &new,
            &marker_content(&other_source, &new, MIGRATION_SCHEMA_VERSION, "success")?,
        )?;

        let result = migrate_root(&old, &new);
        assert!(matches!(
            result,
            Err(MigrationError::MarkerSourceMismatch { .. })
        ));
        // Old data was not merged in.
        assert!(!new.join("settings.json").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn interrupted_marker_reattempts_migration() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;
        fs::create_dir_all(&new)?;
        write_marker_file(
            &new,
            &marker_content(&old, &new, MIGRATION_SCHEMA_VERSION, "incomplete")?,
        )?;

        let state = migrate_root(&old, &new)?;
        assert_eq!(state, MigrationState::Migrated);
        assert!(new.join("settings.json").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn failed_copy_cleans_temp_directory() -> TestResult {
        let root = temp_root()?;
        // `old` is a regular file, so reading it as a directory tree fails
        // after the temp directory was created; the temp dir must be removed.
        let old = root.join("zed");
        fs::write(&old, b"not a directory")?;
        let new = root.join("orion-studio");

        let result = migrate_root(&old, &new);
        assert!(matches!(result, Err(MigrationError::Io { .. })));
        assert!(!new.exists());
        assert!(
            no_migration_temps(&root)?,
            "temp directory must be cleaned up"
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn no_temp_directory_left_after_repeated_runs() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("a.json"), b"{}")?;

        assert_eq!(migrate_root(&old, &new)?, MigrationState::Migrated);
        assert_eq!(migrate_root(&old, &new)?, MigrationState::Completed);
        assert!(no_migration_temps(&root)?);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn cleanup_after_failure_returns_original_when_cleanup_succeeds() -> TestResult {
        let root = temp_root()?;
        let temp = root.join("leftover");
        fs::create_dir_all(&temp)?;
        let original = MigrationError::NoParent(PathBuf::from("/no-parent"));

        let result = cleanup_after_failure(&temp, original);
        assert!(matches!(result, MigrationError::NoParent(_)));
        assert!(!temp.exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn cleanup_after_failure_reports_cleanup_errors() -> TestResult {
        let root = temp_root()?;
        let blocker = root.join("blocker");
        fs::write(&blocker, b"not a directory")?;
        let temp = blocker.join("leftover");
        let original = MigrationError::NoParent(PathBuf::from("/no-parent"));

        let result = cleanup_after_failure(&temp, original);
        assert!(matches!(result, MigrationError::CleanupFailed { .. }));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn cleanup_after_success_removes_temp() -> TestResult {
        let root = temp_root()?;
        let temp = root.join("tempdir");
        fs::create_dir_all(&temp)?;

        assert!(cleanup_after_success(&temp).is_ok());
        assert!(!temp.exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn cleanup_after_success_reports_leftover_temp() -> TestResult {
        let root = temp_root()?;
        let blocker = root.join("blocker");
        fs::write(&blocker, b"not a directory")?;
        let temp = blocker.join("leftover");

        let result = cleanup_after_success(&temp);
        assert!(matches!(
            result,
            Err(MigrationError::TempCleanupFailed { .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn migrate_legacy_dirs_migrates_existing_roots_only() -> TestResult {
        let root = temp_root()?;
        // Legacy DATA exists; legacy CONFIG does not.
        let legacy_config = root.join("zed-config");
        let legacy_data = root.join("zed-data");
        fs::create_dir_all(legacy_data.join("db"))?;
        fs::write(legacy_data.join("db").join("settings.db"), b"data")?;
        let new_config = root.join("orion-config");
        let new_data = root.join("orion-data");

        let outcome = migrate_legacy_dirs(
            Some(&legacy_config),
            &new_config,
            Some(&legacy_data),
            &new_data,
        )?;
        assert_eq!(outcome.config, None);
        assert_eq!(outcome.data, Some(MigrationState::Migrated));
        // The config root was not touched at all (fresh-install behavior).
        assert!(!new_config.exists());
        assert!(new_data.join("db").join("settings.db").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn migrate_legacy_dirs_no_legacy_does_nothing() -> TestResult {
        let root = temp_root()?;
        let legacy_config = root.join("zed-config");
        let legacy_data = root.join("zed-data");
        let new_config = root.join("orion-config");
        let new_data = root.join("orion-data");

        let outcome = migrate_legacy_dirs(
            Some(&legacy_config),
            &new_config,
            Some(&legacy_data),
            &new_data,
        )?;
        assert_eq!(outcome.config, None);
        assert_eq!(outcome.data, None);
        assert!(!new_config.exists());
        assert!(!new_data.exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn incompatible_marker_aborts_without_legacy_directory() -> TestResult {
        let root = temp_root()?;
        let legacy_config = root.join("zed-config");
        let legacy_data = root.join("zed-data");
        let new_config = root.join("orion-config");
        let new_data = root.join("orion-data");
        fs::create_dir_all(&new_config)?;
        write_marker_file(
            &new_config,
            &marker_content(&legacy_config, &new_config, 999, "success")?,
        )?;

        let result = migrate_legacy_dirs(
            Some(&legacy_config),
            &new_config,
            Some(&legacy_data),
            &new_data,
        );
        assert!(matches!(
            result,
            Err(MigrationError::IncompatibleVersion { found: 999, .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn migrate_legacy_dirs_is_idempotent_across_runs() -> TestResult {
        let root = temp_root()?;
        let legacy_config = root.join("zed-config");
        let legacy_data = root.join("zed-data");
        fs::create_dir_all(&legacy_config)?;
        fs::write(legacy_config.join("settings.json"), b"{}")?;
        let new_config = root.join("orion-config");
        let new_data = root.join("orion-data");

        let first = migrate_legacy_dirs(
            Some(&legacy_config),
            &new_config,
            Some(&legacy_data),
            &new_data,
        )?;
        assert_eq!(first.config, Some(MigrationState::Migrated));
        assert_eq!(first.data, None);

        let second = migrate_legacy_dirs(
            Some(&legacy_config),
            &new_config,
            Some(&legacy_data),
            &new_data,
        )?;
        assert_eq!(second.config, Some(MigrationState::Completed));
        // Nothing was copied a second time: only settings.json + marker.
        assert_eq!(entry_count(&new_config)?, 2);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn migrate_legacy_dirs_propagates_failure_and_preserves_legacy() -> TestResult {
        let root = temp_root()?;
        let legacy_config = root.join("zed-config");
        let legacy_data = root.join("zed-data");
        fs::create_dir_all(&legacy_config)?;
        fs::write(legacy_config.join("settings.json"), b"{}")?;
        fs::create_dir_all(&legacy_data)?;
        fs::write(legacy_data.join("db.json"), b"{}")?;
        // Block the data root: its parent is a regular file.
        let blocker = root.join("blocker");
        fs::write(&blocker, b"not a dir")?;
        let new_config = root.join("orion-config");
        let new_data = blocker.join("orion-data");

        let result = migrate_legacy_dirs(
            Some(&legacy_config),
            &new_config,
            Some(&legacy_data),
            &new_data,
        );
        assert!(result.is_err());
        // Legacy config data is untouched...
        assert!(legacy_config.join("settings.json").exists());
        // ...and the config root migration that ran first is still applied.
        assert!(new_config.join("settings.json").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn flatpak_legacy_roots_append_zed() -> TestResult {
        let flatpak_root = PathBuf::from("/flatpak/xdg-config");
        assert_eq!(
            flatpak_legacy_root(flatpak_root.clone().into_os_string()),
            flatpak_root.join(LEGACY_APP_NAME_LOWERCASE)
        );
        Ok(())
    }

    #[test]
    fn rejects_same_ancestor_and_descendant_roots() -> TestResult {
        let root = temp_root()?;

        let same = root.join("same");
        fs::create_dir_all(&same)?;
        assert!(matches!(
            migrate_root(&same, &same),
            Err(MigrationError::OverlappingRoots { .. })
        ));

        let source_parent = root.join("source-parent");
        fs::create_dir_all(&source_parent)?;
        assert!(matches!(
            migrate_root(&source_parent, &source_parent.join("orion-studio")),
            Err(MigrationError::OverlappingRoots { .. })
        ));

        let destination_parent = root.join("destination-parent");
        let nested_source = destination_parent.join("zed");
        fs::create_dir_all(&nested_source)?;
        assert!(matches!(
            migrate_root(&nested_source, &destination_parent),
            Err(MigrationError::OverlappingRoots { .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn canonical_parent_check_rejects_hidden_overlap() -> TestResult {
        let root = temp_root()?;
        let actual_parent = root.join("actual");
        let source = actual_parent.join("zed");
        fs::create_dir_all(&source)?;
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&actual_parent, &alias)?;
        let destination = alias.join("zed").join("orion-studio");

        assert!(matches!(
            migrate_root(&source, &destination),
            Err(MigrationError::OverlappingRoots { .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_source_and_destination_roots() -> TestResult {
        let root = temp_root()?;
        let actual_source = root.join("actual-source");
        fs::create_dir_all(&actual_source)?;
        fs::write(actual_source.join("settings.json"), b"{}")?;
        let source_link = root.join("source-link");
        std::os::unix::fs::symlink(&actual_source, &source_link)?;
        assert!(matches!(
            migrate_root(&source_link, &root.join("new-from-source-link")),
            Err(MigrationError::UnsafeSymlink { .. })
        ));

        let source = root.join("source");
        fs::create_dir_all(&source)?;
        fs::write(source.join("settings.json"), b"{}")?;
        let outside_destination = root.join("outside-destination");
        fs::create_dir_all(&outside_destination)?;
        let destination_link = root.join("destination-link");
        std::os::unix::fs::symlink(&outside_destination, &destination_link)?;
        assert!(matches!(
            migrate_root(&source, &destination_link),
            Err(MigrationError::UnsafeSymlink { .. })
        ));
        assert_eq!(entry_count(&outside_destination)?, 0);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn merge_never_follows_destination_symlink() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        fs::create_dir_all(old.join("auth"))?;
        fs::write(old.join("auth").join("token.json"), b"credential")?;
        let new = root.join("orion-studio");
        fs::create_dir_all(&new)?;
        let outside = root.join("outside");
        fs::create_dir_all(&outside)?;
        std::os::unix::fs::symlink(&outside, new.join("auth"))?;

        assert!(matches!(
            migrate_root(&old, &new),
            Err(MigrationError::UnsafeSymlink { .. })
        ));
        assert!(!outside.join("token.json").exists());
        assert!(!new.join(MIGRATION_MARKER_NAME).exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn atomic_marker_survives_interrupted_temp_write() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?;
        write_marker(&new, &old, "success")?;
        let marker_path = new.join(MIGRATION_MARKER_NAME);
        let expected_marker = fs::read_to_string(&marker_path)?;

        let interrupted_temp = new.join(format!("{MARKER_TEMP_PREFIX}-interrupted"));
        let mut interrupted_file = open_new_file(&interrupted_temp, 0o600)?;
        interrupted_file.write_all(b"orion-migration-marker v3\nschema=")?;
        interrupted_file.sync_all()?;
        drop(interrupted_file);

        assert_eq!(migrate_root(&old, &new)?, MigrationState::Completed);
        assert_eq!(fs::read_to_string(&marker_path)?, expected_marker);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn concurrent_migration_is_idempotent() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first_barrier = barrier.clone();
        let first_old = old.clone();
        let first_new = new.clone();
        let first = std::thread::spawn(move || {
            first_barrier.wait();
            migrate_root(&first_old, &first_new)
        });
        let second_old = old;
        let second_new = new.clone();
        let second = std::thread::spawn(move || {
            barrier.wait();
            migrate_root(&second_old, &second_new)
        });

        let first_state = first
            .join()
            .map_err(|_| io::Error::other("first migration thread panicked"))??;
        let second_state = second
            .join()
            .map_err(|_| io::Error::other("second migration thread panicked"))??;
        assert!(
            matches!(first_state, MigrationState::Migrated)
                ^ matches!(second_state, MigrationState::Migrated)
        );
        assert!(
            matches!(first_state, MigrationState::Completed)
                ^ matches!(second_state, MigrationState::Completed)
        );
        assert_eq!(fs::read_to_string(new.join("settings.json"))?, "{}");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn new_directories_preserve_restricted_permissions() -> TestResult {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root()?;
        let old = root.join("zed");
        let auth = old.join("auth");
        fs::create_dir_all(&auth)?;
        fs::write(auth.join("token.json"), b"credential")?;
        fs::set_permissions(&old, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&auth, fs::Permissions::from_mode(0o700))?;
        let new = root.join("orion-studio");

        migrate_root(&old, &new)?;
        let root_mode = fs::metadata(&new)?.permissions().mode() & 0o777;
        let auth_mode = fs::metadata(new.join("auth"))?.permissions().mode() & 0o777;
        assert_eq!(root_mode, 0o700);
        assert_eq!(auth_mode, 0o700);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn merge_keeps_existing_destination_directory_permissions() -> TestResult {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root()?;
        let old = root.join("zed");
        let old_auth = old.join("auth");
        fs::create_dir_all(&old_auth)?;
        fs::write(old_auth.join("token.json"), b"credential")?;
        fs::set_permissions(&old_auth, fs::Permissions::from_mode(0o700))?;

        let new = root.join("orion-studio");
        let new_auth = new.join("auth");
        fs::create_dir_all(&new_auth)?;
        fs::set_permissions(&new_auth, fs::Permissions::from_mode(0o750))?;

        migrate_root(&old, &new)?;
        let mode = fs::metadata(&new_auth)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o750);
        assert!(new_auth.join("token.json").exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn changing_source_exhausts_retries_without_committing_marker() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        let source_file = old.join("settings.json");
        fs::write(&source_file, b"initial")?;

        let _lock = acquire_migration_lock(&new)?;
        let result = migrate_root_unlocked_with_after_copy(&old, &new, |attempt| {
            fs::write(&source_file, format!("changed-{attempt}")).map_err(|error| {
                MigrationError::io(error, source_file.clone(), "mutate test source")
            })
        });

        assert!(matches!(
            result,
            Err(MigrationError::SourceChanged {
                attempts: MAX_SOURCE_STABILITY_ATTEMPTS,
                ..
            })
        ));
        assert!(!new.join(MIGRATION_MARKER_NAME).exists());
        assert!(old.join("settings.json").exists());
        assert!(no_migration_temps(&root)?);

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn opened_source_file_detects_symlink_swap() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        fs::create_dir_all(&old)?;
        let source_path = old.join("settings.json");
        fs::write(&source_path, b"trusted")?;
        let outside = root.join("outside.json");
        fs::write(&outside, b"outside")?;

        let source = SecureDirectory::open(&old, "open symlink swap source")?;
        let mut pinned =
            source.open_child_file(OsStr::new("settings.json"), false, "pin source before swap")?;
        let original = old.join("settings.original");
        fs::rename(&source_path, &original)?;
        std::os::unix::fs::symlink(&outside, &source_path)?;

        let verification = source.verify_child_identity(
            OsStr::new("settings.json"),
            pinned.identity,
            "verify source after swap",
        );
        assert!(matches!(
            verification,
            Err(MigrationError::UnsafeSymlink { .. })
                | Err(MigrationError::ObjectIdentityChanged { .. })
        ));
        let mut pinned_content = String::new();
        pinned.file.read_to_string(&mut pinned_content)?;
        assert_eq!(pinned_content, "trusted");

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn marker_symlink_is_rejected_without_touching_target() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?;
        fs::write(old.join("settings.json"), b"{}")?;
        let outside = root.join("outside-marker");
        fs::write(&outside, b"outside")?;
        std::os::unix::fs::symlink(&outside, new.join(MIGRATION_MARKER_NAME))?;

        assert!(matches!(
            migrate_root(&old, &new),
            Err(MigrationError::UnsafeSymlink { .. })
        ));
        assert_eq!(fs::read(&outside)?, b"outside");
        assert!(
            fs::symlink_metadata(new.join(MIGRATION_MARKER_NAME))?
                .file_type()
                .is_symlink()
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn lock_symlink_is_rejected_without_committing_marker() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;
        let outside = root.join("outside-lock");
        fs::write(&outside, b"outside")?;
        std::os::unix::fs::symlink(&outside, root.join(MIGRATION_LOCK_NAME))?;

        assert!(matches!(
            migrate_root(&old, &new),
            Err(MigrationError::UnsafeSymlink { .. })
        ));
        assert_eq!(fs::read(&outside)?, b"outside");
        assert!(!new.join(MIGRATION_MARKER_NAME).exists());

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn pinned_parent_prevents_destination_symlink_escape() -> TestResult {
        let root = temp_root()?;
        let parent_path = root.join("destination-parent");
        fs::create_dir(&parent_path)?;
        let parent = SecureDirectory::open(&parent_path, "pin destination parent")?;
        let pinned_path = root.join("pinned-parent");
        fs::rename(&parent_path, &pinned_path)?;
        let outside = root.join("outside");
        fs::create_dir(&outside)?;
        std::os::unix::fs::symlink(&outside, &parent_path)?;

        let mut created = parent.create_child_file(
            OsStr::new("safe.txt"),
            0o600,
            "create through pinned parent",
        )?;
        created.file.write_all(b"safe")?;
        created.sync("sync pinned destination file")?;
        assert!(pinned_path.join("safe.txt").exists());
        assert!(!outside.join("safe.txt").exists());
        assert!(matches!(
            parent.verify_path_identity("verify swapped destination parent"),
            Err(MigrationError::UnsafeSymlink { .. })
                | Err(MigrationError::ObjectIdentityChanged { .. })
        ));

        fs::remove_file(&parent_path)?;
        fs::rename(&pinned_path, &parent_path)?;
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn marker_round_trips_non_utf8_and_newline_paths() -> TestResult {
        use std::os::unix::ffi::OsStringExt;

        let root = temp_root()?;
        let non_utf8 = PathBuf::from(OsString::from_vec(b"legacy\n\xff".to_vec()));
        let encoded_non_utf8 = encode_marker_path(&non_utf8)?;
        assert_eq!(decode_marker_path(&encoded_non_utf8)?, non_utf8);

        let old = root.join("legacy\nsource");
        let new = root.join("orion\ntarget");
        fs::create_dir_all(&old)?;
        fs::write(old.join("settings.json"), b"{}")?;

        assert_eq!(migrate_root(&old, &new)?, MigrationState::Migrated);
        assert_eq!(migrate_root(&old, &new)?, MigrationState::Completed);
        let marker =
            read_marker(&new)?.ok_or_else(|| io::Error::other("migration marker should exist"))?;
        assert_eq!(marker.source, old);
        assert_eq!(marker.target, new);
        let marker_bytes = fs::read(new.join(MIGRATION_MARKER_NAME))?;
        assert!(marker_bytes.starts_with(b"orion-migration-marker v3\n"));
        assert!(!marker_bytes.windows(7).any(|bytes| bytes == b"source\n"));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn marker_round_trips_windows_wtf16_and_newline() -> TestResult {
        use std::os::windows::ffi::OsStringExt;

        let original = PathBuf::from(OsString::from_wide(&[
            b'C' as u16,
            b':' as u16,
            b'\\' as u16,
            b'x' as u16,
            b'\n' as u16,
            0xd800,
        ]));
        let encoded = encode_marker_path(&original)?;
        assert_eq!(decode_marker_path(&encoded)?, original);
        Ok(())
    }

    #[test]
    fn streaming_sha256_matches_known_vector() {
        let mut digest = Sha256State::new();
        digest.update(b"a");
        digest.update(b"bc");
        assert_eq!(
            encode_hex(&digest.finalize()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn marker_v3_and_directory_sync_helpers_are_strict() -> TestResult {
        let root = temp_root()?;
        let old = root.join("zed");
        let new = root.join("orion-studio");
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?;

        write_marker(&new, &old, "success")?;
        sync_directory(&new, "test directory sync")?;
        let marker =
            read_marker(&new)?.ok_or_else(|| io::Error::other("migration marker should exist"))?;
        validate_marker(&marker, &old, &new)?;
        let marker_text = fs::read_to_string(new.join(MIGRATION_MARKER_NAME))?;
        assert!(marker_text.starts_with("orion-migration-marker v3\n"));
        assert!(marker_text.contains(&format!("path_encoding={}\n", marker_path_encoding())));

        let invalid = marker_text.replacen("source=", "source=G", 1);
        assert!(matches!(
            parse_marker(invalid.as_bytes(), &new.join(MIGRATION_MARKER_NAME)),
            Err(MigrationError::MarkerCorrupted { .. })
        ));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn legacy_dirs_use_zed_naming() -> TestResult {
        let legacy_config = legacy_config_dir()
            .ok_or_else(|| io::Error::other("legacy config dir should be resolvable"))?;
        let legacy_data = legacy_data_dir()
            .ok_or_else(|| io::Error::other("legacy data dir should be resolvable"))?;
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
        Ok(())
    }
}
