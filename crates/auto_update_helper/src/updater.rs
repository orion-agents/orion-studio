use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::OsStrExt,
    path::Path,
    sync::LazyLock,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use windows::{
    Win32::{
        Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, HWND, LPARAM, WIN32_ERROR, WPARAM},
        System::RestartManager::{
            CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
            RmShutdown, RmStartSession,
        },
        UI::WindowsAndMessaging::PostMessageW,
    },
    core::{PCWSTR, PWSTR},
};

use crate::windows_impl::WM_JOB_UPDATED;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RollbackState {
    Applied,
    CreatedDirectory,
    NoChange,
    Irreversible,
}

pub(crate) struct Job {
    pub apply: Box<dyn Fn(&Path) -> Result<RollbackState> + Send + Sync>,
    pub rollback: Box<dyn Fn(&Path, RollbackState) -> Result<()> + Send + Sync>,
}

impl Job {
    pub fn mkdir(name: &'static Path) -> Self {
        Job {
            apply: Box::new(move |app_dir| {
                let dir = app_dir.join(name);
                match std::fs::create_dir(&dir) {
                    Ok(()) => Ok(RollbackState::CreatedDirectory),
                    Err(error)
                        if error.kind() == std::io::ErrorKind::AlreadyExists && dir.is_dir() =>
                    {
                        Ok(RollbackState::NoChange)
                    }
                    Err(error) => {
                        Err(error).context(format!("Failed to create directory {}", dir.display()))
                    }
                }
            }),
            rollback: Box::new(move |app_dir, state| {
                let dir = app_dir.join(name);
                if state == RollbackState::CreatedDirectory {
                    std::fs::remove_dir(&dir).context(format!(
                        "Failed to remove directory created by this update {}",
                        dir.display()
                    ))?;
                }
                Ok(())
            }),
        }
    }

    pub fn mkdir_if_exists(name: &'static Path, check: &'static Path) -> Self {
        Job {
            apply: Box::new(move |app_dir| {
                let dir = app_dir.join(name);
                let check = app_dir.join(check);

                if check.exists() {
                    match std::fs::create_dir(&dir) {
                        Ok(()) => Ok(RollbackState::CreatedDirectory),
                        Err(error)
                            if error.kind() == std::io::ErrorKind::AlreadyExists
                                && dir.is_dir() =>
                        {
                            Ok(RollbackState::NoChange)
                        }
                        Err(error) => Err(error)
                            .context(format!("Failed to create directory {}", dir.display())),
                    }
                } else {
                    Ok(RollbackState::NoChange)
                }
            }),
            rollback: Box::new(move |app_dir, state| {
                let dir = app_dir.join(name);

                if state == RollbackState::CreatedDirectory {
                    std::fs::remove_dir(&dir).context(format!(
                        "Failed to remove directory created by this update {}",
                        dir.display()
                    ))?;
                }

                Ok(())
            }),
        }
    }

    pub fn move_file(filename: &'static Path, new_filename: &'static Path) -> Self {
        Job {
            apply: Box::new(move |app_dir| {
                let old_file = app_dir.join(filename);
                let new_file = app_dir.join(new_filename);
                log::info!(
                    "Moving file: {}->{}",
                    old_file.display(),
                    new_file.display()
                );

                std::fs::rename(&old_file, new_file)
                    .context(format!("Failed to move file {}", old_file.display()))?;
                Ok(RollbackState::Applied)
            }),
            rollback: Box::new(move |app_dir, state| {
                if state != RollbackState::Applied {
                    return Ok(());
                }
                let old_file = app_dir.join(filename);
                let new_file = app_dir.join(new_filename);
                log::info!(
                    "Rolling back file move: {}->{}",
                    old_file.display(),
                    new_file.display()
                );

                std::fs::rename(&new_file, &old_file).context(format!(
                    "Failed to rollback file move {}->{}",
                    new_file.display(),
                    old_file.display()
                ))
            }),
        }
    }

