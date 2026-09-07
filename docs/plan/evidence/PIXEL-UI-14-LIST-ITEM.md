# PIXEL-UI-14 List Item

Status: DONE

The style-only ListItem implementation passed UI tests and Project/Agent visual consumers as
recorded in `PIXEL-UI-EXECUTION-RECORD.md`; exhaustive manual states remain a UI-50 condition.

## Scope

- `crates/ui/src/components/list/list_item.rs`

## Implementation evidence

- The diff adds token control radii, selected inset outlines, and focus inset rings.
- Hover, active, selected, and focused styling is applied to both inset and non-inset branches while disabled guards remain visible.
- Existing `on_hover`, children, inset layout, dock matching, and selection conditions remain in place; no caller file is included.

## Behavior boundary

- ARIA, click/toggle handlers, indentation, virtualization, and content slots are not changed by the visible diff.

## Validation checklist

- [x] Component diff inspected.
- [x] Rust formatting, UI compilation and Project/Agent visual consumers passed.
- [ ] UI-50 condition: exhaustive density/inset/focus/disabled matrix is NOT VERIFIED.
- [ ] UI-50 condition: exhaustive keyboard-navigation interaction is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
