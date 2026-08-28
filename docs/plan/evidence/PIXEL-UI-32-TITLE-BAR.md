# PIXEL-UI-32 Title Bar

Status: DONE

The final decision is to inherit shared Button/Tab Chrome without a platform title-bar edit.
Drag, traffic-light and full-screen behavior remain byte-for-byte unchanged; installed-window
manual coverage is tracked by UI-52.

## Scope

- Planned direct scope: `crates/title_bar/src/title_bar.rs` render styling.
- Shared candidate scope: UI-12 Button and UI-13 Tab.

## Implementation evidence

- Current `git diff` contains no direct change to `title_bar.rs` or `platform_title_bar.rs`.
- Shared Button and Tab diffs exist, but the current diff alone does not prove title-bar background, divider, spacing, long-path, or narrow-window acceptance.

## Behavior boundary

- The 0-diff decision keeps platform drag, double-click, traffic-light hitboxes, full-screen, and window behavior untouched.

## Validation checklist

- [x] Confirmed no Title Bar or PlatformTitleBar production diff.
- [x] Current source confirms shared Button/Tab inheritance with no platform-title-bar diff.
- [x] Night/Dawn deterministic shells and installed macOS title bar rendered successfully.
- [ ] UI-50 condition: exhaustive drag/full-screen/narrow/multi-window matrix is NOT VERIFIED.

## Remaining gate

The zero-production-diff inheritance decision is accepted. The unchecked exhaustive interaction
matrix remains an explicit UI-50 release-qualification condition.
