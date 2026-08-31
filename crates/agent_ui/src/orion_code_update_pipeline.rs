use std::sync::Arc;

use futures::{FutureExt as _, future::BoxFuture};
use gpui::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use parking_lot::Mutex;
use util::ResultExt as _;

use crate::{
    orion_code_update::{
        OrionCodeCandidate, OrionCodeCurrentReleaseState, OrionCodeUpdateErrorKind,
    },
    orion_code_update_activation::{OrionCodeActivationCoordinator, OrionCodeActivationStatus},
    orion_code_update_coordinator::OrionCodeUpdateCoordinator,
    orion_code_update_installer::{
        OrionCodeArchiveInstaller, OrionCodeArchiveInstallerError, OrionCodeDownloadProgress,
        OrionCodeDownloadProgressReporter, OrionCodeStagedIdentity,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodePipelineError {
    pub kind: OrionCodeUpdateErrorKind,
    pub diagnostic_code: String,
}

pub trait OrionCodeCandidateInstaller: Send + Sync {
    fn stage(
        &self,
        candidate: OrionCodeCandidate,
        index_sequence: u64,
    ) -> BoxFuture<'static, Result<OrionCodeStagedIdentity, OrionCodePipelineError>>;

    fn stage_with_progress(
        &self,
        candidate: OrionCodeCandidate,
        index_sequence: u64,
        _progress_reporter: Arc<dyn OrionCodeDownloadProgressReporter>,
    ) -> BoxFuture<'static, Result<OrionCodeStagedIdentity, OrionCodePipelineError>> {
        self.stage(candidate, index_sequence)
    }
}

pub struct OrionCodeArchiveInstallerHandle {
    installer: Arc<OrionCodeArchiveInstaller>,
}

impl OrionCodeArchiveInstallerHandle {
    pub fn new(installer: Arc<OrionCodeArchiveInstaller>) -> Self {
        Self { installer }
    }
}

impl OrionCodeCandidateInstaller for OrionCodeArchiveInstallerHandle {
    fn stage(
        &self,
        candidate: OrionCodeCandidate,
        index_sequence: u64,
    ) -> BoxFuture<'static, Result<OrionCodeStagedIdentity, OrionCodePipelineError>> {
        let installer = self.installer.clone();
        async move {
            installer
                .stage(candidate, index_sequence)
                .await
                .map_err(map_installer_error)
        }
        .boxed()
    }

    fn stage_with_progress(
        &self,
        candidate: OrionCodeCandidate,
        index_sequence: u64,
        progress_reporter: Arc<dyn OrionCodeDownloadProgressReporter>,
    ) -> BoxFuture<'static, Result<OrionCodeStagedIdentity, OrionCodePipelineError>> {
        let installer = self.installer.clone();
        async move {
            installer
                .stage_with_progress(candidate, index_sequence, progress_reporter)
                .await
                .map_err(map_installer_error)
        }
        .boxed()
    }
}

struct LatestOrionCodeDownloadProgressReporter {
    sender: Mutex<watch::Sender<OrionCodeDownloadProgress>>,
}

