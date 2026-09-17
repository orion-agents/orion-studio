//! The AI Native **Task Inspector** — the single panel that owns the right dock
//! while the AI Native layout is selected.
//!
//! It is deliberately *not* a stack of the existing `ProjectPanel`, `GitPanel`
//! and `OutlinePanel`. Those remain independent panels with their own docks,
//! settings and commit flows. The inspector is a compact, task-scoped surface
//! with a fixed three-tab strip:
//!
//! - **Changes** — files with working-tree changes, marked when the current
//!   agent task touched them.
//! - **Files** — a compact, collapsed-by-default tree of the project worktree.
//! - **Review** — the *current task's* unreviewed agent edits, with an entry
//!   point into the full `AgentDiffPane`.
//!
//! The tab selection is workspace-level UI state that lives on this panel. It
//! is intentionally not written to global layout settings: which tab you last
//! looked at is not a layout preference.

mod diff_preview;
mod facts;

use crate::{AgentPanel, AgentPanelEvent};
use acp_thread::{AcpThread, AgentThreadEntry, MentionUri};
use agent_settings::{AgentSettings, WindowLayout};
use buffer_diff;
use collections::HashSet;
use git::repository::UpstreamTracking;
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    Render, Subscription, WeakEntity, Window, actions, div, px,
};
use language::Buffer;
use project::ProjectPath;
use settings::{Settings as _, SettingsStore};
use ui::{Divider, Tooltip, prelude::*};
use workspace::{Panel, Workspace, dock::PanelEvent};

pub(crate) const TASK_ENVIRONMENT_PANEL_KEY: &str = "task_inspector_panel";

/// Starting width of the right dock in AI Native. Only a default: the user can
/// drag the dock as usual and that choice is not overwritten.
const DEFAULT_INSPECTOR_WIDTH: f32 = 320.;

actions!(
    task_environment,
    [
        /// Toggles focus on the Task Environment panel.
        ToggleTaskEnvironment,
    ]
);

/// One disclosure row of the Task Environment panel.
///
/// The panel has no tabs and no segmented control: every environment fact is a row
/// in a single vertical column, and a row expands in place instead of switching the
/// panel's whole surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EnvironmentRow {
    Changes,
    LocalWorktree,
    Branch,
    CommitOrPush,
    Plan,
    Subagents,
    Sources,
}

impl EnvironmentRow {
    /// Presentation order, matching the plan's environment table.
    const ALL: [Self; 7] = [
        Self::Changes,
        Self::LocalWorktree,
        Self::Branch,
        Self::CommitOrPush,
        Self::Plan,
        Self::Subagents,
        Self::Sources,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Changes => "Changes",
            Self::LocalWorktree => "Local worktree",
            Self::Branch => "Branch",
            Self::CommitOrPush => "Commit or push",
            Self::Plan => "Plan",
            Self::Subagents => "Subagents",
            Self::Sources => "Sources",
        }
    }
}

/// The single right-dock surface used by the AI Native layout.
pub struct TaskEnvironmentPanel {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    /// Which disclosure rows the user has expanded. Panel-local UI state only —
    /// deliberately not persisted as a layout preference.
    open_rows: HashSet<EnvironmentRow>,
    /// The one file whose local diff is expanded in the Changes row. The plan
    /// allows a single expanded file at a time, so this is a slot, not a set.
    preview_file: Option<ProjectPath>,
    /// The `AgentPanel` whose active thread this environment follows. It is
    /// resolved lazily because panels are loaded concurrently at startup.
    panel: Option<Entity<AgentPanel>>,
    _panel_subscription: Option<Subscription>,
    _workspace_subscription: Subscription,
}

impl TaskEnvironmentPanel {
    /// Builds the panel and its child views.
    ///
    /// `load` is the production entry point; tests construct the panel directly.
    ///
    /// Nothing here reads the workspace: the panel is constructed from inside a
    /// `Workspace` update, and the child view re-syncs its project and git handles
    /// on every render.
    pub(crate) fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let workspace_handle = workspace.downgrade();

        // The center tab bar follows the layout, not the active item (see
        // `sync_center_tab_bar`), so the panel only has to react to pane
        // membership changes — the layout itself is handled when it changes.
        let workspace_subscription =
            cx.subscribe(&workspace, |_, workspace, event: &workspace::Event, cx| {
                if matches!(
                    event,
                    workspace::Event::ActiveItemChanged
                        | workspace::Event::PaneAdded(_)
                        | workspace::Event::ItemAdded { .. }
                        | workspace::Event::ItemRemoved { .. }
                ) {
                    let panes = workspace.read(cx).panes().to_vec();
                    crate::ai_native_conversation_item::sync_center_tab_bar(panes, cx);
                }
            });

