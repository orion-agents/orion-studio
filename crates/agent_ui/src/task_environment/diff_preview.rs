//! The local diff preview shown inside the Task Environment panel's Changes row.
//!
//! This is a preview, not an editor: no hunk actions, no selection, no second diff
//! state machine. The plan is explicit that changing code stays in the code
//! workspace — the only route out is `OpenInCodeWorkspace`, and this preview never
//! activates a pane item.

use buffer_diff;
use gpui::{AnyElement, Entity, px};
use language::{Buffer, Point};
use ui::prelude::*;

/// How many diff lines the preview shows before it truncates.
const MAX_PREVIEW_LINES: usize = 120;

/// Renders a compact unified-diff-style preview straight from the task's own
/// `BufferDiff`, reading base -> current so it reads the way a user expects.
pub(crate) fn render_patch(
    buffer: &Entity<Buffer>,
    diff: &Entity<buffer_diff::BufferDiff>,
    cx: &App,
) -> AnyElement {
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