    pub fn move_if_exists(filename: &'static Path, new_filename: &'static Path) -> Self {
        Job {
            apply: Box::new(move |app_dir| {
                let old_file = app_dir.join(filename);
                let new_file = app_dir.join(new_filename);

                if old_file.exists() {
                    log::info!(
                        "Moving file: {}->{}",
                        old_file.display(),
                        new_file.display()
                    );

                    std::fs::rename(&old_file, new_file)
                        .context(format!("Failed to move file {}", old_file.display()))?;
                    return Ok(RollbackState::Applied);
                }

                Ok(RollbackState::NoChange)
            }),
            rollback: Box::new(move |app_dir, state| {
                if state != RollbackState::Applied {
                    return Ok(());
                }
                let old_file = app_dir.join(filename);
                let new_file = app_dir.join(new_filename);

                if new_file.exists() {
                    log::info!(
                        "Rolling back file move: {}->{}",
                        old_file.display(),
                        new_file.display()
                    );

                    std::fs::rename(&new_file, &old_file).context(format!(
                        "Failed to rollback file move {}->{}",
                        new_file.display(),
                        old_file.display()
                    ))?
                }

                Ok(())
            }),
        }
    }

    pub fn rmdir_nofail(filename: &'static Path) -> Self {
        Job {
            apply: Box::new(move |app_dir| {
                let filename = app_dir.join(filename);
                log::info!("Removing file: {}", filename.display());
                if let Err(e) = std::fs::remove_dir_all(&filename) {
                    log::warn!("Failed to remove directory: {}", e);
                }

                Ok(RollbackState::Irreversible)
            }),
            rollback: Box::new(move |app_dir, state| {
                let filename = app_dir.join(filename);
                if state == RollbackState::Irreversible {
                    anyhow::bail!(
                        "Delete operations cannot be rolled back, file: {}",
                        filename.display()
                    )
                }
                Ok(())
            }),
        }
    }
}

#[cfg(test)]
const TRANSACTIONAL_JOB_COUNT: usize = 28;

fn production_jobs() -> [Job; 31] {
    fn p(value: &str) -> &Path {
        Path::new(value)
    }
    [
        // Preserve the current installation so every completed move can be rolled back.
        Job::mkdir(p("old")),
        Job::move_if_exists(p("orion-studio.exe"), p("old\\orion-studio.exe")),
        // A legacy Zed executable is accepted only as an upgrade input.
        Job::move_if_exists(p("Zed.exe"), p("old\\Zed.exe")),
        Job::mkdir(p("old\\bin")),
        Job::move_if_exists(p("bin\\orion-studio.exe"), p("old\\bin\\orion-studio.exe")),
        Job::move_if_exists(p("bin\\orion.exe"), p("old\\bin\\orion.exe")),
        Job::move_if_exists(p("bin\\zed.exe"), p("old\\bin\\zed.exe")),
        Job::move_if_exists(p("bin\\orion-studio"), p("old\\bin\\orion-studio")),
        Job::move_if_exists(p("bin\\orion"), p("old\\bin\\orion")),
        Job::move_if_exists(p("bin\\zed"), p("old\\bin\\zed")),
        //
        // TODO: remove after a few weeks once everyone is on the new version and this file never exists
        Job::move_if_exists(p("OpenConsole.exe"), p("old\\OpenConsole.exe")),
        Job::mkdir(p("old\\x64")),
        Job::mkdir(p("old\\arm64")),
        Job::move_if_exists(p("x64\\OpenConsole.exe"), p("old\\x64\\OpenConsole.exe")),
        Job::move_if_exists(
            p("arm64\\OpenConsole.exe"),
            p("old\\arm64\\OpenConsole.exe"),
        ),
        //
        Job::move_if_exists(p("conpty.dll"), p("old\\conpty.dll")),
        // Install canonical Orion Studio files and the approved CLI compatibility aliases.
        Job::move_file(p("install\\orion-studio.exe"), p("orion-studio.exe")),
        Job::move_file(
            p("install\\bin\\orion-studio.exe"),
            p("bin\\orion-studio.exe"),
        ),
        Job::move_file(p("install\\bin\\orion.exe"), p("bin\\orion.exe")),
        Job::move_file(p("install\\bin\\zed.exe"), p("bin\\zed.exe")),
        Job::move_file(p("install\\bin\\orion-studio"), p("bin\\orion-studio")),
        Job::move_file(p("install\\bin\\orion"), p("bin\\orion")),
        Job::move_file(p("install\\bin\\zed"), p("bin\\zed")),
        //
        Job::mkdir_if_exists(p("x64"), p("install\\x64")),
        Job::mkdir_if_exists(p("arm64"), p("install\\arm64")),
        Job::move_if_exists(
            p("install\\x64\\OpenConsole.exe"),
            p("x64\\OpenConsole.exe"),
        ),
        Job::move_if_exists(
            p("install\\arm64\\OpenConsole.exe"),
            p("arm64\\OpenConsole.exe"),
        ),
        //
        Job::move_file(p("install\\conpty.dll"), p("conpty.dll")),
        // Cleanup installer and updates folder
        Job::rmdir_nofail(p("updates")),
        Job::rmdir_nofail(p("install")),
        // Cleanup old installation
        Job::rmdir_nofail(p("old")),
    ]
}

