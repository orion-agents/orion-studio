use crate::{AgentPanel, AgentPanelEvent, ConversationView};
use acp_thread::ThreadStatus;
use agent_settings::{AgentSettings, WindowLayout};
use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, ParentElement,
    Render, Styled, Subscription, WeakEntity, Window, actions, div,
};
use settings::SettingsStore;
use ui::{Color, Label, LabelCommon, LabelSize, h_flex, v_flex};
use workspace::{Item, Workspace, item::ItemEvent};

actions!(
    agent,
    [
        /// Focuses the AI Native conversation surface in the center pane.
        FocusAiNativeConversation,
    ]
);

/// The center-pane surface for the AI Native layout.
///
/// This item does **not** own an ACP session. It renders the same
/// `ConversationView` that `AgentPanel` keeps as its active surface, so there is
/// exactly one conversation surface in the window: when AI Native is active the
/// dock-hosted `AgentPanel` stays closed and this item is the only visible
/// rendering position for it.
pub struct AiNativeConversationItem {
    workspace: WeakEntity<Workspace>,
    conversation_view: Option<Entity<ConversationView>>,
    focus_handle: FocusHandle,
    _panel_subscription: Option<Subscription>,
}

impl AiNativeConversationItem {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            workspace,
            conversation_view: None,
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

        let item = cx.new(|cx| Self::new(workspace.weak_handle(), window, cx));
        workspace.add_item_to_center(Box::new(item.clone()), window, cx);
        item
    }

    fn refresh_and_subscribe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(panel) = workspace.read(cx).panel::<AgentPanel>(cx) else {
            return;
        };

        self.conversation_view = panel.read(cx).active_conversation_view().cloned();

        // Re-bind whenever AgentPanel switches, restores or replaces its active
        // surface. The item never creates a second ConversationView.
        self._panel_subscription = Some(cx.subscribe_in(
            &panel,
            window,
            |this, panel, _event: &AgentPanelEvent, _window, cx| {
                this.conversation_view = panel.read(cx).active_conversation_view().cloned();
                cx.notify();
            },
        ));
    }
}

/// Registers the actions that drive the AI Native center surface.
pub(crate) fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        workspace.register_action(|workspace, _: &FocusAiNativeConversation, window, cx| {
            AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);
        });
        if let Some(window) = window {
            cx.observe_global_in::<SettingsStore>(window, |workspace, window, cx| {
                sync_ai_native_surface(workspace, window, cx);
            })
            .detach();
        }
    })
    .detach();
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
    if !matches!(AgentSettings::get_layout(cx), WindowLayout::AiNative(_)) {
        return;
    }

    AiNativeConversationItem::deploy_in_workspace(workspace, window, cx);
    workspace.close_panel::<AgentPanel>(window, cx);
}

impl EventEmitter<ItemEvent> for AiNativeConversationItem {}

impl Focusable for AiNativeConversationItem {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate to the conversation so focus lands on the composer/editor the
        // user expects, and so `AgentPanel`'s activation focus stays consistent.
        self.conversation_view
            .as_ref()
            .map(|conversation_view| conversation_view.read(cx).focus_handle(cx).clone())
            .unwrap_or_else(|| self.focus_handle.clone())
    }
}

impl Render for AiNativeConversationItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let mut container = div().size_full();
        if let Some(conversation_view) = self.conversation_view.clone() {
            let header = self.render_header(cx);
            container =
                container.child(v_flex().size_full().child(header).child(conversation_view));
        }
        container
    }
}

impl AiNativeConversationItem {
    /// Single-line task header: thread title plus the current status.
    ///
    /// The thread title editor is owned by the thread view (the agent panel only
    /// renders it), so it is re-hosted here to keep the task identity visible
    /// while the dock-hosted panel stays closed.
    fn render_header(&self, cx: &mut Context<Self>) -> gpui::Div {
        let title_and_status = self
            .conversation_view
            .as_ref()
            .and_then(|conversation_view| {
                let conversation_view = conversation_view.read(cx);
                conversation_view.active_thread().map(|thread_view| {
                    let thread_view = thread_view.read(cx);
                    let title_editor = thread_view.title_editor.clone();
                    let status = thread_view.thread.read(cx).status();
                    (title_editor, status)
                })
            });

        let mut header = h_flex()
            .h_8()
            .px_2()
            .gap_2()
            .items_center()
            .justify_between();

        if let Some((title_editor, status)) = title_and_status {
            let status_label = match status {
                ThreadStatus::Idle => "Idle",
                ThreadStatus::Generating => "Running",
            };
            header = header.child(title_editor).child(
                Label::new(status_label)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }

        header
    }
}

impl Item for AiNativeConversationItem {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> gpui::SharedString {
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
