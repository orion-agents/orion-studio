use std::path::{Path, PathBuf};

use agent_servers::{
    AcpPreflightFailureKind, AcpPreflightLimits, AcpPreflightRequest,
    run_isolated_acp_preflight_with_executor,
};
use collections::HashMap;
use futures::{FutureExt as _, future::BoxFuture};
use gpui::BackgroundExecutor;
use orion_code_update::OrionCodeCandidate;
use project::agent_server_store::OrionCodeManagedArchiveRuntime;
use uuid::Uuid;

use crate::{
    orion_code_bootstrap::validate_orion_code_initialize,
    orion_code_update::{OrionCodeStagedReceiptV2, OrionCodeUpdateErrorKind},
    orion_code_update_activation::{
        OrionCodeActivationHealthCheck, OrionCodeActivationHealthError,
    },
    orion_code_update_installer::{OrionCodePreflightError, OrionCodeUpdatePreflight},
};

#[derive(Clone)]
pub struct OrionCodeManagedPreflight {
    preflight_root: PathBuf,
    executor: BackgroundExecutor,
    studio_version: String,
}

impl OrionCodeManagedPreflight {
    pub fn new(
        managed_root: impl AsRef<Path>,
        executor: BackgroundExecutor,
        studio_version: impl Into<String>,
    ) -> Result<Self, String> {
        let managed_root = ensure_real_directory(managed_root.as_ref(), "managed root")?;
        let preflight_root = managed_root.join("preflight");
        std::fs::create_dir_all(&preflight_root)
            .map_err(|_| "failed to create the managed preflight root".to_string())?;
        let preflight_root = ensure_real_directory(&preflight_root, "preflight root")?;
        if preflight_root.parent() != Some(managed_root.as_path()) {
            return Err("managed preflight root escaped its parent".to_string());
        }
        let studio_version = studio_version.into();
        if studio_version.trim().is_empty() || studio_version.len() > 128 {
            return Err("Studio version is invalid for ACP preflight".to_string());
        }
        Ok(Self {
            preflight_root,
            executor,
            studio_version,
        })
    }

    async fn verify_candidate(
        &self,
        artifact_root: PathBuf,
        expected_version: String,
        relative_command: String,
    ) -> Result<(), ManagedPreflightFailure> {
        let artifact_root = ensure_real_directory(&artifact_root, "artifact root")
            .map_err(|_| ManagedPreflightFailure::InvalidInput)?;
        let command = canonical_contained_command(&artifact_root, &relative_command)
            .map_err(|_| ManagedPreflightFailure::InvalidInput)?;
        let operation_root = self
            .create_operation_root()
            .map_err(|_| ManagedPreflightFailure::Prepare)?;
        let mut environment = HashMap::default();
        environment.insert(
            "PATH".to_string(),
            "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
        );
        let request = AcpPreflightRequest {
            command,
            arguments: Vec::new(),
            environment,
            working_directory: operation_root.join("working"),
            config_directory: operation_root.join("config"),
            data_directory: operation_root.join("data"),
            client_version: self.studio_version.clone(),
            limits: AcpPreflightLimits::default(),
        };
        let result = run_isolated_acp_preflight_with_executor(request, self.executor.clone())
            .await
            .map_err(|error| ManagedPreflightFailure::Acp(error.kind()))
            .and_then(|snapshot| {
                validate_orion_code_initialize(&snapshot, &expected_version)
                    .map(|_| ())
                    .map_err(|_| ManagedPreflightFailure::Identity)
            });
        let cleanup = cleanup_operation_root(&self.preflight_root, &operation_root);
        match (result, cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (_, Err(())) => Err(ManagedPreflightFailure::Cleanup),
        }
    }

    fn create_operation_root(&self) -> Result<PathBuf, String> {
        let operation_root = self.preflight_root.join(Uuid::new_v4().to_string());
        if operation_root.parent() != Some(self.preflight_root.as_path()) {
            return Err("preflight operation path escaped its root".to_string());
        }
        std::fs::create_dir(&operation_root)
            .map_err(|_| "failed to create a preflight operation directory".to_string())?;
        ensure_real_directory(&operation_root, "operation root")
    }
}

impl OrionCodeUpdatePreflight for OrionCodeManagedPreflight {
    fn verify(
        &self,
        artifact_root: PathBuf,
        candidate: OrionCodeCandidate,
    ) -> BoxFuture<'static, Result<(), OrionCodePreflightError>> {
        let this = self.clone();
        async move {
            this.verify_candidate(
                artifact_root,
                candidate.version.to_string(),
                candidate.target.command,
            )
            .await
            .map_err(|failure| OrionCodePreflightError::Failed {
                diagnostic_code: failure.diagnostic_code().to_string(),
            })
        }
        .boxed()
    }
}

