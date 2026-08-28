# PIXEL-UI-25 Context Menu

Status: DONE

The final decision is to consume shared surface/ListItem styling without a Context Menu-specific
production edit. Menu actions and keyboard flow remain unchanged; manual interaction coverage
remains a UI-50 condition.

## Scope

- Planned direct scope: `crates/ui/src/components/context_menu.rs`.
- Shared candidate scope: UI-11 surfaces and UI-14 list states.

## Implementation evidence

- Current `git diff` contains no direct change to `context_menu.rs`.
- Shared surface/list diffs exist, but the current diff alone does not prove menu rows, separators, shortcuts, or submenus inherit every required state.

## Behavior boundary

- The 0-diff decision avoids command dispatch, keyboard selection, submenu, focus, and caller changes.

## Validation checklist

- [x] Confirmed no direct Context Menu production diff.
- [x] Current source confirms shared surface and ListItem inheritance.
- [x] Deterministic Night/Dawn consumers and installed Component Preview passed.
- [ ] UI-50 condition: exhaustive keyboard/submenu/disabled/focus matrix is NOT VERIFIED.

## Remaining gate

The zero-production-diff inheritance decision is accepted. The unchecked exhaustive interaction
matrix remains an explicit UI-50 release-qualification condition.
