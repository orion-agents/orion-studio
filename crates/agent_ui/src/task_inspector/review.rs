//! The **Review** tab of the Task Inspector.
//!
//! Scoped to the *current agent thread*: it lists the buffers that thread still
//! has unreviewed edits in, with per-file line counts, and offers a single
//! explicit entry point into the full review experience.
//!
//! It intentionally does not re-implement accept/reject. That state machine
//! lives in `AgentDiff` and is keyed by buffer within the action log; a second
//! copy here could act on the wrong task's worktree. The right pane summarises
//! and routes; `AgentDiffPane` remains the place where edits are accepted or
//! rejected.

use super::{active_agent_thread, agent_panel};
use acp_thread::{AcpThread, ThreadStatus};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, Render, Subscription, WeakEntity, Window,
    div,
};
use project::ProjectPath;
use ui::{Divider, prelude::*};
use workspace::Workspace;

use crate::{AgentDiffPane, AgentPanel, AgentPanelEvent};

type ReviewFile = (Option<ProjectPath>, String, u32, u32);

pub struct TaskReviewView {
    workspace: WeakEntity<Workspace>,
    panel: Option<Entity<AgentPanel>>,
    thread: Option<Entity<AcpThread>>,
    focus_handle: FocusHandle,
    _panel_subscription: Option<Subscription>,
    _action_log_subscription: Option<Subscription>,
}

