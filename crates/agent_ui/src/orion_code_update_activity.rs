use std::{
    cell::RefCell,
    error::Error,
    fmt::{self, Display, Formatter},
    rc::Rc,
};

use gpui::{App, AppContext, Entity, Global};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OrionCodeActivitySnapshot {
    pub active_turns: usize,
    pub active_tools: usize,
    pub pending_permissions: usize,
    pub loads: usize,
    pub startups: usize,
    pub shutdowns: usize,
    pub version_switches: usize,
    pub invariant_failures: usize,
}

impl OrionCodeActivitySnapshot {
    pub fn is_idle(self) -> bool {
        self.active_turns == 0
            && self.active_tools == 0
            && self.pending_permissions == 0
            && self.loads == 0
            && self.startups == 0
            && self.shutdowns == 0
            && self.version_switches == 0
            && self.invariant_failures == 0
    }

    fn count(self, kind: OrionCodeActivityKind) -> usize {
        match kind {
            OrionCodeActivityKind::ActiveTurn => self.active_turns,
            OrionCodeActivityKind::ActiveTool => self.active_tools,
            OrionCodeActivityKind::PendingPermission => self.pending_permissions,
            OrionCodeActivityKind::Load => self.loads,
            OrionCodeActivityKind::Startup => self.startups,
            OrionCodeActivityKind::Shutdown => self.shutdowns,
            OrionCodeActivityKind::VersionSwitch => self.version_switches,
        }
    }

    fn set_count(&mut self, kind: OrionCodeActivityKind, count: usize) {
        match kind {
            OrionCodeActivityKind::ActiveTurn => self.active_turns = count,
            OrionCodeActivityKind::ActiveTool => self.active_tools = count,
            OrionCodeActivityKind::PendingPermission => self.pending_permissions = count,
            OrionCodeActivityKind::Load => self.loads = count,
            OrionCodeActivityKind::Startup => self.startups = count,
            OrionCodeActivityKind::Shutdown => self.shutdowns = count,
            OrionCodeActivityKind::VersionSwitch => self.version_switches = count,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrionCodeActivityKind {
    ActiveTurn,
    ActiveTool,
    PendingPermission,
    Load,
    Startup,
    Shutdown,
    VersionSwitch,
}

const ACTIVITY_KINDS: [OrionCodeActivityKind; 7] = [
    OrionCodeActivityKind::ActiveTurn,
    OrionCodeActivityKind::ActiveTool,
    OrionCodeActivityKind::PendingPermission,
    OrionCodeActivityKind::Load,
    OrionCodeActivityKind::Startup,
    OrionCodeActivityKind::Shutdown,
    OrionCodeActivityKind::VersionSwitch,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrionCodeActivityError {
    AlreadyReleased,
    CounterOverflow(OrionCodeActivityKind),
    CounterUnderflow(OrionCodeActivityKind),
}

impl Display for OrionCodeActivityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyReleased => {
                write!(formatter, "Orion Code activity guard was released twice")
            }
            Self::CounterOverflow(kind) => {
                write!(formatter, "Orion Code {kind:?} activity counter overflowed")
            }
            Self::CounterUnderflow(kind) => {
                write!(
                    formatter,
                    "Orion Code {kind:?} activity counter underflowed"
                )
            }
        }
    }
}

impl Error for OrionCodeActivityError {}

struct OrionCodeActivityState {
    snapshot: OrionCodeActivitySnapshot,
    idle_generation: u64,
    snapshot_sender: watch::Sender<OrionCodeActivitySnapshot>,
    _snapshot_keepalive: watch::Receiver<OrionCodeActivitySnapshot>,
    idle_sender: watch::Sender<u64>,
    _idle_keepalive: watch::Receiver<u64>,
}

struct OrionCodeActivityInner {
    state: RefCell<OrionCodeActivityState>,
}

impl OrionCodeActivityInner {
    fn new() -> Self {
        let snapshot = OrionCodeActivitySnapshot::default();
        let (snapshot_sender, snapshot_keepalive) = watch::channel(snapshot);
        let (idle_sender, idle_keepalive) = watch::channel(0);
        Self {
            state: RefCell::new(OrionCodeActivityState {
                snapshot,
                idle_generation: 0,
                snapshot_sender,
                _snapshot_keepalive: snapshot_keepalive,
                idle_sender,
                _idle_keepalive: idle_keepalive,
            }),
        }
    }