#[cfg(not(test))]
pub(crate) static JOBS: LazyLock<[Job; 31]> = LazyLock::new(production_jobs);

#[cfg(test)]
pub(crate) static JOBS: LazyLock<[Job; 9]> = LazyLock::new(|| {
    fn p(value: &str) -> &Path {
        Path::new(value)
    }
    [
        Job {
            apply: Box::new(|_| {
                std::thread::sleep(Duration::from_millis(1000));
                if let Ok(config) = std::env::var("ORION_STUDIO_AUTO_UPDATE_TEST") {
                    match config.as_str() {
                        "err1" => Err(std::io::Error::other("Simulated error")).context("Anyhow!"),
                        "err2" => Ok(RollbackState::Applied),
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("Unknown ORION_STUDIO_AUTO_UPDATE_TEST value: {config}"),
                        ))
                        .context("Invalid test configuration"),
                    }
                } else {
                    Ok(RollbackState::Applied)
                }
            }),
            rollback: Box::new(|_, _| {
                unsafe { std::env::set_var("ORION_STUDIO_AUTO_UPDATE_ROLLBACK_TEST", "rollback1") };
                Ok(())
            }),
        },
        Job::mkdir(p("test1")),
        Job::mkdir_if_exists(p("test_exists"), p("test1")),
        Job::mkdir_if_exists(p("test_missing"), p("dont")),
        Job {
            apply: Box::new(|folder| {
                std::fs::write(folder.join("test1/test"), "test")?;
                Ok(RollbackState::Applied)
            }),
            rollback: Box::new(|folder, _| {
                std::fs::remove_file(folder.join("test1/test"))?;
                Ok(())
            }),
        },
        Job::move_file(p("test1/test"), p("test1/moved")),
        Job::move_if_exists(p("test1/test"), p("test1/noop")),
        Job {
            apply: Box::new(|_| {
                std::thread::sleep(Duration::from_millis(1000));
                if let Ok(config) = std::env::var("ORION_STUDIO_AUTO_UPDATE_TEST") {
                    match config.as_str() {
                        "err1" => Ok(RollbackState::Applied),
                        "err2" => Err(std::io::Error::other("Simulated error")).context("Anyhow!"),
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("Unknown ORION_STUDIO_AUTO_UPDATE_TEST value: {config}"),
                        ))
                        .context("Invalid test configuration"),
                    }
                } else {
                    Ok(RollbackState::Applied)
                }
            }),
            rollback: Box::new(|_, _| Ok(())),
        },
        Job::rmdir_nofail(p("test1/nofolder")),
    ]
});

/// Attempts to use Windows Restart Manager to release file handles held by other processes
/// (e.g., Explorer.exe) on the files we need to move during the update.
///
/// This is a best-effort operation - if it fails, we'll still try the update and rely on
/// the retry logic.
fn initial_restart_manager_capacity(
    status: WIN32_ERROR,
    processes_needed: u32,
) -> Result<Option<usize>> {
    match status {
        ERROR_SUCCESS if processes_needed == 0 => Ok(None),
        ERROR_SUCCESS | ERROR_MORE_DATA if processes_needed > 0 => {
            Ok(Some(usize::try_from(processes_needed)?))
        }
        ERROR_MORE_DATA => {
            anyhow::bail!("RmGetList returned ERROR_MORE_DATA without a required process count")
        }
        _ => anyhow::bail!("RmGetList size query failed: {status:?}"),
    }
}

