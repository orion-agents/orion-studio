# PIXEL-UI-12 Button

Status: DONE

The style-only Button implementation compiled and passed the shared UI and product regression
gates in `PIXEL-UI-EXECUTION-RECORD.md`; exhaustive manual states remain a UI-50 condition.

## Scope

- `crates/ui/src/components/button/button_like.rs`

## Implementation evidence

- The diff gives `ButtonLike` a stable token-width border and token control radii for enabled corners.
- Focus-visible styling adds a zero-offset hard shadow spread by the focus-ring token.
- Existing style selection, sizing, cursor, focus handle, and event expressions remain around the changed style chain; no public API or caller file is part of this task diff.

## Behavior boundary

- The visible changes are styling expressions; action, role, ARIA, tab index, tooltip, and click behavior are not edited.

## Validation checklist

- [x] Component diff inspected.
- [x] Rust formatting, UI compilation and shared regression tests passed.
- [x] Night/Dawn deterministic consumers and installed Component Preview rendered Button.
- [ ] UI-50 condition: exhaustive style/keyboard/focus/disabled layout matrix is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
