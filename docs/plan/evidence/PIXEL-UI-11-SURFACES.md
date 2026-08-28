# PIXEL-UI-11 Shared Surfaces

Status: DONE

Shared hard-shadow/surface implementation, UI tests and visual consumers passed as recorded in
`PIXEL-UI-EXECUTION-RECORD.md`. The broader manual surface matrix remains a UI-50 condition.

## Scope

- `crates/ui/src/styles/elevation.rs`
- `crates/ui/src/traits/styled_ext.rs`

## Implementation evidence

- `ElevationIndex` now maps modal elevation to modal radius and other elevations to surface radius.
- Elevated and modal shadows now consume the Pixel Chrome hard-shadow token with light/dark alpha differences; background, surface, and editor-surface remain shadowless.
- Shared elevation helpers now consume token radius and border width instead of fixed rounded-large and 1px helpers.
- `ElevationIndex` variants remain present; the visible diff does not change dismiss or focus handlers.

## Behavior boundary

- The diff changes shared surface styling only and does not alter element hierarchy, placement, state, or renderer APIs.

## Validation checklist

- [x] Elevation and helper diffs inspected.
- [x] Rust formatting, UI crate compilation and hard-shadow tests passed.
- [x] Night/Dawn deterministic consumers and installed full-app Component Preview passed.
- [ ] UI-50 condition: exhaustive layout/elevation comparison is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive comparison remains an
explicit UI-50 release-qualification condition.
