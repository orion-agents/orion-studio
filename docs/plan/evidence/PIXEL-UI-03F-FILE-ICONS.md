# PIXEL-UI-03F File Icons

Status: DONE

Final XML, icon-test, Project Panel and multi-scale verification is recorded in
`PIXEL-UI-EXECUTION-RECORD.md`.

The final coordinator gate rasterized all allowlisted icons at 10, 12, 14 and 16 px. Every
output had the requested dimensions and non-empty alpha bounds. Manual contact-sheet review and
the One Light GPUI Project Panel confirmed readable silhouettes without clipping; GPUI applies
theme tint independently of the SVG rasterizer's source fill preview.

## Scope

- `folder.svg`, `folder_open.svg`, `file.svg`, `chevron_right.svg`, and `chevron_down.svg` under `assets/icons/file_icons/`.

## Implementation evidence

- The current diff modifies exactly the five file-icon paths approved by the design contract.
- Existing curved/stroked paths are replaced with hard-filled, integer-coordinate path data.
- No icon mapping, registry, schema, or icon-theme-name file is changed by this task.

## Behavior boundary

- Filenames and call sites remain unchanged in the visible diff; the task changes SVG path data only.

## Validation checklist

- [x] Allowlist and SVG diffs inspected.
- [x] `xmllint` accepted all five allowlisted file icons.
- [x] File/icon asset tests passed.
- [x] 10/12/14/16px rasters and live Project Panel rendering passed.
- [ ] UI-50 condition: exhaustive expanded/selected/muted/disabled state review is NOT VERIFIED.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive state matrix remains an
explicit UI-50 release-qualification condition.
