use std::rc::Rc;

use acp_thread::{AgentConnection, LoadError};
use agent_servers::AcpConnection;
use agent_servers::{AgentServer, AgentServerDelegate};
use anyhow::Result;
use collections::{HashMap, HashSet};
use futures::{FutureExt, channel::oneshot, future::Shared};
use gpui::{
    App, AppContext, BorrowAppContext, Context, Entity, EventEmitter, Global, SharedString,
    Subscription, Task, TaskExt, WeakEntity,
};

use project::agent_registry_store::ORION_CODE_AGENT_ID;
use project::{AgentRegistryStore, AgentServerStore, AgentServersUpdated, Project};
use watch::Receiver;

use crate::{
    Agent, orion_code_update::current_orion_code_release_is_revoked,
    orion_code_update_coordinator::OrionCodeUpdateCoordinator,
};

#[path = "orion_code_update_activity.rs"]
pub mod orion_code_update_activity;

pub use orion_code_update_activity::{
    OrionCodeActivityError, OrionCodeActivityKind, OrionCodeActivityLease, OrionCodeActivityOwner,
    OrionCodeActivitySnapshot, OrionCodeUpdateActivity,
};

pub enum AgentConnectionEntry {
    Connecting {
        connect_task: Shared<Task<Result<AgentConnectedState, LoadError>>>,
        cancel_connection_tx: Option<oneshot::Sender<()>>,
        server_version: Option<SharedString>,
    },
    Connected(AgentConnectedState),
    Error {
        error: LoadError,
    },
}

#[derive(Clone)]
pub struct AgentConnectedState {
    pub connection: Rc<dyn AgentConnection>,
    server_version: Option<SharedString>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

impl AgentConnectionEntry {
    pub fn wait_for_connection(&self) -> Shared<Task<Result<AgentConnectedState, LoadError>>> {
        match self {
            AgentConnectionEntry::Connecting { connect_task, .. } => connect_task.clone(),
            AgentConnectionEntry::Connected(state) => Task::ready(Ok(state.clone())).shared(),
            AgentConnectionEntry::Error { error } => Task::ready(Err(error.clone())).shared(),
        }
    }

    pub fn status(&self) -> AgentConnectionStatus {
        match self {
            AgentConnectionEntry::Connecting { .. } => AgentConnectionStatus::Connecting,
            AgentConnectionEntry::Connected(_) => AgentConnectionStatus::Connected,
            AgentConnectionEntry::Error { .. } => AgentConnectionStatus::Disconnected,
        }
    }