        Self {
            workspace: workspace_handle,
            focus_handle: cx.focus_handle(),
            // Changes is the one row worth opening unasked; the rest start folded.
            open_rows: HashSet::from_iter([EnvironmentRow::Changes]),
            preview_file: None,
            panel: None,
            _panel_subscription: None,
            _workspace_subscription: workspace_subscription,
        }
    }

    fn toggle_row(&mut self, row: EnvironmentRow, cx: &mut Context<Self>) {
        if !self.open_rows.remove(&row) {
            self.open_rows.insert(row);
        }
        cx.notify();
    }

    /// Expands the local diff for one file, collapsing whatever was open before:
    /// the environment panel shows at most one file preview at a time.
    fn toggle_preview(&mut self, project_path: ProjectPath, cx: &mut Context<Self>) {
        self.preview_file = match self.preview_file.as_ref() {
            Some(current) if current == &project_path => None,
            _ => Some(project_path),
        };
        cx.notify();
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |_workspace, _window, cx| {
            let workspace_entity = cx.entity();
            cx.new(|cx| Self::new(workspace_entity, cx))
        })
    }

    /// Binds to the workspace's `AgentPanel` once it exists, and forwards
    /// surface changes to the three child views.
    ///
    /// Panels are added to a workspace concurrently during startup, so the
    /// agent panel is frequently not there yet when this panel is constructed.
    /// Resolving it here keeps the binding correct no matter which loads first.
    fn sync_panel_subscription(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).panel::<AgentPanel>(cx));

        if current.as_ref().map(|panel| panel.entity_id())
            == self.panel.as_ref().map(|panel| panel.entity_id())
        {
            return;
        }

        self._panel_subscription = current.as_ref().map(|panel| {
            cx.subscribe_in(
                panel,
                window,
                |this, _panel, event: &AgentPanelEvent, _, cx| {
                    // The center conversation surface and this inspector must never
                    // drift apart: both follow the panel's active surface.
                    if matches!(
                        event,
                        AgentPanelEvent::ActiveViewChanged | AgentPanelEvent::EntryChanged
                    ) {
                        this.refresh(cx);
                    }
                },
            )
        });
        self.panel = current;
        self.refresh(cx);
    }

    /// Every row renders from this panel's own state, so a repaint is the whole
    /// refresh — there are no child views left to forward to.
    fn refresh(&self, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for TaskEnvironmentPanel {}

impl Focusable for TaskEnvironmentPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// A short, human-readable label for one Composer source.
///
/// `MentionUri` deliberately has no `Display`, so each variant is mapped to the
/// detail a user recognises rather than to a raw debug string.
fn mention_label(uri: &MentionUri) -> SharedString {
    match uri {
        MentionUri::File { abs_path } | MentionUri::Directory { abs_path } => {
            abs_path.display().to_string().into()
        }
        MentionUri::PastedImage { name }
        | MentionUri::Symbol { name, .. }
        | MentionUri::Thread { name, .. }
        | MentionUri::Rule { name, .. } => name.clone().into(),
        _ => "context".into(),
    }
}

/// The real Git branch the task's worktree sits on.
struct BranchFact {
    name: SharedString,
    upstream: Option<SharedString>,
    ahead: u32,
    behind: u32,
}

/// One subagent session this task actually spawned.
struct SubagentFact {
    title: SharedString,
    is_finished: bool,
}

/// One task-scoped changed file, shaped for the Changes row.
struct ChangeFact {
    display_path: SharedString,
    project_path: ProjectPath,
    status_letter: &'static str,
    status_color: Color,
    added: u32,
    deleted: u32,
}

impl TaskEnvironmentPanel {
    /// The task's own changes: files the agent actually edited, with real counts.
    ///
    /// Files that are dirty in the working tree but that this task did not touch are
    /// deliberately excluded — the panel answers "what did this task do", not "what
    /// is uncommitted in the repo".
    fn change_facts(&self, cx: &App) -> Vec<ChangeFact> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };
        let project = workspace.read(cx).project().clone();
        let touched = facts::task_touched_paths(&self.workspace, cx);

        facts::all_changed_entries(&project, cx)
            .into_iter()
            .filter(|(path, _, _)| touched.contains(path))
            .map(|(project_path, entry, display)| {
                let (status_letter, status_color) = facts::status_letter_and_color(entry.status);
                let stat = facts::diff_stat_for(&entry);
                ChangeFact {
                    display_path: display.into(),
                    project_path,
                    status_letter,
                    status_color,
                    added: stat.map(|stat| stat.added).unwrap_or(0),
                    deleted: stat.map(|stat| stat.deleted).unwrap_or(0),
                }
            })
            .collect()
    }

    /// The local worktree this task is bound to, when a project is open.
    fn worktree_root(&self, cx: &App) -> Option<SharedString> {
        let workspace = self.workspace.upgrade()?;
        let project = workspace.read(cx).project().clone();
        let project = project.read(cx);
        project
            .worktrees(cx)
            .next()
            .map(|worktree| worktree.read(cx).abs_path().display().to_string().into())
    }

    /// Real Git branch facts, or `None` when there is nothing to show.
    ///
    /// `None` covers both "no project" and "project without Git": the plan is
    /// explicit that a project without Git hides this row rather than rendering an
    /// empty branch name.
    fn branch_fact(&self, cx: &App) -> Option<BranchFact> {
        let workspace = self.workspace.upgrade()?;
        let project = workspace.read(cx).project().clone();
        let git_store = project.read(cx).git_store().clone();
        let git_store = git_store.read(cx);
        let snapshot = git_store
            .repositories()
            .values()
            .next()?
            .read(cx)
            .snapshot();

        let branch = snapshot.branch?;
        let name = branch
            .ref_name
            .strip_prefix("refs/heads/")
            .unwrap_or(branch.ref_name.as_ref())
            .to_string();

        let (upstream, ahead, behind) = match branch.upstream.as_ref() {
            Some(upstream) => {
                let (ahead, behind) = match &upstream.tracking {
                    UpstreamTracking::Tracked(status) => (status.ahead, status.behind),
                    UpstreamTracking::Gone => (0, 0),
                };
                (Some(upstream.ref_name.clone()), ahead, behind)
            }
            None => (None, 0, 0),
        };

        Some(BranchFact {
            name: name.into(),
            upstream,
            ahead,
            behind,
        })
    }

    /// The plan the agent published for this task, when there is one.
    ///
    /// `None` means no plan has been published — the row is hidden rather than
    /// shown with a fabricated progress figure.
    fn plan_counts(&self, cx: &App) -> Option<(u32, usize)> {
        let thread = active_agent_thread(&self.workspace, cx)?;
        let thread = thread.read(cx);
        let plan = thread.plan();
        if plan.is_empty() {
            return None;
        }
        let stats = plan.stats();
        Some((stats.completed, plan.entries.len()))
    }

    /// Subagent sessions this task actually spawned.
    ///
    /// Only tool calls carrying a real `subagent_session_info` count, so the row
    /// cannot invent a subagent the ACP session never announced.
    fn subagent_facts(&self, cx: &App) -> Vec<SubagentFact> {
        let Some(thread) = active_agent_thread(&self.workspace, cx) else {
            return Vec::new();
        };
        let thread = thread.read(cx);
        thread
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                AgentThreadEntry::ToolCall(tool_call) => {
                    let info = tool_call.subagent_session_info.as_ref()?;
                    Some(SubagentFact {
                        title: tool_call.label.read(cx).source().to_string().into(),
                        is_finished: info.message_end_index.is_some(),
                    })
                }
                _ => None,
            })
            .collect()
    }

    /// The context the Composer currently has attached to this task.
    ///
    /// Read straight from the Composer's own mention set, so the panel never keeps a
    /// second copy of context ownership: adding or removing a source stays a
    /// Composer operation with its existing permission and error handling.
    fn source_facts(&self, cx: &App) -> Vec<SharedString> {
        let Some(panel) = self.panel.clone() else {
            return Vec::new();
        };
        let Some(conversation_view) = panel.read(cx).active_conversation_view() else {
            return Vec::new();
        };
        let Some(thread_view) = conversation_view.read(cx).root_thread_view() else {
            return Vec::new();
        };
        let editor = thread_view.read(cx).message_editor.clone();
        let mut sources: Vec<SharedString> = editor
            .read(cx)
            .mention_set()
            .read(cx)
            .mentions()
            .iter()
            .map(mention_label)
            .collect();
        sources.sort();
        sources
    }

    /// The task's own diff for a file, when the task is what changed it.
    ///
    /// Scoped to the active thread's action log, so the preview can only ever show
    /// edits this task made — never whatever else is dirty in the worktree.
    fn task_diff(
        &self,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Option<(Entity<Buffer>, Entity<buffer_diff::BufferDiff>)> {
        let thread = active_agent_thread(&self.workspace, cx)?;
        let action_log = thread.read(cx).action_log().clone();
        let action_log = action_log.read(cx);
        action_log.changed_buffers(cx).find(|(buffer, _)| {
            buffer.read(cx).file().is_some_and(|file| {
                file.worktree_id(cx) == project_path.worktree_id
                    && file.path() == &project_path.path
            })
        })
    }

    /// The shape every "nothing to show here" body uses.
    fn muted_line(text: impl Into<SharedString>) -> impl IntoElement {
        Label::new(text).size(LabelSize::Small).color(Color::Muted)
    }

    /// One disclosure row: a single-line header whose body expands in place.
    ///
    /// No row navigates the center pane. The only way out to an editor is the
    /// explicit `OpenInCodeWorkspace` action.
    fn render_row(
        &self,
        row: EnvironmentRow,
        value: SharedString,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_open = self.open_rows.contains(&row);
        let hover_bg = cx.theme().colors().element_hover;

        v_flex()
            .w_full()
            .child(
                h_flex()
                    .id(("environment-row", row as usize))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .items_center()
                    .cursor_pointer()
                    .hover(move |this| this.bg(hover_bg))
                    .child(
                        Icon::new(if is_open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                    )
                    .child(Label::new(row.label()).size(LabelSize::Small))
                    .child(div().flex_1())
                    .child(Label::new(value).size(LabelSize::Small).color(Color::Muted))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_row(row, cx))),
            )
            .when(is_open, |this| this.child(div().px_3().pb_2().child(body)))
            .into_any()
    }

    /// The whole environment column: one row per fact, no tabs anywhere.
    ///
    /// Rows with no real data say so instead of rendering a zero — the panel must
    /// never invent a branch, a count or a plan to look complete.
    fn render_environment(&self, cx: &mut Context<Self>) -> AnyElement {
        let facts = self.change_facts(cx);
        let worktree_root = self.worktree_root(cx);

        let mut column = v_flex().w_full();

        for row in EnvironmentRow::ALL {
            let mut body = v_flex().w_full().gap_1();
            let value: SharedString = match row {
                EnvironmentRow::Changes => {
                    if facts.is_empty() {
                        body = body.child(Self::muted_line("This task has no Git changes yet."));
                        "No changes".into()
                    } else {
                        let added: u32 = facts.iter().map(|fact| fact.added).sum();
                        let deleted: u32 = facts.iter().map(|fact| fact.deleted).sum();
                        for fact in &facts {
                            let is_previewed =
                                self.preview_file.as_ref() == Some(&fact.project_path);
                            let project_path = fact.project_path.clone();
                            body = body.child(
                                h_flex()
                                    .id((
                                        "environment-file",
                                        fact.project_path.path.as_unix_str().len(),
                                    ))
                                    .w_full()
                                    .gap_2()
                                    .px_1()
                                    .items_center()
                                    .cursor_pointer()
                                    .when(is_previewed, |this| {
                                        this.bg(cx.theme().colors().element_selected)
                                    })
                                    .child(
                                        Label::new(fact.display_path.clone())
                                            .size(LabelSize::Small),
                                    )
                                    .child(div().flex_1())
                                    .child(
                                        Label::new(fact.status_letter)
                                            .size(LabelSize::Small)
                                            .color(fact.status_color),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_preview(project_path.clone(), cx)
                                    })),
                            );
                        }

                        // The local preview for the single selected file. It renders
                        // read-only and in place, and never activates a pane item —
                        // the code workspace is the only way to actually edit.
                        if let Some(preview_path) = self.preview_file.clone() {
                            match self.task_diff(&preview_path, cx) {
                                Some((buffer, diff)) => {
                                    body = body.child(
                                        div()
                                            .w_full()
                                            .border_t_1()
                                            .border_color(cx.theme().colors().border_variant)
                                            .pt_2()
                                            .child(diff_preview::render_patch(&buffer, &diff, cx)),
                                    );
                                }
                                None => {
                                    // A file can be dirty without this task having
                                    // touched it. Saying so beats showing an empty box.
                                    body = body.child(Self::muted_line(
                                        "No task-owned diff for this file — it may have \
                                         changed outside this task.",
                                    ));
                                }
                            }
                        }

                        // The only route from a change to an editor. Selecting a file
                        // above previews it in place; it never activates a center item.
                        body = body.child(
                            Button::new("environment-open-review", "Open review in Code Workspace")
                                .on_click(cx.listener(|_, _, window, cx| {
                                    window.dispatch_action(
                                        Box::new(workspace::OpenInCodeWorkspace),
                                        cx,
                                    );
                                })),
                        );

                        format!("+{added} \u{2212}{deleted}").into()
                    }
                }
                EnvironmentRow::LocalWorktree => match worktree_root.clone() {
                    Some(root) => {
                        body = body.child(Self::muted_line(
                            "The local worktree this task is bound to.",
                        ));
                        root
                    }
                    None => {
                        body = body.child(Self::muted_line(
                            "No project is open, so there is no worktree to show.",
                        ));
                        "unavailable".into()
                    }
                },
                EnvironmentRow::Branch => match self.branch_fact(cx) {
                    Some(branch) => {
                        body = body.child(Self::muted_line(
                            "The branch this task's worktree is checked out on.",
                        ));
                        if let Some(upstream) = branch.upstream.clone() {
                            body = body.child(
                                Label::new(format!(
                                    "upstream {upstream} · ahead {} · behind {}",
                                    branch.ahead, branch.behind
                                ))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                            );
                        }
                        branch.name
                    }
                    // No Git, no row — an empty Branch row would be a fake capability.
                    None => continue,
                },
                EnvironmentRow::CommitOrPush => {
                    // A real summary, not a placeholder: the branch and upstream this
                    // task sits on, how many task changes would go into a commit, and
                    // the explicit exit to Git. Committing itself stays in the code
                    // workspace — this row never grows a commit form.
                    let Some(branch) = self.branch_fact(cx) else {
                        // No Git, no row — the same rule the Branch row follows.
                        continue;
                    };
                    body = body.child(Self::muted_line(match branch.upstream.clone() {
                        Some(upstream) => {
                            format!(
                                "upstream {upstream} · ahead {} · behind {}",
                                branch.ahead, branch.behind
                            )
                        }
                        None => "No upstream configured — this branch has no remote to push to."
                            .to_string(),
                    }));
                    body = body.child(
                        Button::new("environment-open-git", "Open Git in Code Workspace").on_click(
                            cx.listener(|_, _, window, cx| {
                                window
                                    .dispatch_action(Box::new(workspace::OpenInCodeWorkspace), cx);
                            }),
                        ),
                    );
                    let task_changes = facts.len();
                    if task_changes == 0 {
                        "No task changes".into()
                    } else {
                        format!("{task_changes} task change(s)").into()
                    }
                }
                EnvironmentRow::Plan => match self.plan_counts(cx) {
                    Some((completed, total)) => {
                        body = body.child(Self::muted_line(
                            "The plan the agent published for this task. Nothing here is inferred.",
                        ));
                        format!("{completed} / {total}").into()
                    }
                    // No published plan means no row: an empty Plan row would
                    // advertise a capability the session never provided.
                    None => continue,
                },
                EnvironmentRow::Subagents => {
                    let subagents = self.subagent_facts(cx);
                    if subagents.is_empty() {
                        continue;
                    }
                    for subagent in &subagents {
                        let status = if subagent.is_finished {
                            "Done"
                        } else {
                            "Running"
                        };
                        body = body.child(
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .child(Label::new(subagent.title.clone()).size(LabelSize::Small))
                                .child(div().flex_1())
                                .child(
                                    Label::new(status)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                        );
                    }
                    subagents.len().to_string().into()
                }
                EnvironmentRow::Sources => {
                    let sources = self.source_facts(cx);
                    if sources.is_empty() {
                        continue;
                    }
                    body = body.child(Self::muted_line(
                        "Context attached to this task's Composer. Add or remove it from the Composer.",
                    ));
                    for source in &sources {
                        body = body.child(Label::new(source.clone()).size(LabelSize::Small));
                    }
                    sources.len().to_string().into()
                }
            };

            column = column.child(self.render_row(row, value, body.into_any(), cx));
        }

        column.into_any()
    }
}

