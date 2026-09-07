# PIXEL-UI-43 Agent Tool Cards

Status: DONE

The tool-card style slice compiled, passed the exact expanded-search test and appeared in
both-theme Agent captures recorded in `PIXEL-UI-EXECUTION-RECORD.md`.

## Scope

- Generic tool-call container styles in `crates/agent_ui/src/conversation_view/thread_view.rs`.

## Implementation evidence

- The current diff applies Pixel Chrome control radius to an input/output header and surface radii to card headers.
- A generic card-layout branch changes to surface radius and token border while preserving failed/canceled dashed-border conditions.
- No dispatch, approval, async, diff, resource, image, or markdown data-processing expression is changed in the visible hunks.

## Behavior boundary

- The diff remains in container style chains; content renderers and tool data behavior are outside the changed expressions.

## Validation checklist

- [x] Task-specific container diff inspected.
- [x] UI-42 prerequisite completed before this final gate.
- [x] Rust formatting, Agent UI compilation and exact expanded-search test passed.
- [x] Deterministic Agent captures covered representative tool/image shells.
- [ ] UI-50 condition: exhaustive content/error/scroll/resource behavior is NOT VERIFIED.

## Remaining gate

The task implementation and UI-44 prerequisite gate are complete. The unchecked exhaustive
interaction matrix remains an explicit UI-50 release-qualification condition.
