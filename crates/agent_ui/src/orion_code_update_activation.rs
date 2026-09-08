use std::sync::Arc;

use chrono::Utc;
use db::kvp::KeyValueStore;
use futures::future::{BoxFuture, FutureExt as _};
use gpui::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use project::{
    agent_registry_store::AgentRegistryStore,
    agent_server_store::{
        OrionCodeManagedArchiveRuntime, orion_code_managed_archive_runtime,
        set_orion_code_managed_archive_runtime,
    },
};
use util::ResultExt as _;

use crate::{
    agent_connection_store::{
        OrionCodeActivityKind, OrionCodeUpdateActivity, shutdown_all_orion_code_connections,
    },
    agent_panel::select_native_agent_after_orion_code_revocation,
    orion_code_update::{
        OrionCodeStagedReceiptV2, OrionCodeUpdateErrorKind, OrionCodeUpdateRecordV2,
        abort_orion_code_activation, begin_orion_code_activation, commit_orion_code_activation,
        current_orion_code_release_is_revoked, stage_previous_orion_code_release,
        validate_update_record, write_orion_code_update_record,
    },
    orion_code_update_coordinator::OrionCodeUpdateCoordinator,
    orion_code_update_installer::OrionCodeStagedIdentity,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrionCodeActivationHealthError {
    pub kind: OrionCodeUpdateErrorKind,
    pub diagnostic_code: String,
}

impl OrionCodeActivationHealthError {
    pub fn new(kind: OrionCodeUpdateErrorKind, diagnostic_code: impl Into<String>) -> Self {
        Self {
            kind,
            diagnostic_code: sanitize_diagnostic_code(diagnostic_code.into()),
        }
    }
}

pub trait OrionCodeActivationHealthCheck: Send + Sync {
    fn disabled_diagnostic_code(&self) -> Option<&'static str>;

    fn verify(
        &self,
        staged: OrionCodeStagedReceiptV2,
    ) -> BoxFuture<'static, Result<(), OrionCodeActivationHealthError>>;
}

struct DisabledOrionCodeActivationHealthCheck;

impl OrionCodeActivationHealthCheck for DisabledOrionCodeActivationHealthCheck {
    fn disabled_diagnostic_code(&self) -> Option<&'static str> {
        Some("isolated_acp_preflight_not_configured")
    }

    fn verify(
        &self,
        _staged: OrionCodeStagedReceiptV2,
    ) -> BoxFuture<'static, Result<(), OrionCodeActivationHealthError>> {
        futures::future::ready(Err(OrionCodeActivationHealthError::new(
            OrionCodeUpdateErrorKind::Preflight,
            "isolated_acp_preflight_not_configured",
        )))
        .boxed()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrionCodeActivationStatus {
    Idle,
    StagingState {
        version: String,
    },
    Staged {
        version: String,
    },
    WaitingForIdle {
        version: String,
    },
    Activating {
        version: String,
    },
    Activated {
        version: String,
    },
    Disabled {
        diagnostic_code: String,
    },
    Failed {
        kind: OrionCodeUpdateErrorKind,
        diagnostic_code: String,
    },
}

struct GlobalOrionCodeActivationCoordinator(Entity<OrionCodeActivationCoordinator>);

impl Global for GlobalOrionCodeActivationCoordinator {}

pub struct OrionCodeActivationCoordinator {
    status: OrionCodeActivationStatus,
    health_check: Arc<dyn OrionCodeActivationHealthCheck>,
    generation: u64,
    idle_task: Option<Task<()>>,
    operation_task: Option<Task<()>>,
    native_fallback_version: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl OrionCodeActivationCoordinator {
    pub fn init_global(cx: &mut App) -> Entity<Self> {
        Self::init_global_with_health_check(Arc::new(DisabledOrionCodeActivationHealthCheck), cx)
    }

    pub fn init_global_with_health_check(
        health_check: Arc<dyn OrionCodeActivationHealthCheck>,
        cx: &mut App,
    ) -> Entity<Self> {
        if let Some(coordinator) = Self::try_global(cx) {
            return coordinator;
        }
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let update_record = update_coordinator.read(cx).record().clone();
        let initial_status = update_record
            .staged
            .as_ref()
            .map(|staged| OrionCodeActivationStatus::Staged {
                version: staged.version.clone(),
            })
            .unwrap_or(OrionCodeActivationStatus::Idle);
        let coordinator = cx.new(|cx| {
            let update_subscription = cx.observe(
                &update_coordinator,
                |coordinator: &mut OrionCodeActivationCoordinator, _update_coordinator, cx| {
                    coordinator.handle_update_coordinator_changed(cx);
                },
            );
            Self {
                status: initial_status,
                health_check,
                generation: 1,
                idle_task: None,
                operation_task: None,
                native_fallback_version: None,
                _subscriptions: vec![update_subscription],
            }
        });
        cx.set_global(GlobalOrionCodeActivationCoordinator(coordinator.clone()));

        coordinator.update(cx, |coordinator, cx| {
            coordinator.handle_update_coordinator_changed(cx)
        });
        coordinator
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalOrionCodeActivationCoordinator>()
            .0
            .clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalOrionCodeActivationCoordinator>()
            .map(|coordinator| coordinator.0.clone())
    }

    pub fn status(&self) -> &OrionCodeActivationStatus {
        &self.status
    }

    pub fn configure_health_check(
        &mut self,
        health_check: Arc<dyn OrionCodeActivationHealthCheck>,
        cx: &mut Context<Self>,
    ) {
        self.health_check = health_check;
        if self.operation_task.is_some() || self.idle_task.is_some() {
            return;
        }
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let has_staged_release = update_coordinator.read(cx).record().staged.is_some();
        if has_staged_release {
            let automatic_install_enabled = update_coordinator.read(cx).automatic_install_enabled();
            if automatic_install_enabled {
                self.activate_when_idle(cx);
            } else {
                let version = update_coordinator
                    .read(cx)
                    .record()
                    .staged
                    .as_ref()
                    .map(|staged| staged.version.clone())
                    .unwrap_or_default();
                self.status = OrionCodeActivationStatus::Staged { version };
                cx.notify();
            }
        } else if matches!(self.status, OrionCodeActivationStatus::Disabled { .. }) {
            self.status = OrionCodeActivationStatus::Idle;
            cx.notify();
        }
    }

    fn handle_update_coordinator_changed(&mut self, cx: &mut Context<Self>) {
        if self.operation_task.is_some() || self.idle_task.is_some() {
            return;
        }
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        if !update_coordinator.read(cx).managed_operations_ready() {
            return;
        }
        let record = update_coordinator.read(cx).record().clone();
        if record.activation.is_some() {
            self.recover_interrupted_activation(record, cx);
            return;
        }
        if !current_orion_code_release_is_revoked(&record) {
            self.native_fallback_version = None;
            if record.staged.is_some() && update_coordinator.read(cx).automatic_install_enabled() {
                self.activate_when_idle(cx);
            }
            return;
        }
        if stage_previous_orion_code_release(&record, Utc::now()).is_ok() {
            self.use_previous_when_idle(cx);
        } else {
            self.fallback_to_native_when_idle(cx);
        }
    }

    fn fallback_to_native_when_idle(&mut self, cx: &mut Context<Self>) {
        if self.operation_task.is_some() || self.idle_task.is_some() {
            return;
        }
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let record = update_coordinator.read(cx).record().clone();
        if !current_orion_code_release_is_revoked(&record) {
            return;
        }
        let version = record
            .current_verified
            .as_ref()
            .map(|receipt| receipt.version.clone())
            .unwrap_or_default();
        if self.native_fallback_version.as_deref() == Some(version.as_str()) {
            return;
        }
        let activity = OrionCodeUpdateActivity::init_global(cx);
        if activity.read(cx).is_idle() {
            self.start_native_fallback(version, cx);
            return;
        }

        let generation = self.next_generation();
        let mut idle_updates = activity.read(cx).subscribe_idle();
        self.status = OrionCodeActivationStatus::WaitingForIdle {
            version: version.clone(),
        };
        cx.notify();
        self.idle_task = Some(cx.spawn(async move |this, cx| {
            while idle_updates.recv().await.is_ok() {
                let idle = activity.read_with(cx, |activity, _cx| activity.is_idle());
                if !idle {
                    continue;
                }
                this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.idle_task = None;
                    let record = OrionCodeUpdateCoordinator::global(cx)
                        .read(cx)
                        .record()
                        .clone();
                    if current_orion_code_release_is_revoked(&record) {
                        this.start_native_fallback(version.clone(), cx);
                    }
                })
                .log_err();
                break;
            }
        }));
    }

    fn start_native_fallback(&mut self, version: String, cx: &mut Context<Self>) {
        let activity = OrionCodeUpdateActivity::global(cx);
        if !activity.read(cx).is_idle() {
            self.fallback_to_native_when_idle(cx);
            return;
        }
        let version_switch = match activity
            .read(cx)
            .acquire(OrionCodeActivityKind::VersionSwitch)
        {
            Ok(lease) => lease,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Rollback,
                    "native_fallback_barrier_failed",
                    cx,
                );
                return;
            }
        };
        let generation = self.next_generation();
        self.status = OrionCodeActivationStatus::Activating { version };
        cx.notify();
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let _version_switch = version_switch;
            let shutdown_result = cx.update(shutdown_all_orion_code_connections).await;
            let fallback_result = if shutdown_result.is_ok() {
                cx.update(|cx| {
                    set_orion_code_managed_archive_runtime(None, cx);
                    if let Some(registry) = AgentRegistryStore::try_global(cx) {
                        registry.update(cx, |registry, cx| {
                            registry.clear_orion_code_version_pin_and_refresh(cx)
                        });
                    }
                    select_native_agent_after_orion_code_revocation(cx)
                })
                .await
                .map_err(|_| ())
            } else {
                Err(())
            };
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.operation_task = None;
                if fallback_result.is_ok() {
                    this.native_fallback_version = OrionCodeUpdateCoordinator::global(cx)
                        .read(cx)
                        .record()
                        .current_verified
                        .as_ref()
                        .map(|receipt| receipt.version.clone());
                    this.status = OrionCodeActivationStatus::Disabled {
                        diagnostic_code: "current_release_revoked_no_safe_previous".to_string(),
                    };
                    cx.notify();
                } else {
                    this.fail(
                        OrionCodeUpdateErrorKind::Rollback,
                        "native_fallback_failed_restart_required",
                        cx,
                    );
                }
            })
            .log_err();
        }));
    }

    pub fn record_staged_identity(
        &mut self,
        staged: OrionCodeStagedIdentity,
        cx: &mut Context<Self>,
    ) {
        self.record_staged_identity_with_activation(staged, false, cx);
    }

    pub fn use_previous_when_idle(&mut self, cx: &mut Context<Self>) {
        let record = OrionCodeUpdateCoordinator::global(cx)
            .read(cx)
            .record()
            .clone();
        let updated = match stage_previous_orion_code_release(&record, Utc::now()) {
            Ok(updated) => updated,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Rollback,
                    "previous_release_unavailable",
                    cx,
                );
                return;
            }
        };
        let Some(staged) = updated.staged else {
            self.fail(
                OrionCodeUpdateErrorKind::Rollback,
                "previous_release_staging_missing",
                cx,
            );
            return;
        };
        let version = match semver::Version::parse(&staged.version) {
            Ok(version) => version,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Rollback,
                    "previous_release_version_invalid",
                    cx,
                );
                return;
            }
        };
        self.record_staged_identity_with_activation(
            OrionCodeStagedIdentity {
                version,
                target: staged.target,
                archive_sha256: staged.archive_sha256,
                command: staged.command,
                index_sequence: staged.index_sequence,
                artifact_root: std::path::PathBuf::new(),
                receipt_path: std::path::PathBuf::new(),
                reused_existing: true,
            },
            true,
            cx,
        );
    }

    fn record_staged_identity_with_activation(
        &mut self,
        staged: OrionCodeStagedIdentity,
        activate_after_stage: bool,
        cx: &mut Context<Self>,
    ) {
        if self.operation_task.is_some() {
            self.fail(
                OrionCodeUpdateErrorKind::Persistence,
                "staged_state_operation_conflict",
                cx,
            );
            return;
        }
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let expected = update_coordinator.read(cx).record().clone();
        if expected.activation.is_some() {
            self.fail(
                OrionCodeUpdateErrorKind::Activation,
                "activation_already_durable",
                cx,
            );
            return;
        }
        let mut updated = expected.clone();
        updated.staged = Some(OrionCodeStagedReceiptV2 {
            version: staged.version.to_string(),
            target: staged.target,
            archive_sha256: staged.archive_sha256,
            command: staged.command,
            staged_at: Utc::now(),
            index_sequence: staged.index_sequence,
        });
        if updated
            .staged
            .as_ref()
            .is_some_and(|staged| updated.known_revoked_versions.contains(&staged.version))
        {
            self.fail(
                OrionCodeUpdateErrorKind::Rollback,
                "revoked_release_cannot_be_staged",
                cx,
            );
            return;
        }
        if updated
            .staged
            .as_ref()
            .is_some_and(|staged| updated.known_paused_versions.contains(&staged.version))
        {
            self.fail(
                OrionCodeUpdateErrorKind::Rollback,
                "paused_release_cannot_be_staged",
                cx,
            );
            return;
        }
        if validate_update_record(&updated).is_err() {
            self.fail(
                OrionCodeUpdateErrorKind::Persistence,
                "staged_state_invalid",
                cx,
            );
            return;
        }
        let operation_generation = match update_coordinator.update(cx, |coordinator, cx| {
            coordinator.begin_managed_operation(&expected, cx)
        }) {
            Ok(generation) => generation,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Activation,
                    "staged_state_coordinator_busy",
                    cx,
                );
                return;
            }
        };
        let generation = self.next_generation();
        let version = updated
            .staged
            .as_ref()
            .map(|staged| staged.version.clone())
            .unwrap_or_default();
        self.status = OrionCodeActivationStatus::StagingState {
            version: version.clone(),
        };
        cx.notify();
        let key_value_store = KeyValueStore::global(cx);
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let write_result = write_orion_code_update_record(&key_value_store, &updated).await;
            let finish_result = if write_result.is_ok() {
                update_coordinator
                    .update(cx, |coordinator, cx| {
                        coordinator.finish_managed_operation_without_runtime_change(
                            operation_generation,
                            updated.clone(),
                            cx,
                        )
                    })
                    .map_err(|_| ())
            } else {
                update_coordinator
                    .update(cx, |coordinator, cx| {
                        coordinator.cancel_managed_operation(operation_generation, cx)
                    })
                    .map_err(|_| ())
            };
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.operation_task = None;
                if write_result.is_err() || finish_result.is_err() {
                    this.fail(
                        OrionCodeUpdateErrorKind::Persistence,
                        "staged_state_write_failed",
                        cx,
                    );
                    return;
                }
                this.status = OrionCodeActivationStatus::Staged {
                    version: version.clone(),
                };
                cx.notify();
                if activate_after_stage
                    || update_coordinator.read_with(cx, |coordinator, _cx| {
                        coordinator.automatic_install_enabled()
                    })
                {
                    this.activate_when_idle(cx);
                }
            })
            .log_err();
        }));
    }

    pub fn activate_when_idle(&mut self, cx: &mut Context<Self>) {
        if self.operation_task.is_some() || self.idle_task.is_some() {
            return;
        }
        if let Some(code) = self.health_check.disabled_diagnostic_code() {
            self.status = OrionCodeActivationStatus::Disabled {
                diagnostic_code: sanitize_diagnostic_code(code.to_string()),
            };
            cx.notify();
            return;
        }
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let record = update_coordinator.read(cx).record().clone();
        let Some(staged) = record.staged.clone() else {
            self.status = OrionCodeActivationStatus::Idle;
            cx.notify();
            return;
        };
        if record.known_revoked_versions.contains(&staged.version) {
            self.fail(
                OrionCodeUpdateErrorKind::Rollback,
                "revoked_release_cannot_be_activated",
                cx,
            );
            return;
        }
        if record.known_paused_versions.contains(&staged.version) {
            self.fail(
                OrionCodeUpdateErrorKind::Activation,
                "paused_release_cannot_be_activated",
                cx,
            );
            return;
        }
        let activity = OrionCodeUpdateActivity::init_global(cx);
        if activity.read(cx).is_idle() {
            self.start_activation(staged, cx);
            return;
        }

        let generation = self.next_generation();
        let version = staged.version.clone();
        let mut idle_updates = activity.read(cx).subscribe_idle();
        self.status = OrionCodeActivationStatus::WaitingForIdle { version };
        cx.notify();
        self.idle_task = Some(cx.spawn(async move |this, cx| {
            while idle_updates.recv().await.is_ok() {
                let idle = activity.read_with(cx, |activity, _cx| activity.is_idle());
                if !idle {
                    continue;
                }
                this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return;
                    }
                    this.idle_task = None;
                    this.start_activation(staged.clone(), cx);
                })
                .log_err();
                break;
            }
        }));
    }

    fn start_activation(&mut self, staged: OrionCodeStagedReceiptV2, cx: &mut Context<Self>) {
        let activity = OrionCodeUpdateActivity::global(cx);
        if !activity.read(cx).is_idle() {
            self.activate_when_idle(cx);
            return;
        }
        let version_switch = match activity
            .read(cx)
            .acquire(OrionCodeActivityKind::VersionSwitch)
        {
            Ok(lease) => lease,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Activation,
                    "version_switch_barrier_failed",
                    cx,
                );
                return;
            }
        };
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let expected = update_coordinator.read(cx).record().clone();
        if expected.staged.as_ref() != Some(&staged) {
            self.fail(
                OrionCodeUpdateErrorKind::Activation,
                "staged_state_changed_before_activation",
                cx,
            );
            return;
        }
        let generation = self.next_generation();
        let activating = match begin_orion_code_activation(&expected, Utc::now(), generation) {
            Ok(record) => record,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Activation,
                    "activation_state_begin_failed",
                    cx,
                );
                return;
            }
        };
        let operation_generation = match update_coordinator.update(cx, |coordinator, cx| {
            coordinator.begin_managed_operation(&expected, cx)
        }) {
            Ok(generation) => generation,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Activation,
                    "activation_coordinator_busy",
                    cx,
                );
                return;
            }
        };
        let health_check = self.health_check.clone();
        let key_value_store = KeyValueStore::global(cx);
        self.status = OrionCodeActivationStatus::Activating {
            version: staged.version.clone(),
        };
        cx.notify();
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let _version_switch = version_switch;
            let mut activation_is_durable = false;
            let outcome = async {
                write_orion_code_update_record(&key_value_store, &activating)
                    .await
                    .map_err(|_| {
                        activation_failure(
                            OrionCodeUpdateErrorKind::Persistence,
                            "activation_begin_write_failed",
                        )
                    })?;
                activation_is_durable = true;
                update_coordinator
                    .update(cx, |coordinator, cx| {
                        coordinator.checkpoint_managed_operation(
                            operation_generation,
                            activating.clone(),
                            cx,
                        )
                    })
                    .map_err(|_| {
                        activation_failure(
                            OrionCodeUpdateErrorKind::Persistence,
                            "activation_checkpoint_stale",
                        )
                    })?;

                cx.update(shutdown_all_orion_code_connections)
                    .await
                    .map_err(|_| {
                        activation_failure(
                            OrionCodeUpdateErrorKind::Activation,
                            "previous_runtime_shutdown_failed",
                        )
                    })?;
                cx.update(|cx| bind_staged_runtime(&staged, cx))
                    .map_err(|_| {
                        activation_failure(
                            OrionCodeUpdateErrorKind::Activation,
                            "candidate_runtime_binding_failed",
                        )
                    })?;
                health_check
                    .verify(staged.clone())
                    .await
                    .map_err(|error| activation_failure(error.kind, &error.diagnostic_code))?;

                let committed =
                    commit_orion_code_activation(&activating, Utc::now()).map_err(|_| {
                        activation_failure(
                            OrionCodeUpdateErrorKind::Activation,
                            "activation_commit_state_invalid",
                        )
                    })?;
                write_orion_code_update_record(&key_value_store, &committed)
                    .await
                    .map_err(|_| {
                        activation_failure(
                            OrionCodeUpdateErrorKind::Persistence,
                            "activation_commit_write_failed",
                        )
                    })?;
                update_coordinator
                    .update(cx, |coordinator, cx| {
                        coordinator.finish_managed_operation(operation_generation, committed, cx)
                    })
                    .map_err(|_| {
                        activation_failure(
                            OrionCodeUpdateErrorKind::Persistence,
                            "activation_finish_failed",
                        )
                    })?;
                Ok::<(), ActivationFailure>(())
            }
            .await;

            let outcome = match outcome {
                Ok(()) => Ok(()),
                Err(failure) => {
                    let recovery = if activation_is_durable {
                        recover_failed_activation(
                            &key_value_store,
                            &update_coordinator,
                            operation_generation,
                            &activating,
                            failure.kind,
                            cx,
                        )
                        .await
                    } else {
                        update_coordinator
                            .update(cx, |coordinator, cx| {
                                coordinator.cancel_managed_operation(operation_generation, cx)
                            })
                            .map_err(|_| ())
                    };
                    if recovery.is_err() {
                        Err(activation_failure(
                            OrionCodeUpdateErrorKind::Persistence,
                            "activation_recovery_failed_restart_required",
                        ))
                    } else {
                        Err(failure)
                    }
                }
            };

            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.operation_task = None;
                match outcome {
                    Ok(()) => {
                        this.status = OrionCodeActivationStatus::Activated {
                            version: staged.version.clone(),
                        };
                        cx.notify();
                    }
                    Err(failure) => this.fail(failure.kind, failure.diagnostic_code, cx),
                }
            })
            .log_err();
        }));
    }

    fn recover_interrupted_activation(
        &mut self,
        record: OrionCodeUpdateRecordV2,
        cx: &mut Context<Self>,
    ) {
        let update_coordinator = OrionCodeUpdateCoordinator::global(cx);
        let operation_generation = match update_coordinator.update(cx, |coordinator, cx| {
            coordinator.begin_managed_operation(&record, cx)
        }) {
            Ok(generation) => generation,
            Err(_) => {
                self.fail(
                    OrionCodeUpdateErrorKind::Persistence,
                    "activation_recovery_coordinator_busy",
                    cx,
                );
                return;
            }
        };
        let recovered =
            match abort_orion_code_activation(&record, OrionCodeUpdateErrorKind::Rollback) {
                Ok(record) => record,
                Err(_) => {
                    update_coordinator.update(cx, |coordinator, cx| {
                        coordinator
                            .cancel_managed_operation(operation_generation, cx)
                            .log_err();
                    });
                    self.fail(
                        OrionCodeUpdateErrorKind::Rollback,
                        "activation_recovery_state_invalid",
                        cx,
                    );
                    return;
                }
            };
        let generation = self.next_generation();
        let key_value_store = KeyValueStore::global(cx);
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let write_result = write_orion_code_update_record(&key_value_store, &recovered).await;
            let finish_result = if write_result.is_ok() {
                update_coordinator
                    .update(cx, |coordinator, cx| {
                        coordinator.finish_managed_operation(operation_generation, recovered, cx)
                    })
                    .map_err(|_| ())
            } else {
                Err(())
            };
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.operation_task = None;
                if write_result.is_ok() && finish_result.is_ok() {
                    this.fail(
                        OrionCodeUpdateErrorKind::Rollback,
                        "interrupted_activation_rolled_back",
                        cx,
                    );
                } else {
                    this.fail(
                        OrionCodeUpdateErrorKind::Persistence,
                        "activation_recovery_failed_restart_required",
                        cx,
                    );
                }
            })
            .log_err();
        }));
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.generation
    }

    fn fail(
        &mut self,
        kind: OrionCodeUpdateErrorKind,
        diagnostic_code: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.status = OrionCodeActivationStatus::Failed {
            kind,
            diagnostic_code: sanitize_diagnostic_code(diagnostic_code.into()),
        };
        cx.notify();
    }
}

