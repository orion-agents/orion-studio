# PIXEL-UI-45 Agent Composer

Status: DONE

The composer-shell style slice compiled and appeared in both-theme Agent captures. Editor,
focus, send/cancel, attachment and model-selection handlers remain unchanged; the complete live
interaction matrix remains a UI-50 condition.

## Scope

- Outer-shell styling in `ThreadView::render_message_editor` within `crates/agent_ui/src/conversation_view/thread_view.rs`.

## Implementation evidence

- The current `thread_view.rs` diff replaces the composer top-border helper with the Pixel Chrome border token when messages exist.
- The surrounding expand action, height condition, background, layout, and child hierarchy remain visible around the inserted style expression.
- `message_editor.rs` is not changed in the current diff; shared Button/InputField diffs exist elsewhere.

## Behavior boundary

- The visible composer hunk changes one outer-shell border expression; editor/control hierarchy, send, stop, attachment, drag/drop, model selection, focus, and input behavior are not edited there.

## Validation checklist

- [x] Composer shell diff inspected.
- [x] UI-44, UI-12 and UI-16 prerequisites completed before this final gate.
- [x] Rust formatting and Agent UI compilation passed.
- [x] Night/Dawn deterministic Composer-shell captures passed.
- [ ] UI-50 condition: exhaustive send/stop/attachment/model/focus/Chinese-input matrix is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
