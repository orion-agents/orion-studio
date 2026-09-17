//! Read-only, task-scoped facts for the Task Environment panel.
//!
//! These are plain functions over `Project` / `Workspace` / `GitStore` rather than
//! methods on a view. The environment panel reads them directly, so it can answer
//! "what did this task change?" without owning a view entity or a second `GitStore`
//! subscription.
//!
//! Nothing here mutates state, activates a pane item or opens an editor. The only
//! route from a fact to an editor is the explicit `workspace::OpenInCodeWorkspace`
//! action — which is what keeps the conversation the permanent center content.

use std::collections::HashSet;

use git::status::{DiffStat, FileStatus};
use gpui::{App, Entity, WeakEntity};
use project::{
    Project, ProjectPath,
    git_store::{GitStore, StatusEntry},
};
use ui::Color;
use workspace::Workspace;

use super::active_agent_thread;

/// The files the current agent task has recorded edits against.
///
/// This is the "task scope" filter: a file is task-relevant when the agent's action
/// log shows it edited the buffer, not when the file merely looks dirty.
pub(crate) fn task_touched_paths(
    workspace: &WeakEntity<Workspace>,
    cx: &App,
) -> HashSet<ProjectPath> {
    let mut paths = HashSet::default();
    let Some(thread) = active_agent_thread(workspace, cx) else {
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
pub(crate) fn all_changed_entries(
    project: &Entity<Project>,
    cx: &App,
) -> Vec<(ProjectPath, StatusEntry, String)> {
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

/// The line counts for a status entry, preferring the working-tree numbers.
pub(crate) fn diff_stat_for(entry: &StatusEntry) -> Option<DiffStat> {
    entry
        .diff_stat
        .or(entry.unstaged_diff_stat)
        .or(entry.staged_diff_stat)
}

/// The single-letter status badge and the theme colour that carries its meaning.
///
/// The letter is always present, so the status is never communicated by colour
/// alone.
pub(crate) fn status_letter_and_color(status: FileStatus) -> (&'static str, Color) {
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
