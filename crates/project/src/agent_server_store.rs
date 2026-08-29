use std::{
    any::Any,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};

use anyhow::{Context as _, Result, bail};
use collections::HashMap;
use fs::{Fs, RemoveOptions};
use futures::StreamExt;
use gpui::{
    AppContext as _, AsyncApp, Context, Entity, EventEmitter, SharedString, Subscription, Task,
    TaskExt,
};
use http_client::{HttpClient, github::AssetKind};
use node_runtime::NodeRuntime;
use percent_encoding::percent_decode_str;
use remote::RemoteClient;
use rpc::{AnyProtoClient, TypedEnvelope, proto};
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use settings::{AgentConfigOptionValue, RegisterSetting, SettingsStore, update_settings_file};
use sha2::{Digest, Sha256};
use url::Url;
use util::{ResultExt as _, debug_panic};

use crate::ProjectEnvironment;
use crate::agent_registry_store::{
    AgentRegistryStore, ORION_CODE_AGENT_ID, RegistryAgent, RegistryTargetConfig,
};

use crate::worktree_store::WorktreeStore;

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct AgentServerCommand {
    #[serde(rename = "command")]
    pub path: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    pub env: Option<HashMap<String, String>>,
}

impl std::fmt::Debug for AgentServerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let filtered_env = self.env.as_ref().map(|env| {
            env.iter()
                .map(|(k, v)| {
                    (
                        k,
                        if util::redact::should_redact(k) {
                            "[REDACTED]"
                        } else {
                            v
                        },
                    )
                })
                .collect::<Vec<_>>()
        });

        f.debug_struct("AgentServerCommand")
            .field("path", &self.path)
            .field("args", &self.args)
            .field("env", &filtered_env)
            .finish()
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct AgentId(pub SharedString);

impl AgentId {
    pub fn new(id: impl Into<SharedString>) -> Self {
        AgentId(id.into())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&'static str> for AgentId {
    fn from(value: &'static str) -> Self {
        AgentId(value.into())
    }
}

impl From<AgentId> for SharedString {
    fn from(value: AgentId) -> Self {
        value.0
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for AgentId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExternalAgentSource {
    #[default]
    Custom,
    Registry,
}

pub trait ExternalAgentServer {
    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>>;

    fn version(&self) -> Option<&SharedString> {
        None
    }

    fn take_new_version_available_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        None
    }

    fn set_new_version_available_tx(&mut self, _tx: watch::Sender<Option<String>>) {}

    fn take_loading_status_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        None
    }

    fn set_loading_status_tx(&mut self, _tx: watch::Sender<Option<String>>) {}

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

enum AgentServerStoreState {
    Local {
        node_runtime: NodeRuntime,
        fs: Arc<dyn Fs>,
        project_environment: Entity<ProjectEnvironment>,
        downstream_client: Option<(u64, AnyProtoClient)>,
        settings: Option<AllAgentServersSettings>,
        http_client: Arc<dyn HttpClient>,
        _subscriptions: Vec<Subscription>,
    },
    Remote {
        project_id: u64,
        upstream_client: Entity<RemoteClient>,
        worktree_store: Entity<WorktreeStore>,
    },
    Collab,
}

pub struct ExternalAgentEntry {
    server: Box<dyn ExternalAgentServer>,
    icon: Option<SharedString>,
    display_name: Option<SharedString>,
    pub source: ExternalAgentSource,
}

impl ExternalAgentEntry {
    pub fn new(
        server: Box<dyn ExternalAgentServer>,
        source: ExternalAgentSource,
        icon: Option<SharedString>,
        display_name: Option<SharedString>,
    ) -> Self {
        Self {
            server,
            icon,
            display_name,
            source,
        }
    }
}

pub struct AgentServerStore {
    state: AgentServerStoreState,
    pub external_agents: HashMap<AgentId, ExternalAgentEntry>,
}

pub struct AgentServersUpdated;

impl EventEmitter<AgentServersUpdated> for AgentServerStore {}

static EXTENSION_TO_REGISTRY_IDS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        HashMap::from_iter([
            ("opencode", "opencode"),
            ("mistral-vibe", "mistral-vibe"),
            ("auggie", "auggie"),
            ("stakpak", "stakpak"),
            ("codebuddy", "codebuddy-code"),
            ("autohand-acp", "autohand"),
            ("corust-agent", "corust-agent"),
            ("factory-droid", "factory-droid"),
            // Unmaintained
            // ("qqcode", ""),
        ])
    });

impl AgentServerStore {
    pub fn migrate_agent_server_from_extensions(
        &mut self,
        id: Arc<str>,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) {
        let Some(registry_id) = EXTENSION_TO_REGISTRY_IDS.get(id.as_ref()) else {
            return;
        };

        update_settings_file(fs, cx, move |settings, _| {
            let agent_servers = settings.agent_servers.get_or_insert_default();
            // Take the old settings
            let settings = agent_servers.remove(id.as_ref());
            // If they had both installed, just remove the extension settings, leave theirregistry settings alone
            if agent_servers.contains_key(*registry_id) {
                return;
            }
            // Insert the old settings, or write new ones so it is "installed" via the registry
            agent_servers.insert(
                registry_id.to_string(),
                settings.unwrap_or_else(|| settings::CustomAgentServerSettings::Registry {
                    default_mode: None,
                    env: Default::default(),
                    default_config_options: HashMap::default(),
                    favorite_config_option_values: HashMap::default(),
                }),
            );
        });
    }

    pub fn agent_icon(&self, id: &AgentId) -> Option<SharedString> {
        self.external_agents
            .get(id)
            .and_then(|entry| entry.icon.clone())
    }

    pub fn agent_source(&self, name: &AgentId) -> Option<ExternalAgentSource> {
        self.external_agents.get(name).map(|entry| entry.source)
    }
}

impl AgentServerStore {
    pub fn agent_display_name(&self, name: &AgentId) -> Option<SharedString> {
        self.external_agents
            .get(name)
            .and_then(|entry| entry.display_name.clone())
    }

    pub fn init_remote(session: &AnyProtoClient) {
        session.add_entity_message_handler(Self::handle_external_agents_updated);
        session.add_entity_message_handler(Self::handle_loading_status_updated);
        session.add_entity_message_handler(Self::handle_new_version_available);
    }

    pub fn init_headless(session: &AnyProtoClient) {
        session.add_entity_request_handler(Self::handle_get_agent_server_command);
    }

    fn agent_servers_settings_changed(&mut self, cx: &mut Context<Self>) {
        let AgentServerStoreState::Local {
            settings: old_settings,
            ..
        } = &mut self.state
        else {
            debug_panic!(
                "should not be subscribed to agent server settings changes in non-local project"
            );
            return;
        };

        let new_settings = cx
            .global::<SettingsStore>()
            .get::<AllAgentServersSettings>(None)
            .clone();
        if Some(&new_settings) == old_settings.as_ref() {
            return;
        }

        self.reregister_agents(cx);
    }

