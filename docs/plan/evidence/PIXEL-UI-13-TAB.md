# PIXEL-UI-13 Tab

Status: DONE

The style-only Tab implementation compiled and passed the shared UI/workspace visual gates in
`PIXEL-UI-EXECUTION-RECORD.md`; exhaustive drag/focus states remain a UI-50 condition.

## Scope

- `crates/ui/src/components/tab.rs`

## Implementation evidence

- The diff applies the control radius, uses existing hover and active colors, adds an inset selected outline, and adds an inset focus-visible ring.
- Position matching, slot construction, dimensions, and children remain in the same render function; no Pane or drag/close implementation is changed.

## Behavior boundary

- The task changes the shared Tab style chain only; `TabPosition`, close-side behavior, drag/drop, and workspace state are outside the diff.

## Validation checklist

- [x] Component diff inspected.
- [x] Rust formatting, UI compilation and workspace visual consumers passed.
- [ ] UI-50 condition: exhaustive position/dirty/long-title/narrow-pane matrix is NOT VERIFIED.
- [ ] UI-50 condition: exhaustive close/focus/drag split-pane interaction is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
