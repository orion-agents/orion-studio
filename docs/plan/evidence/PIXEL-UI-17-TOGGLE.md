# PIXEL-UI-17 Toggle

Status: DONE

The style-only Toggle implementation compiled and passed the shared UI gate recorded in
`PIXEL-UI-EXECUTION-RECORD.md`; exhaustive pointer/keyboard states remain a UI-50 condition.

## Scope

- `crates/ui/src/components/toggle.rs`

## Implementation evidence

- Checkbox and switch style chains now consume Pixel Chrome radius and stroke tokens.
- The switch outer border uses the focus-ring width; the track uses the normal border width; the thumb and track use control radius.
- Existing selected/unselected state mapping, disabled conditions, group hover, tab-index condition, and child structure remain visible around the changes.

## Behavior boundary

- No action, click, keyboard, ARIA, caller, or public state-model file is changed by this task.

## Validation checklist

- [x] Component diff inspected.
- [x] Rust formatting, UI compilation and shared UI tests passed.
- [ ] UI-50 condition: exhaustive checkbox/switch/radio visual matrix is NOT VERIFIED.
- [ ] UI-50 condition: exhaustive mouse/keyboard/ARIA/hit-area matrix is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