fn query_affected_processes_with(
    mut get_list: impl FnMut(&mut u32, &mut u32, Option<*mut RM_PROCESS_INFO>, &mut u32) -> WIN32_ERROR,
) -> Result<Vec<RM_PROCESS_INFO>> {
    let mut processes_needed = 0;
    let mut process_count = 0;
    let mut reboot_reasons = 0;
    let status = get_list(
        &mut processes_needed,
        &mut process_count,
        None,
        &mut reboot_reasons,
    );
    let Some(initial_capacity) = initial_restart_manager_capacity(status, processes_needed)? else {
        return Ok(Vec::new());
    };

    let mut processes = vec![RM_PROCESS_INFO::default(); initial_capacity];
    for _ in 0..3 {
        process_count = u32::try_from(processes.len())
            .context("Too many processes returned by Restart Manager")?;
        let status = get_list(
            &mut processes_needed,
            &mut process_count,
            Some(processes.as_mut_ptr()),
            &mut reboot_reasons,
        );
        if status == ERROR_SUCCESS {
            let process_count = usize::try_from(process_count)?;
            if process_count > processes.len() {
                anyhow::bail!(
                    "RmGetList returned {process_count} processes for a {}-entry buffer",
                    processes.len()
                );
            }
            processes.truncate(process_count);
            return Ok(processes);
        }
        if status != ERROR_MORE_DATA {
            anyhow::bail!("RmGetList process query failed: {status:?}");
        }

        let required_capacity = usize::try_from(processes_needed)?;
        if required_capacity <= processes.len() {
            anyhow::bail!(
                "RmGetList returned ERROR_MORE_DATA without increasing the required process count"
            );
        }
        processes.resize(required_capacity, RM_PROCESS_INFO::default());
    }

    anyhow::bail!("RmGetList process list kept changing after three attempts")
}

fn query_affected_processes(session: u32) -> Result<Vec<RM_PROCESS_INFO>> {
    query_affected_processes_with(
        |processes_needed, process_count, affected_processes, reboot_reasons| unsafe {
            RmGetList(
                session,
                processes_needed,
                process_count,
                affected_processes,
                reboot_reasons,
            )
        },
    )
}

