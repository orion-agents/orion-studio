//! The **Changes** tab of the Task Inspector.
//!
//! A compact, read-mostly view over the working tree's git status. It
//! deliberately does *not* embed `GitPanel`: no commit form, no history, no
//! repository-level settings, and no second `GitPanel` entity. It reads the
//! same `GitStore` snapshot the full panel reads, so the two never disagree.
//!
//! Scope switches between the current agent task and the whole working tree.
//! Selecting a file opens it as a normal code item *and* shows a compact patch
//! preview built from the task's own `BufferDiff` — not from a second diff
//! engine.

use super::active_agent_thread;
use git::status::{DiffStat, FileStatus};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, Render, Subscription, WeakEntity, Window,
    div, px,
};
use language::{Buffer, Point};
use project::{
    Project, ProjectPath,
    git_store::{GitStore, StatusEntry},
};
use std::collections::HashSet;
use ui::{Divider, TintColor, Tooltip, prelude::*};
use workspace::Workspace;

/// Which files the Changes tab lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChangeScope {
    /// Only files the current agent task has edited and that are still pending
    /// review.
    Task,
    /// Every file with working-tree changes in the project.
    All,
}

/// Everything the preview needs about the selected file. Owned so that row
/// click handlers can capture it without borrowing the row list.
#[derive(Clone)]
struct Selection {
    repo_path: String,
    project_path: Option<ProjectPath>,
    diff_stat: Option<DiffStat>,
}

type Row = (String, ProjectPath, StatusEntry, bool);

pub struct TaskChangesView {
    workspace: WeakEntity<Workspace>,
    scope: ChangeScope,
    selected: Option<Selection>,
    focus_handle: FocusHandle,
    project: Option<Entity<Project>>,
    _git_store_subscription: Option<Subscription>,
}