impl Render for TaskEnvironmentPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.sync_panel_subscription(window, cx);

        let ai_native_active = matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_));

        // No tab strip and no segmented control: the environment is a single
        // vertical column whose rows expand in place. Full editing and diff review
        // are an explicit trip to the code workspace, never a second surface here.
        let body = self.render_environment(cx);

        v_flex()
            .key_context("TaskEnvironmentPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .items_center()
                    .justify_between()
                    .child(
                        Label::new("Environment")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .when(ai_native_active, |this| {
                        this.child(
                            IconButton::new("task-environment-back-to-task", IconName::ArrowLeft)
                                .icon_size(IconSize::XSmall)
                                .tooltip(Tooltip::text("Back to task"))
                                .on_click(|_, window, cx| {
                                    // `ReturnToAiNativeTask` rather than a bare focus:
                                    // it also restores the AI Native layout, so the
                                    // control works after a trip to the code workspace.
                                    window.dispatch_action(
                                        Box::new(workspace::ReturnToAiNativeTask),
                                        cx,
                                    );
                                }),
                        )
                    }),
            )
            .child(Divider::horizontal())
            .child(div().flex_1().min_h_0().child(body))
    }
}

/// Activation priority for [`TaskEnvironmentPanel`].
///
/// Must stay unique across every registered panel: `workspace::dock::Dock::add_panel`
/// panics on a tie (debug assertions), and the application registers all panels at
/// startup — so a collision is a startup crash, not a cosmetic issue. 4 sits
/// between the git panel (3) and the collaboration panel (5), which keeps the
/// status bar order deterministic.
pub(crate) const TASK_INSPECTOR_ACTIVATION_PRIORITY: u32 = 4;