impl OrionCodeActivationHealthCheck for OrionCodeManagedPreflight {
    fn disabled_diagnostic_code(&self) -> Option<&'static str> {
        None
    }

    fn verify(
        &self,
        staged: OrionCodeStagedReceiptV2,
    ) -> BoxFuture<'static, Result<(), OrionCodeActivationHealthError>> {
        let this = self.clone();
        async move {
            let runtime = OrionCodeManagedArchiveRuntime::new(
                &staged.version,
                &staged.target,
                &staged.command,
            )
            .map_err(|_| {
                OrionCodeActivationHealthError::new(
                    OrionCodeUpdateErrorKind::Activation,
                    "activation_runtime_receipt_invalid",
                )
            })?;
            this.verify_candidate(runtime.install_root(), staged.version, staged.command)
                .await
                .map_err(|failure| {
                    OrionCodeActivationHealthError::new(
                        failure.update_error_kind(),
                        failure.diagnostic_code(),
                    )
                })
        }
        .boxed()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ManagedPreflightFailure {
    InvalidInput,
    Prepare,
    Acp(AcpPreflightFailureKind),
    Identity,
    Cleanup,
}

impl ManagedPreflightFailure {
    fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::InvalidInput => "preflight_input_invalid",
            Self::Prepare => "preflight_directory_prepare_failed",
            Self::Acp(AcpPreflightFailureKind::InvalidRequest) => "preflight_request_invalid",
            Self::Acp(AcpPreflightFailureKind::PrepareDirectory) => {
                "preflight_directory_prepare_failed"
            }
            Self::Acp(AcpPreflightFailureKind::Spawn) => "preflight_spawn_failed",
            Self::Acp(AcpPreflightFailureKind::StartupTimeout) => "preflight_startup_timeout",
            Self::Acp(AcpPreflightFailureKind::InitializeTimeout) => "preflight_initialize_timeout",
            Self::Acp(AcpPreflightFailureKind::AgentExited) => "preflight_agent_exited",
            Self::Acp(AcpPreflightFailureKind::UnsupportedProtocol) => {
                "preflight_protocol_unsupported"
            }
            Self::Acp(AcpPreflightFailureKind::Protocol) => "preflight_protocol_failed",
            Self::Acp(AcpPreflightFailureKind::Cleanup) => "preflight_process_cleanup_failed",
            Self::Acp(AcpPreflightFailureKind::WorkerStopped) => "preflight_worker_stopped",
            Self::Identity => "preflight_identity_mismatch",
            Self::Cleanup => "preflight_directory_cleanup_failed",
        }
    }

    fn update_error_kind(&self) -> OrionCodeUpdateErrorKind {
        match self {
            Self::Cleanup | Self::Acp(AcpPreflightFailureKind::Cleanup) => {
                OrionCodeUpdateErrorKind::Activation
            }
            _ => OrionCodeUpdateErrorKind::Preflight,
        }
    }
}

fn ensure_real_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| format!("failed to inspect {label}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("{label} must be a real directory"));
    }
    std::fs::canonicalize(path).map_err(|_| format!("failed to resolve {label}"))
}

fn canonical_contained_command(
    artifact_root: &Path,
    relative_command: &str,
) -> Result<PathBuf, String> {
    let relative = Path::new(relative_command);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err("preflight command is not a normalized relative path".to_string());
    }
    let command = artifact_root.join(relative);
    let metadata = std::fs::symlink_metadata(&command)
        .map_err(|_| "preflight command is missing".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("preflight command must be a real file".to_string());
    }
    let command = std::fs::canonicalize(command)
        .map_err(|_| "failed to resolve preflight command".to_string())?;
    if !command.starts_with(artifact_root) {
        return Err("preflight command escaped the artifact root".to_string());
    }
    Ok(command)
}

fn cleanup_operation_root(preflight_root: &Path, operation_root: &Path) -> Result<(), ()> {
    if operation_root.parent() != Some(preflight_root) {
        return Err(());
    }
    let metadata = std::fs::symlink_metadata(operation_root).map_err(|_| ())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(());
    }
    let canonical = std::fs::canonicalize(operation_root).map_err(|_| ())?;
    if !canonical.starts_with(preflight_root) {
        return Err(());
    }
    std::fs::remove_dir_all(operation_root).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn command_and_cleanup_never_escape_managed_roots() {
        let temporary = TempDir::new().expect("temporary root");
        let artifact = temporary.path().join("artifact");
        let preflight = temporary.path().join("preflight");
        std::fs::create_dir_all(artifact.join("bin")).expect("artifact directories");
        std::fs::create_dir(&preflight).expect("preflight directory");
        std::fs::write(artifact.join("bin/orion-code-acp"), b"fixture").expect("fixture command");
        let artifact = std::fs::canonicalize(artifact).expect("canonical artifact");
        let preflight = std::fs::canonicalize(preflight).expect("canonical preflight");
        assert!(canonical_contained_command(&artifact, "../outside").is_err());
        assert!(canonical_contained_command(&artifact, "bin/orion-code-acp").is_ok());

        let operation = preflight.join("operation");
        std::fs::create_dir(&operation).expect("operation directory");
        assert!(cleanup_operation_root(&preflight, &operation).is_ok());
        assert!(cleanup_operation_root(&preflight, temporary.path()).is_err());
    }

    #[test]
    fn acp_failures_map_to_fixed_diagnostic_codes() {
        let failure = ManagedPreflightFailure::Acp(AcpPreflightFailureKind::InitializeTimeout);
        assert_eq!(failure.diagnostic_code(), "preflight_initialize_timeout");
        assert_eq!(
            failure.update_error_kind(),
            OrionCodeUpdateErrorKind::Preflight
        );
    }
}