    fn reregister_agents(&mut self, cx: &mut Context<Self>) {
        let AgentServerStoreState::Local {
            node_runtime,
            fs,
            project_environment,
            downstream_client,
            settings: old_settings,
            http_client,
            ..
        } = &mut self.state
        else {
            debug_panic!("Non-local projects should never attempt to reregister. This is a bug!");

            return;
        };

        let new_settings = cx
            .global::<SettingsStore>()
            .get::<AllAgentServersSettings>(None)
            .clone();

        // If we don't have agents from the registry loaded yet, trigger a
        // refresh, which will cause this function to be called again
        let registry_store = AgentRegistryStore::try_global(cx);
        if new_settings.has_registry_agents()
            && let Some(registry) = registry_store.as_ref()
        {
            registry.update(cx, |registry, cx| registry.refresh_if_stale(cx));
        }

        let registry_agents_by_id = registry_store
            .as_ref()
            .map(|store| {
                store
                    .read(cx)
                    .agents()
                    .iter()
                    .cloned()
                    .map(|agent| (agent.id().to_string(), agent))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        // Drain the existing versioned agents, extracting reconnect state
        // from any active connection so we can preserve it or trigger a
        // reconnect when the version changes.
        let mut old_versioned_agents: HashMap<
            AgentId,
            (
                SharedString,
                Option<watch::Sender<Option<String>>>,
                Option<watch::Sender<Option<String>>>,
            ),
        > = HashMap::default();
        for (name, mut entry) in self.external_agents.drain() {
            if let Some(version) = entry.server.version().cloned() {
                let new_version_available_tx = entry.server.take_new_version_available_tx();
                let loading_status_tx = entry.server.take_loading_status_tx();
                if new_version_available_tx.is_some() || loading_status_tx.is_some() {
                    old_versioned_agents
                        .insert(name, (version, new_version_available_tx, loading_status_tx));
                }
            }
        }

        for (name, settings) in new_settings.iter() {
            match settings {
                CustomAgentServerSettings::Custom { command, .. } => {
                    let agent_name = AgentId(name.clone().into());
                    self.external_agents.insert(
                        agent_name.clone(),
                        ExternalAgentEntry::new(
                            Box::new(LocalCustomAgent {
                                command: command.clone(),
                                project_environment: project_environment.clone(),
                            }) as Box<dyn ExternalAgentServer>,
                            ExternalAgentSource::Custom,
                            None,
                            None,
                        ),
                    );
                }
                CustomAgentServerSettings::Registry { env, .. } => {
                    let Some(agent) = registry_agents_by_id.get(name) else {
                        if registry_store.is_some() {
                            log::debug!("Registry agent '{}' not found in ACP registry", name);
                        }
                        continue;
                    };

                    let agent_name = AgentId(name.clone().into());
                    match agent {
                        RegistryAgent::Binary(agent) => {
                            if !agent.supports_current_platform {
                                log::warn!(
                                    "Registry agent '{}' has no compatible binary for this platform",
                                    name
                                );
                                continue;
                            }

                            self.external_agents.insert(
                                agent_name.clone(),
                                ExternalAgentEntry::new(
                                    Box::new(LocalRegistryArchiveAgent {
                                        fs: fs.clone(),
                                        http_client: http_client.clone(),
                                        node_runtime: node_runtime.clone(),
                                        project_environment: project_environment.clone(),
                                        installation_dir: paths::external_agents_dir()
                                            .join("registry")
                                            .join(sanitize_path_component(name)),
                                        version: agent.metadata.version.clone(),
                                        targets: agent.targets.clone(),
                                        env: env.clone(),
                                        new_version_available_tx: None,
                                        loading_status_tx: None,
                                    })
                                        as Box<dyn ExternalAgentServer>,
                                    ExternalAgentSource::Registry,
                                    agent.metadata.icon_path.clone(),
                                    Some(agent.metadata.name.clone()),
                                ),
                            );
                        }
                        RegistryAgent::Npx(agent) => {
                            self.external_agents.insert(
                                agent_name.clone(),
                                ExternalAgentEntry::new(
                                    Box::new(LocalRegistryNpxAgent {
                                        fs: fs.clone(),
                                        node_runtime: node_runtime.clone(),
                                        project_environment: project_environment.clone(),
                                        registry_id: Arc::from(name.as_str()),
                                        version: agent.metadata.version.clone(),
                                        package: agent.package.clone(),
                                        args: agent.args.clone(),
                                        distribution_env: agent.env.clone(),
                                        settings_env: env.clone(),
                                        new_version_available_tx: None,
                                    })
                                        as Box<dyn ExternalAgentServer>,
                                    ExternalAgentSource::Registry,
                                    agent.metadata.icon_path.clone(),
                                    Some(agent.metadata.name.clone()),
                                ),
                            );
                        }
                        RegistryAgent::Unavailable(agent) => {
                            log::info!(
                                "Registry agent '{}' is not currently installable: {}",
                                name,
                                agent.reason
                            );
                        }
                    }
                }
            }
        }

        // For each rebuilt versioned agent, compare the version. If it
        // changed, notify the active connection to reconnect. Otherwise,
        // transfer the channel to the new entry so future updates can use it.
        for (name, entry) in &mut self.external_agents {
            let Some((old_version, new_version_available_tx, loading_status_tx)) =
                old_versioned_agents.remove(name)
            else {
                continue;
            };
            let Some(new_version) = entry.server.version() else {
                continue;
            };

            if new_version != &old_version {
                if let Some(mut tx) = new_version_available_tx {
                    tx.send(Some(new_version.to_string())).ok();
                }
            } else {
                if let Some(tx) = new_version_available_tx {
                    entry.server.set_new_version_available_tx(tx);
                }
                if let Some(tx) = loading_status_tx {
                    entry.server.set_loading_status_tx(tx);
                }
            }
        }

        *old_settings = Some(new_settings);

        if let Some((project_id, downstream_client)) = downstream_client {
            downstream_client
                .send(proto::ExternalAgentsUpdated {
                    project_id: *project_id,
                    names: self
                        .external_agents
                        .keys()
                        .map(|name| name.to_string())
                        .collect(),
                })
                .log_err();
        }
        cx.emit(AgentServersUpdated);
    }

    pub fn node_runtime(&self) -> Option<NodeRuntime> {
        match &self.state {
            AgentServerStoreState::Local { node_runtime, .. } => Some(node_runtime.clone()),
            _ => None,
        }
    }

    pub fn local(
        node_runtime: NodeRuntime,
        fs: Arc<dyn Fs>,
        project_environment: Entity<ProjectEnvironment>,
        http_client: Arc<dyn HttpClient>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = vec![cx.observe_global::<SettingsStore>(|this, cx| {
            this.agent_servers_settings_changed(cx);
        })];
        if let Some(registry_store) = AgentRegistryStore::try_global(cx) {
            subscriptions.push(cx.observe(&registry_store, |this, _, cx| {
                this.reregister_agents(cx);
            }));
        }
        let mut this = Self {
            state: AgentServerStoreState::Local {
                node_runtime,
                fs,
                project_environment,
                http_client,
                downstream_client: None,
                settings: None,
                _subscriptions: subscriptions,
            },
            external_agents: HashMap::default(),
        };
        this.agent_servers_settings_changed(cx);
        this
    }

    pub(crate) fn remote(
        project_id: u64,
        upstream_client: Entity<RemoteClient>,
        worktree_store: Entity<WorktreeStore>,
    ) -> Self {
        Self {
            state: AgentServerStoreState::Remote {
                project_id,
                upstream_client,
                worktree_store,
            },
            external_agents: HashMap::default(),
        }
    }

    pub fn collab() -> Self {
        Self {
            state: AgentServerStoreState::Collab,
            external_agents: HashMap::default(),
        }
    }

    pub fn shared(&mut self, project_id: u64, client: AnyProtoClient, cx: &mut Context<Self>) {
        match &mut self.state {
            AgentServerStoreState::Local {
                downstream_client, ..
            } => {
                *downstream_client = Some((project_id, client.clone()));
                // Send the current list of external agents downstream, but only after a delay,
                // to avoid having the message arrive before the downstream project's agent server store
                // sets up its handlers.
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    let names = this.update(cx, |this, _| {
                        this.external_agents()
                            .map(|name| name.to_string())
                            .collect()
                    })?;
                    client
                        .send(proto::ExternalAgentsUpdated { project_id, names })
                        .log_err();
                    anyhow::Ok(())
                })
                .detach();
            }
            AgentServerStoreState::Remote { .. } => {
                debug_panic!(
                    "external agents over collab not implemented, remote project should not be shared"
                );
            }
            AgentServerStoreState::Collab => {
                debug_panic!("external agents over collab not implemented, should not be shared");
            }
        }
    }

    pub fn get_external_agent(
        &mut self,
        name: &AgentId,
    ) -> Option<&mut (dyn ExternalAgentServer + 'static)> {
        self.external_agents
            .get_mut(name)
            .map(|entry| entry.server.as_mut())
    }

    pub fn no_browser(&self) -> bool {
        match &self.state {
            AgentServerStoreState::Local {
                downstream_client, ..
            } => downstream_client
                .as_ref()
                .is_some_and(|(_, client)| !client.has_wsl_interop()),
            _ => false,
        }
    }

    pub fn has_external_agents(&self) -> bool {
        !self.external_agents.is_empty()
    }

    pub fn external_agents(&self) -> impl Iterator<Item = &AgentId> {
        self.external_agents.keys()
    }

    async fn handle_get_agent_server_command(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetAgentServerCommand>,
        mut cx: AsyncApp,
    ) -> Result<proto::AgentServerCommand> {
        let command = this
            .update(&mut cx, |this, cx| {
                let AgentServerStoreState::Local {
                    downstream_client, ..
                } = &this.state
                else {
                    debug_panic!("should not receive GetAgentServerCommand in a non-local project");
                    bail!("unexpected GetAgentServerCommand request in a non-local project");
                };
                let no_browser = this.no_browser();
                let agent = this
                    .external_agents
                    .get_mut(&*envelope.payload.name)
                    .map(|entry| entry.server.as_mut())
                    .with_context(|| format!("agent `{}` not found", envelope.payload.name))?;
                let new_version_available_tx =
                    downstream_client
                        .clone()
                        .map(|(project_id, downstream_client)| {
                            let (new_version_available_tx, mut new_version_available_rx) =
                                watch::channel(None);
                            cx.spawn({
                                let name = envelope.payload.name.clone();
                                async move |_, _| {
                                    if let Some(version) =
                                        new_version_available_rx.recv().await.ok().flatten()
                                    {
                                        downstream_client.send(
                                            proto::NewExternalAgentVersionAvailable {
                                                project_id,
                                                name: name.clone(),
                                                version,
                                            },
                                        )?;
                                    }
                                    anyhow::Ok(())
                                }
                            })
                            .detach_and_log_err(cx);
                            new_version_available_tx
                        });
                let loading_status_tx =
                    downstream_client
                        .clone()
                        .map(|(project_id, downstream_client)| {
                            let (loading_status_tx, mut loading_status_rx) = watch::channel(None);
                            cx.spawn({
                                let name = envelope.payload.name.clone();
                                async move |_, _| {
                                    while let Ok(status) = loading_status_rx.recv().await {
                                        downstream_client.send(
                                            proto::ExternalAgentLoadingStatusUpdated {
                                                project_id,
                                                name: name.clone(),
                                                status,
                                            },
                                        )?;
                                    }
                                    anyhow::Ok(())
                                }
                            })
                            .detach_and_log_err(cx);
                            loading_status_tx
                        });
                let mut extra_env = HashMap::default();
                if no_browser {
                    extra_env.insert("NO_BROWSER".to_owned(), "1".to_owned());
                }
                if let Some(new_version_available_tx) = new_version_available_tx {
                    agent.set_new_version_available_tx(new_version_available_tx);
                }
                if let Some(loading_status_tx) = loading_status_tx {
                    agent.set_loading_status_tx(loading_status_tx);
                }
                anyhow::Ok(agent.get_command(vec![], extra_env, &mut cx.to_async()))
            })?
            .await?;
        Ok(proto::AgentServerCommand {
            path: command.path.to_string_lossy().into_owned(),
            args: command.args,
            env: command
                .env
                .map(|env| env.into_iter().collect())
                .unwrap_or_default(),
            root_dir: envelope
                .payload
                .root_dir
                .unwrap_or_else(|| paths::home_dir().to_string_lossy().to_string()),
            login: None,
        })
    }

    async fn handle_external_agents_updated(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ExternalAgentsUpdated>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            let AgentServerStoreState::Remote {
                project_id,
                upstream_client,
                worktree_store,
            } = &this.state
            else {
                debug_panic!(
                    "handle_external_agents_updated should not be called for a non-remote project"
                );
                bail!("unexpected ExternalAgentsUpdated message")
            };

            let mut previous_entries = std::mem::take(&mut this.external_agents);
            let mut new_version_available_txs = HashMap::default();
            let mut loading_status_txs = HashMap::default();
            let mut metadata = HashMap::default();

            for (name, mut entry) in previous_entries.drain() {
                if let Some(tx) = entry.server.take_new_version_available_tx() {
                    new_version_available_txs.insert(name.clone(), tx);
                }
                if let Some(tx) = entry.server.take_loading_status_tx() {
                    loading_status_txs.insert(name.clone(), tx);
                }

                metadata.insert(name, (entry.icon, entry.display_name, entry.source));
            }

            this.external_agents = envelope
                .payload
                .names
                .into_iter()
                .map(|name| {
                    let agent_id = AgentId(name.into());
                    let (icon, display_name, source) = metadata
                        .remove(&agent_id)
                        .or_else(|| {
                            AgentRegistryStore::try_global(cx)
                                .and_then(|store| store.read(cx).agent(&agent_id))
                                .map(|s| {
                                    (
                                        s.icon_path().cloned(),
                                        Some(s.name().clone()),
                                        ExternalAgentSource::Registry,
                                    )
                                })
                        })
                        .unwrap_or((None, None, ExternalAgentSource::default()));
                    let agent = RemoteExternalAgentServer {
                        project_id: *project_id,
                        upstream_client: upstream_client.clone(),
                        worktree_store: worktree_store.clone(),
                        name: agent_id.clone(),
                        new_version_available_tx: new_version_available_txs.remove(&agent_id),
                        loading_status_tx: loading_status_txs.remove(&agent_id),
                    };
                    (
                        agent_id,
                        ExternalAgentEntry::new(
                            Box::new(agent) as Box<dyn ExternalAgentServer>,
                            source,
                            icon,
                            display_name,
                        ),
                    )
                })
                .collect();
            cx.emit(AgentServersUpdated);
            Ok(())
        })
    }

    async fn handle_loading_status_updated(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ExternalAgentLoadingStatusUpdated>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, _| {
            if let Some(entry) = this.external_agents.get_mut(&*envelope.payload.name)
                && let Some(mut tx) = entry.server.take_loading_status_tx()
            {
                tx.send(envelope.payload.status).ok();
                entry.server.set_loading_status_tx(tx);
            }
        });
        Ok(())
    }

    async fn handle_new_version_available(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::NewExternalAgentVersionAvailable>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, _| {
            if let Some(entry) = this.external_agents.get_mut(&*envelope.payload.name)
                && let Some(mut tx) = entry.server.take_new_version_available_tx()
            {
                tx.send(Some(envelope.payload.version)).ok();
                entry.server.set_new_version_available_tx(tx);
            }
        });
        Ok(())
    }
}