impl Panel for TaskEnvironmentPanel {
    fn persistent_name() -> &'static str {
        "TaskEnvironmentPanel"
    }

    fn panel_key() -> &'static str {
        TASK_ENVIRONMENT_PANEL_KEY
    }

    fn position(&self, _window: &Window, _cx: &App) -> workspace::dock::DockPosition {
        workspace::dock::DockPosition::Right
    }

    fn position_is_valid(&self, position: workspace::dock::DockPosition) -> bool {
        position == workspace::dock::DockPosition::Right
    }

    fn set_position(
        &mut self,
        _position: workspace::dock::DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // The inspector is only meaningful against the task's code evidence, so
        // it stays on the right. Dock position is not user configurable here.
    }

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(DEFAULT_INSPECTOR_WIDTH)
    }

    fn min_size(&self, _window: &Window, _cx: &App) -> Option<Pixels> {
        Some(px(180.))
    }

    fn icon(&self, _window: &Window, cx: &App) -> Option<IconName> {
        // Only surfaced while the AI Native layout is active; in Classic and
        // Agentic the inspector has no button of its own.
        matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_))
            .then_some(IconName::FileDiff)
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Task Inspector")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleTaskEnvironment)
    }

    /// Unique across every registered panel: `Dock::add_panel` panics on a
    /// collision, so this must not reuse another panel's priority. 4 sits between
    /// the git panel (3) and the collaboration panel (5), which keeps the status
    /// bar order deterministic.
    fn activation_priority(&self) -> u32 {
        TASK_INSPECTOR_ACTIVATION_PRIORITY
    }

    /// The AI Native layout wants the inspector open without the user asking.
    fn starts_open(&self, _window: &Window, cx: &App) -> bool {
        matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_))
    }
}