    fn snapshot(&self) -> OrionCodeActivitySnapshot {
        self.state.borrow().snapshot
    }

    fn idle_generation(&self) -> u64 {
        self.state.borrow().idle_generation
    }

    fn subscribe(&self) -> watch::Receiver<OrionCodeActivitySnapshot> {
        self.state.borrow().snapshot_sender.receiver()
    }

    fn subscribe_idle(&self) -> watch::Receiver<u64> {
        self.state.borrow().idle_sender.receiver()
    }

    fn replace_owner_activity(
        &self,
        previous: OrionCodeActivitySnapshot,
        next: OrionCodeActivitySnapshot,
    ) -> Result<(), OrionCodeActivityError> {
        let mut state = self.state.borrow_mut();
        let mut updated = state.snapshot;

        for kind in ACTIVITY_KINDS {
            let total = updated.count(kind);
            let previous_count = previous.count(kind);
            let Some(without_owner) = total.checked_sub(previous_count) else {
                Self::record_invariant_failure(&mut state);
                return Err(OrionCodeActivityError::CounterUnderflow(kind));
            };
            let Some(with_owner) = without_owner.checked_add(next.count(kind)) else {
                Self::record_invariant_failure(&mut state);
                return Err(OrionCodeActivityError::CounterOverflow(kind));
            };
            updated.set_count(kind, with_owner);
        }

        Self::publish(&mut state, updated);
        Ok(())
    }

    fn record_invariant_failure(state: &mut OrionCodeActivityState) {
        state.snapshot.invariant_failures = state.snapshot.invariant_failures.saturating_add(1);
        let snapshot = state.snapshot;
        if let Err(error) = state.snapshot_sender.send(snapshot) {
            log::error!("Failed to publish Orion Code activity invariant failure: {error}");
        }
    }

    fn publish(state: &mut OrionCodeActivityState, updated: OrionCodeActivitySnapshot) {
        let was_idle = state.snapshot.is_idle();
        let is_idle = updated.is_idle();
        state.snapshot = updated;

        if !was_idle && is_idle {
            if let Some(next_generation) = state.idle_generation.checked_add(1) {
                state.idle_generation = next_generation;
                if let Err(error) = state.idle_sender.send(next_generation) {
                    log::error!("Failed to publish Orion Code idle wake: {error}");
                }
            } else {
                Self::record_invariant_failure(state);
                log::error!("Orion Code idle generation overflowed");
                return;
            }
        }

        if let Err(error) = state.snapshot_sender.send(updated) {
            log::error!("Failed to publish Orion Code activity snapshot: {error}");
        }
    }
}

struct GlobalOrionCodeUpdateActivity(Entity<OrionCodeUpdateActivity>);

impl Global for GlobalOrionCodeUpdateActivity {}

pub struct OrionCodeUpdateActivity {
    inner: Rc<OrionCodeActivityInner>,
}

impl OrionCodeUpdateActivity {
    fn new() -> Self {
        Self {
            inner: Rc::new(OrionCodeActivityInner::new()),
        }
    }

    pub fn init_global(cx: &mut App) -> Entity<Self> {
        if let Some(activity) = Self::try_global(cx) {
            return activity;
        }

        let activity = cx.new(|_cx| Self::new());
        cx.set_global(GlobalOrionCodeUpdateActivity(activity.clone()));
        activity
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalOrionCodeUpdateActivity>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalOrionCodeUpdateActivity>()
            .map(|activity| activity.0.clone())
    }

    pub fn snapshot(&self) -> OrionCodeActivitySnapshot {
        self.inner.snapshot()
    }

    pub fn is_idle(&self) -> bool {
        self.snapshot().is_idle()
    }