struct RemoteExternalAgentServer {
    project_id: u64,
    upstream_client: Entity<RemoteClient>,
    worktree_store: Entity<WorktreeStore>,
    name: AgentId,
    new_version_available_tx: Option<watch::Sender<Option<String>>>,
    loading_status_tx: Option<watch::Sender<Option<String>>>,
}

impl ExternalAgentServer for RemoteExternalAgentServer {
    fn take_new_version_available_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.new_version_available_tx.take()
    }

    fn set_new_version_available_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.new_version_available_tx = Some(tx);
    }

    fn take_loading_status_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.loading_status_tx.take()
    }

    fn set_loading_status_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.loading_status_tx = Some(tx);
    }

    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>> {
        let project_id = self.project_id;
        let name = self.name.to_string();
        let upstream_client = self.upstream_client.downgrade();
        let worktree_store = self.worktree_store.clone();
        cx.spawn(async move |cx| {
            let root_dir = worktree_store.read_with(cx, |worktree_store, cx| {
                crate::Project::default_visible_worktree_paths(worktree_store, cx)
                    .into_iter()
                    .next()
                    .map(|path| path.display().to_string())
            });

            let mut response = upstream_client
                .update(cx, |upstream_client, _| {
                    upstream_client
                        .proto_client()
                        .request(proto::GetAgentServerCommand {
                            project_id,
                            name,
                            root_dir,
                        })
                })?
                .await?;
            response.args.extend(extra_args);
            response.env.extend(extra_env);

            Ok(AgentServerCommand {
                path: response.path.into(),
                args: response.args,
                env: Some(response.env.into_iter().collect()),
            })
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RegistryArchiveKind {
    Archive(AssetKind),
    /// The archive URL points directly at an executable, per the ACP registry
    /// schema: "URL to download archive (.zip, .tar.gz, .tgz, .tar.bz2, .tbz2,
    /// or raw binary)".
    RawBinary {
        file_name: String,
    },
}

fn registry_archive_kind_for_url(archive_url: &str) -> Result<RegistryArchiveKind> {
    const UNSUPPORTED_SUFFIXES: &[&str] = &[
        // Installer formats explicitly rejected by the registry schema.
        ".dmg",
        ".pkg",
        ".deb",
        ".rpm",
        ".msi",
        ".appimage",
        // Archive formats we cannot extract; treating them as raw binaries
        // would produce a broken install.
        ".tar.xz",
        ".txz",
        ".tar",
        ".gz",
        ".bz2",
        ".xz",
        ".7z",
    ];

    let archive_path = Url::parse(archive_url)
        .ok()
        .map(|url| url.path().to_string())
        .unwrap_or_else(|| archive_url.to_string());
    let lowercase_path = archive_path.to_lowercase();

    if lowercase_path.ends_with(".zip") {
        Ok(RegistryArchiveKind::Archive(AssetKind::Zip))
    } else if lowercase_path.ends_with(".tar.gz") || lowercase_path.ends_with(".tgz") {
        Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
    } else if lowercase_path.ends_with(".tar.bz2") || lowercase_path.ends_with(".tbz2") {
        Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
    } else if let Some(suffix) = UNSUPPORTED_SUFFIXES
        .iter()
        .find(|suffix| lowercase_path.ends_with(*suffix))
    {
        bail!("unsupported archive type {suffix} in URL: {archive_url}");
    } else {
        let file_name = raw_binary_file_name(&archive_path)
            .with_context(|| format!("determining binary file name from URL: {archive_url}"))?;
        Ok(RegistryArchiveKind::RawBinary { file_name })
    }
}

fn raw_binary_file_name(archive_path: &str) -> Result<String> {
    let last_segment = archive_path
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .context("URL has no file name")?;
    let file_name = percent_decode_str(last_segment)
        .decode_utf8()
        .context("file name is not valid UTF-8")?
        .into_owned();
    anyhow::ensure!(
        !file_name.is_empty()
            && file_name != "."
            && file_name != ".."
            && !file_name.contains(['/', '\\'])
            && !file_name.contains('\0'),
        "invalid binary file name: {file_name}"
    );
    Ok(file_name)
}

struct GithubReleaseArchive {
    repo_name_with_owner: String,
    tag: String,
    asset_name: String,
}

fn github_release_archive_from_url(archive_url: &str) -> Option<GithubReleaseArchive> {
    fn decode_path_segment(segment: &str) -> Option<String> {
        percent_decode_str(segment)
            .decode_utf8()
            .ok()
            .map(|segment| segment.into_owned())
    }

    let url = Url::parse(archive_url).ok()?;
    if url.scheme() != "https" || url.host_str()? != "github.com" {
        return None;
    }

    let segments = url.path_segments()?.collect::<Vec<_>>();
    if segments.len() < 6 || segments[2] != "releases" || segments[3] != "download" {
        return None;
    }

    Some(GithubReleaseArchive {
        repo_name_with_owner: format!("{}/{}", segments[0], segments[1]),
        tag: decode_path_segment(segments[4])?,
        asset_name: segments[5..]
            .iter()
            .map(|segment| decode_path_segment(segment))
            .collect::<Option<Vec<_>>>()?
            .join("/"),
    })
}

fn sanitize_path_component(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => character,
            _ => '-',
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn versioned_archive_cache_dir(
    base_dir: &Path,
    version: Option<&str>,
    archive_url: &str,
    sha256: Option<&str>,
) -> PathBuf {
    let version = version.unwrap_or_default();
    let sanitized_version = sanitize_path_component(version);

    let mut version_hasher = Sha256::new();
    version_hasher.update(version.as_bytes());
    let version_hash = format!("{:x}", version_hasher.finalize());

    let mut archive_hasher = Sha256::new();
    archive_hasher.update(archive_url.as_bytes());
    if let Some(sha256) = sha256 {
        archive_hasher.update(b"\0sha256:");
        archive_hasher.update(sha256.to_ascii_lowercase().as_bytes());
    }
    let archive_hash = format!("{:x}", archive_hasher.finalize());

    base_dir.join(format!(
        "v_{sanitized_version}_{}_{}",
        &version_hash[..16],
        &archive_hash[..16],
    ))
}

// The `v_` prefix here must stay in sync with `versioned_archive_cache_dir`,
// so we only ever remove directories that we created ourselves.
const VERSIONED_ARCHIVE_CACHE_DIR_PREFIX: &str = "v_";

async fn remove_stale_versioned_archive_cache_dirs(
    fs: Arc<dyn Fs>,
    base_dir: &Path,
    current_version_dir: &Path,
) -> Result<()> {
    let Some(current_dir_name) = current_version_dir.file_name() else {
        return Ok(());
    };

    let current_mtime = fs
        .metadata(current_version_dir)
        .await
        .with_context(|| format!("reading metadata for {current_version_dir:?}"))?
        .with_context(|| format!("missing metadata for {current_version_dir:?}"))?
        .mtime;

    let mut entries = fs
        .read_dir(base_dir)
        .await
        .with_context(|| format!("reading archive cache directory {base_dir:?}"))?;

    while let Some(entry) = entries.next().await {
        let entry = entry.with_context(|| format!("reading entry in {base_dir:?}"))?;
        let Some(entry_name) = entry.file_name() else {
            continue;
        };

        if entry_name == current_dir_name
            || !entry_name
                .to_string_lossy()
                .starts_with(VERSIONED_ARCHIVE_CACHE_DIR_PREFIX)
        {
            continue;
        }

        let Some(entry_metadata) = fs.metadata(&entry).await.log_err().flatten() else {
            continue;
        };
        if !entry_metadata.is_dir {
            continue;
        }
        // Only remove directories that predate the current version's directory.
        // This avoids racing with a concurrent extraction of a different version
        // that finished after we cached the current version's mtime.
        if !current_mtime.bad_is_greater_than(entry_metadata.mtime) {
            continue;
        }

        fs.remove_dir(
            &entry,
            RemoveOptions {
                recursive: true,
                ignore_if_not_exists: true,
            },
        )
        .await
        .with_context(|| format!("removing stale archive cache directory {entry:?}"))?;
    }

    Ok(())
}

pub async fn remove_orion_code_managed_install(fs: Arc<dyn Fs>) -> Result<()> {
    remove_orion_code_managed_install_from(fs, paths::external_agents_dir().to_path_buf()).await
}

async fn remove_orion_code_managed_install_from(
    fs: Arc<dyn Fs>,
    external_agents_directory: PathBuf,
) -> Result<()> {
    let Some(root_metadata) = fs
        .metadata(&external_agents_directory)
        .await
        .with_context(|| {
            format!("reading metadata for managed Agent root {external_agents_directory:?}")
        })?
    else {
        return Ok(());
    };
    if root_metadata.is_symlink || !root_metadata.is_dir {
        bail!(
            "refusing to remove Orion Code from non-directory or symlink Agent root {external_agents_directory:?}"
        );
    }
    let canonical_root = fs
        .canonicalize(&external_agents_directory)
        .await
        .with_context(|| {
            format!("canonicalizing managed Agent root {external_agents_directory:?}")
        })?;
    let managed_versions_directory = external_agents_directory
        .join("registry")
        .join("npx")
        .join(ORION_CODE_AGENT_ID);
    if !validate_orion_code_managed_install_path(
        fs.as_ref(),
        &external_agents_directory,
        &canonical_root,
        &managed_versions_directory,
    )
    .await?
    {
        return Ok(());
    }

    let mut entries = fs
        .read_dir(&managed_versions_directory)
        .await
        .with_context(|| {
            format!("reading managed Orion Code versions from {managed_versions_directory:?}")
        })?;
    let mut validated_install_directories = Vec::new();
    while let Some(entry) = entries.next().await {
        let entry = entry.with_context(|| {
            format!("reading entry in managed Orion Code directory {managed_versions_directory:?}")
        })?;
        let Some(version_name) = entry.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Ok(version) = Version::parse(version_name) else {
            continue;
        };
        if version.to_string() != version_name {
            continue;
        }
        validate_orion_code_registry_npx_version_path(&external_agents_directory, &entry)?;
        if validate_orion_code_managed_install_path(
            fs.as_ref(),
            &external_agents_directory,
            &canonical_root,
            &entry,
        )
        .await?
        {
            validated_install_directories.push(entry);
        }
    }

    for install_directory in validated_install_directories {
        if !validate_orion_code_managed_install_path(
            fs.as_ref(),
            &external_agents_directory,
            &canonical_root,
            &install_directory,
        )
        .await?
        {
            continue;
        }
        fs.remove_dir(
            &install_directory,
            RemoveOptions {
                recursive: true,
                ignore_if_not_exists: true,
            },
        )
        .await
        .with_context(|| format!("removing managed Orion Code install {install_directory:?}"))?;
    }

    Ok(())
}

async fn validate_orion_code_managed_install_path(
    fs: &dyn Fs,
    external_agents_directory: &Path,
    canonical_root: &Path,
    install_directory: &Path,
) -> Result<bool> {
    let relative_install = install_directory
        .strip_prefix(external_agents_directory)
        .with_context(|| {
            format!(
                "managed Orion Code install {install_directory:?} is not below {external_agents_directory:?}"
            )
        })?;
    let mut current = external_agents_directory.to_path_buf();
    for component in relative_install.components() {
        let std::path::Component::Normal(component) = component else {
            bail!("refusing invalid managed Orion Code install path {install_directory:?}");
        };
        current.push(component);
        let Some(metadata) = fs
            .metadata(&current)
            .await
            .with_context(|| format!("reading metadata for {current:?}"))?
        else {
            return Ok(false);
        };
        if metadata.is_symlink || !metadata.is_dir {
            bail!("refusing non-directory or symlink in managed install path {current:?}");
        }
    }

    let canonical_install = fs.canonicalize(install_directory).await.with_context(|| {
        format!("canonicalizing managed Orion Code install {install_directory:?}")
    })?;
    if canonical_install == canonical_root || !canonical_install.starts_with(canonical_root) {
        bail!(
            "refusing to remove Orion Code install outside the managed Agent root: {canonical_install:?}"
        );
    }
    Ok(true)
}

fn registry_npx_install_directory(
    external_agents_directory: &Path,
    registry_id: &str,
    version: &str,
) -> Result<PathBuf> {
    let agent_directory = external_agents_directory
        .join("registry")
        .join("npx")
        .join(sanitize_path_component(registry_id));
    if registry_id != ORION_CODE_AGENT_ID {
        return Ok(agent_directory);
    }

    let version = Version::parse(version)
        .with_context(|| format!("invalid Orion Code Registry version {version:?}"))?;
    let install_directory = agent_directory.join(version.to_string());
    validate_orion_code_registry_npx_version_path(external_agents_directory, &install_directory)?;
    Ok(install_directory)
}

fn validate_orion_code_registry_npx_version_path(
    external_agents_directory: &Path,
    install_directory: &Path,
) -> Result<()> {
    let mut components = install_directory
        .strip_prefix(external_agents_directory)
        .with_context(|| {
            format!(
                "Orion Code install {install_directory:?} is not below {external_agents_directory:?}"
            )
        })?
        .components();
    let expected_prefix = ["registry", "npx", ORION_CODE_AGENT_ID];
    for expected in expected_prefix {
        match components.next() {
            Some(std::path::Component::Normal(component))
                if component == std::ffi::OsStr::new(expected) => {}
            _ => bail!("invalid Orion Code Registry install path {install_directory:?}"),
        }
    }
    let Some(std::path::Component::Normal(version_component)) = components.next() else {
        bail!("invalid Orion Code Registry install path {install_directory:?}");
    };
    if components.next().is_some() {
        bail!("invalid Orion Code Registry install path {install_directory:?}");
    }
    let version_component = version_component
        .to_str()
        .with_context(|| format!("non-Unicode Orion Code version path {install_directory:?}"))?;
    let version = Version::parse(version_component)
        .with_context(|| format!("invalid Orion Code version path {install_directory:?}"))?;
    if version.to_string() != version_component {
        bail!("non-normalized Orion Code version path {install_directory:?}");
    }

    Ok(())
}

async fn prepare_orion_code_registry_npx_install_directory(
    fs: &dyn Fs,
    external_agents_directory: &Path,
    install_directory: &Path,
) -> Result<()> {
    validate_orion_code_registry_npx_version_path(external_agents_directory, install_directory)?;
    if fs.metadata(external_agents_directory).await?.is_none() {
        fs.create_dir(external_agents_directory)
            .await
            .with_context(|| {
                format!("creating managed Agent root {external_agents_directory:?}")
            })?;
    }

    let root_metadata = fs
        .metadata(external_agents_directory)
        .await?
        .with_context(|| format!("missing managed Agent root {external_agents_directory:?}"))?;
    if root_metadata.is_symlink || !root_metadata.is_dir {
        bail!(
            "refusing Orion Code install through non-directory or symlink Agent root {external_agents_directory:?}"
        );
    }
    let canonical_root = fs
        .canonicalize(external_agents_directory)
        .await
        .with_context(|| {
            format!("canonicalizing managed Agent root {external_agents_directory:?}")
        })?;

    let relative_install = install_directory
        .strip_prefix(external_agents_directory)
        .with_context(|| {
            format!(
                "Orion Code install {install_directory:?} is not below {external_agents_directory:?}"
            )
        })?;
    let mut current = external_agents_directory.to_path_buf();
    for component in relative_install.components() {
        let std::path::Component::Normal(component) = component else {
            bail!("invalid Orion Code Registry install path {install_directory:?}");
        };
        current.push(component);
        let Some(metadata) = fs
            .metadata(&current)
            .await
            .with_context(|| format!("reading metadata for {current:?}"))?
        else {
            break;
        };
        if metadata.is_symlink || !metadata.is_dir {
            bail!("refusing non-directory or symlink in managed install path {current:?}");
        }
    }

    fs.create_dir(install_directory)
        .await
        .with_context(|| format!("creating managed Orion Code install {install_directory:?}"))?;
    if !validate_orion_code_managed_install_path(
        fs,
        external_agents_directory,
        &canonical_root,
        install_directory,
    )
    .await?
    {
        bail!("managed Orion Code install was not created at {install_directory:?}");
    }

    Ok(())
}

struct LocalRegistryArchiveAgent {
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    node_runtime: NodeRuntime,
    project_environment: Entity<ProjectEnvironment>,
    installation_dir: PathBuf,
    version: SharedString,
    targets: HashMap<String, RegistryTargetConfig>,
    env: HashMap<String, String>,
    new_version_available_tx: Option<watch::Sender<Option<String>>>,
    loading_status_tx: Option<watch::Sender<Option<String>>>,
}

impl ExternalAgentServer for LocalRegistryArchiveAgent {
    fn version(&self) -> Option<&SharedString> {
        Some(&self.version)
    }

    fn take_new_version_available_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.new_version_available_tx.take()
    }

    fn set_new_version_available_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.new_version_available_tx = Some(tx);
    }

    fn take_loading_status_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.loading_status_tx.take()
    }

    fn set_loading_status_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.loading_status_tx = Some(tx);
    }

    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>> {
        let fs = self.fs.clone();
        let http_client = self.http_client.clone();
        let node_runtime = self.node_runtime.clone();
        let project_environment = self.project_environment.downgrade();
        let installation_dir = self.installation_dir.clone();
        let targets = self.targets.clone();
        let settings_env = self.env.clone();
        let version = self.version.clone();
        let loading_status_tx = self.loading_status_tx.take();

        cx.spawn(async move |cx| {
            let mut env = project_environment
                .update(cx, |project_environment, cx| {
                    project_environment.default_environment(cx)
                })?
                .await
                .unwrap_or_default();

            let dir = installation_dir;
            fs.create_dir(&dir).await?;

            let os = if cfg!(target_os = "macos") {
                "darwin"
            } else if cfg!(target_os = "linux") {
                "linux"
            } else if cfg!(target_os = "windows") {
                "windows"
            } else {
                anyhow::bail!("unsupported OS");
            };

            let arch = if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else if cfg!(target_arch = "x86_64") {
                "x86_64"
            } else {
                anyhow::bail!("unsupported architecture");
            };

            let platform_key = format!("{}-{}", os, arch);
            let target_config = targets.get(&platform_key).with_context(|| {
                format!(
                    "no target specified for platform '{}'. Available platforms: {}",
                    platform_key,
                    targets
                        .keys()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;

            env.extend(target_config.env.clone());
            env.extend(extra_env);
            env.extend(settings_env);

            let archive_url = &target_config.archive;
            let version_dir = versioned_archive_cache_dir(
                &dir,
                Some(version.as_ref()),
                archive_url,
                target_config.sha256.as_deref(),
            );

            if !fs.is_dir(&version_dir).await {
                let mut loading_status_tx = loading_status_tx;
                if let Some(tx) = loading_status_tx.as_mut() {
                    tx.send(Some(format!("Installing {}…", version.as_ref())))
                        .ok();
                }

                let sha256 = if let Some(provided_sha) = &target_config.sha256 {
                    Some(provided_sha.clone())
                } else if let Some(github_archive) = github_release_archive_from_url(archive_url) {
                    if let Ok(release) = ::http_client::github::get_release_by_tag_name(
                        &github_archive.repo_name_with_owner,
                        &github_archive.tag,
                        http_client.clone(),
                    )
                    .await
                    {
                        if let Some(asset) = release
                            .assets
                            .iter()
                            .find(|a| a.name == github_archive.asset_name)
                        {
                            asset.digest.as_ref().and_then(|d| {
                                d.strip_prefix("sha256:")
                                    .map(|s| s.to_string())
                                    .or_else(|| Some(d.clone()))
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                match registry_archive_kind_for_url(archive_url)? {
                    RegistryArchiveKind::Archive(asset_kind) => {
                        ::http_client::github_download::download_server_binary(
                            &*http_client,
                            archive_url,
                            sha256.as_deref(),
                            &version_dir,
                            asset_kind,
                        )
                        .await?;
                    }
                    RegistryArchiveKind::RawBinary { file_name } => {
                        ::http_client::github_download::download_server_raw_binary(
                            &*http_client,
                            archive_url,
                            sha256.as_deref(),
                            &version_dir,
                            &file_name,
                        )
                        .await?;
                    }
                }
            }

            let cmd = &target_config.cmd;

            let cmd_path = if cmd == "node" {
                node_runtime.binary_path().await?
            } else {
                if cmd.contains("..") {
                    anyhow::bail!("command path cannot contain '..': {}", cmd);
                }

                if cmd.starts_with("./") || cmd.starts_with(".\\") {
                    let cmd_path = version_dir.join(&cmd[2..]);
                    anyhow::ensure!(
                        fs.is_file(&cmd_path).await,
                        "Missing command {} after extraction",
                        cmd_path.to_string_lossy()
                    );
                    cmd_path
                } else {
                    anyhow::bail!("command must be relative (start with './'): {}", cmd);
                }
            };

            cx.background_spawn({
                let fs = fs.clone();
                let dir = dir.clone();
                let version_dir = version_dir.clone();
                async move {
                    remove_stale_versioned_archive_cache_dirs(fs, &dir, &version_dir)
                        .await
                        .log_err();
                }
            })
            .detach();

            let mut args = target_config.args.clone();
            args.extend(extra_args);

            let command = AgentServerCommand {
                path: cmd_path,
                args,
                env: Some(env),
            };

            Ok(command)
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct LocalRegistryNpxAgent {
    fs: Arc<dyn Fs>,
    node_runtime: NodeRuntime,
    project_environment: Entity<ProjectEnvironment>,
    registry_id: Arc<str>,
    version: SharedString,
    package: SharedString,
    args: Vec<String>,
    distribution_env: HashMap<String, String>,
    settings_env: HashMap<String, String>,
    new_version_available_tx: Option<watch::Sender<Option<String>>>,
}

impl ExternalAgentServer for LocalRegistryNpxAgent {
    fn version(&self) -> Option<&SharedString> {
        Some(&self.version)
    }

    fn take_new_version_available_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.new_version_available_tx.take()
    }

    fn set_new_version_available_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.new_version_available_tx = Some(tx);
    }

    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>> {
        let fs = self.fs.clone();
        let node_runtime = self.node_runtime.clone();
        let project_environment = self.project_environment.downgrade();
        let registry_id = self.registry_id.clone();
        let version = self.version.clone();
        let package = self.package.clone();
        let args = self.args.clone();
        let distribution_env = self.distribution_env.clone();
        let settings_env = self.settings_env.clone();

        cx.spawn(async move |cx| {
            let mut env = project_environment
                .update(cx, |project_environment, cx| {
                    project_environment.default_environment(cx)
                })?
                .await
                .unwrap_or_default();

            let external_agents_directory = paths::external_agents_dir();
            let install_dir = registry_npx_install_directory(
                external_agents_directory,
                &registry_id,
                version.as_ref(),
            )?;
            if registry_id.as_ref() == ORION_CODE_AGENT_ID {
                prepare_orion_code_registry_npx_install_directory(
                    fs.as_ref(),
                    external_agents_directory,
                    &install_dir,
                )
                .await?;
            } else {
                fs.create_dir(&install_dir).await?;
            }

            let (package_name, package_spec) = registry_npm_package_spec(&registry_id, &package);
            let node_modules_directory = install_dir.join("node_modules");
            let installed_version = if registry_id.as_ref() == ORION_CODE_AGENT_ID {
                match node_runtime::read_package_installed_version(
                    node_modules_directory.clone(),
                    package_name,
                )
                .await
                {
                    Ok(installed_version) => installed_version,
                    Err(error) => {
                        log::warn!(
                            "Failed to inspect cached Registry package {package_name}: {error:#}"
                        );
                        None
                    }
                }
            } else {
                None
            };
            let cached_executable = if should_reuse_cached_registry_npx_install(
                &registry_id,
                version.as_ref(),
                installed_version.as_ref(),
            ) {
                match node_runtime::read_package_executable(
                    node_modules_directory.clone(),
                    package_name,
                )
                .await
                {
                    Ok(executable) if fs.is_file(&executable).await => Some(executable),
                    Ok(executable) => {
                        log::warn!(
                            "Cached Orion Code {version} executable is missing at {executable:?}; reinstalling"
                        );
                        None
                    }
                    Err(error) => {
                        log::warn!(
                            "Cached Orion Code {version} has no usable executable; reinstalling: {error:#}"
                        );
                        None
                    }
                }
            } else {
                None
            };
            let executable = if let Some(executable) = cached_executable {
                executable
            } else {
                node_runtime
                    .run_npm_subcommand(
                        Some(&install_dir),
                        "install",
                        &[package_spec.as_str(), "--save-exact"],
                    )
                    .await?;
                node_runtime::read_package_executable(node_modules_directory, package_name).await?
            };

            let node_binary = node_runtime.binary_path().await?;
            env.extend(node_runtime::npm_command_env(&node_binary));
            env.extend(distribution_env);
            env.extend(extra_env);
            env.extend(settings_env);
            enforce_orion_code_managed_environment(&registry_id, paths::data_dir(), &mut env);

            let mut command_args = vec![executable.to_string_lossy().into_owned()];
            command_args.extend(args);
            command_args.extend(extra_args);

            let command = AgentServerCommand {
                path: node_binary,
                args: command_args,
                env: Some(env),
            };

            Ok(command)
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn enforce_orion_code_managed_environment(
    registry_id: &str,
    studio_data_directory: &Path,
    environment: &mut HashMap<String, String>,
) {
    if registry_id != ORION_CODE_AGENT_ID {
        return;
    }

    environment.insert("ORION_CODE_DISABLE_ENV_FILES".to_string(), "1".to_string());
    environment.insert(
        "ORION_CODE_DATA_DIR".to_string(),
        studio_data_directory
            .join("orion-code")
            .to_string_lossy()
            .into_owned(),
    );
}

/// People are using min-release-age more frequently. Which means a fresh registry will likely have
/// new package versions than the user can install.
/// We set the version to now be a ceiling and not an exact pin instead. This allows npm to resolve
/// the latest version it can find that satisfies the constraint. npm seems to check regularly enough
/// that new versions are available. This does have a few downsides:
/// - The user might have an older cached version of the package that satisfies the constraint, until
///   npm checks for updates again.
/// - The registry args/env may not be valid for the resolved version.
///
/// This is a best-effort attempt to install a version that works without overriding the user's
/// security settings, as the args don't change often. The registry will need to support this better
/// at some point, but until then, this is a best-effort workaround that hopefully solves the issue
/// for most users.
///
/// We use npm's hyphen-range syntax (`0.0.0 - <version>`, equivalent to `<=<version>`) instead of
/// the more compact `<=<version>` form because on Windows, `npm` is `npm.cmd` (a batch file run by
/// cmd.exe), and the quotes our shell builder emits are PowerShell string-literal syntax that PS
/// strips during parsing. PS only re-adds CRT-style transport quotes around native command args
/// containing whitespace, so `package@<=0.25.3` reaches cmd.exe bare and the unquoted `<` is
/// interpreted as input redirection. See zed-industries/zed#55921.
fn bounded_npm_package_spec(package_spec: &str) -> (&str, String) {
    let Some((package_name, version)) = package_spec.rsplit_once('@') else {
        return (package_spec, package_spec.to_string());
    };
    if package_name.is_empty() {
        return (package_spec, package_spec.to_string());
    }
    if Version::parse(version).is_err() {
        return (package_name, package_spec.to_string());
    }

    (package_name, format!("{package_name}@0.0.0 - {version}"))
}

fn registry_npm_package_spec<'a>(registry_id: &str, package_spec: &'a str) -> (&'a str, String) {
    let (package_name, bounded_spec) = bounded_npm_package_spec(package_spec);
    if registry_id == ORION_CODE_AGENT_ID {
        (package_name, package_spec.to_string())
    } else {
        (package_name, bounded_spec)
    }
}

fn should_reuse_cached_registry_npx_install(
    registry_id: &str,
    requested_version: &str,
    installed_version: Option<&Version>,
) -> bool {
    if registry_id != ORION_CODE_AGENT_ID {
        return false;
    }
    Version::parse(requested_version)
        .ok()
        .as_ref()
        .is_some_and(|requested| Some(requested) == installed_version)
}

struct LocalCustomAgent {
    project_environment: Entity<ProjectEnvironment>,
    command: AgentServerCommand,
}

impl ExternalAgentServer for LocalCustomAgent {
    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>> {
        let mut command = self.command.clone();
        let project_environment = self.project_environment.downgrade();
        cx.spawn(async move |cx| {
            let mut env = project_environment
                .update(cx, |project_environment, cx| {
                    project_environment.default_environment(cx)
                })?
                .await
                .unwrap_or_default();
            env.extend(command.env.unwrap_or_default());
            env.extend(extra_env);
            command.env = Some(env);
            command.args.extend(extra_args);
            Ok(command)
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default, Clone, JsonSchema, Debug, PartialEq, RegisterSetting)]
pub struct AllAgentServersSettings(pub HashMap<String, CustomAgentServerSettings>);

impl std::ops::Deref for AllAgentServersSettings {
    type Target = HashMap<String, CustomAgentServerSettings>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for AllAgentServersSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AllAgentServersSettings {
    pub fn has_registry_agents(&self) -> bool {
        self.values()
            .any(|s| matches!(s, CustomAgentServerSettings::Registry { .. }))
    }
}

#[derive(Clone, JsonSchema, Debug, PartialEq)]
pub enum CustomAgentServerSettings {
    Custom {
        command: AgentServerCommand,
        /// The default mode to use for this agent.
        ///
        /// Note: Not only all agents support modes.
        ///
        /// Default: None
        default_mode: Option<String>,
        /// Default values for session config options.
        ///
        /// This is a map from config option ID to the default value for that option.
        ///
        /// Default: {}
        default_config_options: HashMap<String, AgentConfigOptionValue>,
        /// Favorited values for session config options.
        ///
        /// This is a map from config option ID to a list of favorited value IDs.
        ///
        /// Default: {}
        favorite_config_option_values: HashMap<String, Vec<String>>,
    },
    Registry {
        /// Additional environment variables to pass to the agent.
        ///
        /// Default: {}
        env: HashMap<String, String>,
        /// The default mode to use for this agent.
        ///
        /// Note: Not only all agents support modes.
        ///
        /// Default: None
        default_mode: Option<String>,
        /// Default values for session config options.
        ///
        /// This is a map from config option ID to the default value for that option.
        ///
        /// Default: {}
        default_config_options: HashMap<String, AgentConfigOptionValue>,
        /// Favorited values for session config options.
        ///
        /// This is a map from config option ID to a list of favorited value IDs.
        ///
        /// Default: {}
        favorite_config_option_values: HashMap<String, Vec<String>>,
    },
}

impl CustomAgentServerSettings {
    pub fn command(&self) -> Option<&AgentServerCommand> {
        match self {
            CustomAgentServerSettings::Custom { command, .. } => Some(command),
            CustomAgentServerSettings::Registry { .. } => None,
        }
    }

    pub fn default_mode(&self) -> Option<&str> {
        match self {
            CustomAgentServerSettings::Custom { default_mode, .. }
            | CustomAgentServerSettings::Registry { default_mode, .. } => default_mode.as_deref(),
        }
    }

    pub fn default_config_option(&self, config_id: &str) -> Option<&AgentConfigOptionValue> {
        match self {
            CustomAgentServerSettings::Custom {
                default_config_options,
                ..
            }
            | CustomAgentServerSettings::Registry {
                default_config_options,
                ..
            } => default_config_options.get(config_id),
        }
    }

    pub fn favorite_config_option_values(&self, config_id: &str) -> Option<&[String]> {
        match self {
            CustomAgentServerSettings::Custom {
                favorite_config_option_values,
                ..
            }
            | CustomAgentServerSettings::Registry {
                favorite_config_option_values,
                ..
            } => favorite_config_option_values
                .get(config_id)
                .map(|v| v.as_slice()),
        }
    }
}

impl From<settings::CustomAgentServerSettings> for CustomAgentServerSettings {
    fn from(value: settings::CustomAgentServerSettings) -> Self {
        match value {
            settings::CustomAgentServerSettings::Custom {
                path,
                args,
                env,
                default_mode,
                default_config_options,
                favorite_config_option_values,
            } => CustomAgentServerSettings::Custom {
                command: AgentServerCommand {
                    path: PathBuf::from(shellexpand::tilde(&path.to_string_lossy()).as_ref()),
                    args,
                    env: Some(env),
                },
                default_mode,
                default_config_options,
                favorite_config_option_values,
            },
            settings::CustomAgentServerSettings::Registry {
                env,
                default_mode,
                default_config_options,
                favorite_config_option_values,
            } => CustomAgentServerSettings::Registry {
                env,
                default_mode,
                default_config_options,
                favorite_config_option_values,
            },
        }
    }
}

impl settings::Settings for AllAgentServersSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let agent_settings = content.agent_servers.clone().unwrap();
        Self(
            agent_settings
                .0
                .into_iter()
                .map(|(k, v)| {
                    (
                        EXTENSION_TO_REGISTRY_IDS
                            .get(&k.as_str())
                            .map(|v| v.to_string())
                            .unwrap_or(k),
                        v.into(),
                    )
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_registry_store::{
        AgentRegistryStore, RegistryAgent, RegistryAgentMetadata, RegistryNpxAgent,
    };
    use crate::worktree_store::{WorktreeIdCounter, WorktreeStore};
    use gpui::TestAppContext;
    #[cfg(feature = "test-support")]
    use http_client::{AsyncBody, FakeHttpClient, Response};
    use node_runtime::NodeRuntime;
    use settings::Settings as _;

    #[cfg(feature = "test-support")]
    const TEST_ARCHIVE_URL: &str = "https://example.test/agent";

    #[cfg(feature = "test-support")]
    fn static_http_client(body: Vec<u8>) -> Arc<dyn HttpClient> {
        FakeHttpClient::create(move |_| {
            let body = body.clone();
            async move {
                Ok(Response::builder()
                    .status(200)
                    .body(AsyncBody::from(body))?)
            }
        })
    }

    #[cfg(feature = "test-support")]
    fn make_registry_archive_agent(
        cx: &mut TestAppContext,
        installation_dir: PathBuf,
        http_client: Arc<dyn HttpClient>,
        sha256: Option<String>,
    ) -> LocalRegistryArchiveAgent {
        let fs: Arc<dyn Fs> = Arc::new(fs::RealFs::new(None, cx.executor()));
        let target = RegistryTargetConfig {
            archive: TEST_ARCHIVE_URL.to_string(),
            cmd: "./agent".to_string(),
            args: Vec::new(),
            sha256,
            env: HashMap::default(),
        };
        let targets = [
            "darwin-aarch64",
            "darwin-x86_64",
            "linux-aarch64",
            "linux-x86_64",
            "windows-aarch64",
            "windows-x86_64",
        ]
        .into_iter()
        .map(|platform| (platform.to_string(), target.clone()))
        .collect();

        cx.update(|cx| {
            let worktree_store =
                cx.new(|cx| WorktreeStore::local(false, fs.clone(), WorktreeIdCounter::get(cx)));
            let project_environment = cx.new(|cx| {
                crate::ProjectEnvironment::new(None, worktree_store.downgrade(), None, false, cx)
            });

            LocalRegistryArchiveAgent {
                fs,
                http_client,
                node_runtime: NodeRuntime::unavailable(),
                project_environment,
                installation_dir,
                version: "1.0.0".into(),
                targets,
                env: HashMap::default(),
                new_version_available_tx: None,
                loading_status_tx: None,
            }
        })
    }

    fn make_npx_agent(id: &str, version: &str) -> RegistryAgent {
        let id = SharedString::from(id.to_string());
        RegistryAgent::Npx(RegistryNpxAgent {
            metadata: RegistryAgentMetadata {
                id: AgentId::new(id.clone()),
                name: id.clone(),
                description: SharedString::from(""),
                version: SharedString::from(version.to_string()),
                repository: None,
                website: None,
                icon_path: None,
            },
            package: id,
            args: Vec::new(),
            env: HashMap::default(),
        })
    }

    fn init_test_settings(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }

    fn init_registry(
        cx: &mut TestAppContext,
        agents: Vec<RegistryAgent>,
    ) -> gpui::Entity<AgentRegistryStore> {
        cx.update(|cx| AgentRegistryStore::init_test_global(cx, agents))
    }

    fn set_registry_settings(cx: &mut TestAppContext, agent_names: &[&str]) {
        cx.update(|cx| {
            AllAgentServersSettings::override_global(
                AllAgentServersSettings(
                    agent_names
                        .iter()
                        .map(|name| {
                            (
                                name.to_string(),
                                settings::CustomAgentServerSettings::Registry {
                                    env: HashMap::default(),
                                    default_mode: None,
                                    default_config_options: HashMap::default(),
                                    favorite_config_option_values: HashMap::default(),
                                }
                                .into(),
                            )
                        })
                        .collect(),
                ),
                cx,
            );
        });
    }

    fn create_agent_server_store(cx: &mut TestAppContext) -> gpui::Entity<AgentServerStore> {
        cx.update(|cx| {
            let fs: Arc<dyn Fs> = fs::FakeFs::new(cx.background_executor().clone());
            let worktree_store =
                cx.new(|cx| WorktreeStore::local(false, fs.clone(), WorktreeIdCounter::get(cx)));
            let project_environment = cx.new(|cx| {
                crate::ProjectEnvironment::new(None, worktree_store.downgrade(), None, false, cx)
            });
            let http_client = http_client::FakeHttpClient::with_404_response();

            cx.new(|cx| {
                AgentServerStore::local(
                    NodeRuntime::unavailable(),
                    fs,
                    project_environment,
                    http_client,
                    cx,
                )
            })
        })
    }

    #[test]
    fn builds_bounded_npm_package_specs() {
        assert_eq!(
            bounded_npm_package_spec("agent-package@1.2.3"),
            ("agent-package", "agent-package@0.0.0 - 1.2.3".to_string())
        );
        assert_eq!(
            bounded_npm_package_spec("@scope/agent-package@1.2.3-beta.1"),
            (
                "@scope/agent-package",
                "@scope/agent-package@0.0.0 - 1.2.3-beta.1".to_string()
            )
        );
        assert_eq!(
            bounded_npm_package_spec("@scope/agent-package"),
            ("@scope/agent-package", "@scope/agent-package".to_string())
        );
        assert_eq!(
            bounded_npm_package_spec("agent-package@latest"),
            ("agent-package", "agent-package@latest".to_string())
        );
    }

    #[test]
    fn orion_code_uses_the_exact_registry_package_version() {
        assert_eq!(
            registry_npm_package_spec(ORION_CODE_AGENT_ID, "@orion-agents/orion-code@0.3.2"),
            (
                "@orion-agents/orion-code",
                "@orion-agents/orion-code@0.3.2".to_string()
            )
        );
        assert_eq!(
            registry_npm_package_spec("another-agent", "agent-package@1.2.3"),
            ("agent-package", "agent-package@0.0.0 - 1.2.3".to_string())
        );
    }

    #[test]
    fn orion_code_managed_environment_is_host_owned() {
        let mut environment = HashMap::from_iter([
            ("ORION_CODE_DISABLE_ENV_FILES".to_string(), "0".to_string()),
            (
                "ORION_CODE_DATA_DIR".to_string(),
                "/untrusted/override".to_string(),
            ),
        ]);

        enforce_orion_code_managed_environment(
            ORION_CODE_AGENT_ID,
            Path::new("/studio-data"),
            &mut environment,
        );

        assert_eq!(
            environment
                .get("ORION_CODE_DISABLE_ENV_FILES")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            environment.get("ORION_CODE_DATA_DIR").map(String::as_str),
            Some("/studio-data/orion-code")
        );
    }

    #[test]
    fn only_reuses_an_exact_cached_orion_code_version() {
        let installed = Version::parse("0.3.2").expect("valid installed version");
        assert!(should_reuse_cached_registry_npx_install(
            ORION_CODE_AGENT_ID,
            "0.3.2",
            Some(&installed)
        ));
        assert!(!should_reuse_cached_registry_npx_install(
            ORION_CODE_AGENT_ID,
            "0.3.3",
            Some(&installed)
        ));
        assert!(!should_reuse_cached_registry_npx_install(
            "another-agent",
            "0.3.2",
            Some(&installed)
        ));
        assert!(!should_reuse_cached_registry_npx_install(
            ORION_CODE_AGENT_ID,
            "latest",
            Some(&installed)
        ));
    }

    #[test]
    fn orion_code_registry_npx_install_directory_is_versioned_and_contained() {
        let external_agents_directory = Path::new("/external-agents");
        assert_eq!(
            registry_npx_install_directory(
                external_agents_directory,
                ORION_CODE_AGENT_ID,
                "0.3.2-beta.1+build.7",
            )
            .expect("valid Orion Code version"),
            external_agents_directory.join("registry/npx/orion-code/0.3.2-beta.1+build.7")
        );
        assert!(
            registry_npx_install_directory(
                external_agents_directory,
                ORION_CODE_AGENT_ID,
                "../0.3.2",
            )
            .is_err()
        );
        assert_eq!(
            registry_npx_install_directory(
                external_agents_directory,
                "another-agent",
                "not-a-semver",
            )
            .expect("non-Orion Registry behavior remains version-independent"),
            external_agents_directory.join("registry/npx/another-agent")
        );
    }

    #[test]
    fn detects_supported_archive_suffixes() {
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.zip"),
            Ok(RegistryArchiveKind::Archive(AssetKind::Zip))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.zip?download=1"),
            Ok(RegistryArchiveKind::Archive(AssetKind::Zip))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tar.gz"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tar.gz?download=1#latest"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tgz"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tgz#download"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarGz))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tar.bz2"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tar.bz2?download=1"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tbz2"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.tbz2#download"),
            Ok(RegistryArchiveKind::Archive(AssetKind::TarBz2))
        ));
        assert!(matches!(
            registry_archive_kind_for_url("https://example.com/agent.ZIP"),
            Ok(RegistryArchiveKind::Archive(AssetKind::Zip))
        ));
    }

    #[test]
    fn detects_raw_binary_archive_urls() {
        assert_eq!(
            registry_archive_kind_for_url("https://x.ai/cli/grok-0.2.20-macos-aarch64").unwrap(),
            RegistryArchiveKind::RawBinary {
                file_name: "grok-0.2.20-macos-aarch64".to_string()
            },
        );
        assert_eq!(
            registry_archive_kind_for_url("https://x.ai/cli/grok-0.2.20-windows-x86_64.exe")
                .unwrap(),
            RegistryArchiveKind::RawBinary {
                file_name: "grok-0.2.20-windows-x86_64.exe".to_string()
            },
        );
        assert_eq!(
            registry_archive_kind_for_url("https://example.com/agent-binary?download=1#latest")
                .unwrap(),
            RegistryArchiveKind::RawBinary {
                file_name: "agent-binary".to_string()
            },
        );
        assert_eq!(
            registry_archive_kind_for_url("https://example.com/agent%20binary").unwrap(),
            RegistryArchiveKind::RawBinary {
                file_name: "agent binary".to_string()
            },
        );
        // No file name to install the binary as.
        assert!(registry_archive_kind_for_url("https://example.com/").is_err());
        // Percent-decoding must not allow path traversal in the file name.
        assert!(registry_archive_kind_for_url("https://example.com/a%2F..%2Fevil").is_err());
        assert!(registry_archive_kind_for_url("https://example.com/%2E%2E").is_err());
    }

    #[test]
    fn parses_github_release_archive_urls() {
        let github_archive = github_release_archive_from_url(
            "https://github.com/owner/repo/releases/download/release%2F2.3.5/agent.tar.bz2?download=1",
        )
        .unwrap();

        assert_eq!(github_archive.repo_name_with_owner, "owner/repo");
        assert_eq!(github_archive.tag, "release/2.3.5");
        assert_eq!(github_archive.asset_name, "agent.tar.bz2");
    }

    #[test]
    fn rejects_unsupported_archive_suffixes() {
        let error = registry_archive_kind_for_url("https://example.com/agent.tar.xz")
            .err()
            .map(|error| error.to_string());

        assert_eq!(
            error,
            Some(
                "unsupported archive type .tar.xz in URL: https://example.com/agent.tar.xz"
                    .to_string()
            ),
        );

        for installer_url in [
            "https://example.com/agent.dmg",
            "https://example.com/agent.pkg",
            "https://example.com/agent.deb",
            "https://example.com/agent.rpm",
            "https://example.com/agent.msi",
            "https://example.com/agent.AppImage",
        ] {
            assert!(
                registry_archive_kind_for_url(installer_url).is_err(),
                "expected {installer_url} to be rejected"
            );
        }
    }

    #[test]
    fn versioned_archive_cache_dir_includes_artifact_identity() {
        let slash_version_dir = versioned_archive_cache_dir(
            Path::new("/tmp/agents"),
            Some("release/2.3.5"),
            "https://example.com/agent.zip",
            None,
        );
        let colon_version_dir = versioned_archive_cache_dir(
            Path::new("/tmp/agents"),
            Some("release:2.3.5"),
            "https://example.com/agent.zip",
            None,
        );
        let file_name = slash_version_dir
            .file_name()
            .and_then(|name| name.to_str())
            .expect("cache directory should have a file name");

        assert!(file_name.starts_with("v_release-2.3.5_"));
        assert_ne!(slash_version_dir, colon_version_dir);

        let lowercase_checksum_dir = versioned_archive_cache_dir(
            Path::new("/tmp/agents"),
            Some("release/2.3.5"),
            "https://example.com/agent.zip",
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let uppercase_checksum_dir = versioned_archive_cache_dir(
            Path::new("/tmp/agents"),
            Some("release/2.3.5"),
            "https://example.com/agent.zip",
            Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        );
        let changed_checksum_dir = versioned_archive_cache_dir(
            Path::new("/tmp/agents"),
            Some("release/2.3.5"),
            "https://example.com/agent.zip",
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        );

        assert_ne!(slash_version_dir, lowercase_checksum_dir);
        assert_eq!(lowercase_checksum_dir, uppercase_checksum_dir);
        assert_ne!(lowercase_checksum_dir, changed_checksum_dir);
    }

    #[cfg(feature = "test-support")]
    #[gpui::test]
    async fn registry_raw_binary_checksum_invalidates_unverified_cache_and_blocks_mismatch(
        cx: &mut TestAppContext,
    ) {
        init_test_settings(cx);
        cx.executor().allow_parking();
        let temp_dir = tempfile::tempdir().unwrap();
        let installation_dir = temp_dir.path().join("agent");
        let old_version_dir =
            versioned_archive_cache_dir(&installation_dir, Some("1.0.0"), TEST_ARCHIVE_URL, None);
        std::fs::create_dir_all(&old_version_dir).unwrap();
        std::fs::write(old_version_dir.join("agent"), b"unverified agent").unwrap();

        let expected_sha256 = "0000000000000000000000000000000000000000000000000000000000000000";
        let http_client = static_http_client(b"unexpected agent".to_vec());
        let mut agent = make_registry_archive_agent(
            cx,
            installation_dir.clone(),
            http_client,
            Some(expected_sha256.to_string()),
        );
        let get_command =
            cx.update(|cx| agent.get_command(Vec::new(), HashMap::default(), &mut cx.to_async()));

        let error = get_command.await.unwrap_err();
        assert!(
            error.to_string().contains("SHA-256 mismatch"),
            "unexpected error: {error:#}"
        );
        assert!(old_version_dir.exists());
        assert!(
            !versioned_archive_cache_dir(
                &installation_dir,
                Some("1.0.0"),
                TEST_ARCHIVE_URL,
                Some(expected_sha256),
            )
            .exists()
        );
    }

    #[cfg(feature = "test-support")]
    #[gpui::test]
    async fn registry_raw_binary_with_checksum_installs(cx: &mut TestAppContext) {
        init_test_settings(cx);
        cx.executor().allow_parking();
        let temp_dir = tempfile::tempdir().unwrap();
        let installation_dir = temp_dir.path().join("agent");
        let contents = b"verified agent";
        let expected_sha256 = format!("{:X}", Sha256::digest(contents));
        let http_client = static_http_client(contents.to_vec());
        let mut agent = make_registry_archive_agent(
            cx,
            installation_dir.clone(),
            http_client,
            Some(expected_sha256.clone()),
        );
        let get_command =
            cx.update(|cx| agent.get_command(Vec::new(), HashMap::default(), &mut cx.to_async()));

        let command = get_command.await.unwrap();
        cx.run_until_parked();
        assert_eq!(
            command.path,
            versioned_archive_cache_dir(
                &installation_dir,
                Some("1.0.0"),
                TEST_ARCHIVE_URL,
                Some(&expected_sha256),
            )
            .join("agent")
        );
        assert_eq!(std::fs::read(command.path).unwrap(), contents);
    }

    #[cfg(feature = "test-support")]
    #[gpui::test]
    async fn registry_raw_binary_without_checksum_installs(cx: &mut TestAppContext) {
        init_test_settings(cx);
        cx.executor().allow_parking();
        let temp_dir = tempfile::tempdir().unwrap();
        let installation_dir = temp_dir.path().join("agent");
        let contents = b"unchecked agent";
        let http_client = static_http_client(contents.to_vec());
        let mut agent =
            make_registry_archive_agent(cx, installation_dir.clone(), http_client, None);
        let get_command =
            cx.update(|cx| agent.get_command(Vec::new(), HashMap::default(), &mut cx.to_async()));

        let command = get_command.await.unwrap();
        cx.run_until_parked();
        assert_eq!(
            command.path,
            versioned_archive_cache_dir(&installation_dir, Some("1.0.0"), TEST_ARCHIVE_URL, None,)
                .join("agent")
        );
        assert_eq!(std::fs::read(command.path).unwrap(), contents);
    }

    #[gpui::test]
    async fn test_remove_stale_versioned_archive_cache_dirs(cx: &mut TestAppContext) {
        let fs = fs::FakeFs::new(cx.executor());
        let base_dir = Path::new("/cache");

        // FakeFs increments mtime on every create, so creation order is
        // ascending mtime: v_old_1 < v_old_2 < other < v_not_a_dir < v_current < v_newer.
        fs.insert_tree(
            base_dir,
            serde_json::json!({
                "v_old_1": {},
                "v_old_2": {},
                "other": {},
            }),
        )
        .await;
        fs.insert_file(base_dir.join("v_not_a_dir"), b"keep me".to_vec())
            .await;
        let current_version_dir = base_dir.join("v_current");
        fs.create_dir(&current_version_dir).await.unwrap();
        // Sibling that "finished extracting" after the current dir was cached.
        fs.create_dir(&base_dir.join("v_newer")).await.unwrap();

        remove_stale_versioned_archive_cache_dirs(
            fs.clone() as Arc<dyn Fs>,
            base_dir,
            &current_version_dir,
        )
        .await
        .unwrap();

        let mut remaining = fs
            .read_dir(base_dir)
            .await
            .unwrap()
            .filter_map(|entry| async move { entry.ok() })
            .map(|path| {
                path.file_name()
                    .expect("entry has a name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>()
            .await;
        remaining.sort();

        assert_eq!(
            remaining,
            vec![
                "other".to_string(),
                "v_current".to_string(),
                "v_newer".to_string(),
                "v_not_a_dir".to_string(),
            ]
        );
    }

    #[gpui::test]
    async fn orion_code_versioned_installs_preserve_existing_versions(cx: &mut TestAppContext) {
        let fs = fs::FakeFs::new(cx.executor());
        let external_agents_directory = PathBuf::from("/external-agents");
        fs.create_dir(&external_agents_directory)
            .await
            .expect("create managed Agent root");
        let old_install = registry_npx_install_directory(
            &external_agents_directory,
            ORION_CODE_AGENT_ID,
            "0.3.2",
        )
        .expect("old version path");
        prepare_orion_code_registry_npx_install_directory(
            fs.as_ref(),
            &external_agents_directory,
            &old_install,
        )
        .await
        .expect("prepare old version");
        fs.insert_file(old_install.join("verified"), b"keep".to_vec())
            .await;

        let new_install = registry_npx_install_directory(
            &external_agents_directory,
            ORION_CODE_AGENT_ID,
            "0.3.3",
        )
        .expect("new version path");
        prepare_orion_code_registry_npx_install_directory(
            fs.as_ref(),
            &external_agents_directory,
            &new_install,
        )
        .await
        .expect("prepare new version");

        assert_ne!(old_install, new_install);
        assert!(fs.is_file(&old_install.join("verified")).await);
        assert!(fs.is_dir(&new_install).await);
    }

    #[gpui::test]
    async fn refuses_to_prepare_orion_code_install_through_a_symlink(cx: &mut TestAppContext) {
        let fs = fs::FakeFs::new(cx.executor());
        let external_agents_directory = PathBuf::from("/external-agents");
        let outside_directory = PathBuf::from("/outside");
        fs.create_dir(&external_agents_directory.join("registry/npx"))
            .await
            .expect("create managed npx directory");
        fs.create_dir(&outside_directory)
            .await
            .expect("create outside directory");
        fs.create_symlink(
            &external_agents_directory.join("registry/npx/orion-code"),
            outside_directory.clone(),
        )
        .await
        .expect("create malicious private-root symlink");
        let install_directory = registry_npx_install_directory(
            &external_agents_directory,
            ORION_CODE_AGENT_ID,
            "0.3.2",
        )
        .expect("version path");

        let error = prepare_orion_code_registry_npx_install_directory(
            fs.as_ref(),
            &external_agents_directory,
            &install_directory,
        )
        .await
        .expect_err("private-root symlink must be rejected");

        assert!(error.to_string().contains("symlink"));
        assert!(!fs.is_dir(&outside_directory.join("0.3.2")).await);
    }

    #[gpui::test]
    async fn removes_only_orion_code_private_version_directories(cx: &mut TestAppContext) {
        let fs = fs::FakeFs::new(cx.executor());
        let external_agents_directory = PathBuf::from("/external-agents");
        fs.insert_tree(
            &external_agents_directory,
            serde_json::json!({
                "registry": {
                    "orion-code": { "archive": "keep" },
                    "another-agent": { "archive": "keep" },
                    "npx": {
                        "orion-code": {
                            "0.3.1": { "package.json": "managed" },
                            "0.3.2-beta.1+build.7": { "package.json": "managed" },
                            "current": { "package.json": "keep" },
                            "notes.txt": "keep"
                        },
                        "another-agent": { "package.json": "keep" }
                    }
                }
            }),
        )
        .await;

        remove_orion_code_managed_install_from(fs.clone(), external_agents_directory.clone())
            .await
            .expect("remove exact Orion Code managed directories");

        assert!(
            fs.is_dir(&external_agents_directory.join("registry/orion-code"))
                .await
        );
        assert!(
            fs.is_dir(&external_agents_directory.join("registry/another-agent"))
                .await
        );
        assert!(
            !fs.is_dir(&external_agents_directory.join("registry/npx/orion-code/0.3.1"))
                .await
        );
        assert!(
            !fs.is_dir(
                &external_agents_directory.join("registry/npx/orion-code/0.3.2-beta.1+build.7")
            )
            .await
        );
        assert!(
            fs.is_dir(&external_agents_directory.join("registry/npx/orion-code/current"))
                .await
        );
        assert!(
            fs.is_file(&external_agents_directory.join("registry/npx/orion-code/notes.txt"))
                .await
        );
        assert!(
            fs.is_dir(&external_agents_directory.join("registry/npx/another-agent"))
                .await
        );
    }

    #[gpui::test]
    async fn refuses_to_remove_orion_code_managed_install_through_a_symlink(
        cx: &mut TestAppContext,
    ) {
        let fs = fs::FakeFs::new(cx.executor());
        let external_agents_directory = PathBuf::from("/external-agents");
        let outside_directory = PathBuf::from("/outside");
        fs.create_dir(&external_agents_directory.join("registry"))
            .await
            .expect("create Registry directory");
        fs.create_dir(&external_agents_directory.join("registry/npx/orion-code"))
            .await
            .expect("create Orion Code versions directory");
        fs.create_dir(&outside_directory)
            .await
            .expect("create outside directory");
        fs.insert_file(outside_directory.join("keep"), b"keep".to_vec())
            .await;
        fs.create_symlink(
            &external_agents_directory.join("registry/npx/orion-code/0.3.2"),
            outside_directory.clone(),
        )
        .await
        .expect("create malicious managed-install symlink");

        let error = remove_orion_code_managed_install_from(fs.clone(), external_agents_directory)
            .await
            .expect_err("managed install symlink must be rejected");

        assert!(error.to_string().contains("symlink"));
        assert!(fs.is_file(&outside_directory.join("keep")).await);
    }

    #[gpui::test]
    async fn orion_code_refuses_symlinks_in_intermediate_managed_install_directories(
        cx: &mut TestAppContext,
    ) {
        let fs = fs::FakeFs::new(cx.executor());
        let registry_symlink_root = PathBuf::from("/registry-symlink");
        fs.create_dir(&registry_symlink_root)
            .await
            .expect("create first managed root");
        fs.insert_tree(
            &registry_symlink_root.join("alternate"),
            serde_json::json!({ "orion-code": { "keep": "data" } }),
        )
        .await;
        fs.create_symlink(
            &registry_symlink_root.join("registry"),
            registry_symlink_root.join("alternate"),
        )
        .await
        .expect("create Registry symlink");

        let error =
            remove_orion_code_managed_install_from(fs.clone(), registry_symlink_root.clone())
                .await
                .expect_err("Registry symlink must be rejected");
        assert!(error.to_string().contains("symlink"));
        assert!(
            fs.is_file(&registry_symlink_root.join("alternate/orion-code/keep"))
                .await
        );

        let npx_symlink_root = PathBuf::from("/npx-symlink");
        fs.create_dir(&npx_symlink_root.join("registry"))
            .await
            .expect("create second Registry directory");
        fs.insert_tree(
            &npx_symlink_root.join("alternate"),
            serde_json::json!({ "orion-code": { "keep": "data" } }),
        )
        .await;
        fs.create_symlink(
            &npx_symlink_root.join("registry/npx"),
            npx_symlink_root.join("alternate"),
        )
        .await
        .expect("create npx symlink");

        let error = remove_orion_code_managed_install_from(fs.clone(), npx_symlink_root.clone())
            .await
            .expect_err("npx symlink must be rejected");
        assert!(error.to_string().contains("symlink"));
        assert!(
            fs.is_file(&npx_symlink_root.join("alternate/orion-code/keep"))
                .await
        );
    }

    #[gpui::test]
    fn test_version_change_sends_notification(cx: &mut TestAppContext) {
        init_test_settings(cx);
        let registry = init_registry(cx, vec![make_npx_agent("test-agent", "1.0.0")]);
        set_registry_settings(cx, &["test-agent"]);
        let store = create_agent_server_store(cx);

        // Verify the agent was registered with version 1.0.0.
        store.read_with(cx, |store, _| {
            let entry = store
                .external_agents
                .get(&AgentId::new("test-agent"))
                .expect("agent should be registered");
            assert_eq!(
                entry.server.version().map(|v| v.to_string()),
                Some("1.0.0".to_string())
            );
        });

        // Set up a watch channel and store the tx on the agent.
        let (tx, mut rx) = watch::channel::<Option<String>>(None);
        store.update(cx, |store, _| {
            let entry = store
                .external_agents
                .get_mut(&AgentId::new("test-agent"))
                .expect("agent should be registered");
            entry.server.set_new_version_available_tx(tx);
        });

        // Update the registry to version 2.0.0.
        registry.update(cx, |store, cx| {
            store.set_agents(vec![make_npx_agent("test-agent", "2.0.0")], cx);
        });
        cx.run_until_parked();

        // The watch channel should have received the new version.
        assert_eq!(rx.borrow().as_deref(), Some("2.0.0"));
    }

    #[gpui::test]
    fn test_same_version_preserves_tx(cx: &mut TestAppContext) {
        init_test_settings(cx);
        let registry = init_registry(cx, vec![make_npx_agent("test-agent", "1.0.0")]);
        set_registry_settings(cx, &["test-agent"]);
        let store = create_agent_server_store(cx);

        let (tx, mut rx) = watch::channel::<Option<String>>(None);
        store.update(cx, |store, _| {
            let entry = store
                .external_agents
                .get_mut(&AgentId::new("test-agent"))
                .expect("agent should be registered");
            entry.server.set_new_version_available_tx(tx);
        });

        // "Refresh" the registry with the same version.
        registry.update(cx, |store, cx| {
            store.set_agents(vec![make_npx_agent("test-agent", "1.0.0")], cx);
        });
        cx.run_until_parked();

        // No notification should have been sent.
        assert_eq!(rx.borrow().as_deref(), None);

        // The tx should have been transferred to the rebuilt agent entry.
        store.update(cx, |store, _| {
            let entry = store
                .external_agents
                .get_mut(&AgentId::new("test-agent"))
                .expect("agent should be registered");
            assert!(
                entry.server.take_new_version_available_tx().is_some(),
                "tx should have been transferred to the rebuilt agent"
            );
        });
    }

    #[gpui::test]
    fn test_no_tx_stored_does_not_panic_on_version_change(cx: &mut TestAppContext) {
        init_test_settings(cx);
        let registry = init_registry(cx, vec![make_npx_agent("test-agent", "1.0.0")]);
        set_registry_settings(cx, &["test-agent"]);
        let _store = create_agent_server_store(cx);

        // Update the registry without having stored any tx — should not panic.
        registry.update(cx, |store, cx| {
            store.set_agents(vec![make_npx_agent("test-agent", "2.0.0")], cx);
        });
        cx.run_until_parked();
    }

    #[gpui::test]
    fn test_multiple_agents_independent_notifications(cx: &mut TestAppContext) {
        init_test_settings(cx);
        let registry = init_registry(
            cx,
            vec![
                make_npx_agent("agent-a", "1.0.0"),
                make_npx_agent("agent-b", "3.0.0"),
            ],
        );
        set_registry_settings(cx, &["agent-a", "agent-b"]);
        let store = create_agent_server_store(cx);

        let (tx_a, mut rx_a) = watch::channel::<Option<String>>(None);
        let (tx_b, mut rx_b) = watch::channel::<Option<String>>(None);
        store.update(cx, |store, _| {
            store
                .external_agents
                .get_mut(&AgentId::new("agent-a"))
                .expect("agent-a should be registered")
                .server
                .set_new_version_available_tx(tx_a);
            store
                .external_agents
                .get_mut(&AgentId::new("agent-b"))
                .expect("agent-b should be registered")
                .server
                .set_new_version_available_tx(tx_b);
        });

        // Update only agent-a to a new version; agent-b stays the same.
        registry.update(cx, |store, cx| {
            store.set_agents(
                vec![
                    make_npx_agent("agent-a", "2.0.0"),
                    make_npx_agent("agent-b", "3.0.0"),
                ],
                cx,
            );
        });
        cx.run_until_parked();

        // agent-a should have received a notification.
        assert_eq!(rx_a.borrow().as_deref(), Some("2.0.0"));

        // agent-b should NOT have received a notification.
        assert_eq!(rx_b.borrow().as_deref(), None);

        // agent-b's tx should have been transferred.
        store.update(cx, |store, _| {
            assert!(
                store
                    .external_agents
                    .get_mut(&AgentId::new("agent-b"))
                    .expect("agent-b should be registered")
                    .server
                    .take_new_version_available_tx()
                    .is_some(),
                "agent-b tx should have been transferred"
            );
        });
    }
}
