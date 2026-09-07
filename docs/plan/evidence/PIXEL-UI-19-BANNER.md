# PIXEL-UI-19 Banner

Status: DONE

The style-only Banner implementation compiled and passed the shared UI gate recorded in
`PIXEL-UI-EXECUTION-RECORD.md`; exhaustive semantic-state review remains a UI-50 condition.

## Scope

- `crates/ui/src/components/banner.rs`

## Implementation evidence

- The diff replaces fixed surface radius and border helpers with Pixel Chrome surface-radius and border tokens.
- Existing severity selection, wrapping, layout, icon, background, and border-color logic remains after the changed shell chain.

## Behavior boundary

- Dismiss/action handlers, notification state, callers, and severity model are not changed by this task diff.

## Validation checklist

- [x] Component diff inspected.
- [x] Rust formatting, UI compilation and shared UI tests passed.
- [ ] UI-50 condition: exhaustive severity/long-text/button visual matrix is NOT VERIFIED.
- [ ] UI-50 condition: exhaustive dismiss/overflow/non-color behavior is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
