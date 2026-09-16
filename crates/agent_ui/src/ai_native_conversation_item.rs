//! The center-pane surface of the AI Native layout.
//!
//! This item does **not** own an ACP session. It mirrors whichever surface
//! `AgentPanel` would render — the active `ConversationView`, or the active
//! `TerminalView` when the current thread is a terminal — so there is exactly
//! one conversation surface in the window: while AI Native is active the
//! dock-hosted `AgentPanel` stays closed and this item is the only visible
//! rendering position for it.
//!
//! The panel keeps owning the thread map, the ACP session, restoration and
//! persistence; the item only borrows its active surface and re-binds on
//! `AgentPanelEvent`.

use crate::{AgentPanel, AgentPanelEvent, ConversationSurface, ConversationView, TerminalId};
use agent_settings::{AgentSettings, WindowLayout};
use gpui::{
    Anchor, App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window, div,
};
use settings::{Settings as _, SettingsStore};
use std::sync::Arc;
use terminal_view::TerminalView;
use ui::{AlertModal, PopoverMenu, prelude::*};
use workspace::{
    ConfirmExitToCodeWorkspace, FocusAiNativeConversation, Item, ModalView, OpenInCodeWorkspace,
    Panel, ReturnToAiNativeTask, TabBarSettings, UseAiNativeLayout, Workspace, item::ItemEvent,
    pane::Pane,
};

/// Which surface the center pane is currently mirroring.
///
/// The variants wrap the panel's own entities. Nothing here is constructed by
/// the item, which is what keeps the single-surface rule true.
#[derive(Clone)]
enum CenterSurface {
    /// No active surface yet: the panel is still restoring, or the user has not
    /// started a task.
    Empty,
    Conversation(Entity<ConversationView>),
    /// A terminal thread. The terminal is not dressed up as a chat transcript —
    /// the header says so and the surface is the real terminal.
    Terminal {
        id: TerminalId,
        view: Entity<TerminalView>,
    },
}

impl CenterSurface {
    fn is_ready(&self) -> bool {
        !matches!(self, CenterSurface::Empty)
    }

    fn as_conversation(&self) -> Option<&Entity<ConversationView>> {
        match self {
            CenterSurface::Conversation(conversation_view) => Some(conversation_view),
            _ => None,
        }
    }
}

/// The center-pane surface for the AI Native layout.
pub struct AiNativeConversationItem {
    /// The panel that owns the ACP session and the thread map. Holding the
    /// panel (rather than the workspace) lets the item read the active surface
    /// without leasing the workspace entity.
    panel: Option<Entity<AgentPanel>>,
    surface: CenterSurface,
    focus_handle: FocusHandle,
    _panel_subscription: Option<Subscription>,
}

impl AiNativeConversationItem {
    pub(crate) fn new(
        panel: Option<Entity<AgentPanel>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            panel,
            surface: CenterSurface::Empty,
            focus_handle: cx.focus_handle(),
            _panel_subscription: None,
        };
        this.refresh_and_subscribe(window, cx);
        this
    }

    /// Finds the existing item in the workspace, or creates and activates one.
    pub fn deploy_in_workspace(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        if let Some(existing) = workspace.item_of_type::<AiNativeConversationItem>(cx) {
            workspace.activate_item(&existing, true, true, window, cx);
            return existing;
        }

        let panel = workspace.panel::<AgentPanel>(cx);
        let item = cx.new(|cx| Self::new(panel, window, cx));
        workspace.add_item_to_center(Box::new(item.clone()), window, cx);
        item
    }

    fn refresh_and_subscribe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_surface(cx);

        let Some(panel) = self.panel.clone() else {
            return;
        };

        // Re-bind whenever AgentPanel switches, restores or replaces its active
        // surface. The item never creates a second ConversationView.
        self._panel_subscription = Some(cx.subscribe_in(
            &panel,
            window,
            |this, _panel, _event: &AgentPanelEvent, _window, cx| {
                this.refresh_surface(cx);
                cx.notify();
            },
        ));
    }

    /// Mirrors whichever surface the panel would render.
    ///
    /// Using the panel's `visible_*` accessors — rather than re-deriving the
    /// active entry from the thread store — is what keeps the two in step when
    /// the user switches between an agent thread and a terminal thread.
    fn refresh_surface(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.clone() else {
            self.surface = CenterSurface::Empty;
            return;
        };

        let next_surface = {
            let panel = panel.read(cx);
            if let Some(conversation_view) = panel.visible_conversation_view() {
                CenterSurface::Conversation(conversation_view.clone())
            } else if let Some(id) = panel.active_terminal_id()
                && let Some(view) = panel.visible_terminal_view()
            {
                CenterSurface::Terminal {
                    id,
                    view: view.clone(),
                }
            } else {
                CenterSurface::Empty
            }
        };

        // This item is the only visible rendering position for the active
        // conversation, so it selects the `Center` layout contract. The view
        // keeps owning every piece of session state — only the presentation
        // differs (plan §4.6.1).
        if let Some(conversation_view) = next_surface.as_conversation() {
            conversation_view.update(cx, |view, cx| {
                view.set_surface(ConversationSurface::Center, cx);
            });
        }

        // A conversation this item is no longer presenting goes back to the dock
        // contract, so it renders exactly as before wherever else it appears.
        if let Some(previous) = self.surface.as_conversation()
            && next_surface.as_conversation().map(|next| next.entity_id())
                != Some(previous.entity_id())
        {
            previous.update(cx, |view, cx| {
                view.set_surface(ConversationSurface::Dock, cx);
            });
        }

        self.surface = next_surface;
    }
}