#[derive(Clone, Debug)]
struct ActivationFailure {
    kind: OrionCodeUpdateErrorKind,
    diagnostic_code: String,
}

fn activation_failure(
    kind: OrionCodeUpdateErrorKind,
    diagnostic_code: impl Into<String>,
) -> ActivationFailure {
    ActivationFailure {
        kind,
        diagnostic_code: sanitize_diagnostic_code(diagnostic_code.into()),
    }
}

async fn recover_failed_activation(
    key_value_store: &KeyValueStore,
    update_coordinator: &Entity<OrionCodeUpdateCoordinator>,
    operation_generation: u64,
    activating: &OrionCodeUpdateRecordV2,
    kind: OrionCodeUpdateErrorKind,
    cx: &mut gpui::AsyncApp,
) -> Result<(), ()> {
    let aborted = abort_orion_code_activation(activating, kind).map_err(|_| ())?;
    write_orion_code_update_record(key_value_store, &aborted)
        .await
        .map_err(|_| ())?;
    update_coordinator
        .update(cx, |coordinator, cx| {
            coordinator.finish_managed_operation(operation_generation, aborted, cx)
        })
        .map_err(|_| ())
}

fn bind_staged_runtime(staged: &OrionCodeStagedReceiptV2, cx: &mut App) -> Result<(), String> {
    let runtime =
        OrionCodeManagedArchiveRuntime::new(&staged.version, &staged.target, &staged.command)
            .map_err(|_| "candidate runtime receipt is invalid".to_string())?;
    let previous_runtime = orion_code_managed_archive_runtime(cx);
    set_orion_code_managed_archive_runtime(Some(runtime), cx);
    if let Some(registry) = AgentRegistryStore::try_global(cx)
        && registry
            .update(cx, |registry, cx| {
                registry.pin_orion_code_to_verified_version(&staged.version, cx)
            })
            .is_err()
    {
        set_orion_code_managed_archive_runtime(previous_runtime, cx);
        return Err("candidate registry binding failed".to_string());
    }
    Ok(())
}

fn sanitize_diagnostic_code(code: String) -> String {
    if code.is_empty()
        || code.len() > 96
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        "invalid_diagnostic_code".to_string()
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_codes_cannot_expose_paths_or_control_characters() {
        assert_eq!(
            sanitize_diagnostic_code("/Users/example/.env\nsecret".to_string()),
            "invalid_diagnostic_code"
        );
        assert_eq!(
            OrionCodeActivationHealthError::new(
                OrionCodeUpdateErrorKind::Preflight,
                "candidate_identity_mismatch",
            )
            .diagnostic_code,
            "candidate_identity_mismatch"
        );
    }
}
