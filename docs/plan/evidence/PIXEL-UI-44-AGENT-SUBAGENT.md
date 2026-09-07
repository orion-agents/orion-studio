# PIXEL-UI-44 Agent Subagent Cards

Status: DONE

The Subagent-card style slice compiled, passed the exact pending-state test and appeared in
both-theme Agent captures recorded in `PIXEL-UI-EXECUTION-RECORD.md`.

## Scope

- Subagent card styles in `crates/agent_ui/src/conversation_view/thread_view.rs`.

## Implementation evidence

- The current diff changes the subagent card shell to Pixel Chrome surface radius and token border.
- A subagent header/control container changes to control radius.
- Existing dashed-border status condition, border-color selection, and child construction remain around the style changes.
- No session, subagent state, scroll-handle, cancel, or synchronization expression is changed in the visible hunks.

## Behavior boundary

- The visible changes are shell styles; session identity, state synchronization, navigation, cancel, and scroll position remain outside the changed expressions.

## Validation checklist

- [x] Task-specific container diff inspected.
- [x] UI-43 prerequisite completed before this final gate.
- [x] Rust formatting, Agent UI compilation and exact pending-state test passed.
- [x] Expanded/collapsed deterministic Subagent-card captures passed.
- [ ] UI-50 condition: exhaustive lifecycle/navigation/scroll behavior is NOT VERIFIED.

## Remaining gate

The task implementation and UI-45 prerequisite gate are complete. The unchecked exhaustive
interaction matrix remains an explicit UI-50 release-qualification condition.
