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

mod changes;
mod files;
mod review;

pub use changes::TaskChangesView;
pub use files::TaskFilesView;
pub use review::TaskReviewView;

use crate::{AgentPanel, AgentPanelEvent};
use acp_thread::AcpThread;
use agent_settings::{AgentSettings, WindowLayout};
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    Render, Subscription, WeakEntity, Window, actions, div, px,
};
use settings::{Settings as _, SettingsStore};
use ui::{
    Divider, ToggleButtonGroup, ToggleButtonGroupSize, ToggleButtonGroupStyle, ToggleButtonSimple,
    Tooltip, prelude::*,
};
use workspace::{FocusAiNativeConversation, Panel, Workspace, dock::PanelEvent};

pub(crate) const TASK_INSPECTOR_PANEL_KEY: &str = "task_inspector_panel";

/// `rems`, used for the tab strip height so it matches other compact strips.
const TAB_STRIP_HEIGHT: f32 = 24.;

/// Starting width of the right dock in AI Native. Only a default: the user can
/// drag the dock as usual and that choice is not overwritten.
const DEFAULT_INSPECTOR_WIDTH: f32 = 320.;

actions!(
    task_inspector,
    [
        /// Toggles focus on the Task Inspector panel.
        ToggleTaskInspector,
        /// Switches the Task Inspector to the Changes tab.
        ShowTaskChanges,
        /// Switches the Task Inspector to the Files tab.
        ShowTaskFiles,
        /// Switches the Task Inspector to the Review tab.
        ShowTaskReview,
    ]
);

/// The three fixed tabs of the Task Inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskInspectorTab {
    Changes,
    Files,
    Review,
}

impl TaskInspectorTab {
    fn index(self) -> usize {
        match self {
            Self::Changes => 0,
            Self::Files => 1,
            Self::Review => 2,
        }
    }
}

/// The single right-dock surface used by the AI Native layout.
pub struct TaskInspectorPanel {
    workspace: WeakEntity<Workspace>,
    active_tab: TaskInspectorTab,
    focus_handle: FocusHandle,
    changes_view: Entity<TaskChangesView>,
    files_view: Entity<TaskFilesView>,
    review_view: Entity<TaskReviewView>,
    /// The `AgentPanel` whose active thread this inspector follows. It is
    /// resolved lazily because panels are loaded concurrently at startup.
    panel: Option<Entity<AgentPanel>>,
    _panel_subscription: Option<Subscription>,
    _workspace_subscription: Subscription,
}

