# PIXEL-UI-30 Project Rows

Status: DONE

Project row styling passed four focused Project Panel tests and both-theme deterministic visual
capture as recorded in `PIXEL-UI-EXECUTION-RECORD.md`.

## Scope

- Style expressions in `crates/project_panel/src/project_panel.rs` entry rendering.

## Implementation evidence

- The project-panel diff imports Pixel Chrome radius/stroke tokens.
- The entry style changes from no radius plus asymmetric border helpers to control radius and a token-width border.
- Existing background, hover background/border, cursor, grouping, and sticky-item condition remain around the changed style chain.

## Behavior boundary

- The visible hunk does not edit drag/drop, click, selection, open, Project/Worktree state, or behavior-test assertions.

## Validation checklist

- [x] Project-row style diff inspected.
- [x] Rust formatting and Project Panel compilation passed.
- [x] Four exact editing/selection/diagnostic/chevron regression tests passed.
- [ ] UI-50 condition: exhaustive Git/drop/drag/hover state matrix is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
