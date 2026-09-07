# PIXEL-UI-18 Chip

Status: DONE

The style-only Chip implementation compiled and passed the shared UI gate recorded in
`PIXEL-UI-EXECUTION-RECORD.md`; exhaustive manual states remain a UI-50 condition.

## Scope

- `crates/ui/src/components/chip.rs`

## Implementation evidence

- The diff replaces fixed border/radius helpers with Pixel Chrome border and control-radius tokens.
- Existing gap, padding, truncation, background, border color, and overflow behavior remain in the style chain.

## Behavior boundary

- Delete/click actions, content model, callers, dimensions, and public API are not edited by the visible diff.

## Validation checklist

- [x] Component diff inspected.
- [x] Rust formatting, UI compilation and shared UI tests passed.
- [ ] UI-50 condition: exhaustive state/long-text visual matrix is NOT VERIFIED.
- [ ] UI-50 condition: exhaustive keyboard/delete/clipping/hit-area behavior is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