impl TaskInspectorPanel {
    /// Builds the panel and its three child views.
    ///
    /// Takes the workspace *entity* rather than a weak handle so the pane tab
    /// bar can be kept in step with item activation. `load` is the production
    /// entry point; tests construct the panel directly.
    ///
    /// Nothing here reads the workspace: the panel is constructed from inside a
    /// `Workspace` update, and the children re-sync their project and git
    /// handles on every render.
    pub(crate) fn new(workspace: Entity<Workspace>, cx: &mut Context<Self>) -> Self {
        let workspace_handle = workspace.downgrade();

        let changes_view = cx.new(|cx| TaskChangesView::new(workspace_handle.clone(), cx));
        let files_view = cx.new(|cx| TaskFilesView::new(workspace_handle.clone(), cx));
        let review_view = cx.new(|cx| TaskReviewView::new(workspace_handle.clone(), cx));

        // The pane tab bar is hidden only while the AI Native center surface is
        // the active item, so we have to follow item activation rather than the
        // layout alone.
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
            active_tab: TaskInspectorTab::Changes,
            focus_handle: cx.focus_handle(),
            changes_view,
            files_view,
            review_view,
            panel: None,
            _panel_subscription: None,
            _workspace_subscription: workspace_subscription,
        }
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |_workspace, _window, cx| {
            let workspace_entity = cx.entity().clone();
            cx.new(|cx| Self::new(workspace_entity, cx))
        })
    }

    /// The tab currently shown. Exposed for tests and for the layout contract
    /// that the inspector always comes up on **Changes**.
    pub fn active_tab(&self) -> TaskInspectorTab {
        self.active_tab
    }

    fn set_active_tab(&mut self, tab: TaskInspectorTab, cx: &mut Context<Self>) {
        if self.active_tab == tab {
            return;
        }
        self.active_tab = tab;
        cx.notify();
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
                        this.notify_children(cx);
                    }
                },
            )
        });
        self.panel = current;
        self.notify_children(cx);
    }

    fn notify_children(&self, cx: &mut Context<Self>) {
        self.changes_view.update(cx, |_, cx| cx.notify());
        self.files_view.update(cx, |_, cx| cx.notify());
        self.review_view.update(cx, |_, cx| cx.notify());
    }

    fn render_tab_strip(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl gpui::IntoElement {
        let ai_native_active = matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_));

        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_1()
            .items_center()
            .child(
                div().flex_1().min_w_0().child(
                    ToggleButtonGroup::single_row(
                        "task-inspector-tabs",
                        [
                            ToggleButtonSimple::new(
                                "Changes",
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_tab(TaskInspectorTab::Changes, cx)
                                }),
                            ),
                            ToggleButtonSimple::new(
                                "Files",
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_tab(TaskInspectorTab::Files, cx)
                                }),
                            ),
                            ToggleButtonSimple::new(
                                "Review",
                                cx.listener(|this, _, _, cx| {
                                    this.set_active_tab(TaskInspectorTab::Review, cx)
                                }),
                            ),
                        ],
                    )
                    .style(ToggleButtonGroupStyle::Outlined)
                    .size(ToggleButtonGroupSize::Custom(rems_from_px(
                        TAB_STRIP_HEIGHT,
                    )))
                    .label_size(LabelSize::Small)
                    .selected_index(self.active_tab.index())
                    .full_width(),
                ),
            )
            .when(ai_native_active, |this| {
                this.child(
                    IconButton::new("task-inspector-back-to-task", IconName::ArrowLeft)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Back to task"))
                        .on_click(|_, window, cx| {
                            // Route back to the single conversation surface; the
                            // open code items are left untouched.
                            window.dispatch_action(Box::new(FocusAiNativeConversation), cx);
                        }),
                )
            })
    }
}

impl EventEmitter<PanelEvent> for TaskInspectorPanel {}

impl Focusable for TaskInspectorPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TaskInspectorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.sync_panel_subscription(window, cx);

        let body = match self.active_tab {
            TaskInspectorTab::Changes => self.changes_view.clone().into_any_element(),
            TaskInspectorTab::Files => self.files_view.clone().into_any_element(),
            TaskInspectorTab::Review => self.review_view.clone().into_any_element(),
        };

        v_flex()
            .key_context("TaskInspectorPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .child(self.render_tab_strip(window, cx))
            .child(Divider::horizontal())
            .child(div().flex_1().min_h_0().child(body))
    }
}

impl Panel for TaskInspectorPanel {
    fn persistent_name() -> &'static str {
        "TaskInspectorPanel"
    }

    fn panel_key() -> &'static str {
        TASK_INSPECTOR_PANEL_KEY
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
        Box::new(ToggleTaskInspector)
    }

    fn activation_priority(&self) -> u32 {
        1
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

/// The `AgentPanel` that hosts the ACP session, if it has been added yet.
pub(crate) fn agent_panel(
    workspace: &WeakEntity<Workspace>,
    cx: &App,
) -> Option<Entity<AgentPanel>> {
    workspace.upgrade()?.read(cx).panel::<AgentPanel>(cx)
}

/// Registers the inspector's actions and keeps it in step with the AI Native
/// layout.
pub(crate) fn init(cx: &mut App) {
    update_inspector_action_filter(cx);
    cx.observe_global::<SettingsStore>(update_inspector_action_filter)
        .detach();

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        workspace.register_action(|workspace, _: &ToggleTaskInspector, window, cx| {
            workspace.toggle_panel_focus::<TaskInspectorPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &ShowTaskChanges, window, cx| {
            show_tab(workspace, TaskInspectorTab::Changes, window, cx);
        });
        workspace.register_action(|workspace, _: &ShowTaskFiles, window, cx| {
            show_tab(workspace, TaskInspectorTab::Files, window, cx);
        });
        workspace.register_action(|workspace, _: &ShowTaskReview, window, cx| {
            show_tab(workspace, TaskInspectorTab::Review, window, cx);
        });

        if let Some(window) = window {
            cx.observe_global_in::<SettingsStore>(window, |workspace, window, cx| {
                sync_task_inspector(workspace, window, cx);
            })
            .detach();
        }
    })
    .detach();
}