/// Registers the actions that drive the AI Native center surface.
pub(crate) fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        workspace.register_action(|workspace, _: &FocusAiNativeConversation, window, cx| {
            AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);
        });

        workspace.register_action(|workspace, _: &UseAiNativeLayout, window, cx| {
            use_ai_native_layout(workspace, window, cx);
        });

        workspace.register_action(|workspace, _: &OpenInCodeWorkspace, window, cx| {
            // Ask first. The modal writes nothing, so backing out costs nothing.
            workspace.toggle_modal(window, cx, |_, cx| ExitToCodeWorkspaceModal {
                focus_handle: cx.focus_handle(),
            });
        });

        workspace.register_action(|workspace, _: &ConfirmExitToCodeWorkspace, window, cx| {
            workspace.hide_modal(window, cx);
            leave_for_code_workspace(workspace, window, cx);
        });

        workspace.register_action(|workspace, _: &ReturnToAiNativeTask, window, cx| {
            return_to_ai_native_task(workspace, window, cx);
        });

        // Focusing the agent panel has to reach the center surface while AI
        // Native owns it; see `PanelFocusRedirect`.
        workspace.set_panel_focus_redirect(Arc::new(
            |workspace: &mut Workspace,
             window: &mut Window,
             cx: &mut Context<Workspace>,
             persistent_name: &'static str| {
                if persistent_name != AgentPanel::persistent_name()
                    || !matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_))
                {
                    return false;
                }
                let item = AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);
                window.focus(&item.read(cx).focus_handle(cx), cx);
                true
            },
        ));

        if let Some(window) = window {
            cx.observe_global_in::<SettingsStore>(window, |workspace, window, cx| {
                sync_ai_native_surface(workspace, window, cx);
            })
            .detach();
        }
    })
    .detach();
}

/// Applies the AI Native preset and brings the whole task layout up together.
///
/// The order is fixed by the v1.17.0 plan: write the preset, open the Threads
/// sidebar, bind the center surface, stand the dock-hosted agent panel down,
/// then put the cursor in the composer.
fn use_ai_native_layout(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // 1. Write the preset. The dock combination lands when the settings store
    //    reloads, which also re-runs `sync_ai_native_surface`; that observer is
    //    what completes the hand-over when the panel is still restoring its
    //    thread and has nothing to hand over yet.
    let fs = <dyn fs::Fs>::global(cx);
    drop(AgentSettings::set_layout(
        WindowLayout::AiNative(None),
        fs,
        cx,
    ));

    // 2. Threads live in the workspace sidebar. AI Native keeps it on the left
    //    (`agent.sidebar_side` is written with the preset above); opening it
    //    here does not move focus, so step 5 still wins.
    open_threads_sidebar(workspace, cx);

    // 3 & 4. Bind the center surface and stand the dock-hosted panel down.
    //    This also runs on the settings observer, but doing it here keeps the
    //    sequence correct when the preset write is a no-op — re-selecting AI
    //    Native while a code item is active should still return to the task.
    let item = AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);
    if item.read(cx).surface.is_ready() {
        workspace.close_panel_if_active::<AgentPanel>(window, cx);
    }

    // 5. Land focus in the composer.
    window.focus(&item.read(cx).focus_handle(cx), cx);
}