/// Resolves the agent thread the AI Native surfaces are currently showing.
pub(crate) fn active_agent_thread(
    workspace: &WeakEntity<Workspace>,
    cx: &App,
) -> Option<Entity<AcpThread>> {
    let workspace = workspace.upgrade()?;
    let panel = workspace.read(cx).panel::<AgentPanel>(cx)?;
    panel.read(cx).active_agent_thread(cx)
}

/// Registers the inspector's actions and keeps it in step with the AI Native
/// layout.
pub(crate) fn init(cx: &mut App) {
    update_inspector_action_filter(cx);
    cx.observe_global::<SettingsStore>(update_inspector_action_filter)
        .detach();

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        workspace.register_action(|workspace, _: &ToggleTaskEnvironment, window, cx| {
            workspace.toggle_panel_focus::<TaskEnvironmentPanel>(window, cx);
        });

        if let Some(window) = window {
            cx.observe_global_in::<SettingsStore>(window, |workspace, window, cx| {
                sync_task_environment(workspace, window, cx);
            })
            .detach();
        }
    })
    .detach();
}

/// Keeps the environment panel's action out of the command palette while AI is
/// disabled, matching how the panel-layout actions behave.
fn update_inspector_action_filter(cx: &mut App) {
    use std::any::TypeId;

    let disable_ai = project::DisableAiSettings::get_global(cx).disable_ai;
    let actions = [TypeId::of::<ToggleTaskEnvironment>()];

    command_palette_hooks::CommandPaletteFilter::update_global(cx, |filter, _| {
        if disable_ai {
            filter.hide_action_types(&actions);
        } else {
            filter.show_action_types(actions.iter());
        }
    });
}

