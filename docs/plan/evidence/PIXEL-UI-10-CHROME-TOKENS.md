# PIXEL-UI-10 Chrome Tokens

Status: DONE

Token unit tests, formatting, UI tests and both-theme consumers passed as recorded in
`PIXEL-UI-EXECUTION-RECORD.md`; the draft checklist below is superseded by that gate.

## Scope

- `crates/ui/src/styles/chrome.rs`
- `crates/ui/src/styles.rs`

## Implementation evidence

- The added module defines typed control/surface/modal radii, border/focus-ring widths, and elevated/modal hard-shadow offsets.
- The values in the added file are 2px control/surface radius, 4px modal radius, 1px border, 2px focus ring, and 2px/4px hard-shadow offsets.
- The added unit tests assert token values and zero blur/spread for the generated hard shadows.
- `styles.rs` exports the new module; no theme-name branch appears in this task diff.

## Behavior boundary

- The token file does not modify GPUI, settings schema, theme schema, density, or runtime theme selection.

## Validation checklist

- [x] Added-file and export diff inspected.
- [x] `cargo fmt --all -- --check` passed.
- [x] UI crate compilation and token unit tests passed.
- [x] Night/Dawn compile-level and deterministic visual consumers passed.

## Remaining gate

No task-specific gate remains; exhaustive interaction coverage is tracked by UI-50.
