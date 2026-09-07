# PIXEL-UI-35 Terminal Rename

Status: DONE

Terminal rename styling compiled and three focused title/tab-content behavior tests passed as
recorded in `PIXEL-UI-EXECUTION-RECORD.md`; handlers and title state remain unchanged.

## Scope

- Terminal tab rename editor wrapper styling in `crates/terminal_view/src/terminal_view.rs`.

## Implementation evidence

- The diff adds control radius, token border, existing focused-border color, and editor background to the full-size rename wrapper.
- The editor remains the wrapper child and the existing confirm action follows the inserted style chain unchanged in the visible hunk.

## Behavior boundary

- Editor configuration, rename state, confirm/cancel logic, PTY, and tab state are not changed by the visible diff.

## Validation checklist

- [x] Rename-wrapper diff inspected.
- [x] Rust formatting, Terminal View compilation and three exact title/content tests passed.
- [ ] UI-50 condition: exhaustive rename/Chinese/resize behavior is NOT VERIFIED.
- [x] Night/Dawn deterministic Terminal-shell consumers passed.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