fn release_file_handles(app_dir: &Path) -> Result<()> {
    // Files that commonly get locked by Explorer or other processes
    let files_to_release = [
        app_dir.join("orion-studio.exe"),
        app_dir.join("bin\\orion-studio.exe"),
        app_dir.join("bin\\orion.exe"),
        app_dir.join("bin\\zed.exe"),
        app_dir.join("bin\\orion-studio"),
        app_dir.join("bin\\orion"),
        app_dir.join("bin\\zed"),
        // A legacy root executable can still exist on the first Orion upgrade.
        app_dir.join("Zed.exe"),
        app_dir.join("conpty.dll"),
    ];

    log::info!("Attempting to release file handles using Restart Manager...");

    let mut session: u32 = 0;
    let mut session_key = [0u16; CCH_RM_SESSION_KEY as usize + 1];

    // Start a Restart Manager session
    let status = unsafe {
        RmStartSession(
            &mut session,
            Some(0),
            PWSTR::from_raw(session_key.as_mut_ptr()),
        )
    };
    if status != ERROR_SUCCESS {
        anyhow::bail!("RmStartSession failed: {status:?}");
    }

    // Ensure we end the session when done
    let _session_guard = scopeguard::guard(session, |session| {
        let status = unsafe { RmEndSession(session) };
        if status != ERROR_SUCCESS {
            log::warn!("Unable to end Restart Manager session: {status:?}");
        }
    });

    // Convert paths to wide strings for Windows API
    let wide_paths: Vec<Vec<u16>> = files_to_release
        .iter()
        .filter(|p| p.exists())
        .map(|p| {
            OsStr::new(p)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();

    if wide_paths.is_empty() {
        log::info!("No files to release handles for");
        return Ok(());
    }

    let pcwstr_paths: Vec<PCWSTR> = wide_paths
        .iter()
        .map(|p| PCWSTR::from_raw(p.as_ptr()))
        .collect();

    // Register the files we want to modify
    let status = unsafe { RmRegisterResources(session, Some(&pcwstr_paths), None, None) };
    if status != ERROR_SUCCESS {
        anyhow::bail!("RmRegisterResources failed: {status:?}");
    }

    let affected_processes = query_affected_processes(session)?;
    if affected_processes.is_empty() {
        log::info!("No processes are holding handles to the files");
        return Ok(());
    }

    log::info!(
        "{} process(es) are holding handles to the files, requesting release...",
        affected_processes.len()
    );

    // Request processes to release their handles
    // RmShutdown with flags=0 asks applications to release handles gracefully
    // For Explorer, this typically releases icon cache handles without closing Explorer
    let status = unsafe { RmShutdown(session, 0, None) };
    if status != ERROR_SUCCESS {
        anyhow::bail!("RmShutdown failed: {status:?}");
    }

    log::info!("Successfully requested handle release");
    Ok(())
}

#[allow(clippy::disallowed_methods, reason = "doesn't run in the main binary")]
fn orion_studio_launch_command(
    app_dir: &Path,
    launch_arguments: &[OsString],
) -> std::process::Command {
    let mut command = std::process::Command::new(app_dir.join("orion-studio.exe"));
    command.args(launch_arguments);
    command
}

pub(crate) fn perform_update(
    app_dir: &Path,
    hwnd: Option<isize>,
    launch: bool,
    launch_arguments: &[OsString],
) -> Result<()> {
    let hwnd = hwnd.map(|ptr| HWND(ptr as _));

    // Try to release file handles before starting the update
    if let Err(e) = release_file_handles(app_dir) {
        log::warn!("Restart Manager failed (will continue anyway): {}", e);
    }

    let mut completed_jobs = Vec::with_capacity(JOBS.len());
    'outer: for (i, job) in JOBS.iter().enumerate() {
        let start = Instant::now();
        loop {
            if start.elapsed().as_secs() > 2 {
                log::error!("Timed out, rolling back");
                break 'outer;
            }
            match (job.apply)(app_dir) {
                Ok(rollback_state) => {
                    completed_jobs.push((i, rollback_state));
                    if let Err(error) =
                        unsafe { PostMessageW(hwnd, WM_JOB_UPDATED, WPARAM(0), LPARAM(0)) }
                    {
                        log::warn!("Unable to update the updater progress dialog: {error:?}");
                    }
                    break;
                }
                Err(err) => match err.downcast_ref::<std::io::Error>() {
                    Some(io_err) => match io_err.kind() {
                        std::io::ErrorKind::NotFound => {
                            log::error!("Operation failed with file not found, aborting: {}", err);
                            break 'outer;
                        }
                        _ => {
                            log::error!("Operation failed (retrying): {}", err);
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    },
                    None => {
                        log::error!("Operation failed with unexpected error, aborting: {}", err);
                        break 'outer;
                    }
                },
            }
        }
    }

    if completed_jobs.len() != JOBS.len() {
        if completed_jobs.is_empty() {
            anyhow::bail!("Autoupdate failed, nothing to rollback");
        }

        for (job_index, rollback_state) in completed_jobs.into_iter().rev() {
            let job = &JOBS[job_index];
            if let Err(e) = (job.rollback)(app_dir, rollback_state) {
                anyhow::bail!(
                    "Job rollback failed, the app might be left in an inconsistent state: ({:?})",
                    e
                );
            }
        }

        anyhow::bail!("Autoupdate failed, rollback successful");
    }

    if launch {
        #[allow(clippy::disallowed_methods, reason = "doesn't run in the main binary")]
        let _child = orion_studio_launch_command(app_dir, launch_arguments)
            .spawn()
            .context("Failed to relaunch Orion Studio after the update")?;
    }
    log::info!("Update completed successfully");
    Ok(())
}

#[cfg(test)]
mod test {
    use std::{ffi::OsString, path::Path};

    use super::{
        RollbackState, TRANSACTIONAL_JOB_COUNT, initial_restart_manager_capacity,
        orion_studio_launch_command, perform_update, production_jobs,
        query_affected_processes_with,
    };
    use anyhow::Result;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_MORE_DATA, ERROR_SUCCESS};

    fn write_fixture_file(app_directory: &Path, relative_path: &str, content: &str) -> Result<()> {
        let path = app_directory.join(relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    fn prepare_production_update_fixture(app_directory: &Path) -> Result<()> {
        for (relative_path, content) in [
            ("orion-studio.exe", "old-main"),
            ("Zed.exe", "legacy-main"),
            ("bin\\orion-studio.exe", "old-canonical-cli"),
            ("bin\\orion.exe", "old-short-cli"),
            ("bin\\zed.exe", "old-legacy-cli"),
            ("bin\\orion-studio", "old-canonical-shell"),
            ("bin\\orion", "old-short-shell"),
            ("bin\\zed", "old-legacy-shell"),
            ("OpenConsole.exe", "old-root-console"),
            ("x64\\OpenConsole.exe", "old-x64-console"),
            ("arm64\\OpenConsole.exe", "old-arm64-console"),
            ("conpty.dll", "old-conpty"),
            ("install\\orion-studio.exe", "new-main"),
            ("install\\bin\\orion-studio.exe", "new-canonical-cli"),
            ("install\\bin\\orion.exe", "new-short-cli"),
            ("install\\bin\\zed.exe", "new-legacy-cli"),
            ("install\\bin\\orion-studio", "new-canonical-shell"),
            ("install\\bin\\orion", "new-short-shell"),
            ("install\\bin\\zed", "new-legacy-shell"),
            ("install\\x64\\OpenConsole.exe", "new-x64-console"),
            ("install\\arm64\\OpenConsole.exe", "new-arm64-console"),
            ("install\\conpty.dll", "new-conpty"),
        ] {
            write_fixture_file(app_directory, relative_path, content)?;
        }
        Ok(())
    }

    fn assert_production_fixture_restored(app_directory: &Path) -> Result<()> {
        for (relative_path, content) in [
            ("orion-studio.exe", "old-main"),
            ("Zed.exe", "legacy-main"),
            ("bin\\orion-studio.exe", "old-canonical-cli"),
            ("bin\\orion.exe", "old-short-cli"),
            ("bin\\zed.exe", "old-legacy-cli"),
            ("bin\\orion-studio", "old-canonical-shell"),
            ("bin\\orion", "old-short-shell"),
            ("bin\\zed", "old-legacy-shell"),
            ("OpenConsole.exe", "old-root-console"),
            ("x64\\OpenConsole.exe", "old-x64-console"),
            ("arm64\\OpenConsole.exe", "old-arm64-console"),
            ("conpty.dll", "old-conpty"),
            ("install\\orion-studio.exe", "new-main"),
            ("install\\bin\\orion-studio.exe", "new-canonical-cli"),
            ("install\\bin\\orion.exe", "new-short-cli"),
            ("install\\bin\\zed.exe", "new-legacy-cli"),
            ("install\\bin\\orion-studio", "new-canonical-shell"),
            ("install\\bin\\orion", "new-short-shell"),
            ("install\\bin\\zed", "new-legacy-shell"),
            ("install\\x64\\OpenConsole.exe", "new-x64-console"),
            ("install\\arm64\\OpenConsole.exe", "new-arm64-console"),
            ("install\\conpty.dll", "new-conpty"),
        ] {
            assert_eq!(
                std::fs::read_to_string(app_directory.join(relative_path))?,
                content,
                "unexpected contents for {relative_path}"
            );
        }
        assert!(!app_directory.join("old").exists());
        Ok(())
    }

    #[test]
    fn test_orion_studio_launch_command_preserves_arguments() {
        let arguments = vec![
            OsString::from("--user-data-dir"),
            OsString::from(r"C:\Orion Studio Data"),
        ];
        let command =
            orion_studio_launch_command(Path::new(r"C:\Program Files\Orion Studio"), &arguments);

        assert_eq!(
            command.get_program(),
            Path::new(r"C:\Program Files\Orion Studio\orion-studio.exe").as_os_str()
        );
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            arguments
                .iter()
                .map(OsString::as_os_str)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn restart_manager_more_data_requests_a_second_query() -> Result<()> {
        assert_eq!(
            initial_restart_manager_capacity(ERROR_MORE_DATA, 2)?,
            Some(2)
        );
        assert_eq!(initial_restart_manager_capacity(ERROR_SUCCESS, 0)?, None);
        assert!(initial_restart_manager_capacity(ERROR_MORE_DATA, 0).is_err());
        assert!(initial_restart_manager_capacity(ERROR_ACCESS_DENIED, 0).is_err());
        Ok(())
    }

    #[test]
    fn restart_manager_locked_files_reach_the_process_result() -> Result<()> {
        let mut query_count = 0;
        let processes = query_affected_processes_with(
            |processes_needed, process_count, affected_processes, _reboot_reasons| {
                query_count += 1;
                if query_count == 1 {
                    *processes_needed = 2;
                    ERROR_MORE_DATA
                } else {
                    assert!(affected_processes.is_some());
                    *process_count = 2;
                    ERROR_SUCCESS
                }
            },
        )?;

        assert_eq!(query_count, 2);
        assert_eq!(processes.len(), 2);
        Ok(())
    }

    #[test]
    fn test_perform_update() -> Result<(), Box<dyn std::error::Error>> {
        let app_dir = tempfile::tempdir()?;
        let app_dir = app_dir.path();
        assert!(perform_update(app_dir, None, false, &[]).is_ok());

        let app_dir = tempfile::tempdir()?;
        let app_dir = app_dir.path();
        // Simulate a timeout
        unsafe { std::env::set_var("ORION_STUDIO_AUTO_UPDATE_TEST", "err1") };
        let return_value = perform_update(app_dir, None, false, &[]);
        assert!(
            return_value
                .is_err_and(|error| error.to_string() == "Autoupdate failed, nothing to rollback")
        );

        let app_dir = tempfile::tempdir()?;
        let app_dir = app_dir.path();
        // Simulate a timeout
        unsafe { std::env::set_var("ORION_STUDIO_AUTO_UPDATE_TEST", "err2") };
        let return_value = perform_update(app_dir, None, false, &[]);
        assert!(
            return_value
                .is_err_and(|error| error.to_string() == "Autoupdate failed, rollback successful")
        );
        assert!(
            std::env::var("ORION_STUDIO_AUTO_UPDATE_ROLLBACK_TEST")
                .is_ok_and(|value| value == "rollback1")
        );
        unsafe {
            std::env::remove_var("ORION_STUDIO_AUTO_UPDATE_TEST");
            std::env::remove_var("ORION_STUDIO_AUTO_UPDATE_ROLLBACK_TEST");
        }
        Ok(())
    }

    #[test]
    fn production_jobs_replace_canonical_cli_and_preserve_compatibility_aliases() -> Result<()> {
        let app_directory = tempfile::tempdir()?;
        let app_directory = app_directory.path();
        prepare_production_update_fixture(app_directory)?;

        for job in production_jobs() {
            let _rollback_state = (job.apply)(app_directory)?;
        }

        assert_eq!(
            std::fs::read_to_string(app_directory.join("orion-studio.exe"))?,
            "new-main"
        );
        assert_eq!(
            std::fs::read_to_string(app_directory.join("bin\\orion-studio.exe"))?,
            "new-canonical-cli"
        );
        assert_eq!(
            std::fs::read_to_string(app_directory.join("bin\\orion.exe"))?,
            "new-short-cli"
        );
        assert_eq!(
            std::fs::read_to_string(app_directory.join("bin\\zed.exe"))?,
            "new-legacy-cli"
        );
        assert_eq!(
            std::fs::read_to_string(app_directory.join("bin\\orion-studio"))?,
            "new-canonical-shell"
        );
        assert_eq!(
            std::fs::read_to_string(app_directory.join("bin\\orion"))?,
            "new-short-shell"
        );
        assert_eq!(
            std::fs::read_to_string(app_directory.join("bin\\zed"))?,
            "new-legacy-shell"
        );
        assert!(!app_directory.join("Zed.exe").exists());
        assert!(!app_directory.join("old").exists());
        assert!(!app_directory.join("install").exists());

        Ok(())
    }

    #[test]
    fn production_jobs_restore_the_complete_installation_at_every_failure_boundary() -> Result<()> {
        for failure_boundary in 0..=TRANSACTIONAL_JOB_COUNT {
            let app_directory = tempfile::tempdir()?;
            let app_directory = app_directory.path();
            prepare_production_update_fixture(app_directory)?;
            let jobs = production_jobs();
            let mut completed_jobs = Vec::new();

            for (job_index, job) in jobs.iter().take(failure_boundary).enumerate() {
                let rollback_state = (job.apply)(app_directory)?;
                completed_jobs.push((job_index, rollback_state));
            }
            for (job_index, rollback_state) in completed_jobs.into_iter().rev() {
                (jobs[job_index].rollback)(app_directory, rollback_state)?;
            }

            assert_production_fixture_restored(app_directory)?;
        }
        Ok(())
    }

    #[test]
    fn mkdir_rollback_preserves_a_preexisting_directory() -> Result<()> {
        let app_directory = tempfile::tempdir()?;
        let app_directory = app_directory.path();
        std::fs::create_dir(app_directory.join("x64"))?;
        write_fixture_file(app_directory, "x64\\sentinel", "keep")?;

        let job = super::Job::mkdir(Path::new("x64"));
        let rollback_state = (job.apply)(app_directory)?;
        assert_eq!(rollback_state, RollbackState::NoChange);
        (job.rollback)(app_directory, rollback_state)?;

        assert_eq!(
            std::fs::read_to_string(app_directory.join("x64\\sentinel"))?,
            "keep"
        );
        Ok(())
    }
}