/// Leaves AI Native for the Agentic code workspace.
///
/// Nothing about the task is torn down: `AgentPanel` keeps owning the thread,
/// draft, queue and permissions across the layout change, so
/// [`FocusAiNativeConversation`] brings the user back to the same conversation.
///
/// Asks before leaving AI Native for the code workspace.
///
/// Leaving is reversible — [`return_to_ai_native_task`] comes back to the same task
/// — but the center pane changes shape, so the user is told what survives first.
/// This view only asks: it writes nothing, so cancelling is a genuine no-op. The
/// layout changes only once `ConfirmExitToCodeWorkspace` runs.
struct ExitToCodeWorkspaceModal {
    focus_handle: FocusHandle,
}

impl Focusable for ExitToCodeWorkspaceModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for ExitToCodeWorkspaceModal {}

impl ModalView for ExitToCodeWorkspaceModal {
    fn fade_out_background(&self) -> bool {
        false
    }
}

impl Render for ExitToCodeWorkspaceModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        AlertModal::new("ai-native-exit-modal")
            .width(rems(34.))
            .key_context("AiNativeExitModal")
            .on_action(
                cx.listener(|_, _: &ConfirmExitToCodeWorkspace, window, cx| {
                    window.dispatch_action(Box::new(ConfirmExitToCodeWorkspace), cx);
                }),
            )
            .title("Open in Code Workspace")
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new(
                        "Your task stays exactly as it is — the conversation, draft, queued \
                         messages, scroll position and any code files you already have open \
                         all come back when you return.",
                    ))
                    .child(
                        Label::new("Opens the Agentic layout with its usual editor tabs.")
                            .color(Color::Muted),
                    ),
            )
            .footer(
                h_flex()
                    .w_full()
                    .gap_1()
                    .justify_end()
                    .child(
                        Button::new("ai-native-exit-cancel", "Cancel")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
                    )
                    .child(
                        Button::new("ai-native-exit-confirm", "Open in Code Workspace").on_click(
                            cx.listener(|_, _, window, cx| {
                                window.dispatch_action(Box::new(ConfirmExitToCodeWorkspace), cx);
                            }),
                        ),
                    ),
            )
    }
}

/// Only the layout write is needed here. The settings observer runs
/// `sync_ai_native_surface`, which restores the pane's normal tab-bar predicate on
/// the way out — and because this is a real navigation rather than a silent state
/// change, the user's existing code items come back untouched.
fn leave_for_code_workspace(
    _workspace: &mut Workspace,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // Mark the trip so the code workspace can offer `Return to task` — a user who
    // switched layouts directly has no AI Native task to go back to.
    workspace::set_ai_native_trip_active(true, cx);
    let fs = <dyn fs::Fs>::global(cx);
    drop(AgentSettings::set_layout(WindowLayout::Agent(None), fs, cx));
}

/// Returns from a code workspace trip to the same AI Native task.
///
/// The inverse of [`open_in_code_workspace`], and deliberately just as small: it
/// writes the layout back and re-deploys the center surface. The task's thread,
/// draft, queue, permissions and scroll position live in `AgentPanel`, which the
/// outbound trip never tore down — so returning restores the user to exactly where
/// they left, with no new session and no new conversation surface.
fn return_to_ai_native_task(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // The trip is over; the entry disappears until the user leaves again.
    workspace::set_ai_native_trip_active(false, cx);
    let fs = <dyn fs::Fs>::global(cx);
    drop(AgentSettings::set_layout(
        WindowLayout::AiNative(None),
        fs,
        cx,
    ));
    AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);
}

fn open_threads_sidebar(workspace: &Workspace, cx: &mut Context<Workspace>) {
    let Some(multi_workspace) = workspace.multi_workspace().and_then(|weak| weak.upgrade()) else {
        return;
    };
    multi_workspace.update(cx, |multi_workspace, cx| {
        if multi_workspace.multi_workspace_enabled(cx) && !multi_workspace.sidebar_open() {
            multi_workspace.open_sidebar(cx);
        }
    });
}

/// Keeps the AI Native center surface and the dock-hosted AgentPanel in sync.
///
/// When AI Native is selected the center item becomes the only visible
/// rendering position for the conversation, so the dock-hosted AgentPanel is
/// closed. Closing it does not destroy it: AgentPanel stays the lifecycle host
/// (it owns the thread map, the ACP session and persistence) and the center
/// item only borrows its active surface.
fn sync_ai_native_surface(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // Keep the pane tab bar in step with the layout regardless of which branch
    // we take below: leaving AI Native has to give the standard tab bar back.
    let panes = workspace.panes().to_vec();
    sync_center_tab_bar(panes, cx);

    if !matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_)) {
        return;
    }

    let item = AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);

    // Only close the dock-hosted panel once it actually has a surface to hand
    // over. Otherwise the panel would stop initializing its thread and the
    // center surface would have nothing to render.
    if item.read(cx).surface.is_ready() {
        workspace.close_panel_if_active::<AgentPanel>(window, cx);
    }
}

