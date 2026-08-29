use std::sync::Arc;

use agent_client_protocol::schema::ProtocolVersion;
use agent_servers::AcpInitializeSnapshot;
use anyhow::{Context as _, Result, anyhow, bail};
use collections::HashMap;
use db::kvp::KeyValueStore;
use fs::Fs;
use gpui::{App, AppContext as _, Context, Entity, Global, SharedString, Task};
use parking_lot::Mutex;
use project::agent_registry_store::{AgentRegistryStore, ORION_CODE_AGENT_ID};
use project::agent_server_store::{
    AllAgentServersSettings, CustomAgentServerSettings, remove_orion_code_managed_install,
};
use serde::{Deserialize, Serialize};
use settings::{Settings as _, SettingsContent, update_settings_file_with_completion};

use crate::agent_connection_store::shutdown_all_orion_code_connections;

pub const ORION_CODE_BOOTSTRAP_NAMESPACE: &str = "orion-code-bootstrap";
pub const ORION_CODE_BOOTSTRAP_STATE_KEY: &str = "state-v1";
const ORION_CODE_BOOTSTRAP_SCHEMA_VERSION: u32 = 1;

pub fn validate_orion_code_initialize(
    snapshot: &AcpInitializeSnapshot,
    expected_version: &str,
) -> Result<String> {
    if snapshot.protocol_version != ProtocolVersion::V1 {
        bail!(
            "Orion Code reported ACP protocol {:?}; protocol v1 is required",
            snapshot.protocol_version
        );
    }

    let agent_info = snapshot
        .agent_info
        .as_ref()
        .context("Orion Code initialize response did not include agentInfo")?;
    if agent_info.name != ORION_CODE_AGENT_ID {
        bail!(
            "Orion Code initialize identity mismatch: expected {}, received {}",
            ORION_CODE_AGENT_ID,
            agent_info.name
        );
    }

    let expected = semver::Version::parse(expected_version)
        .with_context(|| format!("invalid expected Orion Code version {expected_version}"))?;
    let reported = semver::Version::parse(&agent_info.version)
        .with_context(|| format!("invalid reported Orion Code version {}", agent_info.version))?;
    if reported != expected || agent_info.version != expected_version {
        bail!(
            "Orion Code version mismatch: expected {}, received {}",
            expected_version,
            agent_info.version
        );
    }
    if !snapshot.agent_capabilities.load_session {
        bail!("Orion Code must support session/load");
    }
    if snapshot
        .agent_capabilities
        .session_capabilities
        .close
        .is_none()
    {
        bail!("Orion Code must support session/close");
    }

    Ok(agent_info.version.clone())
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeBootstrapChoice {
    #[default]
    NotAsked,
    Accepted,
    Declined,
    Removed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeInstallSource {
    Npx,
    Archive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrionCodeBootstrapErrorKind {
    RegistryUnreachable,
    UnsupportedPlatform,
    NpmInstall,
    Download,
    Checksum,
    Extract,
    MissingCommand,
    Spawn,
    Initialize,
    Configuration,
    RuntimeExit,
    Persistence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrionCodeBootstrapRecord {
    pub schema_version: u32,
    pub choice: OrionCodeBootstrapChoice,
    pub source: Option<OrionCodeInstallSource>,
    pub last_attempted_version: Option<String>,
    pub last_verified_version: Option<String>,
    pub last_error_kind: Option<OrionCodeBootstrapErrorKind>,
}

impl Default for OrionCodeBootstrapRecord {
    fn default() -> Self {
        Self {
            schema_version: ORION_CODE_BOOTSTRAP_SCHEMA_VERSION,
            choice: OrionCodeBootstrapChoice::NotAsked,
            source: None,
            last_attempted_version: None,
            last_verified_version: None,
            last_error_kind: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OrionCodeBootstrapPhase {
    #[default]
    Pending,
    Configuring,
    Configured,
    Verifying,
    Ready,
    Failed,
    DisabledByUser,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnsureOrionCodeRegistrySettings {
    Inserted,
    AlreadyConfigured,
    Superseded,
}

struct GlobalOrionCodeBootstrap(Entity<OrionCodeBootstrap>);

impl Global for GlobalOrionCodeBootstrap {}

pub struct OrionCodeBootstrap {
    is_new_install: bool,
    record: OrionCodeBootstrapRecord,
    phase: OrionCodeBootstrapPhase,
    durable_ready_version: Option<String>,
    last_error: Option<SharedString>,
    next_verification_attempt: u64,
    active_verification: Option<OrionCodeVerification>,
    next_lifecycle_operation: u64,
    active_lifecycle_operation: u64,
    lifecycle_operation_lock: Arc<futures::lock::Mutex<()>>,
    agent_selection_generation: u64,
    agent_preference_write_lock: Arc<futures::lock::Mutex<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OrionCodeVerification {
    attempt_id: u64,
    owner_id: u64,
    expected_version: String,
}

impl OrionCodeBootstrap {
    pub fn init_global(is_new_install: bool, cx: &mut App) -> Entity<Self> {
        if let Some(bootstrap) = Self::try_global(cx) {
            return bootstrap;
        }

        let (record, read_error) =
            match read_orion_code_bootstrap_record(&KeyValueStore::global(cx)) {
                Ok(record) => (record, None),
                Err(error) => (
                    OrionCodeBootstrapRecord::default(),
                    Some(SharedString::from(format!(
                        "Failed to read Orion Code bootstrap state: {error:#}"
                    ))),
                ),
            };
        let phase = if read_error.is_some() {
            OrionCodeBootstrapPhase::Failed
        } else {
            if let Some(version) = record.last_verified_version.as_deref()
                && let Some(registry) = AgentRegistryStore::try_global(cx)
                && let Err(error) = registry.update(cx, |registry, cx| -> Result<()> {
                    registry.set_orion_code_version_floor(version, cx)?;
                    if record.last_error_kind.is_some() {
                        registry.pin_orion_code_to_verified_version(version, cx)?;
                    }
                    Ok(())
                })
            {
                log::error!("Failed to apply Orion Code version floor: {error:#}");
            }
            initial_phase(&record, cx)
        };
        let durable_ready_version = (phase == OrionCodeBootstrapPhase::Ready)
            .then(|| record.last_verified_version.clone())
            .flatten();
        let bootstrap = cx.new(|_| Self {
            is_new_install,
            record,
            phase,
            durable_ready_version,
            last_error: read_error,
            next_verification_attempt: 0,
            active_verification: None,
            next_lifecycle_operation: 0,
            active_lifecycle_operation: 0,
            lifecycle_operation_lock: Arc::new(futures::lock::Mutex::new(())),
            agent_selection_generation: 0,
            agent_preference_write_lock: Arc::new(futures::lock::Mutex::new(())),
        });
        cx.set_global(GlobalOrionCodeBootstrap(bootstrap.clone()));
        bootstrap
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalOrionCodeBootstrap>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalOrionCodeBootstrap>()
            .map(|bootstrap| bootstrap.0.clone())
    }

    pub fn is_new_install(&self) -> bool {
        self.is_new_install
    }

    pub fn record(&self) -> &OrionCodeBootstrapRecord {
        &self.record
    }

    pub fn phase(&self) -> OrionCodeBootstrapPhase {
        self.phase
    }

    pub fn last_error(&self) -> Option<&SharedString> {
        self.last_error.as_ref()
    }

    pub fn is_ready_for_product_default(&self) -> bool {
        self.is_new_install
            && self.record.choice == OrionCodeBootstrapChoice::Accepted
            && self.durable_ready_version.is_some()
            && self.phase == OrionCodeBootstrapPhase::Ready
    }

    pub fn agent_selection_generation(&self) -> u64 {
        self.agent_selection_generation
    }

    pub fn note_agent_selection(&mut self) -> u64 {
        self.agent_selection_generation = self.agent_selection_generation.wrapping_add(1).max(1);
        self.agent_selection_generation
    }

    pub fn agent_preference_write_lock(&self) -> Arc<futures::lock::Mutex<()>> {
        self.agent_preference_write_lock.clone()
    }

    pub fn can_start_verification(&self, expected_version: &str) -> bool {
        self.record.choice == OrionCodeBootstrapChoice::Accepted
            && (self.phase == OrionCodeBootstrapPhase::Configured
                || (self.phase == OrionCodeBootstrapPhase::Ready
                    && self.record.last_verified_version.as_deref() != Some(expected_version)))
    }

    pub fn begin_verification(
        &mut self,
        owner_id: u64,
        attempted_version: String,
        cx: &mut Context<Self>,
    ) -> (u64, Task<Result<()>>) {
        self.next_verification_attempt = self.next_verification_attempt.wrapping_add(1).max(1);
        let attempt_id = self.next_verification_attempt;
        self.active_verification = Some(OrionCodeVerification {
            attempt_id,
            owner_id,
            expected_version: attempted_version.clone(),
        });
        let mut record = self.record.clone();
        record.last_attempted_version = Some(attempted_version);
        record.last_error_kind = None;
        let persistence =
            self.persist_transition(record, OrionCodeBootstrapPhase::Verifying, None, cx);
        (attempt_id, persistence)
    }

    pub fn verification_is_current(&self, attempt_id: u64, expected_version: &str) -> bool {
        self.record.choice == OrionCodeBootstrapChoice::Accepted
            && self.phase == OrionCodeBootstrapPhase::Verifying
            && self.verification_attempt_matches(attempt_id, expected_version)
    }

    fn verification_attempt_matches(&self, attempt_id: u64, expected_version: &str) -> bool {
        self.active_verification.as_ref().is_some_and(|attempt| {
            attempt.attempt_id == attempt_id && attempt.expected_version == expected_version
        })
    }

    pub fn is_ready_at_version(&self, expected_version: &str) -> bool {
        self.record.choice == OrionCodeBootstrapChoice::Accepted
            && self.phase == OrionCodeBootstrapPhase::Ready
            && self.durable_ready_version.as_deref() == Some(expected_version)
    }

    pub fn mark_ready_if_current(
        &mut self,
        attempt_id: u64,
        verified_version: String,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        if !self.verification_is_current(attempt_id, &verified_version) {
            return None;
        }
        let mut record = self.record.clone();
        record.last_attempted_version = Some(verified_version.clone());
        record.last_verified_version = Some(verified_version);
        record.last_error_kind = None;
        if let Some(registry) = AgentRegistryStore::try_global(cx)
            && let Some(version) = record.last_verified_version.as_deref()
            && let Err(error) = registry.update(cx, |registry, cx| -> Result<()> {
                registry.clear_orion_code_version_pin_and_refresh(cx);
                registry.set_orion_code_version_floor(version, cx)
            })
        {
            return Some(Task::ready(Err(error)));
        }
        Some(self.persist_transition(record, OrionCodeBootstrapPhase::Ready, None, cx))
    }

    pub fn mark_failed(
        &mut self,
        kind: OrionCodeBootstrapErrorKind,
        message: SharedString,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.active_verification = None;
        let mut record = self.record.clone();
        record.last_error_kind = Some(kind);
        self.persist_transition(record, OrionCodeBootstrapPhase::Failed, Some(message), cx)
    }

    pub fn mark_verification_failed_if_current(
        &mut self,
        attempt_id: u64,
        expected_version: &str,
        kind: OrionCodeBootstrapErrorKind,
        message: SharedString,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let attempt_is_current = self.verification_attempt_matches(attempt_id, expected_version)
            && matches!(
                self.phase,
                OrionCodeBootstrapPhase::Verifying | OrionCodeBootstrapPhase::Ready
            );
        if self.record.choice != OrionCodeBootstrapChoice::Accepted || !attempt_is_current {
            return None;
        }
        if let Some(last_verified_version) = self.record.last_verified_version.clone()
            && last_verified_version != expected_version
        {
            if let Some(registry) = AgentRegistryStore::try_global(cx)
                && let Err(error) = registry.update(cx, |registry, cx| {
                    registry.pin_orion_code_to_verified_version(&last_verified_version, cx)
                })
            {
                return Some(self.mark_failed(
                    OrionCodeBootstrapErrorKind::Persistence,
                    SharedString::from(format!(
                        "Failed to restore verified Orion Code {last_verified_version}: {error:#}"
                    )),
                    cx,
                ));
            }
            self.active_verification = None;
            let mut record = self.record.clone();
            record.last_error_kind = Some(kind);
            return Some(self.persist_transition(
                record,
                OrionCodeBootstrapPhase::Ready,
                Some(message),
                cx,
            ));
        }
        Some(self.mark_failed(kind, message, cx))
    }

    pub fn cancel_verification_for_owner(&mut self, owner_id: u64, cx: &mut Context<Self>) {
        let is_owner = self
            .active_verification
            .as_ref()
            .is_some_and(|attempt| attempt.owner_id == owner_id);
        if !is_owner || self.phase != OrionCodeBootstrapPhase::Verifying {
            return;
        }
        self.begin_lifecycle_operation();
        self.active_verification = None;
        self.phase = OrionCodeBootstrapPhase::Configured;
        self.last_error = None;
        cx.notify();
    }

    fn begin_lifecycle_operation(&mut self) -> u64 {
        self.next_lifecycle_operation = self.next_lifecycle_operation.wrapping_add(1).max(1);
        self.active_lifecycle_operation = self.next_lifecycle_operation;
        self.active_lifecycle_operation
    }

    fn lifecycle_operation_is_current(&self, operation_id: u64) -> bool {
        self.active_lifecycle_operation == operation_id
    }

    fn persist_transition(
        &mut self,
        record: OrionCodeBootstrapRecord,
        phase: OrionCodeBootstrapPhase,
        last_error: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let operation_id = self.begin_lifecycle_operation();
        let lifecycle_operation_lock = self.lifecycle_operation_lock.clone();
        self.record = record.clone();
        self.phase = phase;
        self.durable_ready_version = None;
        self.last_error = last_error;
        cx.notify();

        let key_value_store = KeyValueStore::global(cx);
        cx.spawn(async move |this, cx| {
            let _operation_guard = lifecycle_operation_lock.lock().await;
            if !this.read_with(cx, |this, _cx| {
                this.lifecycle_operation_is_current(operation_id)
            })? {
                return Ok(());
            }
            if let Err(error) = write_orion_code_bootstrap_record(&key_value_store, &record).await {
                let message = SharedString::from(format!(
                    "Failed to persist Orion Code bootstrap state: {error:#}"
                ));
                this.update(cx, |this, cx| {
                    if !this.lifecycle_operation_is_current(operation_id) {
                        return;
                    }
                    this.active_verification = None;
                    this.record.last_error_kind = Some(OrionCodeBootstrapErrorKind::Persistence);
                    this.phase = OrionCodeBootstrapPhase::Failed;
                    this.last_error = Some(message);
                    cx.notify();
                })?;
                return Err(error);
            }
            this.update(cx, |this, cx| {
                if !this.lifecycle_operation_is_current(operation_id) {
                    return;
                }
                this.durable_ready_version = (phase == OrionCodeBootstrapPhase::Ready)
                    .then(|| record.last_verified_version.clone())
                    .flatten();
                cx.notify();
            })?;
            Ok(())
        })
    }

    pub fn accept_and_configure(
        &mut self,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) -> Task<Result<EnsureOrionCodeRegistrySettings>> {
        let mut accepted_record = self.record.clone();
        let preserve_verified_version = self.record.choice == OrionCodeBootstrapChoice::Accepted
            && self.record.last_verified_version.is_some();
        accepted_record.choice = OrionCodeBootstrapChoice::Accepted;
        accepted_record
            .source
            .get_or_insert(OrionCodeInstallSource::Npx);
        if preserve_verified_version {
            accepted_record.last_attempted_version = None;
        } else {
            clear_orion_code_install_receipt(&mut accepted_record);
        }
        accepted_record.last_error_kind = None;

        if let Some(registry) = AgentRegistryStore::try_global(cx) {
            registry.update(cx, |registry, cx| {
                registry.clear_orion_code_version_pin_and_refresh(cx);
            });
        }

        let operation_id = self.begin_lifecycle_operation();
        let lifecycle_operation_lock = self.lifecycle_operation_lock.clone();
        self.active_verification = None;
        self.phase = OrionCodeBootstrapPhase::Configuring;
        self.durable_ready_version = None;
        self.last_error = None;
        cx.notify();

        let key_value_store = KeyValueStore::global(cx);
        cx.spawn(async move |this, cx| {
            let _operation_guard = lifecycle_operation_lock.lock().await;
            if !this.read_with(cx, |this, _cx| {
                this.lifecycle_operation_is_current(operation_id)
            })? {
                return Ok(EnsureOrionCodeRegistrySettings::Superseded);
            }

            let result = async {
                write_orion_code_bootstrap_record(&key_value_store, &accepted_record).await?;
                let settings_task = cx.update(|cx| ensure_orion_code_registry_settings(fs, cx));
                let outcome = settings_task.await?;
                anyhow::Ok(outcome)
            }
            .await;

            match result {
                Ok(outcome) => {
                    let committed = this.update(cx, |this, cx| {
                        if !this.lifecycle_operation_is_current(operation_id) {
                            return false;
                        }
                        this.record = accepted_record;
                        this.phase = OrionCodeBootstrapPhase::Configured;
                        this.durable_ready_version = None;
                        this.last_error = None;
                        cx.notify();
                        true
                    })?;
                    Ok(if committed {
                        outcome
                    } else {
                        EnsureOrionCodeRegistrySettings::Superseded
                    })
                }
                Err(error) => {
                    if !this.read_with(cx, |this, _cx| {
                        this.lifecycle_operation_is_current(operation_id)
                    })? {
                        log::warn!(
                            "Ignoring superseded Orion Code configuration failure: {error:#}"
                        );
                        return Ok(EnsureOrionCodeRegistrySettings::Superseded);
                    }
                    let mut failed_record = accepted_record;
                    failed_record.last_error_kind =
                        Some(OrionCodeBootstrapErrorKind::Configuration);
                    let persistence_error =
                        write_orion_code_bootstrap_record(&key_value_store, &failed_record)
                            .await
                            .err();
                    let message = persistence_error.as_ref().map_or_else(
                        || SharedString::from(format!("Failed to configure Orion Code: {error:#}")),
                        |persistence_error| {
                            SharedString::from(format!(
                                "Failed to configure Orion Code: {error:#}. The failure state could not be persisted: {persistence_error:#}"
                            ))
                        },
                    );
                    this.update(cx, |this, cx| {
                        if !this.lifecycle_operation_is_current(operation_id) {
                            return;
                        }
                        this.active_verification = None;
                        this.record = failed_record;
                        this.durable_ready_version = None;
                        if persistence_error.is_some() {
                            this.record.last_error_kind =
                                Some(OrionCodeBootstrapErrorKind::Persistence);
                        }
                        this.phase = OrionCodeBootstrapPhase::Failed;
                        this.last_error = Some(message);
                        cx.notify();
                    })?;
                    Err(error)
                }
            }
        })
    }

    pub fn remove_and_disable(
        &mut self,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let mut removed_record = self.record.clone();
        removed_record.choice = OrionCodeBootstrapChoice::Removed;
        clear_orion_code_install_receipt(&mut removed_record);
        removed_record.last_error_kind = None;

        if let Some(registry) = AgentRegistryStore::try_global(cx) {
            registry.update(cx, |registry, cx| {
                registry.clear_orion_code_version_pin_and_refresh(cx);
            });
        }

        let operation_id = self.begin_lifecycle_operation();
        let lifecycle_operation_lock = self.lifecycle_operation_lock.clone();
        self.active_verification = None;
        self.phase = OrionCodeBootstrapPhase::Configuring;
        self.durable_ready_version = None;
        self.last_error = None;
        cx.notify();

        let key_value_store = KeyValueStore::global(cx);
        cx.spawn(async move |this, cx| {
            let _operation_guard = lifecycle_operation_lock.lock().await;
            if !this.read_with(cx, |this, _cx| {
                this.lifecycle_operation_is_current(operation_id)
            })? {
                return Ok(());
            }

            let result = async {
                write_orion_code_bootstrap_record(&key_value_store, &removed_record).await?;
                let shutdown_task = cx.update(shutdown_all_orion_code_connections);
                shutdown_task
                    .await
                    .context("stopping Orion Code connections before removal")?;
                let settings_task =
                    cx.update(|cx| remove_orion_code_registry_settings(fs.clone(), cx));
                settings_task.await?;
                remove_orion_code_managed_install(fs).await?;
                anyhow::Ok(())
            }
            .await;

            match result {
                Ok(()) => {
                    this.update(cx, |this, cx| {
                        if !this.lifecycle_operation_is_current(operation_id) {
                            return;
                        }
                        this.active_verification = None;
                        this.record = removed_record;
                        this.phase = OrionCodeBootstrapPhase::DisabledByUser;
                        this.durable_ready_version = None;
                        this.last_error = None;
                        cx.notify();
                    })?;
                    Ok(())
                }
                Err(error) => {
                    if !this.read_with(cx, |this, _cx| {
                        this.lifecycle_operation_is_current(operation_id)
                    })? {
                        log::warn!("Ignoring superseded Orion Code removal failure: {error:#}");
                        return Ok(());
                    }
                    let mut failed_record = removed_record;
                    failed_record.last_error_kind =
                        Some(OrionCodeBootstrapErrorKind::Configuration);
                    let persistence_error =
                        write_orion_code_bootstrap_record(&key_value_store, &failed_record)
                            .await
                            .err();
                    let message = persistence_error.as_ref().map_or_else(
                        || SharedString::from(format!("Failed to remove Orion Code: {error:#}")),
                        |persistence_error| {
                            SharedString::from(format!(
                                "Failed to remove Orion Code: {error:#}. The failure state could not be persisted: {persistence_error:#}"
                            ))
                        },
                    );
                    this.update(cx, |this, cx| {
                        if !this.lifecycle_operation_is_current(operation_id) {
                            return;
                        }
                        this.active_verification = None;
                        this.record = failed_record;
                        this.durable_ready_version = None;
                        if persistence_error.is_some() {
                            this.record.last_error_kind =
                                Some(OrionCodeBootstrapErrorKind::Persistence);
                        }
                        this.phase = OrionCodeBootstrapPhase::Failed;
                        this.last_error = Some(message);
                        cx.notify();
                    })?;
                    Err(error)
                }
            }
        })
    }
}

fn clear_orion_code_install_receipt(record: &mut OrionCodeBootstrapRecord) {
    record.last_attempted_version = None;
    record.last_verified_version = None;
}

fn initial_phase(record: &OrionCodeBootstrapRecord, cx: &App) -> OrionCodeBootstrapPhase {
    initial_phase_for_configuration(record, orion_code_registry_configuration(cx))
}

fn initial_phase_for_configuration(
    record: &OrionCodeBootstrapRecord,
    configuration: OrionCodeRegistryConfiguration,
) -> OrionCodeBootstrapPhase {
    match record.choice {
        OrionCodeBootstrapChoice::Declined => OrionCodeBootstrapPhase::DisabledByUser,
        OrionCodeBootstrapChoice::Removed if record.last_error_kind.is_some() => {
            OrionCodeBootstrapPhase::Failed
        }
        OrionCodeBootstrapChoice::Removed => OrionCodeBootstrapPhase::DisabledByUser,
        OrionCodeBootstrapChoice::Accepted => match configuration {
            OrionCodeRegistryConfiguration::Registry if record.last_verified_version.is_some() => {
                OrionCodeBootstrapPhase::Ready
            }
            OrionCodeRegistryConfiguration::Registry if record.last_error_kind.is_some() => {
                OrionCodeBootstrapPhase::Failed
            }
            OrionCodeRegistryConfiguration::Registry => OrionCodeBootstrapPhase::Configured,
            OrionCodeRegistryConfiguration::Missing | OrionCodeRegistryConfiguration::Conflict => {
                OrionCodeBootstrapPhase::Failed
            }
        },
        OrionCodeBootstrapChoice::NotAsked => OrionCodeBootstrapPhase::Pending,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrionCodeRegistryConfiguration {
    Missing,
    Registry,
    Conflict,
}

fn orion_code_registry_configuration(cx: &App) -> OrionCodeRegistryConfiguration {
    match AllAgentServersSettings::get_global(cx).get(ORION_CODE_AGENT_ID) {
        None => OrionCodeRegistryConfiguration::Missing,
        Some(CustomAgentServerSettings::Registry { .. }) => {
            OrionCodeRegistryConfiguration::Registry
        }
        Some(CustomAgentServerSettings::Custom { .. }) => OrionCodeRegistryConfiguration::Conflict,
    }
}

pub fn read_orion_code_bootstrap_record(
    key_value_store: &KeyValueStore,
) -> Result<OrionCodeBootstrapRecord> {
    let Some(json) = key_value_store
        .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
        .read(ORION_CODE_BOOTSTRAP_STATE_KEY)
        .context("reading scoped Orion Code bootstrap state")?
    else {
        return Ok(OrionCodeBootstrapRecord::default());
    };

    let record: OrionCodeBootstrapRecord =
        serde_json::from_str(&json).context("parsing Orion Code bootstrap state")?;
    if record.schema_version != ORION_CODE_BOOTSTRAP_SCHEMA_VERSION {
        bail!(
            "unsupported Orion Code bootstrap schema version {}",
            record.schema_version
        );
    }
    for (field, version) in [
        (
            "lastAttemptedVersion",
            record.last_attempted_version.as_deref(),
        ),
        (
            "lastVerifiedVersion",
            record.last_verified_version.as_deref(),
        ),
    ] {
        if let Some(version) = version {
            semver::Version::parse(version)
                .with_context(|| format!("invalid {field} in Orion Code bootstrap state"))?;
        }
    }
    Ok(record)
}

pub async fn write_orion_code_bootstrap_record(
    key_value_store: &KeyValueStore,
    record: &OrionCodeBootstrapRecord,
) -> Result<()> {
    if record.schema_version != ORION_CODE_BOOTSTRAP_SCHEMA_VERSION {
        bail!(
            "refusing to write Orion Code bootstrap schema version {}",
            record.schema_version
        );
    }
    let json = serde_json::to_string(record).context("serializing Orion Code bootstrap state")?;
    key_value_store
        .scoped(ORION_CODE_BOOTSTRAP_NAMESPACE)
        .write(ORION_CODE_BOOTSTRAP_STATE_KEY.to_string(), json)
        .await
        .context("writing scoped Orion Code bootstrap state")
}

pub fn ensure_orion_code_registry_settings_content(
    settings: &mut SettingsContent,
) -> Result<EnsureOrionCodeRegistrySettings> {
    let agent_servers = settings.agent_servers.get_or_insert_default();
    match agent_servers.get(ORION_CODE_AGENT_ID) {
        Some(settings::CustomAgentServerSettings::Registry { .. }) => {
            Ok(EnsureOrionCodeRegistrySettings::AlreadyConfigured)
        }
        Some(settings::CustomAgentServerSettings::Custom { .. }) => bail!(
            "agent_servers.{ORION_CODE_AGENT_ID} is a custom configuration; refusing to overwrite it"
        ),
        None => {
            agent_servers.insert(
                ORION_CODE_AGENT_ID.to_string(),
                settings::CustomAgentServerSettings::Registry {
                    env: HashMap::default(),
                    default_mode: None,
                    default_config_options: HashMap::default(),
                    favorite_config_option_values: HashMap::default(),
                },
            );
            Ok(EnsureOrionCodeRegistrySettings::Inserted)
        }
    }
}

pub fn ensure_orion_code_registry_settings(
    fs: Arc<dyn Fs>,
    cx: &mut App,
) -> Task<Result<EnsureOrionCodeRegistrySettings>> {
    let outcome = Arc::new(Mutex::new(None));
    let outcome_for_update = outcome.clone();
    let completion = update_settings_file_with_completion(fs, cx, move |settings, _cx| {
        *outcome_for_update.lock() = Some(ensure_orion_code_registry_settings_content(settings));
    });

    cx.background_spawn(async move {
        completion
            .await
            .context("waiting for Orion Code registry settings update")??;
        outcome
            .lock()
            .take()
            .ok_or_else(|| anyhow!("Orion Code registry settings update did not run"))?
    })
}

pub fn remove_orion_code_registry_settings(fs: Arc<dyn Fs>, cx: &mut App) -> Task<Result<()>> {
    let outcome = Arc::new(Mutex::new(None));
    let outcome_for_update = outcome.clone();
    let completion = update_settings_file_with_completion(fs, cx, move |settings, _cx| {
        *outcome_for_update.lock() = Some(remove_orion_code_registry_settings_content(settings));
    });

    cx.background_spawn(async move {
        completion
            .await
            .context("waiting for Orion Code registry settings removal")??;
        outcome
            .lock()
            .take()
            .ok_or_else(|| anyhow!("Orion Code registry settings removal did not run"))??;
        Ok(())
    })
}

pub fn remove_orion_code_registry_settings_content(settings: &mut SettingsContent) -> Result<bool> {
    let Some(agent_servers) = settings.agent_servers.as_mut() else {
        return Ok(false);
    };
    match agent_servers.get(ORION_CODE_AGENT_ID) {
        Some(settings::CustomAgentServerSettings::Registry { .. }) => {}
        Some(settings::CustomAgentServerSettings::Custom { .. }) => {
            bail!(
                "agent_servers.{ORION_CODE_AGENT_ID} changed to a custom configuration; refusing to remove it"
            )
        }
        None => return Ok(false),
    }
    agent_servers.remove(ORION_CODE_AGENT_ID);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1 as acp;
    use std::path::PathBuf;

    fn healthy_initialize_snapshot() -> AcpInitializeSnapshot {
        AcpInitializeSnapshot {
            protocol_version: ProtocolVersion::V1,
            agent_info: Some(acp::Implementation::new(ORION_CODE_AGENT_ID, "0.3.1")),
            agent_capabilities: acp::AgentCapabilities::default()
                .load_session(true)
                .session_capabilities(
                    acp::SessionCapabilities::default()
                        .close(acp::SessionCloseCapabilities::default()),
                ),
        }
    }

    #[test]
    fn initialize_health_requires_exact_identity_version_and_capabilities() {
        assert_eq!(
            validate_orion_code_initialize(&healthy_initialize_snapshot(), "0.3.1")
                .expect("healthy Orion Code initialize response"),
            "0.3.1"
        );

        let mut wrong_protocol = healthy_initialize_snapshot();
        wrong_protocol.protocol_version = ProtocolVersion::V0;
        assert!(validate_orion_code_initialize(&wrong_protocol, "0.3.1").is_err());

        let mut missing_info = healthy_initialize_snapshot();
        missing_info.agent_info = None;
        assert!(validate_orion_code_initialize(&missing_info, "0.3.1").is_err());

        let mut wrong_name = healthy_initialize_snapshot();
        wrong_name.agent_info = Some(acp::Implementation::new("another-agent", "0.3.1"));
        assert!(validate_orion_code_initialize(&wrong_name, "0.3.1").is_err());

        let mut wrong_version = healthy_initialize_snapshot();
        wrong_version.agent_info = Some(acp::Implementation::new(ORION_CODE_AGENT_ID, "0.3.0"));
        assert!(validate_orion_code_initialize(&wrong_version, "0.3.1").is_err());

        let mut missing_load = healthy_initialize_snapshot();
        missing_load.agent_capabilities.load_session = false;
        assert!(validate_orion_code_initialize(&missing_load, "0.3.1").is_err());

        let mut missing_close = healthy_initialize_snapshot();
        missing_close.agent_capabilities.session_capabilities.close = None;
        assert!(validate_orion_code_initialize(&missing_close, "0.3.1").is_err());
    }

    #[gpui::test]
    async fn bootstrap_record_round_trips_in_scoped_namespace() {
        let key_value_store = KeyValueStore::open_test_db("orion_code_bootstrap_round_trip").await;
        key_value_store
            .write_kvp(
                ORION_CODE_BOOTSTRAP_STATE_KEY.to_string(),
                "unrelated".to_string(),
            )
            .await
            .expect("write unscoped control value");
        let record = OrionCodeBootstrapRecord {
            choice: OrionCodeBootstrapChoice::Accepted,
            source: Some(OrionCodeInstallSource::Npx),
            last_attempted_version: Some("0.3.2".to_string()),
            last_verified_version: Some("0.3.1".to_string()),
            ..OrionCodeBootstrapRecord::default()
        };

        write_orion_code_bootstrap_record(&key_value_store, &record)
            .await
            .expect("write scoped bootstrap record");

        assert_eq!(
            read_orion_code_bootstrap_record(&key_value_store)
                .expect("read scoped bootstrap record"),
            record
        );
        assert_eq!(
            key_value_store
                .read_kvp(ORION_CODE_BOOTSTRAP_STATE_KEY)
                .expect("read unscoped control value")
                .as_deref(),
            Some("unrelated")
        );
    }

    #[test]
    fn registry_settings_write_is_idempotent_and_preserves_registry_values() {
        let mut settings = SettingsContent::default();
        assert_eq!(
            ensure_orion_code_registry_settings_content(&mut settings)
                .expect("insert Orion Code registry settings"),
            EnsureOrionCodeRegistrySettings::Inserted
        );
        let configured = settings
            .agent_servers
            .as_mut()
            .expect("agent servers settings should exist")
            .get_mut(ORION_CODE_AGENT_ID)
            .expect("Orion Code settings should exist");
        let settings::CustomAgentServerSettings::Registry { env, .. } = configured else {
            panic!("Orion Code should be configured as a registry agent");
        };
        env.insert("EXISTING".to_string(), "preserved".to_string());

        assert_eq!(
            ensure_orion_code_registry_settings_content(&mut settings)
                .expect("keep existing registry settings"),
            EnsureOrionCodeRegistrySettings::AlreadyConfigured
        );
        let settings::CustomAgentServerSettings::Registry { env, .. } = settings
            .agent_servers
            .as_ref()
            .and_then(|servers| servers.get(ORION_CODE_AGENT_ID))
            .expect("Orion Code settings should remain")
        else {
            panic!("Orion Code should remain a registry agent");
        };
        assert_eq!(env.get("EXISTING").map(String::as_str), Some("preserved"));
    }

    #[test]
    fn registry_settings_refuse_custom_id_conflict() {
        let mut settings = SettingsContent::default();
        settings.agent_servers.get_or_insert_default().insert(
            ORION_CODE_AGENT_ID.to_string(),
            settings::CustomAgentServerSettings::Custom {
                path: PathBuf::from("/custom/orion-code"),
                args: Vec::new(),
                env: HashMap::default(),
                default_mode: None,
                default_config_options: HashMap::default(),
                favorite_config_option_values: HashMap::default(),
            },
        );

        let error = ensure_orion_code_registry_settings_content(&mut settings)
            .expect_err("custom configuration must not be overwritten");
        assert!(error.to_string().contains("refusing to overwrite"));
        assert!(matches!(
            settings
                .agent_servers
                .as_ref()
                .and_then(|servers| servers.get(ORION_CODE_AGENT_ID)),
            Some(settings::CustomAgentServerSettings::Custom { .. })
        ));
    }

    #[test]
    fn restart_phase_preserves_verified_and_retryable_states() {
        let accepted = OrionCodeBootstrapRecord {
            choice: OrionCodeBootstrapChoice::Accepted,
            source: Some(OrionCodeInstallSource::Npx),
            ..OrionCodeBootstrapRecord::default()
        };
        assert_eq!(
            initial_phase_for_configuration(&accepted, OrionCodeRegistryConfiguration::Registry),
            OrionCodeBootstrapPhase::Configured
        );

        let verified = OrionCodeBootstrapRecord {
            last_verified_version: Some("0.3.2".to_string()),
            ..accepted.clone()
        };
        assert_eq!(
            initial_phase_for_configuration(&verified, OrionCodeRegistryConfiguration::Registry),
            OrionCodeBootstrapPhase::Ready
        );

        let failed = OrionCodeBootstrapRecord {
            last_error_kind: Some(OrionCodeBootstrapErrorKind::Initialize),
            ..verified
        };
        assert_eq!(
            initial_phase_for_configuration(&failed, OrionCodeRegistryConfiguration::Registry),
            OrionCodeBootstrapPhase::Ready
        );
        let failed_without_verified = OrionCodeBootstrapRecord {
            last_error_kind: Some(OrionCodeBootstrapErrorKind::Initialize),
            ..accepted.clone()
        };
        assert_eq!(
            initial_phase_for_configuration(
                &failed_without_verified,
                OrionCodeRegistryConfiguration::Registry
            ),
            OrionCodeBootstrapPhase::Failed
        );
        assert_eq!(
            initial_phase_for_configuration(&accepted, OrionCodeRegistryConfiguration::Missing),
            OrionCodeBootstrapPhase::Failed
        );

        let removed_after_failure = OrionCodeBootstrapRecord {
            choice: OrionCodeBootstrapChoice::Removed,
            last_error_kind: Some(OrionCodeBootstrapErrorKind::Configuration),
            ..OrionCodeBootstrapRecord::default()
        };
        assert_eq!(
            initial_phase_for_configuration(
                &removed_after_failure,
                OrionCodeRegistryConfiguration::Missing
            ),
            OrionCodeBootstrapPhase::Failed
        );
    }

    #[test]
    fn ready_bootstrap_reverifies_only_when_the_registry_version_changes() {
        let bootstrap = OrionCodeBootstrap {
            is_new_install: false,
            record: OrionCodeBootstrapRecord {
                choice: OrionCodeBootstrapChoice::Accepted,
                last_verified_version: Some("0.3.1".to_string()),
                ..OrionCodeBootstrapRecord::default()
            },
            phase: OrionCodeBootstrapPhase::Ready,
            durable_ready_version: Some("0.3.1".to_string()),
            last_error: None,
            next_verification_attempt: 1,
            active_verification: None,
            next_lifecycle_operation: 0,
            active_lifecycle_operation: 0,
            lifecycle_operation_lock: Arc::new(futures::lock::Mutex::new(())),
            agent_selection_generation: 0,
            agent_preference_write_lock: Arc::new(futures::lock::Mutex::new(())),
        };

        assert!(!bootstrap.can_start_verification("0.3.1"));
        assert!(bootstrap.can_start_verification("0.3.2"));
    }

    #[test]
    fn remove_and_reenable_clear_the_previous_health_receipt() {
        let mut record = OrionCodeBootstrapRecord {
            choice: OrionCodeBootstrapChoice::Accepted,
            source: Some(OrionCodeInstallSource::Npx),
            last_attempted_version: Some("0.3.2".to_string()),
            last_verified_version: Some("0.3.2".to_string()),
            last_error_kind: None,
            ..OrionCodeBootstrapRecord::default()
        };

        record.choice = OrionCodeBootstrapChoice::Removed;
        clear_orion_code_install_receipt(&mut record);
        record.choice = OrionCodeBootstrapChoice::Accepted;
        clear_orion_code_install_receipt(&mut record);

        assert_eq!(record.last_attempted_version, None);
        assert_eq!(record.last_verified_version, None);
        assert_eq!(
            initial_phase_for_configuration(&record, OrionCodeRegistryConfiguration::Registry),
            OrionCodeBootstrapPhase::Configured
        );
    }

    #[test]
    fn latest_lifecycle_operation_supersedes_older_window_actions() {
        let mut bootstrap = OrionCodeBootstrap {
            is_new_install: true,
            record: OrionCodeBootstrapRecord::default(),
            phase: OrionCodeBootstrapPhase::Pending,
            durable_ready_version: None,
            last_error: None,
            next_verification_attempt: 0,
            active_verification: None,
            next_lifecycle_operation: 0,
            active_lifecycle_operation: 0,
            lifecycle_operation_lock: Arc::new(futures::lock::Mutex::new(())),
            agent_selection_generation: 0,
            agent_preference_write_lock: Arc::new(futures::lock::Mutex::new(())),
        };

        let install = bootstrap.begin_lifecycle_operation();
        let remove = bootstrap.begin_lifecycle_operation();

        assert!(!bootstrap.lifecycle_operation_is_current(install));
        assert!(bootstrap.lifecycle_operation_is_current(remove));
    }

    #[test]
    fn agent_selection_generation_invalidates_an_older_product_default_attempt() {
        let mut bootstrap = OrionCodeBootstrap {
            is_new_install: true,
            record: OrionCodeBootstrapRecord::default(),
            phase: OrionCodeBootstrapPhase::Pending,
            durable_ready_version: None,
            last_error: None,
            next_verification_attempt: 0,
            active_verification: None,
            next_lifecycle_operation: 0,
            active_lifecycle_operation: 0,
            lifecycle_operation_lock: Arc::new(futures::lock::Mutex::new(())),
            agent_selection_generation: 0,
            agent_preference_write_lock: Arc::new(futures::lock::Mutex::new(())),
        };

        let initial_generation = bootstrap.agent_selection_generation();
        let updated_generation = bootstrap.note_agent_selection();

        assert_ne!(initial_generation, updated_generation);
        assert_eq!(bootstrap.agent_selection_generation(), updated_generation);
    }

    #[test]
    fn registry_settings_removal_only_removes_the_managed_registry_entry() {
        let mut registry_settings = SettingsContent::default();
        ensure_orion_code_registry_settings_content(&mut registry_settings)
            .expect("insert managed registry entry");
        assert!(
            remove_orion_code_registry_settings_content(&mut registry_settings)
                .expect("remove managed Registry settings")
        );
        assert!(
            registry_settings
                .agent_servers
                .as_ref()
                .is_none_or(|servers| !servers.contains_key(ORION_CODE_AGENT_ID))
        );

        let mut custom_settings = SettingsContent::default();
        custom_settings
            .agent_servers
            .get_or_insert_default()
            .insert(
                ORION_CODE_AGENT_ID.to_string(),
                settings::CustomAgentServerSettings::Custom {
                    path: PathBuf::from("/custom/orion-code"),
                    args: Vec::new(),
                    env: HashMap::default(),
                    default_mode: None,
                    default_config_options: HashMap::default(),
                    favorite_config_option_values: HashMap::default(),
                },
            );
        let error = remove_orion_code_registry_settings_content(&mut custom_settings)
            .expect_err("custom settings must not be removed");
        assert!(error.to_string().contains("refusing to remove"));
        assert!(matches!(
            custom_settings
                .agent_servers
                .as_ref()
                .and_then(|servers| servers.get(ORION_CODE_AGENT_ID)),
            Some(settings::CustomAgentServerSettings::Custom { .. })
        ));
    }
}