    pub fn idle_generation(&self) -> u64 {
        self.inner.idle_generation()
    }

    pub fn subscribe(&self) -> watch::Receiver<OrionCodeActivitySnapshot> {
        self.inner.subscribe()
    }

    pub fn subscribe_idle(&self) -> watch::Receiver<u64> {
        self.inner.subscribe_idle()
    }

    pub fn acquire(
        &self,
        kind: OrionCodeActivityKind,
    ) -> Result<OrionCodeActivityLease, OrionCodeActivityError> {
        OrionCodeActivityLease::new(self.inner.clone(), kind)
    }

    pub fn owner(&self) -> OrionCodeActivityOwner {
        OrionCodeActivityOwner {
            inner: self.inner.clone(),
            current: OrionCodeActivitySnapshot::default(),
            released: false,
        }
    }
}

pub struct OrionCodeActivityLease {
    inner: Rc<OrionCodeActivityInner>,
    kind: OrionCodeActivityKind,
    released: bool,
}

impl OrionCodeActivityLease {
    fn new(
        inner: Rc<OrionCodeActivityInner>,
        kind: OrionCodeActivityKind,
    ) -> Result<Self, OrionCodeActivityError> {
        let mut activity = OrionCodeActivitySnapshot::default();
        activity.set_count(kind, 1);
        inner.replace_owner_activity(OrionCodeActivitySnapshot::default(), activity)?;
        Ok(Self {
            inner,
            kind,
            released: false,
        })
    }

    pub fn release(&mut self) -> Result<(), OrionCodeActivityError> {
        if self.released {
            return Err(OrionCodeActivityError::AlreadyReleased);
        }
        self.released = true;

        let mut activity = OrionCodeActivitySnapshot::default();
        activity.set_count(self.kind, 1);
        self.inner
            .replace_owner_activity(activity, OrionCodeActivitySnapshot::default())
    }
}

impl Drop for OrionCodeActivityLease {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Err(error) = self.release() {
            log::error!("Failed to release Orion Code activity lease: {error}");
        }
    }
}

pub struct OrionCodeActivityOwner {
    inner: Rc<OrionCodeActivityInner>,
    current: OrionCodeActivitySnapshot,
    released: bool,
}

impl OrionCodeActivityOwner {
    pub fn set_session_activity(
        &mut self,
        active_turns: usize,
        active_tools: usize,
        pending_permissions: usize,
    ) -> Result<(), OrionCodeActivityError> {
        if self.released {
            return Err(OrionCodeActivityError::AlreadyReleased);
        }

        let next = OrionCodeActivitySnapshot {
            active_turns,
            active_tools,
            pending_permissions,
            ..OrionCodeActivitySnapshot::default()
        };
        self.inner.replace_owner_activity(self.current, next)?;
        self.current = next;
        Ok(())
    }

    pub fn release(&mut self) -> Result<(), OrionCodeActivityError> {
        if self.released {
            return Err(OrionCodeActivityError::AlreadyReleased);
        }
        self.released = true;
        self.inner
            .replace_owner_activity(self.current, OrionCodeActivitySnapshot::default())
    }
}