impl TaskReviewView {
    pub(crate) fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            panel: None,
            thread: None,
            focus_handle: cx.focus_handle(),
            _panel_subscription: None,
            _action_log_subscription: None,
        }
    }

    /// Re-binds to the agent panel and the thread's action log.
    ///
    /// Called from `render` because both the panel and the active thread can be
    /// replaced at any time — during startup, on thread switch, and on thread
    /// restore.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = agent_panel(&self.workspace, cx);
        if panel.as_ref().map(|panel| panel.entity_id())
            != self.panel.as_ref().map(|panel| panel.entity_id())
        {
            self._panel_subscription = panel.as_ref().map(|panel| {
                cx.subscribe_in(panel, window, |_, _, _: &AgentPanelEvent, _, cx| {
                    cx.notify()
                })
            });
            self.panel = panel;
            // Force the thread binding below to be re-resolved.
            self.thread = None;
        }

        let thread = active_agent_thread(&self.workspace, cx);
        if thread.as_ref().map(|thread| thread.entity_id())
            != self.thread.as_ref().map(|thread| thread.entity_id())
        {
            self._action_log_subscription = thread.as_ref().map(|thread| {
                let action_log = thread.read(cx).action_log().clone();
                cx.observe(&action_log, |_, _, cx| cx.notify())
            });
            self.thread = thread;
        }
    }

    /// Every buffer this thread still has unreviewed edits in.
    fn unreviewed_files(&self, cx: &App) -> Vec<ReviewFile> {
        let Some(thread) = self.thread.as_ref() else {
            return Vec::new();
        };
        let action_log = thread.read(cx).action_log().clone();
        let action_log = action_log.read(cx);

        let mut files: Vec<ReviewFile> = action_log
            .changed_buffers(cx)
            .map(|(buffer, diff)| {
                let buffer = buffer.read(cx);
                let file = buffer.file();
                let project_path = file.map(|file| ProjectPath {
                    worktree_id: file.worktree_id(cx),
                    path: file.path().clone(),
                });
                let display = project_path
                    .as_ref()
                    .map(|path| path.path.as_unix_str().to_string())
                    .unwrap_or_else(|| "untitled".to_string());
                let (added, removed) = diff.read(cx).snapshot(cx).changed_row_counts();
                (project_path, display, added, removed)
            })
            .collect();

        files.sort_by(|a, b| a.1.cmp(&b.1));
        files
    }

    fn open_file(&self, project_path: ProjectPath, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace
            .update(cx, |workspace, cx| {
                workspace.open_path_preview(project_path, None, true, true, true, window, cx)
            })
            .detach_and_log_err(cx);
    }

    /// Hands off to the existing review experience. The pane becomes a normal
    /// center item; the conversation surface stays loaded behind it and the
    /// inspector keeps a way back to the task.
    fn open_full_review(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.thread.clone() else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            AgentDiffPane::deploy_in_workspace(thread.clone(), workspace, window, cx);
        });
    }

    fn render_header(&self, cx: &App) -> AnyElement {
        let thread = self.thread.as_ref();
        let status_label = match thread.map(|thread| thread.read(cx).status()) {
            Some(ThreadStatus::Generating) => "Running",
            Some(ThreadStatus::Idle) => "Idle",
            None => "No task",
        };
        let title = thread
            .and_then(|thread| thread.read(cx).title())
            .map(|title| title.to_string())
            .unwrap_or_else(|| "No task selected".to_string());
        let (added, removed) = thread
            .map(|thread| {
                let action_log = thread.read(cx).action_log().clone();
                let stats = action_log.read(cx).diff_stats(cx);
                (stats.lines_added, stats.lines_removed)
            })
            .unwrap_or((0, 0));

        let summary = if added == 0 && removed == 0 {
            "Nothing pending review".to_string()
        } else {
            format!("{added} added, {removed} removed")
        };

        v_flex()
            .w_full()
            .px_2()
            .py_2()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(Label::new(title).size(LabelSize::Small).truncate())
                    .child(
                        Label::new(status_label)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
            .child(
                Label::new(summary)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .into_any_element()
    }

    fn render_empty_state(&self, files_are_empty: bool) -> AnyElement {
        let message = if self.thread.is_none() {
            "Select or start a task to review its changes."
        } else if files_are_empty {
            "All of this task's edits have been reviewed."
        } else {
            "No files to review."
        };

        v_flex()
            .px_3()
            .py_4()
            .child(
                Label::new(message)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .into_any_element()
    }

    fn render_row(&self, index: usize, file: &ReviewFile, cx: &mut Context<Self>) -> AnyElement {
        let (project_path, display, added, removed) = file;
        let colors = cx.theme().colors();
        let file_name = project_path
            .as_ref()
            .and_then(|path| path.path.file_name())
            .map(|name| name.to_string())
            .unwrap_or_else(|| display.clone());
        let directory = project_path
            .as_ref()
            .and_then(|path| path.path.parent())
            .map(|parent| parent.as_unix_str().to_string())
            .unwrap_or_default();
        let open_path = project_path.clone();

        h_flex()
            .id(("task-review-row", index))
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .when_some(open_path, |this, open_path| {
                this.cursor_pointer()
                    .hover(|this| this.bg(colors.element_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_file(open_path.clone(), window, cx)
                    }))
            })
            .child(
                Icon::new(IconName::Diff)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(Label::new(file_name).size(LabelSize::Small).truncate())
                    .when(!directory.is_empty(), |this| {
                        this.child(
                            Label::new(directory.clone())
                                .size(LabelSize::XSmall)
                                .color(Color::Muted)
                                .truncate(),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .child(
                        Label::new(format!("+{added}"))
                            .size(LabelSize::XSmall)
                            .color(Color::Success),
                    )
                    .child(
                        Label::new(format!("-{removed}"))
                            .size(LabelSize::XSmall)
                            .color(Color::Error),
                    ),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .child(
                Button::new("task-review-open-full", "Open Full Review")
                    .label_size(LabelSize::Small)
                    .style(ButtonStyle::Outlined)
                    .disabled(self.thread.is_none())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_full_review(window, cx);
                    })),
            )
            .into_any_element()
    }
}

impl Focusable for TaskReviewView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TaskReviewView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(window, cx);

        let files = self.unreviewed_files(cx);

        let mut list = v_flex()
            .id("task-review-list")
            .size_full()
            .overflow_y_scroll();

        if files.is_empty() {
            let empty = self.render_empty_state(files.is_empty());
            list = list.child(empty);
        } else {
            for (index, file) in files.iter().enumerate() {
                list = list.child(self.render_row(index, file, cx));
            }
        }

        v_flex()
            .size_full()
            .overflow_hidden()
            .child(self.render_header(cx))
            .child(Divider::horizontal())
            .child(div().flex_1().min_h_0().child(list))
            .child(self.render_footer(cx))
    }
}