impl TaskChangesView {
    /// The view is built without touching the workspace.
    ///
    /// `render` re-syncs the git store on every frame, and the panel is
    /// constructed from inside a `Workspace` update — reading the workspace
    /// here would panic.
    pub(crate) fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        Self {
            workspace,
            scope: ChangeScope::Task,
            selected: None,
            focus_handle: cx.focus_handle(),
            project: None,
            _git_store_subscription: None,
        }
    }

    /// Binds to the project's `GitStore` so status updates repaint the list.
    /// Re-read on every render: the inspector outlives project replacements.
    fn sync_git_store(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let project = workspace.read(cx).project().clone();

        if self.project.as_ref().map(|project| project.entity_id()) == Some(project.entity_id()) {
            return;
        }

        let git_store: Entity<GitStore> = project.read(cx).git_store().clone();
        self._git_store_subscription =
            Some(cx.observe_in(&git_store, window, |_, _, _, cx| cx.notify()));
        self.project = Some(project);
    }

    /// The files the current agent task has recorded edits against.
    fn task_touched_paths(&self, cx: &App) -> HashSet<ProjectPath> {
        let mut paths = HashSet::default();
        let Some(thread) = active_agent_thread(&self.workspace, cx) else {
            return paths;
        };
        let action_log = thread.read(cx).action_log().clone();
        let action_log = action_log.read(cx);

        for (buffer, _) in action_log.changed_buffers(cx) {
            let buffer = buffer.read(cx);
            if let Some(file) = buffer.file() {
                paths.insert(ProjectPath {
                    worktree_id: file.worktree_id(cx),
                    path: file.path().clone(),
                });
            }
        }
        paths
    }

    /// Every changed file in the working tree, regardless of scope.
    fn all_entries(&self, cx: &App) -> Vec<(ProjectPath, StatusEntry, String)> {
        let Some(project) = self.project.as_ref() else {
            return Vec::new();
        };
        let git_store: Entity<GitStore> = project.read(cx).git_store().clone();
        let git_store = git_store.read(cx);

        let mut entries = Vec::new();
        for repository in git_store.repositories().values() {
            let repository = repository.read(cx);
            let snapshot = repository.snapshot();
            for entry in snapshot.status() {
                if !entry.status.has_changes() {
                    continue;
                }
                let Some(project_path) = repository.repo_path_to_project_path(&entry.repo_path, cx)
                else {
                    continue;
                };
                let display = entry.repo_path.as_std_path().to_string_lossy().into_owned();
                entries.push((project_path, entry, display));
            }
        }
        entries.sort_by(|a, b| a.2.cmp(&b.2));
        entries
    }

    /// The rows for the active scope, task-touched files first.
    fn rows(&self, cx: &App) -> Vec<Row> {
        let task_paths = match self.scope {
            ChangeScope::Task => self.task_touched_paths(cx),
            ChangeScope::All => HashSet::default(),
        };

        let mut rows: Vec<Row> = self
            .all_entries(cx)
            .into_iter()
            .filter_map(|(project_path, entry, display)| {
                let is_task_touched = task_paths.contains(&project_path);
                if self.scope == ChangeScope::Task && !is_task_touched {
                    return None;
                }
                Some((display, project_path, entry, is_task_touched))
            })
            .collect();

        rows.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));
        rows
    }

    fn has_any_change(&self, cx: &App) -> bool {
        !self.all_entries(cx).is_empty()
    }

    /// The task's own diff for a file, when the task is what changed it.
    fn task_diff(
        &self,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Option<(Entity<Buffer>, Entity<buffer_diff::BufferDiff>)> {
        let thread = active_agent_thread(&self.workspace, cx)?;
        let action_log = thread.read(cx).action_log().clone();
        let action_log = action_log.read(cx);
        action_log.changed_buffers(cx).find(|(buffer, _)| {
            buffer
                .read(cx)
                .file()
                .is_some_and(|file| file.path().as_ref() == project_path.path.as_ref())
        })
    }

    /// Clicking a file both previews it and opens it as a normal code item, so
    /// the right pane never replaces the editor — it points at it.
    fn select_and_open(
        &mut self,
        selection: Selection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let already_selected = self
            .selected
            .as_ref()
            .is_some_and(|current| current.repo_path == selection.repo_path);
        self.selected = if already_selected {
            None
        } else {
            Some(selection.clone())
        };

        if let Some(project_path) = selection.project_path
            && let Some(workspace) = self.workspace.upgrade()
        {
            workspace
                .update(cx, |workspace, cx| {
                    workspace.open_path_preview(project_path, None, true, true, true, window, cx)
                })
                .detach_and_log_err(cx);
        }

        cx.notify();
    }

    fn scope_button_style(&self, scope: ChangeScope) -> ButtonStyle {
        if self.scope == scope {
            ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ButtonStyle::Subtle
        }
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .justify_between()
            .child(
                Label::new("Changed files")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("task-changes-scope-task", "Task")
                            .label_size(LabelSize::Small)
                            .style(self.scope_button_style(ChangeScope::Task))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.scope = ChangeScope::Task;
                                this.selected = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("task-changes-scope-all", "All")
                            .label_size(LabelSize::Small)
                            .style(self.scope_button_style(ChangeScope::All))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.scope = ChangeScope::All;
                                this.selected = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// An empty list must never be a dead end: the Task scope offers a
    /// one-click way to the working-tree view.
    fn render_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let other_scope_has_rows = self.scope == ChangeScope::Task && self.has_any_change(cx);

        let (title, detail) = match self.scope {
            ChangeScope::Task if other_scope_has_rows => (
                "No changes from this task",
                "This task has not edited any files yet.",
            ),
            ChangeScope::Task => ("No changes from this task", "Nothing to review right now."),
            ChangeScope::All => ("No changes", "The working tree is clean."),
        };

        v_flex()
            .px_3()
            .py_4()
            .gap_1()
            .child(Label::new(title).size(LabelSize::Small))
            .child(
                Label::new(detail)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .when(other_scope_has_rows, |this| {
                this.child(
                    div().pt_2().child(
                        Button::new("task-changes-show-all", "Show all changes")
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.scope = ChangeScope::All;
                                cx.notify();
                            })),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_row(&self, index: usize, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let (display, project_path, entry, is_task_touched) = row;
        let colors = cx.theme().colors();
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|selection| &selection.repo_path == display);

        let (status_letter, status_color) = status_letter_and_color(entry.status);
        let diff_stat = diff_stat_for(entry);
        let file_name = project_path
            .path
            .file_name()
            .map(|name| name.to_string())
            .unwrap_or_default();
        let directory = project_path
            .path
            .parent()
            .map(|parent| parent.as_unix_str().to_string())
            .unwrap_or_default();

        let selection = Selection {
            repo_path: display.clone(),
            project_path: Some(project_path.clone()),
            diff_stat,
        };

        h_flex()
            .id(("task-change-row", index))
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .cursor_pointer()
            .when(selected, |this| this.bg(colors.element_selected))
            .hover(|this| this.bg(colors.element_hover))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_and_open(selection.clone(), window, cx);
            }))
            .child(
                div().w(px(12.)).flex_shrink_0().child(
                    Label::new(status_letter)
                        .size(LabelSize::XSmall)
                        .color(status_color),
                ),
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
            .when(*is_task_touched, |this| {
                this.child(
                    Icon::new(IconName::OrionAssistant)
                        .size(IconSize::XSmall)
                        .color(Color::Accent),
                )
            })
            .child(render_diff_stat(diff_stat))
            .into_any_element()
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selection = self.selected.as_ref()?;
        let colors = cx.theme().colors();

        let mut body = v_flex().w_full().gap_1().px_2().py_1().child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .justify_between()
                .child(
                    Label::new(selection.repo_path.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                )
                .child(render_diff_stat(selection.diff_stat)),
        );

        let patch = selection
            .project_path
            .as_ref()
            .and_then(|project_path| self.task_diff(project_path, cx))
            .map(|(buffer, diff)| render_patch(&buffer, &diff, cx));

        body = match patch {
            Some(patch) => body.child(patch),
            None => body.child(
                Label::new(
                    "No pending task edits in this file — the change is in your working tree.",
                )
                .size(LabelSize::XSmall)
                .color(Color::Muted),
            ),
        };

        let project_path = selection.project_path.clone();

        Some(
            v_flex()
                .w_full()
                .max_h(rems_from_px(220_f32))
                .overflow_hidden()
                .bg(colors.panel_background)
                .child(Divider::horizontal())
                .child(
                    h_flex()
                        .w_full()
                        .px_2()
                        .py_1()
                        .gap_2()
                        .items_center()
                        .justify_between()
                        .child(
                            Label::new("Task preview")
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .when_some(project_path, |this, project_path| {
                                    this.child(
                                        Button::new("task-changes-open-file", "Open File")
                                            .label_size(LabelSize::XSmall)
                                            .style(ButtonStyle::Subtle)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                let Some(workspace) = this.workspace.upgrade()
                                                else {
                                                    return;
                                                };
                                                let project_path = project_path.clone();
                                                workspace
                                                    .update(cx, |workspace, cx| {
                                                        workspace.open_path_preview(
                                                            project_path,
                                                            None,
                                                            true,
                                                            true,
                                                            true,
                                                            window,
                                                            cx,
                                                        )
                                                    })
                                                    .detach_and_log_err(cx);
                                            })),
                                    )
                                })
                                .child(
                                    IconButton::new("task-changes-close-preview", IconName::Close)
                                        .icon_size(IconSize::XSmall)
                                        .tooltip(Tooltip::text("Close preview"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.selected = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .id("task-changes-preview-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body),
                )
                .into_any_element(),
        )
    }
}

impl Focusable for TaskChangesView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TaskChangesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_git_store(window, cx);

        let rows = self.rows(cx);

        let mut list = v_flex()
            .id("task-changes-list")
            .size_full()
            .overflow_y_scroll();

        if rows.is_empty() {
            list = list.child(self.render_empty_state(cx));
        } else {
            for (index, row) in rows.iter().enumerate() {
                list = list.child(self.render_row(index, row, cx));
            }
        }

        v_flex()
            .size_full()
            .overflow_hidden()
            .child(self.render_toolbar(cx))
            .child(Divider::horizontal())
            .child(div().flex_1().min_h_0().child(list))
            .children(self.render_preview(cx))
    }
}

fn diff_stat_for(entry: &StatusEntry) -> Option<DiffStat> {
    entry
        .diff_stat
        .clone()
        .or_else(|| entry.unstaged_diff_stat.clone())
        .or_else(|| entry.staged_diff_stat.clone())
}

fn status_letter_and_color(status: FileStatus) -> (&'static str, Color) {
    if status.is_conflicted() {
        ("U", Color::VersionControlConflict)
    } else if status.is_created() {
        ("A", Color::VersionControlAdded)
    } else if status.is_deleted() {
        ("D", Color::VersionControlDeleted)
    } else if status.is_modified() {
        ("M", Color::VersionControlModified)
    } else if status.is_ignored() {
        ("!", Color::VersionControlIgnored)
    } else {
        ("?", Color::Muted)
    }
}

fn render_diff_stat(stat: Option<DiffStat>) -> AnyElement {
    h_flex()
        .gap_1()
        .flex_shrink_0()
        .when_some(stat, |this, stat| {
            this.child(
                Label::new(format!("+{}", stat.added))
                    .size(LabelSize::XSmall)
                    .color(Color::Success),
            )
            .child(
                Label::new(format!("-{}", stat.deleted))
                    .size(LabelSize::XSmall)
                    .color(Color::Error),
            )
        })
        .into_any_element()
}

/// Renders a compact unified-diff-style preview straight from the task's own
/// `BufferDiff`. This is a preview, not an editor: no hunk actions, no
/// selection, and no dependency on a second diff surface.
fn render_patch(
    buffer: &Entity<Buffer>,
    diff: &Entity<buffer_diff::BufferDiff>,
    cx: &App,
) -> AnyElement {
    const MAX_PREVIEW_LINES: usize = 120;

    let buffer_snapshot = buffer.read(cx).snapshot();
    let diff_snapshot = diff.read(cx).snapshot(cx);

    if !diff_snapshot.base_text_exists() {
        return Label::new("New file — nothing to compare against.")
            .size(LabelSize::XSmall)
            .color(Color::Muted)
            .into_any_element();
    }

    // The diff snapshot maps the buffer onto its base text; invert it so the
    // edits read as base -> current, the direction a user expects.
    let mut patch = diff_snapshot.patch_for_buffer_range(
        Point::zero()..=buffer_snapshot.max_point(),
        &buffer_snapshot,
    );
    patch.invert();

    let base_text = diff_snapshot.base_text();
    let mut children: Vec<AnyElement> = Vec::new();
    let mut truncated = false;

    'edits: for edit in patch.edits() {
        let removed: String = base_text.text_for_range(edit.old.clone()).collect();
        for line in removed.lines() {
            if children.len() >= MAX_PREVIEW_LINES {
                truncated = true;
                break 'edits;
            }
            children.push(diff_line("-", line, Color::Error));
        }

        let added: String = buffer_snapshot.text_for_range(edit.new.clone()).collect();
        for line in added.lines() {
            if children.len() >= MAX_PREVIEW_LINES {
                truncated = true;
                break 'edits;
            }
            children.push(diff_line("+", line, Color::Success));
        }
    }

    if children.is_empty() {
        return Label::new("No pending edits in this file.")
            .size(LabelSize::XSmall)
            .color(Color::Muted)
            .into_any_element();
    }

    if truncated {
        children.push(
            Label::new("Preview truncated — open the file for the full diff.")
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .into_any_element(),
        );
    }

    v_flex()
        .w_full()
        .gap_0p5()
        .children(children)
        .into_any_element()
}

fn diff_line(prefix: &'static str, line: &str, color: Color) -> AnyElement {
    h_flex()
        .w_full()
        .gap_1()
        .items_start()
        .child(
            div()
                .w(px(8.))
                .flex_shrink_0()
                .child(Label::new(prefix).size(LabelSize::XSmall).color(color)),
        )
        .child(
            Label::new(line.to_string())
                .size(LabelSize::XSmall)
                .color(color)
                .truncate(),
        )
        .into_any_element()
}
