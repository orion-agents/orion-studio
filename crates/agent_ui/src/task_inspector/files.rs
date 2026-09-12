//! The **Files** tab of the Task Inspector.
//!
//! A compact, collapsed-by-default tree of the visible worktrees, plus the
//! files the current task has built up so far. It walks only the expanded
//! directories (`Snapshot::child_entries`), so the cost tracks what is on
//! screen rather than the size of the repository.
//!
//! It is not `ProjectPanel`: no drag and drop, no rename/delete, no project
//! settings menu, and no second entity rendering the same tree.

use super::active_agent_thread;
use gpui::{
    AnyElement, Context, ElementId, Entity, FocusHandle, Focusable, Render, Subscription,
    WeakEntity, Window, div, px,
};
use project::{Entry, Project, ProjectPath, Worktree, WorktreeId};
use std::collections::HashSet;
use std::sync::Arc;
use ui::{Divider, Tooltip, prelude::*};
use util::rel_path::RelPath;
use workspace::Workspace;
use worktree::Snapshot;

/// Row cap so a very wide expanded tree cannot dominate a frame.
const MAX_ROWS: usize = 400;

pub struct TaskFilesView {
    workspace: WeakEntity<Workspace>,
    /// Directories the user has opened. Collapsed by default is the point.
    expanded: HashSet<Arc<RelPath>>,
    focus_handle: FocusHandle,
    project: Option<Entity<Project>>,
    _project_subscription: Option<Subscription>,
}

impl TaskFilesView {
    /// The view is built without touching the workspace.
    ///
    /// `render` re-syncs the project on every frame, and the panel is
    /// constructed from inside a `Workspace` update — reading the workspace
    /// here would panic.
    pub(crate) fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let mut expanded = HashSet::default();
        expanded.insert(RelPath::empty_arc());