/// Opens the inspector while AI Native is active, and gives the right dock back
/// when it is not.
///
/// Leaving AI Native only closes the dock when the inspector is what the user
/// is looking at, so a right dock the user had opened for something else is not
/// taken away.
fn sync_task_environment(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_)) {
        workspace.open_panel::<TaskEnvironmentPanel>(window, cx);
    } else {
        workspace.close_panel_if_active::<TaskEnvironmentPanel>(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{init_test as init_agent_ui, open_thread_with_connection};
    use crate::{AgentDiffPane, AiNativeConversationItem};
    use acp_thread::StubAgentConnection;
    use command_palette_hooks::CommandPaletteFilter;
    use fs::FakeFs;
    use gpui::{AnyWindowHandle, TestAppContext, UpdateGlobal, VisualTestContext};
    use project::Project;
    use settings::SettingsStore;
    use settings::settings_content::SaturatingBool;
    use workspace::{MultiWorkspace, Pane, dock::DockPosition};

    /// The dock combination `PanelLayout::AI_NATIVE` writes. Selecting AI Native
    /// from the menu writes exactly this; the tests drive the same path so the
    /// layout observers run for real.
    const AI_NATIVE_SETTINGS: &str = r#"{
        "agent": { "dock": "bottom" },
        "project_panel": { "dock": "right" },
        "outline_panel": { "dock": "right" },
        "collaboration_panel": { "dock": "right" },
        "git_panel": { "dock": "right" }
    }"#;

    fn init_test(cx: &mut TestAppContext) {
        init_agent_ui(cx);
        cx.update(|cx| {
            // `AgentPanel::new` reads the thread metadata store.
            crate::thread_metadata_store::ThreadMetadataStore::init_global(cx);
            // `task_inspector::init` installs a command palette filter, so the
            // global has to exist first — same order as application startup.
            command_palette_hooks::init(cx);
            crate::ai_native_conversation_item::init(cx);
            init(cx);
        });
    }

    async fn setup_workspace(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>) {
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let multi_workspace =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = multi_workspace
            .read_with(cx, |multi_workspace, _cx| {
                multi_workspace.workspace().clone()
            })
            .expect("test MultiWorkspace should own a workspace");
        (multi_workspace.into(), workspace)
    }

    fn add_inspector(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Entity<TaskEnvironmentPanel> {
        workspace.update_in(cx, |workspace, window, cx| {
            let workspace_entity = cx.entity();
            let inspector = cx.new(|cx| TaskEnvironmentPanel::new(workspace_entity, cx));
            workspace.add_panel(inspector.clone(), window, cx);
            inspector
        })
    }

    fn dock_is_open<T: Panel>(workspace: &Entity<Workspace>, cx: &VisualTestContext) -> bool {
        workspace.read_with(cx, |workspace, cx| {
            workspace.all_docks().iter().any(|dock| {
                let dock = dock.read(cx);
                dock.panel::<T>().is_some() && dock.is_open()
            })
        })
    }

    fn center_surface_is_active(pane: &Entity<Pane>, cx: &VisualTestContext) -> bool {
        pane.read_with(cx, |pane, cx| {
            pane.active_item()
                .is_some_and(|item| item.act_as::<AiNativeConversationItem>(cx).is_some())
        })
    }

    /// The environment panel is tab-less by contract: no tab strip, no segmented
    /// control, no tab index to switch. What used to be `Changes / Files / Review`
    /// is now one scrollable column of disclosure rows, so there is nothing here
    /// to select — and the dock key must keep its v1 value so a layout saved by
    /// the old inspector still restores this panel instead of leaving the right
    /// dock empty or registering a duplicate.
    #[gpui::test]
    async fn test_environment_panel_is_tab_less_and_keeps_its_dock_key(cx: &mut TestAppContext) {
        init_test(cx);
        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let _environment = add_inspector(&workspace, cx);

        assert_eq!(
            TaskEnvironmentPanel::panel_key(),
            "task_inspector_panel",
            "the dock key must keep the v1 value so saved layouts still restore"
        );
    }

    #[gpui::test]
    async fn test_inspector_is_right_dock_only(cx: &mut TestAppContext) {
        init_test(cx);
        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let inspector = add_inspector(&workspace, cx);

        assert!(
            inspector.read_with(cx, |inspector, _cx| inspector
                .position_is_valid(DockPosition::Right)),
            "the inspector belongs on the right, next to the code evidence"
        );
        assert!(!inspector.read_with(cx, |inspector, _cx| {
            inspector.position_is_valid(DockPosition::Left)
        }));
        assert!(!inspector.read_with(cx, |inspector, _cx| {
            inspector.position_is_valid(DockPosition::Bottom)
        }));

        assert_eq!(
            TaskEnvironmentPanel::persistent_name(),
            "TaskEnvironmentPanel"
        );
        assert_eq!(
            TaskEnvironmentPanel::panel_key(),
            TASK_ENVIRONMENT_PANEL_KEY
        );
    }

    #[gpui::test]
    fn test_environment_command_is_hidden_when_ai_is_disabled(cx: &mut TestAppContext) {
        init_test(cx);

        cx.update(|cx| {
            let filter = CommandPaletteFilter::try_global(cx).unwrap();
            assert!(!filter.is_hidden(&ToggleTaskEnvironment));

            // Written through user settings rather than `override_global` so it
            // survives `recompute_values` and reaches the registered settings.
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |content| {
                    content.project.disable_ai = Some(SaturatingBool(true));
                });
            });
        });
        cx.run_until_parked();

        cx.update(|cx| {
            assert!(
                project::DisableAiSettings::get_global(cx).disable_ai,
                "the test should actually have disabled AI"
            );

            let filter = CommandPaletteFilter::try_global(cx).unwrap();
            assert!(
                filter.is_hidden(&ToggleTaskEnvironment),
                "the environment panel must not be reachable while AI is disabled"
            );
        });
    }

    /// The single-surface rule, end to end: the conversation moves to the center
    /// pane, the dock-hosted panel stands down, the inspector takes the right
    /// dock, and opening code keeps a normal tab bar while "back to task"
    /// returns to the *same* surface.
    #[gpui::test]
    async fn test_ai_native_owns_the_center_surface_and_the_inspector(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        open_thread_with_connection(&panel, StubAgentConnection::new(), cx);

        let _inspector = add_inspector(&workspace, cx);

        assert!(
            panel.read_with(cx, |panel, _cx| panel.visible_conversation_view().is_some()),
            "the stub thread should give the panel a surface to hand over"
        );

        // Select AI Native exactly as the title bar menu does.
        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.set_user_settings(AI_NATIVE_SETTINGS, cx).unwrap();
            });
        });
        cx.run_until_parked();

        let pane = workspace.read_with(cx, |workspace, _cx| workspace.active_pane().clone());
        let center_item = workspace
            .read_with(cx, |workspace, cx| {
                workspace.item_of_type::<AiNativeConversationItem>(cx)
            })
            .expect("AI Native should deploy the center conversation surface");

        assert!(
            center_surface_is_active(&pane, cx),
            "the center surface should be the active item"
        );
        assert!(
            !dock_is_open::<AgentPanel>(&workspace, cx),
            "the dock-hosted AgentPanel must stay closed in AI Native"
        );
        assert!(
            dock_is_open::<TaskEnvironmentPanel>(&workspace, cx),
            "the Task Inspector should own the right dock in AI Native"
        );
        assert!(
            !pane.update_in(cx, |pane, window, cx| pane
                .should_display_tab_bar(window, cx)),
            "no tab bar over a conversation"
        );

        // Focusing the panel must reach the center surface, not reopen the dock.
        let focused = workspace.update_in(cx, |workspace, window, cx| {
            workspace.focus_panel::<AgentPanel>(window, cx)
        });
        assert!(
            focused.is_none(),
            "focusing the agent panel should be redirected to the center surface"
        );
        assert!(
            !dock_is_open::<AgentPanel>(&workspace, cx),
            "the redirect must not reopen the dock"
        );

        // Opening a code-scale item must NOT bring the tab row back. Under AI
        // Native the conversation is the only center content, so the tab bar is
        // off for the whole layout regardless of which item is active — this is
        // exactly the v1 loophole the v2 plan closes (§2.2, §6.2).
        let thread = panel.read_with(cx, |panel, cx| panel.active_agent_thread(cx).unwrap());
        workspace.update_in(cx, |workspace, window, cx| {
            AgentDiffPane::deploy_in_workspace(thread, workspace, window, cx);
        });
        cx.run_until_parked();
        assert!(
            !pane.update_in(cx, |pane, window, cx| pane
                .should_display_tab_bar(window, cx)),
            "opening a code item must not restore the tab row inside AI Native"
        );

        // "Back to task" returns to the same conversation, and does not create a
        // second surface.
        workspace.update_in(cx, |workspace, window, cx| {
            AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);
        });
        cx.run_until_parked();

        assert!(
            center_surface_is_active(&pane, cx),
            "returning to the task re-activates the center surface"
        );
        assert!(
            !pane.update_in(cx, |pane, window, cx| pane
                .should_display_tab_bar(window, cx)),
            "the tab bar hides again once the conversation is active"
        );
        assert_eq!(
            center_item.entity_id(),
            workspace
                .read_with(cx, |workspace, cx| {
                    workspace.item_of_type::<AiNativeConversationItem>(cx)
                })
                .unwrap()
                .entity_id(),
            "returning must reuse the existing center surface"
        );
    }

    /// The plan's §4.6.2 reading-column contract, asserted against real layout.
    ///
    /// The dock contract fills its container and introduces no column at all, so
    /// nothing changes for the agent panel. The AI Native center contract caps the
    /// conversation and centers it inside its container.
    #[gpui::test]
    async fn test_center_surface_presents_a_bounded_centered_reading_column(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
        });

        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        open_thread_with_connection(&panel, StubAgentConnection::new(), cx);
        let _inspector = add_inspector(&workspace, cx);
        cx.run_until_parked();

        // Select AI Native exactly as the title bar menu does. Only then is the
        // conversation actually rendered — the dock-hosted panel is stood down,
        // so the center pane is the one place it is drawn.
        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.set_user_settings(AI_NATIVE_SETTINGS, cx).unwrap();
            });
        });
        cx.run_until_parked();

        assert!(
            panel.read_with(cx, |panel, _cx| panel.visible_conversation_view().is_some()),
            "the stub thread should give the panel a surface to hand over"
        );

        let column = cx
            .debug_bounds("ai-native-reading-column")
            .expect("AI Native must present the conversation in a reading column");
        let body = cx
            .debug_bounds("conversation-body")
            .expect("the conversation body should render in the center pane");

        let cap = gpui::px(crate::CENTER_READING_COLUMN_WIDTH);
        println!(
            "reading-column: column={:?} body={:?} cap={cap:?}",
            column.size.width, body.size.width
        );

        assert!(
            column.size.width <= cap,
            "the center column {:?} must never exceed the cap {cap:?}",
            column.size.width
        );
        assert!(
            column.size.width <= body.size.width,
            "the center column {:?} must fit inside its container {:?}",
            column.size.width,
            body.size.width
        );

        if body.size.width > cap {
            assert!(
                column.size.width < body.size.width,
                "a container wider than the cap must leave gutters: \
                 column {:?} vs body {:?}",
                column.size.width,
                body.size.width
            );

            let left_gutter = column.left() - body.left();
            let right_gutter = body.right() - column.right();
            assert!(
                (left_gutter - right_gutter).abs() <= gpui::px(1.0),
                "the column must be centered: left gutter {left_gutter:?} vs \
                 right gutter {right_gutter:?}"
            );
        } else {
            assert_eq!(
                column.size.width, body.size.width,
                "a container narrower than the cap must be filled exactly"
            );
        }
    }

    /// A shared activation priority makes `Dock::add_panel` panic while the app
    /// registers its panels at startup, so the app never reaches a window. This
    /// locks the inspector off the priorities the other panels already own.
    #[test]
    fn test_inspector_activation_priority_is_unique() {
        // agent 0, project 1, terminal 2, git 3, collaboration 5, outline 6.
        const TAKEN: [u32; 6] = [0, 1, 2, 3, 5, 6];
        assert!(
            !TAKEN.contains(&TASK_INSPECTOR_ACTIVATION_PRIORITY),
            "activation priority {} collides with another panel; \
             Dock::add_panel panics on a tie",
            TASK_INSPECTOR_ACTIVATION_PRIORITY
        );
    }

    /// `OpenInCodeWorkspace` is the only sanctioned exit from AI Native to an
    /// editor, and the v2 plan fixes its fallback layout as Agentic. This also
    /// checks the exit is real navigation: the task state that owns the
    /// conversation is untouched, so the user can come back.
    #[gpui::test]
    async fn test_open_in_code_workspace_confirms_before_leaving(cx: &mut TestAppContext) {
        init_test(cx);
        // `AgentSettings::set_layout` writes through the global `Fs`.
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs, cx);
        });

        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        open_thread_with_connection(&panel, StubAgentConnection::new(), cx);
        let _inspector = add_inspector(&workspace, cx);

        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.set_user_settings(AI_NATIVE_SETTINGS, cx).unwrap();
            });
        });
        cx.run_until_parked();
        assert!(
            cx.update(|_window, app| matches!(
                AgentSettings::get_layout(app),
                WindowLayout::AiNative(_)
            )),
            "the fixture should have entered AI Native"
        );
        // No trip yet, so the code workspace must not offer `Return to task`.
        assert!(
            !cx.update(|_window, cx| workspace::ai_native_trip_active(cx)),
            "a fresh AI Native session is not a trip in progress"
        );

        // Asking first is deliberately inert: the confirmation layer writes no
        // layout state, so backing out of it costs the user nothing.
        workspace.update_in(cx, |_workspace, window, cx| {
            window.dispatch_action(Box::new(workspace::OpenInCodeWorkspace), cx);
        });
        cx.run_until_parked();

        assert!(
            cx.update(|_window, app| matches!(
                AgentSettings::get_layout(app),
                WindowLayout::AiNative(_)
            )),
            "asking must not leave AI Native on its own"
        );

        workspace.update_in(cx, |_workspace, window, cx| {
            window.dispatch_action(Box::new(workspace::ConfirmExitToCodeWorkspace), cx);
        });
        cx.run_until_parked();

        assert!(
            cx.update(|_window, app| matches!(
                AgentSettings::get_layout(app),
                WindowLayout::Agent(_)
            )),
            "ConfirmExitToCodeWorkspace must fall back to the Agentic layout"
        );
        // Now the code workspace *can* offer `Return to task`.
        assert!(
            cx.update(|_window, cx| workspace::ai_native_trip_active(cx)),
            "a confirmed trip must mark the round trip as in progress"
        );
        assert!(
            panel.read_with(cx, |panel, _cx| panel.visible_conversation_view().is_some()),
            "leaving must not tear down the panel that owns the task state"
        );
    }

    /// The center tab bar rule is a **layout** rule, not an item rule: while AI
    /// Native is active no center pane shows a tab bar, and outside AI Native the
    /// original predicate comes back — which is what keeps Classic and Agentic
    /// behaving exactly as before.
    #[gpui::test]
    async fn test_center_tab_bar_follows_the_layout(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs, cx);
        });

        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let tab_bar_visible = |workspace: &Entity<Workspace>, cx: &mut VisualTestContext| {
            workspace.update_in(cx, |workspace, window, cx| {
                let pane = workspace.active_pane().clone();
                pane.update(cx, |pane, cx| pane.should_display_tab_bar(window, cx))
            })
        };

        // Outside AI Native the predicate is whatever the settings say, so the
        // pane keeps its normal tab row.
        let outside = tab_bar_visible(&workspace, cx);

        // Entering AI Native hides it — this must hold whatever the pane holds.
        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.set_user_settings(AI_NATIVE_SETTINGS, cx).unwrap();
            });
        });
        cx.run_until_parked();

        assert!(
            !tab_bar_visible(&workspace, cx),
            "AI Native must hide the center pane's tab bar"
        );

        // Leaving restores the predicate that was in force before.
        workspace.update_in(cx, |_workspace, window, cx| {
            window.dispatch_action(Box::new(workspace::ConfirmExitToCodeWorkspace), cx);
        });
        cx.run_until_parked();

        assert_eq!(
            tab_bar_visible(&workspace, cx),
            outside,
            "leaving AI Native must restore the original tab bar predicate"
        );
    }

    /// The Branch and Commit/push rows follow the same rule: **no Git, no row**.
    /// The commit summary is built from the same branch facts, so it hides with them
    /// rather than showing a branch-less summary.
    #[gpui::test]
    async fn test_commit_row_hides_without_git(cx: &mut TestAppContext) {
        init_test(cx);
        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let environment = add_inspector(&workspace, cx);
        assert!(
            environment.read_with(cx, |environment, cx| environment.branch_fact(cx).is_none()),
            "the test project has no Git repository, so there must be no branch facts"
        );
    }

    /// Task-scoped facts come from the **active thread's** action log, so they can
    /// never leak another task's files — and with no thread active at all, nothing
    /// is fabricated: the Changes row simply has nothing to show.
    #[gpui::test]
    async fn test_task_scoped_change_facts_stay_scoped_to_the_active_thread(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        // No thread active: the adapter has nothing to scope to and must not
        // invent facts for the panel to render.
        assert!(
            cx.update(|_window, cx| facts::task_touched_paths(&workspace.downgrade(), cx))
                .is_empty(),
            "with no active thread there must be no task-scoped change facts"
        );

        // The same holds once the panel exists: a fresh draft has made no edits, so
        // the Changes row's facts are empty even though the panel is live.
        let environment = add_inspector(&workspace, cx);
        assert!(
            environment.read_with(cx, |environment, cx| environment
                .change_facts(cx)
                .is_empty()),
            "a fresh draft must not produce change facts"
        );

        // Whatever the panel does show stays within the project's real change set —
        // the task scope is a filter over real Git facts, never a second source.
        let project_paths: HashSet<String> = workspace.read_with(cx, |workspace, cx| {
            facts::all_changed_entries(&workspace.project().clone(), cx)
                .into_iter()
                .map(|(_, _, display)| display)
                .collect()
        });
        environment.read_with(cx, |environment, cx| {
            for fact in environment.change_facts(cx) {
                assert!(
                    project_paths.contains(fact.display_path.as_ref()),
                    "a task fact must come from the project's real change set"
                );
            }
        });
    }

    /// The outbound trip and the return trip are a matched pair: leaving puts the
    /// user in the code workspace, returning restores the AI Native layout *and* the
    /// same task — the round trip never creates a second conversation surface.
    #[gpui::test]
    async fn test_return_to_ai_native_task_restores_layout_and_task(cx: &mut TestAppContext) {
        init_test(cx);
        // `AgentSettings::set_layout` writes through the global `Fs`.
        let fs = FakeFs::new(cx.executor());
        cx.update(|cx| {
            agent::ThreadStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            <dyn fs::Fs>::set_global(fs, cx);
        });

        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
            workspace.add_panel(panel.clone(), window, cx);
            panel
        });
        open_thread_with_connection(&panel, StubAgentConnection::new(), cx);
        let _inspector = add_inspector(&workspace, cx);

        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.set_user_settings(AI_NATIVE_SETTINGS, cx).unwrap();
            });
        });
        cx.run_until_parked();

        workspace.update_in(cx, |_workspace, window, cx| {
            window.dispatch_action(Box::new(workspace::ConfirmExitToCodeWorkspace), cx);
        });
        cx.run_until_parked();
        assert!(
            cx.update(|_window, app| matches!(
                AgentSettings::get_layout(app),
                WindowLayout::Agent(_)
            )),
            "the fixture should have left for the code workspace"
        );

        workspace.update_in(cx, |_workspace, window, cx| {
            window.dispatch_action(Box::new(workspace::ReturnToAiNativeTask), cx);
        });
        cx.run_until_parked();

        assert!(
            cx.update(|_window, app| matches!(
                AgentSettings::get_layout(app),
                WindowLayout::AiNative(_)
            )),
            "ReturnToAiNativeTask must restore the AI Native layout"
        );
        // The trip is over, so the code workspace stops offering `Return to task`.
        assert!(
            !cx.update(|_window, cx| workspace::ai_native_trip_active(cx)),
            "returning must clear the trip marker"
        );
        assert!(
            panel.read_with(cx, |panel, _cx| panel.visible_conversation_view().is_some()),
            "the same panel must still own the task after a round trip"
        );
    }
}