impl OrionCodeDownloadProgressReporter for LatestOrionCodeDownloadProgressReporter {
    fn report(&self, progress: OrionCodeDownloadProgress) {
        if self.sender.lock().send(progress).is_err() {
            log::debug!("Orion Code download progress receiver was closed");
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrionCodeUpdatePipelineStatus {
    Disabled {
        diagnostic_code: String,
    },
    Idle,
    CandidateAvailable {
        version: String,
        archive_bytes: u64,
        release_notes_url: String,
        signed_rollback: bool,
    },
    Downloading {
        version: String,
        archive_bytes: u64,
        received_bytes: u64,
        total_bytes: u64,
    },
    Staged {
        version: String,
    },
    Failed {
        version: Option<String>,
        kind: OrionCodeUpdateErrorKind,
        diagnostic_code: String,
    },
}

struct GlobalOrionCodeUpdatePipeline(Entity<OrionCodeUpdatePipeline>);

impl Global for GlobalOrionCodeUpdatePipeline {}

pub struct OrionCodeUpdatePipeline {
    status: OrionCodeUpdatePipelineStatus,
    installer: Option<Arc<dyn OrionCodeCandidateInstaller>>,
    active_download: Option<Task<()>>,
    attempted_candidate: Option<(u64, String)>,
    _subscriptions: Vec<Subscription>,
}

impl OrionCodeUpdatePipeline {
    pub fn init_global(cx: &mut App) -> Entity<Self> {
        Self::init_global_with_installer(None, cx)
    }

    pub fn init_global_with_installer(
        installer: Option<Arc<dyn OrionCodeCandidateInstaller>>,
        cx: &mut App,
    ) -> Entity<Self> {
        if let Some(pipeline) = Self::try_global(cx) {
            if let Some(installer) = installer {
                pipeline.update(cx, |pipeline, cx| {
                    pipeline.installer = Some(installer);
                    pipeline.attempted_candidate = None;
                    pipeline.reconcile(true, cx);
                });
            }
            return pipeline;
        }
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let activation_coordinator = OrionCodeActivationCoordinator::global(cx);
        let pipeline = cx.new(|cx| {
            let update_subscription = cx.observe(
                &update_coordinator,
                |pipeline: &mut OrionCodeUpdatePipeline, _coordinator, cx| {
                    pipeline.reconcile(true, cx);
                },
            );
            let activation_subscription = cx.observe(
                &activation_coordinator,
                |pipeline: &mut OrionCodeUpdatePipeline, _coordinator, cx| {
                    pipeline.reconcile(false, cx);
                },
            );
            Self {
                status: OrionCodeUpdatePipelineStatus::Idle,
                installer,
                active_download: None,
                attempted_candidate: None,
                _subscriptions: vec![update_subscription, activation_subscription],
            }
        });
        cx.set_global(GlobalOrionCodeUpdatePipeline(pipeline.clone()));
        pipeline.update(cx, |pipeline, cx| pipeline.reconcile(true, cx));
        pipeline
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalOrionCodeUpdatePipeline>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalOrionCodeUpdatePipeline>()
            .map(|pipeline| pipeline.0.clone())
    }

    pub fn status(&self) -> &OrionCodeUpdatePipelineStatus {
        &self.status
    }

    #[cfg(test)]
    pub(crate) fn take_active_download_for_test(&mut self) -> Option<Task<()>> {
        self.active_download.take()
    }

    pub fn configure_installer(
        &mut self,
        installer: Arc<dyn OrionCodeCandidateInstaller>,
        cx: &mut Context<Self>,
    ) {
        self.installer = Some(installer);
        self.attempted_candidate = None;
        self.reconcile(true, cx);
    }

    pub fn download_available(&mut self, cx: &mut Context<Self>) {
        self.start_download(true, cx);
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        self.attempted_candidate = None;
        self.start_download(true, cx);
    }

    fn reconcile(&mut self, allow_automatic_download: bool, cx: &mut Context<Self>) {
        if self.active_download.is_some() {
            return;
        }
        let activation_status = OrionCodeActivationCoordinator::global(cx)
            .read(cx)
            .status()
            .clone();
        match activation_status {
            OrionCodeActivationStatus::Staged { version }
            | OrionCodeActivationStatus::WaitingForIdle { version }
            | OrionCodeActivationStatus::Activating { version } => {
                self.status = OrionCodeUpdatePipelineStatus::Staged { version };
                cx.notify();
                return;
            }
            _ => {}
        }

        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let coordinator = update_coordinator.read(cx);
        let resolution = coordinator.latest_resolution().cloned();
        let automatic = coordinator.automatic_install_enabled();
        let index_sequence = coordinator.record().highest_index_sequence;

        let Some(resolution) = resolution else {
            self.status = if self.installer.is_none() {
                OrionCodeUpdatePipelineStatus::Disabled {
                    diagnostic_code: "managed_archive_installer_not_configured".to_string(),
                }
            } else {
                OrionCodeUpdatePipelineStatus::Idle
            };
            cx.notify();
            return;
        };
        let Some(candidate) = resolution.candidate else {
            self.status =
                if resolution.current_release_state == OrionCodeCurrentReleaseState::Revoked {
                    OrionCodeUpdatePipelineStatus::Failed {
                        version: None,
                        kind: OrionCodeUpdateErrorKind::Rollback,
                        diagnostic_code: "revoked_release_has_no_safe_rollback".to_string(),
                    }
                } else {
                    OrionCodeUpdatePipelineStatus::Idle
                };
            cx.notify();
            return;
        };
        self.status = OrionCodeUpdatePipelineStatus::CandidateAvailable {
            version: candidate.version.to_string(),
            archive_bytes: candidate.target.archive_bytes,
            release_notes_url: candidate.release.release_notes_url.clone(),
            signed_rollback: candidate.is_signed_rollback,
        };
        cx.notify();
        if allow_automatic_download && automatic {
            self.start_candidate_download(candidate, index_sequence, false, cx);
        }
    }

    fn start_download(&mut self, user_initiated: bool, cx: &mut Context<Self>) {
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let coordinator = update_coordinator.read(cx);
        let candidate = coordinator
            .latest_resolution()
            .and_then(|resolution| resolution.candidate.clone());
        let index_sequence = coordinator.record().highest_index_sequence;
        let Some(candidate) = candidate else {
            self.status = OrionCodeUpdatePipelineStatus::Failed {
                version: None,
                kind: OrionCodeUpdateErrorKind::Incompatible,
                diagnostic_code: "no_compatible_candidate_available".to_string(),
            };
            cx.notify();
            return;
        };
        self.start_candidate_download(candidate, index_sequence, user_initiated, cx);
    }

    fn start_candidate_download(
        &mut self,
        candidate: OrionCodeCandidate,
        index_sequence: u64,
        user_initiated: bool,
        cx: &mut Context<Self>,
    ) {
        if self.active_download.is_some() {
            return;
        }
        let Some(installer) = self.installer.clone() else {
            self.status = OrionCodeUpdatePipelineStatus::Disabled {
                diagnostic_code: "managed_archive_installer_not_configured".to_string(),
            };
            cx.notify();
            return;
        };
        let identity = (index_sequence, candidate.version.to_string());
        if !user_initiated && self.attempted_candidate.as_ref() == Some(&identity) {
            return;
        }
        self.attempted_candidate = Some(identity);
        let version = candidate.version.to_string();
        let initial_progress = OrionCodeDownloadProgress::new(0, candidate.target.archive_bytes);
        self.status = OrionCodeUpdatePipelineStatus::Downloading {
            version: version.clone(),
            archive_bytes: initial_progress.total_bytes(),
            received_bytes: initial_progress.received_bytes(),
            total_bytes: initial_progress.total_bytes(),
        };
        cx.notify();
        let (progress_sender, mut progress_receiver) = watch::channel(initial_progress);
        let progress_reporter = Arc::new(LatestOrionCodeDownloadProgressReporter {
            sender: Mutex::new(progress_sender),
        });
        self.active_download = Some(cx.spawn(async move |pipeline, cx| {
            let stage = installer.stage_with_progress(candidate, index_sequence, progress_reporter);
            let progress_updates = async {
                while let Ok(progress) = progress_receiver.recv().await {
                    pipeline
                        .update(cx, |pipeline, cx| {
                            if apply_download_progress(&mut pipeline.status, &version, progress) {
                                cx.notify();
                            }
                        })
                        .log_err();
                }
            };
            let (result, ()) = futures::future::join(stage, progress_updates).await;
            pipeline
                .update(cx, |pipeline, cx| {
                    pipeline.active_download = None;
                    match result {
                        Ok(staged) => {
                            pipeline.status = OrionCodeUpdatePipelineStatus::Staged {
                                version: staged.version.to_string(),
                            };
                            cx.notify();
                            OrionCodeActivationCoordinator::global(cx)
                                .update(cx, |activation, cx| {
                                    activation.record_staged_identity(staged, cx)
                                });
                        }
                        Err(error) => {
                            pipeline.status = OrionCodeUpdatePipelineStatus::Failed {
                                version: Some(version.clone()),
                                kind: error.kind,
                                diagnostic_code: error.diagnostic_code,
                            };
                            cx.notify();
                        }
                    }
                })
                .log_err();
        }));
    }
}

fn apply_download_progress(
    status: &mut OrionCodeUpdatePipelineStatus,
    version: &str,
    progress: OrionCodeDownloadProgress,
) -> bool {
    let OrionCodeUpdatePipelineStatus::Downloading {
        version: active_version,
        received_bytes,
        total_bytes,
        ..
    } = status
    else {
        return false;
    };
    if active_version != version {
        return false;
    }

    let next_received_bytes = progress.received_bytes().min(*total_bytes);
    if next_received_bytes <= *received_bytes {
        return false;
    }
    *received_bytes = next_received_bytes;
    true
}

fn map_installer_error(error: OrionCodeArchiveInstallerError) -> OrionCodePipelineError {
    let (kind, diagnostic_code) = match error {
        OrionCodeArchiveInstallerError::Configuration(_)
        | OrionCodeArchiveInstallerError::MissingPreflight
        | OrionCodeArchiveInstallerError::InvalidCandidate(_)
        | OrionCodeArchiveInstallerError::UnsafeManagedPath(_) => (
            OrionCodeUpdateErrorKind::Configuration,
            "archive_installer_configuration_failed",
        ),
        OrionCodeArchiveInstallerError::Request(_)
        | OrionCodeArchiveInstallerError::Transport(_)
        | OrionCodeArchiveInstallerError::Redirect(_)
        | OrionCodeArchiveInstallerError::HttpStatus(_)
        | OrionCodeArchiveInstallerError::MissingContentLength
        | OrionCodeArchiveInstallerError::InvalidContentLength(_)
        | OrionCodeArchiveInstallerError::ContentLengthMismatch { .. }
        | OrionCodeArchiveInstallerError::BodyTooLarge { .. }
        | OrionCodeArchiveInstallerError::TruncatedBody { .. } => (
            OrionCodeUpdateErrorKind::Download,
            "archive_download_failed",
        ),
        OrionCodeArchiveInstallerError::ArchiveDigestMismatch => (
            OrionCodeUpdateErrorKind::Checksum,
            "archive_checksum_mismatch",
        ),
        OrionCodeArchiveInstallerError::Archive(_) => (
            OrionCodeUpdateErrorKind::Manifest,
            "archive_verification_failed",
        ),
        OrionCodeArchiveInstallerError::Platform(_) => (
            OrionCodeUpdateErrorKind::PlatformSignature,
            "platform_trust_verification_failed",
        ),
        OrionCodeArchiveInstallerError::Preflight(_) => (
            OrionCodeUpdateErrorKind::Preflight,
            "archive_preflight_failed",
        ),
        OrionCodeArchiveInstallerError::ExistingState(_)
        | OrionCodeArchiveInstallerError::Receipt(_)
        | OrionCodeArchiveInstallerError::Io { .. }
        | OrionCodeArchiveInstallerError::Cleanup { .. } => (
            OrionCodeUpdateErrorKind::Persistence,
            "archive_staging_persistence_failed",
        ),
    };
    OrionCodePipelineError {
        kind,
        diagnostic_code: diagnostic_code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_errors_map_to_fixed_non_sensitive_diagnostics() {
        let error = map_installer_error(OrionCodeArchiveInstallerError::Transport(
            "/Users/example/token=secret".to_string(),
        ));
        assert_eq!(error.kind, OrionCodeUpdateErrorKind::Download);
        assert_eq!(error.diagnostic_code, "archive_download_failed");
    }

    #[test]
    fn download_progress_is_bounded_monotonic_and_keeps_the_candidate_total() {
        let mut status = OrionCodeUpdatePipelineStatus::Downloading {
            version: "0.4.0".to_string(),
            archive_bytes: 100,
            received_bytes: 0,
            total_bytes: 100,
        };

        assert!(apply_download_progress(
            &mut status,
            "0.4.0",
            OrionCodeDownloadProgress::new(75, 100),
        ));
        assert!(!apply_download_progress(
            &mut status,
            "0.4.0",
            OrionCodeDownloadProgress::new(50, 100),
        ));
        assert!(apply_download_progress(
            &mut status,
            "0.4.0",
            OrionCodeDownloadProgress::new(u64::MAX, u64::MAX),
        ));
        assert!(!apply_download_progress(
            &mut status,
            "9.9.9",
            OrionCodeDownloadProgress::new(100, 100),
        ));

        assert_eq!(
            status,
            OrionCodeUpdatePipelineStatus::Downloading {
                version: "0.4.0".to_string(),
                archive_bytes: 100,
                received_bytes: 100,
                total_bytes: 100,
            }
        );
    }
}