        Self {
            workspace,
            expanded,
            focus_handle: cx.focus_handle(),
            project: None,
            _project_subscription: None,
        }
    }

    fn sync_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let project = workspace.read(cx).project().clone();

        if self.project.as_ref().map(|project| project.entity_id()) == Some(project.entity_id()) {
            return;
        }

        self._project_subscription =
            Some(cx.observe_in(&project, window, |_, _, _, cx| cx.notify()));
        self.project = Some(project);
    }

    fn toggle_dir(&mut self, path: Arc<RelPath>, cx: &mut Context<Self>) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        cx.notify();
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

    /// Files the current task has already touched. Surfaced above the tree so
    /// the code evidence for the task is one glance away.
    fn task_files(&self, cx: &App) -> Vec<(ProjectPath, String)> {
        let Some(thread) = active_agent_thread(&self.workspace, cx) else {
            return Vec::new();
        };
        let action_log = thread.read(cx).action_log().clone();
        let action_log = action_log.read(cx);

        let mut files: Vec<(ProjectPath, String)> = action_log
            .changed_buffers(cx)
            .filter_map(|(buffer, _)| {
                let buffer = buffer.read(cx);
                let file = buffer.file()?;
                let path = file.path().clone();
                let display = path.as_unix_str().to_string();
                Some((
                    ProjectPath {
                        worktree_id: file.worktree_id(cx),
                        path,
                    },
                    display,
                ))
            })
            .collect();
        files.sort_by(|a, b| a.1.cmp(&b.1));
        files.dedup_by(|a, b| a.1 == b.1);
        files
    }

    fn render_task_files(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let files = self.task_files(cx);
        if files.is_empty() {
            return None;
        }

        let rows: Vec<AnyElement> = files
            .into_iter()
            .map(|(project_path, display)| {
                let file_name = project_path
                    .path
                    .file_name()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| display.clone());
                h_flex()
                    .id(ElementId::Name(
                        format!("task-file-recent-{display}").into(),
                    ))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .cursor_pointer()
                    .hover(|this| this.bg(cx.theme().colors().element_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_file(project_path.clone(), window, cx)
                    }))
                    .child(
                        Icon::new(IconName::File)
                            .size(IconSize::XSmall)
                            .color(Color::Accent),
                    )
                    .child(Label::new(file_name).size(LabelSize::Small).truncate())
                    .into_any_element()
            })
            .collect();

        Some(
            v_flex()
                .w_full()
                .child(
                    h_flex().w_full().px_2().py_1().child(
                        Label::new("This task")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
                )
                .children(rows)
                .child(Divider::horizontal())
                .into_any_element(),
        )
    }

    fn render_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(project) = self.project.as_ref() else {
            return Vec::new();
        };

        let worktrees: Vec<Entity<Worktree>> = project.read(cx).visible_worktrees(cx).collect();
        let mut rows = Vec::new();

        for worktree in worktrees {
            if rows.len() >= MAX_ROWS {
                break;
            }

            let (worktree_id, snapshot, root_name) = {
                let worktree = worktree.read(cx);
                (
                    worktree.id(),
                    worktree.snapshot(),
                    worktree.root_name_str().to_string(),
                )
            };

            rows.push(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .child(
                        Label::new(root_name)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .into_any_element(),
            );

            let root = RelPath::empty_arc();
            self.collect_dir(&snapshot, &root, worktree_id, 0, &mut rows, cx);
        }

        rows
    }

    /// Depth-first walk that descends only into expanded directories.
    fn collect_dir(
        &self,
        snapshot: &Snapshot,
        dir: &Arc<RelPath>,
        worktree_id: WorktreeId,
        depth: usize,
        out: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        if out.len() >= MAX_ROWS {
            return;
        }

        let children: Vec<Entry> = snapshot.child_entries(dir.as_ref()).cloned().collect();
        for entry in children {
            if out.len() >= MAX_ROWS {
                return;
            }

            let path = entry.path.clone();
            let is_dir = entry.is_dir();
            let key = path.to_string();
            let file_name = path
                .file_name()
                .map(|name| name.to_string())
                .unwrap_or_default();
            let is_expanded = self.expanded.contains(&path);
            let colors = cx.theme().colors();

            let row = h_flex()
                .id(ElementId::Name(format!("task-file-row-{key}").into()))
                .w_full()
                .pl(px(8. + (depth as f32) * 12.))
                .pr_2()
                .py_1()
                .gap_1()
                .items_center()
                .cursor_pointer()
                .hover(|this| this.bg(colors.element_hover))
                .child(
                    Icon::new(if is_dir {
                        if is_expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        }
                    } else {
                        IconName::File
                    })
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
                )
                .child(Label::new(file_name).size(LabelSize::Small).truncate());

            let row = if is_dir {
                let path = path.clone();
                row.on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_dir(path.clone(), cx);
                }))
            } else {
                let project_path = ProjectPath {
                    worktree_id,
                    path: path.clone(),
                };
                row.on_click(cx.listener(move |this, _, window, cx| {
                    this.open_file(project_path.clone(), window, cx)
                }))
            };

            out.push(row.into_any_element());

            if is_dir && is_expanded {
                let path = path.clone();
                self.collect_dir(snapshot, &path, worktree_id, depth + 1, out, cx);
            }
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
                Label::new("Files")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        IconButton::new("task-files-collapse", IconName::ChevronRight)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Collapse all"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.expanded.clear();
                                this.expanded.insert(RelPath::empty_arc());
                                cx.notify();
                            })),
                    )
                    .child(
                        IconButton::new("task-files-expand", IconName::ChevronDown)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Expand all"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(project) = this.project.clone() {
                                    let worktrees: Vec<Entity<Worktree>> =
                                        project.read(cx).visible_worktrees(cx).collect();
                                    for worktree in worktrees {
                                        let snapshot = worktree.read(cx).snapshot();
                                        for entry in snapshot.directories(false, 0) {
                                            this.expanded.insert(entry.path.clone());
                                        }
                                    }
                                }
                                cx.notify();
                            })),
                    ),
            )
    }
}

impl Focusable for TaskFilesView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TaskFilesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_project(window, cx);

        let rows = self.render_rows(cx);

        let mut body = v_flex()
            .id("task-files-list")
            .size_full()
            .overflow_y_scroll();
        body = body.children(self.render_task_files(cx));
        if rows.is_empty() {
            body = body.child(
                v_flex().px_3().py_4().child(
                    Label::new("No project files")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            );
        } else {
            body = body.children(rows);
        }

        v_flex()
            .size_full()
            .overflow_hidden()
            .child(self.render_toolbar(cx))
            .child(Divider::horizontal())
            .child(div().flex_1().min_h_0().child(body))
    }
}