impl Drop for OrionCodeActivityOwner {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Err(error) = self.release() {
            log::error!("Failed to release Orion Code activity owner: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    #[test]
    fn equivalent_window_owners_are_aggregated_and_owner_drop_releases_them() {
        let activity = OrionCodeUpdateActivity::new();
        let mut first_window = activity.owner();
        let mut second_window = activity.owner();

        first_window
            .set_session_activity(1, 1, 0)
            .expect("first window activity should be recorded");
        second_window
            .set_session_activity(1, 0, 1)
            .expect("second window activity should be recorded");
        assert_eq!(
            activity.snapshot(),
            OrionCodeActivitySnapshot {
                active_turns: 2,
                active_tools: 1,
                pending_permissions: 1,
                ..OrionCodeActivitySnapshot::default()
            }
        );

        drop(first_window);
        assert_eq!(activity.snapshot().active_turns, 1);
        assert_eq!(activity.snapshot().pending_permissions, 1);
        drop(second_window);
        assert!(activity.is_idle());
    }

    #[test]
    fn long_tool_permission_and_load_each_hold_the_barrier() {
        let activity = OrionCodeUpdateActivity::new();
        let mut session = activity.owner();
        let mut load = activity
            .acquire(OrionCodeActivityKind::Load)
            .expect("load activity should be acquired");

        session
            .set_session_activity(1, 1, 1)
            .expect("session activity should be recorded");
        assert!(!activity.is_idle());
        load.release().expect("load activity should be released");
        assert_eq!(activity.snapshot().loads, 0);
        assert!(!activity.is_idle());

        session
            .set_session_activity(0, 1, 0)
            .expect("long-running tool should remain active");
        assert!(!activity.is_idle());
        session
            .set_session_activity(0, 0, 0)
            .expect("completed tool should release the barrier");
        assert!(activity.is_idle());
    }

    #[test]
    fn startup_shutdown_and_version_switch_are_independent() {
        let activity = OrionCodeUpdateActivity::new();
        let startup = activity
            .acquire(OrionCodeActivityKind::Startup)
            .expect("startup activity should be acquired");
        let shutdown = activity
            .acquire(OrionCodeActivityKind::Shutdown)
            .expect("shutdown activity should be acquired");
        let version_switch = activity
            .acquire(OrionCodeActivityKind::VersionSwitch)
            .expect("version switch activity should be acquired");

        let snapshot = activity.snapshot();
        assert_eq!(snapshot.startups, 1);
        assert_eq!(snapshot.shutdowns, 1);
        assert_eq!(snapshot.version_switches, 1);

        drop(startup);
        drop(shutdown);
        assert!(!activity.is_idle());
        drop(version_switch);
        assert!(activity.is_idle());
    }

    #[test]
    fn duplicate_release_is_visible_without_counter_underflow() {
        let activity = OrionCodeUpdateActivity::new();
        let mut lease = activity
            .acquire(OrionCodeActivityKind::ActiveTurn)
            .expect("turn activity should be acquired");

        lease.release().expect("first release should succeed");
        assert_eq!(
            lease.release(),
            Err(OrionCodeActivityError::AlreadyReleased)
        );
        assert_eq!(activity.snapshot().invariant_failures, 0);
        assert!(activity.is_idle());
    }

    #[test]
    fn counter_underflow_fails_closed_and_is_visible_in_the_snapshot() {
        let activity = OrionCodeUpdateActivity::new();
        let previous = OrionCodeActivitySnapshot {
            active_tools: 1,
            ..OrionCodeActivitySnapshot::default()
        };

        assert_eq!(
            activity
                .inner
                .replace_owner_activity(previous, OrionCodeActivitySnapshot::default(),),
            Err(OrionCodeActivityError::CounterUnderflow(
                OrionCodeActivityKind::ActiveTool
            ))
        );
        assert_eq!(activity.snapshot().invariant_failures, 1);
        assert!(!activity.is_idle());
    }

    #[gpui::test]
    async fn busy_to_idle_emits_one_wake_for_the_transition(cx: &mut TestAppContext) {
        let activity = cx.update(OrionCodeUpdateActivity::init_global);
        let mut idle_updates = activity.read_with(cx, |activity, _cx| activity.subscribe_idle());
        let (first, second) = activity.read_with(cx, |activity, _cx| {
            (
                activity
                    .acquire(OrionCodeActivityKind::ActiveTurn)
                    .expect("turn activity should be acquired"),
                activity
                    .acquire(OrionCodeActivityKind::ActiveTool)
                    .expect("tool activity should be acquired"),
            )
        });

        drop(first);
        assert_eq!(
            activity.read_with(cx, |activity, _cx| activity.idle_generation()),
            0
        );
        drop(second);
        assert_eq!(
            idle_updates
                .recv()
                .await
                .expect("idle transition should wake subscribers"),
            1
        );
        assert_eq!(
            activity.read_with(cx, |activity, _cx| activity.idle_generation()),
            1
        );
    }
}