    fn server_version(&self) -> Option<&SharedString> {
        match self {
            AgentConnectionEntry::Connecting { server_version, .. } => server_version.as_ref(),
            AgentConnectionEntry::Connected(state) => state.server_version.as_ref(),
            AgentConnectionEntry::Error { .. } => None,
        }
    }
}

pub enum AgentConnectionEntryEvent {
    NewVersionAvailable(SharedString),
    LoadingStatusChanged(Option<SharedString>),
}

impl EventEmitter<AgentConnectionEntryEvent> for AgentConnectionEntry {}

#[derive(Clone)]
pub struct ActiveAcpConnection {
    pub agent_id: project::AgentId,
    pub connection: Rc<AcpConnection>,
}

pub struct AgentConnectionStore {
    project: Entity<Project>,
    entries: HashMap<Agent, Entity<AgentConnectionEntry>>,
    orion_code_shutdown_barrier: Option<Shared<Task<Result<(), SharedString>>>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct OrionCodeConnectionRegistry {
    stores: Vec<WeakEntity<AgentConnectionStore>>,
}

impl Global for OrionCodeConnectionRegistry {}

impl AgentConnectionStore {
    pub fn new(project: Entity<Project>, cx: &mut Context<Self>) -> Self {
        let agent_server_store = project.read(cx).agent_server_store().clone();
        let subscription = cx.subscribe(&agent_server_store, Self::handle_agent_servers_updated);
        if !cx.has_global::<OrionCodeConnectionRegistry>() {
            cx.set_global(OrionCodeConnectionRegistry::default());
        }
        let this = cx.weak_entity();
        cx.update_global::<OrionCodeConnectionRegistry, _>(|registry, _cx| {
            registry.stores.retain(|store| store.upgrade().is_some());
            registry.stores.push(this);
        });
        Self {
            project,
            entries: HashMap::default(),
            orion_code_shutdown_barrier: None,
            _subscriptions: vec![subscription],
        }
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    pub fn entry(&self, key: &Agent) -> Option<&Entity<AgentConnectionEntry>> {
        self.entries.get(key)
    }

    pub fn connection_status(&self, key: &Agent, cx: &App) -> AgentConnectionStatus {
        self.entries
            .get(key)
            .map(|entry| entry.read(cx).status())
            .unwrap_or(AgentConnectionStatus::Disconnected)
    }

    pub fn agent_version(&self, key: &Agent, cx: &App) -> Option<SharedString> {
        match self.entries.get(key)?.read(cx) {
            AgentConnectionEntry::Connected(state) => state.connection.agent_version(),
            AgentConnectionEntry::Connecting { .. } | AgentConnectionEntry::Error { .. } => None,
        }
    }

    pub fn active_acp_connections(&self, cx: &App) -> Vec<ActiveAcpConnection> {
        self.entries
            .values()
            .filter_map(|entry| match entry.read(cx) {
                AgentConnectionEntry::Connected(state) => state
                    .connection
                    .clone()
                    .downcast::<AcpConnection>()
                    .map(|connection| ActiveAcpConnection {
                        agent_id: state.connection.agent_id(),
                        connection,
                    }),
                AgentConnectionEntry::Connecting { .. } | AgentConnectionEntry::Error { .. } => {
                    None
                }
            })
            .collect()
    }

    pub fn restart_connection(
        &mut self,
        key: Agent,
        server: Rc<dyn AgentServer>,
        cx: &mut Context<Self>,
    ) -> Entity<AgentConnectionEntry> {
        if is_orion_code_agent(&key) && orion_code_new_connections_are_blocked(cx) {
            return revoked_orion_code_connection_entry(cx);
        }
        if let Some(entry) = self.entries.get(&key) {
            if matches!(entry.read(cx), AgentConnectionEntry::Connecting { .. }) {
                return entry.clone();
            }
        }

        self.entries.remove(&key);
        self.request_connection(key, server, cx)
    }

    pub fn request_connection(
        &mut self,
        key: Agent,
        server: Rc<dyn AgentServer>,
        cx: &mut Context<Self>,
    ) -> Entity<AgentConnectionEntry> {
        self.request_connection_with_version(key, server, None, cx)
    }

    pub fn request_connection_for_version(
        &mut self,
        key: Agent,
        server: Rc<dyn AgentServer>,
        server_version: SharedString,
        cx: &mut Context<Self>,
    ) -> Entity<AgentConnectionEntry> {
        self.request_connection_with_version(key, server, Some(server_version), cx)
    }

    fn request_connection_with_version(
        &mut self,
        key: Agent,
        server: Rc<dyn AgentServer>,
        server_version: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> Entity<AgentConnectionEntry> {
        if is_orion_code_agent(&key) && orion_code_new_connections_are_blocked(cx) {
            return revoked_orion_code_connection_entry(cx);
        }
        let version_changed = server_version.as_ref().is_some_and(|server_version| {
            self.entries
                .get(&key)
                .is_some_and(|entry| entry.read(cx).server_version() != Some(server_version))
        });
        if !version_changed {
            if let Some(entry) = self.entries.get(&key) {
                return entry.clone();
            }
        }

        let shutdown_previous = if version_changed {
            self.invalidate_current_entry(
                &key,
                "Agent connection was replaced by a different server version.",
                cx,
            )
        } else if is_orion_code_agent(&key) {
            self.orion_code_shutdown_barrier_task(cx)
        } else {
            None
        };
        let startup_activity = is_orion_code_agent(&key)
            .then(|| acquire_orion_code_activity(OrionCodeActivityKind::Startup, cx))
            .flatten();
        let version_switch_activity = (version_changed && is_orion_code_agent(&key))
            .then(|| acquire_orion_code_activity(OrionCodeActivityKind::VersionSwitch, cx))
            .flatten();

        let (cancel_connection_tx, cancel_connection_rx) = oneshot::channel();
        let (mut new_version_rx, mut loading_status_rx, connect_task) = self.start_connection(
            server,
            server_version.clone(),
            shutdown_previous,
            startup_activity,
            version_switch_activity,
            cancel_connection_rx,
            cx,
        );
        let connect_task = connect_task.shared();

        let entry = cx.new(|_cx| AgentConnectionEntry::Connecting {
            connect_task: connect_task.clone(),
            cancel_connection_tx: Some(cancel_connection_tx),
            server_version,
        });

        self.entries.insert(key.clone(), entry.clone());
        cx.notify();

        cx.spawn({
            let key = key.clone();
            let entry = entry.downgrade();
            async move |this, cx| match connect_task.await {
                Ok(connected_state) => {
                    this.update(cx, move |this, cx| {
                        if this.entries.get(&key) != entry.upgrade().as_ref() {
                            return;
                        }

                        entry
                            .update(cx, move |entry, cx| {
                                if let AgentConnectionEntry::Connecting { .. } = entry {
                                    *entry = AgentConnectionEntry::Connected(connected_state);
                                    cx.notify();
                                }
                            })
                            .ok();
                        cx.notify();
                    })
                    .ok();
                }
                Err(error) => {
                    this.update(cx, move |this, cx| {
                        if this.entries.get(&key) != entry.upgrade().as_ref() {
                            return;
                        }

                        entry
                            .update(cx, move |entry, cx| {
                                if let AgentConnectionEntry::Connecting { .. } = entry {
                                    *entry = AgentConnectionEntry::Error { error };
                                    cx.notify();
                                }
                            })
                            .ok();
                        this.entries.remove(&key);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();

        cx.spawn({
            let key = key.clone();
            let entry = entry.downgrade();
            async move |this, cx| {
                while let Ok(version) = new_version_rx.recv().await {
                    let Some(version) = version else {
                        continue;
                    };

                    this.update(cx, move |this, cx| {
                        if this.entries.get(&key) != entry.upgrade().as_ref() {
                            return;
                        }

                        entry
                            .update(cx, move |_entry, cx| {
                                cx.emit(AgentConnectionEntryEvent::NewVersionAvailable(
                                    version.into(),
                                ));
                            })
                            .ok();
                        if is_orion_code_agent(&key) {
                            if let Some(shutdown_task) = this.invalidate_current_entry(
                                &key,
                                "Orion Code connection was replaced by a newer version.",
                                cx,
                            ) {
                                shutdown_task.detach_and_log_err(cx);
                            }
                        } else {
                            this.entries.remove(&key);
                            cx.notify();
                        }
                    })
                    .ok();
                    break;
                }
            }
        })
        .detach();

        cx.spawn({
            let entry = entry.downgrade();
            async move |this, cx| {
                while let Ok(status) = loading_status_rx.recv().await {
                    let status = status.map(SharedString::from);
                    let key = key.clone();
                    let entry = entry.clone();
                    this.update(cx, move |this, cx| {
                        if this.entries.get(&key) != entry.upgrade().as_ref() {
                            return;
                        }

                        entry
                            .update(cx, move |_entry, cx| {
                                cx.emit(AgentConnectionEntryEvent::LoadingStatusChanged(status));
                            })
                            .ok();
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();

        entry
    }

    /// Invalidates `expected_entry` only if it is still the current connection attempt for `key`.
    ///
    /// Replacing the entry before removing it drops the connection task held by the entry and
    /// wakes the completion observer so it drops its copy. Once external waiters release their
    /// copies, dropping the final task handle cancels the in-flight connection and its child.
    pub fn invalidate_connection_attempt(
        &mut self,
        key: &Agent,
        expected_entry: &Entity<AgentConnectionEntry>,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let Some(current_entry) = self.entries.get(key).cloned() else {
            return None;
        };
        if &current_entry != expected_entry {
            return None;
        }

        self.invalidate_current_entry(key, "Agent connection attempt was invalidated.", cx)
    }

    fn invalidate_current_entry(
        &mut self,
        key: &Agent,
        reason: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        let current_entry = self.entries.get(key)?.clone();
        let shutdown_task = match current_entry.read(cx) {
            AgentConnectionEntry::Connecting { connect_task, .. } => {
                let connect_task = connect_task.clone();
                Some(cx.spawn(async move |_this, cx| match connect_task.await {
                    Ok(connected_state) => shutdown_connected_state(connected_state, cx).await,
                    Err(_) => Ok(()),
                }))
            }
            AgentConnectionEntry::Connected(connected_state) => {
                let connected_state = connected_state.clone();
                Some(cx.spawn(async move |_this, cx| {
                    shutdown_connected_state(connected_state, cx).await
                }))
            }
            AgentConnectionEntry::Error { .. } => None,
        };

        current_entry.update(cx, |entry, cx| {
            if let AgentConnectionEntry::Connecting {
                cancel_connection_tx,
                ..
            } = entry
            {
                if cancel_connection_tx
                    .take()
                    .is_some_and(|sender| sender.send(()).is_err())
                {
                    log::debug!("connection cancellation receiver was already closed");
                }
            }
            *entry = AgentConnectionEntry::Error {
                error: LoadError::Other(reason.into()),
            };
            cx.notify();
        });
        self.entries.remove(key);
        cx.notify();
        if is_orion_code_agent(key) {
            shutdown_task.map(|shutdown_task| self.track_orion_code_shutdown(shutdown_task, cx))
        } else {
            shutdown_task
        }
    }

    fn track_orion_code_shutdown(
        &mut self,
        shutdown_task: Task<Result<()>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let shutdown_activity = acquire_orion_code_activity(OrionCodeActivityKind::Shutdown, cx);
        let shared_shutdown = cx
            .spawn(async move |_this, _cx| {
                let _shutdown_activity = shutdown_activity;
                shutdown_task
                    .await
                    .map_err(|error| SharedString::from(error.to_string()))
            })
            .shared();
        self.orion_code_shutdown_barrier = Some(shared_shutdown.clone());
        cx.spawn(async move |_this, _cx| {
            shared_shutdown
                .await
                .map_err(|error| anyhow::Error::msg(error.to_string()))
        })
    }

    fn orion_code_shutdown_barrier_task(&self, cx: &mut Context<Self>) -> Option<Task<Result<()>>> {
        let shared_shutdown = self.orion_code_shutdown_barrier.clone()?;
        Some(cx.spawn(async move |_this, _cx| {
            shared_shutdown
                .await
                .map_err(|error| anyhow::Error::msg(error.to_string()))
        }))
    }

    fn shutdown_orion_code(&mut self, cx: &mut Context<Self>) -> Option<Task<Result<()>>> {
        let orion_code = Agent::Custom {
            id: project::AgentId::new(ORION_CODE_AGENT_ID),
        };
        self.invalidate_current_entry(
            &orion_code,
            "Orion Code was shut down before uninstalling.",
            cx,
        )
        .or_else(|| self.orion_code_shutdown_barrier_task(cx))
    }

    fn handle_agent_servers_updated(
        &mut self,
        store: Entity<AgentServerStore>,
        _: &AgentServersUpdated,
        cx: &mut Context<Self>,
    ) {
        let store = store.read(cx);
        let registered_agents: HashSet<_> = store.external_agents.keys().cloned().collect();
        let orion_code = Agent::Custom {
            id: project::AgentId::new(ORION_CODE_AGENT_ID),
        };
        let orion_registry_version = AgentRegistryStore::try_global(cx).and_then(|registry| {
            registry
                .read(cx)
                .agents()
                .iter()
                .find(|agent| agent.id().as_ref() == ORION_CODE_AGENT_ID && agent.is_installable())
                .map(|agent| agent.version().clone())
        });
        let revoked_version_is_draining = orion_code_new_connections_are_blocked(cx);
        let orion_connection_is_stale = !revoked_version_is_draining
            && self.entries.get(&orion_code).is_some_and(|entry| {
                entry.read(cx).server_version() != orion_registry_version.as_ref()
            });
        if orion_connection_is_stale {
            if let Some(shutdown_task) = self.invalidate_current_entry(
                &orion_code,
                "Orion Code registry version changed.",
                cx,
            ) {
                shutdown_task.detach_and_log_err(cx);
            }
        }
        self.entries.retain(|key, _| match key {
            Agent::NativeAgent => true,
            Agent::Custom { id } => {
                (revoked_version_is_draining && is_orion_code_agent_id(id))
                    || registered_agents.contains(id)
            }
            #[cfg(any(test, feature = "test-support"))]
            Agent::Stub => true,
        });
        cx.notify();
    }

    fn start_connection(
        &self,
        server: Rc<dyn AgentServer>,
        server_version: Option<SharedString>,
        shutdown_previous: Option<Task<Result<()>>>,
        startup_activity: Option<OrionCodeActivityLease>,
        version_switch_activity: Option<OrionCodeActivityLease>,
        cancel_connection_rx: oneshot::Receiver<()>,
        cx: &mut Context<Self>,
    ) -> (
        Receiver<Option<String>>,
        Receiver<Option<String>>,
        Task<Result<AgentConnectedState, LoadError>>,
    ) {
        let (new_version_tx, new_version_rx) = watch::channel::<Option<String>>(None);
        let (loading_status_tx, loading_status_rx) = watch::channel::<Option<String>>(None);

        let agent_server_store = self.project.read(cx).agent_server_store().clone();
        let delegate = AgentServerDelegate::new(
            agent_server_store,
            Some(new_version_tx),
            Some(loading_status_tx),
        );

        let project = self.project.clone();
        let connect_task = cx.spawn(async move |_this, cx| {
            let _startup_activity = startup_activity;
            let _version_switch_activity = version_switch_activity;
            let mut cancel_connection_rx = cancel_connection_rx.boxed_local();
            if let Some(shutdown_previous) = shutdown_previous {
                match futures::future::select(shutdown_previous, cancel_connection_rx).await {
                    futures::future::Either::Left((shutdown_result, remaining_cancel)) => {
                        shutdown_result.map_err(|error| {
                            LoadError::Other(SharedString::from(error.to_string()))
                        })?;
                        cancel_connection_rx = remaining_cancel;
                    }
                    futures::future::Either::Right((_cancelled, _shutdown_previous)) => {
                        return Err(LoadError::Other("Agent connection was cancelled.".into()));
                    }
                }
            }

            let connect_task = cx.update(|cx| server.connect(delegate, project, cx));
            let connect_result =
                match futures::future::select(connect_task, cancel_connection_rx).await {
                    futures::future::Either::Left((connect_result, _cancel_connection_rx)) => {
                        connect_result
                    }
                    futures::future::Either::Right((_cancelled, _connect_task)) => {
                        return Err(LoadError::Other("Agent connection was cancelled.".into()));
                    }
                };

            match connect_result {
                Ok(connection) => Ok(AgentConnectedState {
                    connection,
                    server_version,
                }),
                Err(err) => match err.downcast::<LoadError>() {
                    Ok(load_error) => Err(load_error),
                    Err(err) => Err(LoadError::Other(SharedString::from(err.to_string()))),
                },
            }
        });
        (new_version_rx, loading_status_rx, connect_task)
    }
}

pub(crate) fn is_orion_code_agent(agent: &Agent) -> bool {
    matches!(agent, Agent::Custom { id } if id.as_ref() == ORION_CODE_AGENT_ID)
}

pub(crate) fn is_orion_code_agent_id(agent_id: &project::AgentId) -> bool {
    agent_id.as_ref() == ORION_CODE_AGENT_ID
}

pub(crate) fn orion_code_new_connections_are_blocked(cx: &App) -> bool {
    OrionCodeUpdateCoordinator::try_global(cx).is_some_and(|coordinator| {
        current_orion_code_release_is_revoked(coordinator.read(cx).record())
    })
}

fn revoked_orion_code_connection_entry(
    cx: &mut Context<AgentConnectionStore>,
) -> Entity<AgentConnectionEntry> {
    cx.new(|_cx| AgentConnectionEntry::Error {
        error: LoadError::Other(
            "This Orion Code release was revoked. New sessions are blocked while Orion Studio switches to a safe runtime."
                .into(),
        ),
    })
}

fn acquire_orion_code_activity(
    kind: OrionCodeActivityKind,
    cx: &mut App,
) -> Option<OrionCodeActivityLease> {
    let activity = OrionCodeUpdateActivity::init_global(cx);
    match activity.read(cx).acquire(kind) {
        Ok(activity) => Some(activity),
        Err(error) => {
            log::error!("Failed to acquire Orion Code {kind:?} activity: {error}");
            None
        }
    }
}

async fn shutdown_connected_state(
    connected_state: AgentConnectedState,
    cx: &mut gpui::AsyncApp,
) -> Result<()> {
    let Some(connection) = connected_state.connection.downcast::<AcpConnection>() else {
        return Ok(());
    };
    let shutdown_task = cx.update(|cx| connection.shutdown(cx));
    shutdown_task.await
}

/// Stops every Orion Code connection attempt and subprocess registered by any open Agent panel.
/// The task does not resolve until all in-flight attempts are cancelled and all connected ACP
/// children have exited.
pub fn shutdown_all_orion_code_connections(cx: &mut App) -> Task<Result<()>> {
    let shutdown_activity = acquire_orion_code_activity(OrionCodeActivityKind::Shutdown, cx);
    let stores = cx
        .try_global::<OrionCodeConnectionRegistry>()
        .map(|registry| registry.stores.clone())
        .unwrap_or_default();
    let mut shutdown_tasks = Vec::new();
    for store in stores {
        let Some(store) = store.upgrade() else {
            continue;
        };
        if let Some(shutdown_task) = store.update(cx, |store, cx| store.shutdown_orion_code(cx)) {
            shutdown_tasks.push(shutdown_task);
        }
    }

    cx.spawn(async move |_cx| {
        let _shutdown_activity = shutdown_activity;
        let mut first_error = None;
        for shutdown_task in shutdown_tasks {
            if let Err(error) = shutdown_task.await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    })
}

#[cfg(test)]
mod tests {
    use std::{
        any::Any,
        rc::Rc,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use fs::FakeFs;
    use gpui::TestAppContext;
    use project::AgentId;

    use super::*;

    struct ConnectionTaskDropProbe(Arc<AtomicUsize>);

    impl Drop for ConnectionTaskDropProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct HangingAgentServer {
        agent_id: AgentId,
        connect_count: Arc<AtomicUsize>,
        connection_task_drop_count: Arc<AtomicUsize>,
    }

    impl HangingAgentServer {
        fn new() -> Self {
            Self::with_id("hanging-agent")
        }

        fn with_id(agent_id: &str) -> Self {
            Self {
                agent_id: AgentId::new(agent_id.to_string()),
                connect_count: Arc::new(AtomicUsize::new(0)),
                connection_task_drop_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl AgentServer for HangingAgentServer {
        fn logo(&self) -> ui::IconName {
            ui::IconName::Sparkle
        }

        fn agent_id(&self) -> AgentId {
            self.agent_id.clone()
        }

        fn connect(
            &self,
            _delegate: AgentServerDelegate,
            _project: Entity<Project>,
            cx: &mut App,
        ) -> Task<anyhow::Result<Rc<dyn AgentConnection>>> {
            self.connect_count.fetch_add(1, Ordering::SeqCst);
            let drop_probe = ConnectionTaskDropProbe(self.connection_task_drop_count.clone());
            cx.spawn(async move |_cx| {
                let _drop_probe = drop_probe;
                futures::future::pending::<anyhow::Result<Rc<dyn AgentConnection>>>().await
            })
        }

        fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
            self
        }
    }

    #[gpui::test]
    async fn timed_out_attempt_is_cancelled_and_retry_starts_a_new_attempt(
        cx: &mut TestAppContext,
    ) {
        crate::test_support::init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection_store = cx.update(|cx| cx.new(|cx| AgentConnectionStore::new(project, cx)));
        let agent = Agent::Custom {
            id: AgentId::new("hanging-agent"),
        };
        let server = Rc::new(HangingAgentServer::new());

        let first_attempt = connection_store.update(cx, |store, cx| {
            store.request_connection(agent.clone(), server.clone(), cx)
        });
        cx.run_until_parked();
        assert_eq!(server.connect_count.load(Ordering::SeqCst), 1);

        cx.background_executor.timer(Duration::from_millis(1)).await;
        let shutdown_task = connection_store.update(cx, |store, cx| {
            store.invalidate_connection_attempt(&agent, &first_attempt, cx)
        });
        shutdown_task
            .expect("the first attempt should be current")
            .await
            .expect("the first attempt should shut down");
        cx.run_until_parked();
        assert_eq!(
            server.connection_task_drop_count.load(Ordering::SeqCst),
            1,
            "invalidating a timed-out attempt should cancel its connection task"
        );

        let second_attempt = connection_store.update(cx, |store, cx| {
            store.request_connection(agent.clone(), server.clone(), cx)
        });
        cx.run_until_parked();
        assert!(first_attempt != second_attempt);
        assert_eq!(server.connect_count.load(Ordering::SeqCst), 2);

        let old_attempt_removed_new_attempt = connection_store.update(cx, |store, cx| {
            store.invalidate_connection_attempt(&agent, &first_attempt, cx)
        });
        assert!(old_attempt_removed_new_attempt.is_none());
        connection_store.read_with(cx, |store, _cx| {
            assert_eq!(store.entry(&agent), Some(&second_attempt));
        });

        let shutdown_task = connection_store.update(cx, |store, cx| {
            store.invalidate_connection_attempt(&agent, &second_attempt, cx)
        });
        shutdown_task
            .expect("the second attempt should be current")
            .await
            .expect("the second attempt should shut down");
        cx.run_until_parked();
        assert_eq!(
            server.connection_task_drop_count.load(Ordering::SeqCst),
            2,
            "the replacement attempt should remain independently cancellable"
        );
    }

    #[gpui::test]
    async fn global_shutdown_waits_for_connecting_orion_code_in_every_store(
        cx: &mut TestAppContext,
    ) {
        crate::test_support::init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let first_project = Project::test(fs.clone(), [], cx).await;
        let second_project = Project::test(fs, [], cx).await;
        let first_store = cx.update(|cx| cx.new(|cx| AgentConnectionStore::new(first_project, cx)));
        let second_store =
            cx.update(|cx| cx.new(|cx| AgentConnectionStore::new(second_project, cx)));
        let agent = Agent::Custom {
            id: AgentId::new(ORION_CODE_AGENT_ID),
        };
        let first_server = Rc::new(HangingAgentServer::with_id(ORION_CODE_AGENT_ID));
        let second_server = Rc::new(HangingAgentServer::with_id(ORION_CODE_AGENT_ID));

        first_store.update(cx, |store, cx| {
            store.request_connection_for_version(
                agent.clone(),
                first_server.clone(),
                "1.0.0".into(),
                cx,
            );
        });
        second_store.update(cx, |store, cx| {
            store.request_connection_for_version(
                agent.clone(),
                second_server.clone(),
                "1.0.0".into(),
                cx,
            );
        });
        cx.run_until_parked();

        let activity = cx.update(|cx| OrionCodeUpdateActivity::global(cx));
        assert_eq!(
            activity.read_with(cx, |activity, _cx| activity.snapshot().startups),
            2,
            "both Orion Code connection attempts should hold startup activity"
        );

        let shutdown_task = cx.update(shutdown_all_orion_code_connections);
        assert!(activity.read_with(cx, |activity, _cx| {
            let snapshot = activity.snapshot();
            snapshot.shutdowns > 0 && !snapshot.is_idle()
        }));
        shutdown_task
            .await
            .expect("all Orion Code attempts should shut down");
        cx.run_until_parked();

        assert_eq!(
            first_server
                .connection_task_drop_count
                .load(Ordering::SeqCst),
            1
        );
        assert_eq!(
            second_server
                .connection_task_drop_count
                .load(Ordering::SeqCst),
            1
        );
        assert!(first_store.read_with(cx, |store, _cx| store.entry(&agent).is_none()));
        assert!(second_store.read_with(cx, |store, _cx| store.entry(&agent).is_none()));
        assert!(activity.read_with(cx, |activity, _cx| activity.is_idle()));
    }

    #[gpui::test]
    async fn global_shutdown_is_idempotent_for_connected_orion_code(cx: &mut TestAppContext) {
        crate::test_support::init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let harness = agent_servers::connect_fake_acp_connection(project.clone(), cx).await;
        let connection = harness.connection.clone();
        let connection_store = cx.update(|cx| cx.new(|cx| AgentConnectionStore::new(project, cx)));
        let agent = Agent::Custom {
            id: AgentId::new(ORION_CODE_AGENT_ID),
        };
        connection_store.update(cx, |store, cx| {
            let connection: Rc<dyn AgentConnection> = connection.clone();
            let entry = cx.new(|_cx| {
                AgentConnectionEntry::Connected(AgentConnectedState {
                    connection,
                    server_version: Some("1.0.0".into()),
                })
            });
            store.entries.insert(agent.clone(), entry);
        });

        cx.update(shutdown_all_orion_code_connections)
            .await
            .expect("connected Orion Code should shut down");
        cx.update(shutdown_all_orion_code_connections)
            .await
            .expect("repeated shutdown should be a no-op");

        assert!(connection.shutdown_requested_for_test());
        assert!(connection_store.read_with(cx, |store, _cx| store.entry(&agent).is_none()));
    }

    #[gpui::test]
    async fn versioned_request_never_reuses_an_older_orion_code_connection(
        cx: &mut TestAppContext,
    ) {
        crate::test_support::init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let version_one = agent_servers::connect_fake_acp_connection(project.clone(), cx).await;
        let first_connection = version_one.connection;
        let second_server = Rc::new(HangingAgentServer::with_id(ORION_CODE_AGENT_ID));
        let connection_store = cx.update(|cx| cx.new(|cx| AgentConnectionStore::new(project, cx)));
        let agent = Agent::Custom {
            id: AgentId::new(ORION_CODE_AGENT_ID),
        };

        let first_entry = connection_store.update(cx, |store, cx| {
            let connection: Rc<dyn AgentConnection> = first_connection.clone();
            let entry = cx.new(|_cx| {
                AgentConnectionEntry::Connected(AgentConnectedState {
                    connection,
                    server_version: Some("1.0.0".into()),
                })
            });
            store.entries.insert(agent.clone(), entry.clone());
            entry
        });

        let second_entry = connection_store.update(cx, |store, cx| {
            store.request_connection_for_version(
                agent.clone(),
                second_server.clone(),
                "2.0.0".into(),
                cx,
            )
        });
        cx.run_until_parked();

        assert_ne!(first_entry, second_entry);
        assert!(first_connection.shutdown_requested_for_test());
        assert_eq!(second_server.connect_count.load(Ordering::SeqCst), 1);
        assert!(second_entry.read_with(cx, |entry, _cx| matches!(
            entry,
            AgentConnectionEntry::Connecting { .. }
        )));
        assert_eq!(
            second_entry.read_with(cx, |entry, _cx| entry.server_version().cloned()),
            Some(SharedString::from("2.0.0"))
        );
    }
}
