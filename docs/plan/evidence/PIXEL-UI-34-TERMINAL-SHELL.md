# PIXEL-UI-34 Terminal Shell

Status: DONE

Terminal shell styling compiled and three focused title/tab-content behavior tests passed as
recorded in `PIXEL-UI-EXECUTION-RECORD.md`; exhaustive ANSI/manual interaction remains UI-50.

## Scope

- `terminal-view-container` styling in `crates/terminal_view/src/terminal_view.rs`.

## Implementation evidence

- The diff imports Pixel Chrome tokens and adds surface radius, token border, and existing `border_variant` color to the terminal container.
- The existing editor background remains and `TerminalElement::new` remains a child after the inserted shell styles.
- No terminal crate, PTY, character-grid, cursor, selection, copy/paste, link, or action-handler file is changed by this task.

## Behavior boundary

- The visible diff is outside the terminal character element and changes only the surrounding container style.

## Validation checklist

- [x] Terminal-container diff inspected.
- [x] Rust formatting, Terminal View compilation and three exact title/content tests passed.
- [x] Installed-app Terminal shell opened and rendered under Pixel Chrome.
- [ ] UI-50 condition: exhaustive ANSI/cursor/resize/copy/link matrix is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive interaction matrix remains
an explicit UI-50 release-qualification condition.
