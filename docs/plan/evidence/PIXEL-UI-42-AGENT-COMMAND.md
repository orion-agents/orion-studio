# PIXEL-UI-42 Agent Command Cards

Status: DONE

The command-card style slice compiled and appeared in deterministic Agent captures; state and
dispatch code remain unchanged. Full live command interaction remains a UI-50 condition.

## Scope

- Command and terminal-tool card styles in `crates/agent_ui/src/conversation_view/thread_view.rs`.

## Implementation evidence

- The current diff changes command/terminal card borders and radii to Pixel Chrome tokens.
- The output section changes its lower corners to the surface radius, while existing failure/cancel dashed-border conditions remain visible.
- No command execution, permission, cancellation, copy, or expansion callback is changed in the visible hunks.

## Behavior boundary

- The visible changes are container styles; execution, permission, cancel, output state, copy, and expand behavior remain outside the changed expressions.

## Validation checklist

- [x] Task-specific container diff inspected.
- [x] UI-41 prerequisite completed before this final gate.
- [x] Rust formatting and Agent UI compilation passed.
- [x] Collapsed/expanded deterministic command-card captures passed.
- [ ] UI-50 condition: exhaustive execution/permission/cancel/long-output matrix is NOT VERIFIED.

## Remaining gate

The task implementation and UI-43 prerequisite gate are complete. The unchecked exhaustive
interaction matrix remains an explicit UI-50 release-qualification condition.