fn show_tab(
    workspace: &mut Workspace,
    tab: TaskInspectorTab,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(panel) = workspace.panel::<TaskInspectorPanel>(cx) {
        panel.update(cx, |panel, cx| panel.set_active_tab(tab, cx));
    }
    workspace.open_panel::<TaskInspectorPanel>(window, cx);
}

/// Keeps the inspector's actions out of the command palette while AI is
/// disabled, matching how the panel-layout actions behave.
fn update_inspector_action_filter(cx: &mut App) {
    use std::any::TypeId;

    let disable_ai = project::DisableAiSettings::get_global(cx).disable_ai;
    let actions = [
        TypeId::of::<ToggleTaskInspector>(),
        TypeId::of::<ShowTaskChanges>(),
        TypeId::of::<ShowTaskFiles>(),
        TypeId::of::<ShowTaskReview>(),
    ];

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
fn sync_task_inspector(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_)) {
        workspace.open_panel::<TaskInspectorPanel>(window, cx);
    } else {
        workspace.close_panel_if_active::<TaskInspectorPanel>(window, cx);
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
    ) -> Entity<TaskInspectorPanel> {
        workspace.update_in(cx, |workspace, window, cx| {
            let workspace_entity = cx.entity().clone();
            let inspector = cx.new(|cx| TaskInspectorPanel::new(workspace_entity, cx));
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

    #[gpui::test]
    async fn test_inspector_defaults_to_changes_and_switches_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let (window_handle, workspace) = setup_workspace(cx).await;
        let cx = &mut VisualTestContext::from_window(window_handle, cx);

        let inspector = add_inspector(&workspace, cx);
        assert_eq!(
            inspector.read_with(cx, |inspector, _cx| inspector.active_tab()),
            TaskInspectorTab::Changes,
            "the inspector always comes up on Changes"
        );

        for tab in [
            TaskInspectorTab::Files,
            TaskInspectorTab::Review,
            TaskInspectorTab::Changes,
        ] {
            workspace.update_in(cx, |workspace, window, cx| {
                show_tab(workspace, tab, window, cx)
            });
            assert_eq!(
                inspector.read_with(cx, |inspector, _cx| inspector.active_tab()),
                tab,
                "showing a tab should select it"
            );
        }
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

        assert_eq!(TaskInspectorPanel::persistent_name(), "TaskInspectorPanel");
        assert_eq!(TaskInspectorPanel::panel_key(), TASK_INSPECTOR_PANEL_KEY);
    }

    #[gpui::test]
    fn test_inspector_commands_are_hidden_when_ai_is_disabled(cx: &mut TestAppContext) {
        init_test(cx);

        cx.update(|cx| {
            let filter = CommandPaletteFilter::try_global(cx).unwrap();
            assert!(!filter.is_hidden(&ToggleTaskInspector));
            assert!(!filter.is_hidden(&ShowTaskChanges));
            assert!(!filter.is_hidden(&ShowTaskFiles));
            assert!(!filter.is_hidden(&ShowTaskReview));

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
                filter.is_hidden(&ToggleTaskInspector),
                "the inspector must not be reachable while AI is disabled"
            );
            assert!(filter.is_hidden(&ShowTaskChanges));
            assert!(filter.is_hidden(&ShowTaskFiles));
            assert!(filter.is_hidden(&ShowTaskReview));
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
            dock_is_open::<TaskInspectorPanel>(&workspace, cx),
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

        // Opening a code-scale item brings the standard tab bar back.
        let thread = panel.read_with(cx, |panel, cx| panel.active_agent_thread(cx).unwrap());
        workspace.update_in(cx, |workspace, window, cx| {
            AgentDiffPane::deploy_in_workspace(thread, workspace, window, cx);
        });
        cx.run_until_parked();
        assert!(
            pane.update_in(cx, |pane, window, cx| pane
                .should_display_tab_bar(window, cx)),
            "a normal item must get the standard tab bar back"
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
}