/// Keeps the center pane's tab bar off for the whole AI Native layout, and
/// restores the standard predicate for every other layout.
///
/// The decision comes from the layout alone, not from the pane's active item.
/// AI Native presents one conversation as the only center content, so activating
/// a code item must not bring a tab row back — leaving the layout is what restores
/// the pane's normal predicate and the user's existing items.
pub(crate) fn sync_center_tab_bar(panes: Vec<Entity<Pane>>, cx: &mut App) {
    let ai_native = matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_));

    for pane in panes {
        pane.update(cx, |pane, cx| {
            if ai_native {
                pane.set_should_display_tab_bar(|_, _| false);
            } else {
                pane.set_should_display_tab_bar(|_, cx| TabBarSettings::get_global(cx).show);
            }
            cx.notify();
        });
    }
}

impl EventEmitter<ItemEvent> for AiNativeConversationItem {}

impl Focusable for AiNativeConversationItem {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate to the surface so focus lands on the composer/editor the
        // user expects, and so `AgentPanel`'s activation focus stays consistent.
        match &self.surface {
            CenterSurface::Conversation(conversation_view) => {
                conversation_view.read(cx).focus_handle(cx)
            }
            CenterSurface::Terminal { view, .. } => view.read(cx).focus_handle(cx),
            CenterSurface::Empty => self.focus_handle.clone(),
        }
    }
}

impl Render for AiNativeConversationItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content: AnyElement = match self.surface.clone() {
            CenterSurface::Empty => self.render_empty_state(cx).into_any_element(),
            // The conversation draws its own task header under the `Center`
            // contract, so stacking a second one here would double the identity.
            CenterSurface::Conversation(conversation_view) => conversation_view.into_any_element(),
            CenterSurface::Terminal { id, view } => v_flex()
                .size_full()
                .child(self.render_terminal_header(id, cx))
                .child(view)
                .into_any_element(),
        };

        div().size_full().child(content)
    }
}

impl AiNativeConversationItem {
    /// Terminal threads keep their own identity instead of being dressed up as
    /// a conversation: icon, real terminal title, and an explicit label.
    fn render_terminal_header(&self, id: TerminalId, cx: &mut Context<Self>) -> Div {
        let info = self.panel.as_ref().and_then(|panel| {
            panel
                .read(cx)
                .terminals(cx)
                .into_iter()
                .find(|info| info.id == id)
        });
        let title: SharedString = info
            .and_then(|info| info.custom_title.or(Some(info.title)))
            .unwrap_or_else(|| SharedString::from("Terminal"));

        h_flex()
            .h_8()
            .px_2()
            .gap_2()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Icon::new(IconName::Terminal)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(Label::new(title).size(LabelSize::Small).truncate()),
            )
            .child(
                Label::new("Terminal thread")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    /// No active thread yet.
    ///
    /// AI Native is a conversation-first layout, so the empty state still shows
    /// the way in — but it must not start an agent on the user's behalf.
    fn render_empty_state(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Label::new("No active task")
                    .size(LabelSize::Large)
                    .color(Color::Muted),
            )
            .child(
                Label::new("Pick a thread on the left, or start a new one.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(self.render_new_conversation_chooser(cx))
    }

    /// The AI Native entry point for a new conversation.
    ///
    /// A chooser rather than a direct `NewThread`: the plan makes the agent type an
    /// explicit first step, and the options come from the same `NewThreadChoice`
    /// model the AgentPanel toolbar renders, so the two entry points cannot
    /// disagree about which agents exist or what they are called.
    fn render_new_conversation_chooser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.panel.as_ref().map(|panel| panel.read(cx).workspace());
        let focus_handle = cx.focus_handle();

        PopoverMenu::new("ai-native-new-conversation")
            .trigger(
                Button::new("ai-native-new-conversation", "New conversation")
                    .style(ButtonStyle::Outlined)
                    .label_size(LabelSize::Small),
            )
            .anchor(Anchor::TopLeft)
            .menu({
                move |window, cx| {
                    Some(crate::agent_panel::build_agent_choice_menu(
                        workspace.as_ref(),
                        false,
                        focus_handle.clone(),
                        window,
                        cx,
                    ))
                }
            })
    }
}

impl Item for AiNativeConversationItem {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "AI Conversation".into()
    }

    fn include_in_nav_history() -> bool {
        false
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn deactivated(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Release the subscription so a closed item cannot keep observing the
        // panel. The session itself is still owned and persisted by AgentPanel.
        self._panel_subscription = None;
        cx.notify();
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event);
    }
}
