# PIXEL-UI-15 Tree Item

Status: DONE

The style-only TreeViewItem implementation passed UI tests plus Project expanded/collapsed
visual and focused behavior gates recorded in `PIXEL-UI-EXECUTION-RECORD.md`.

## Scope

- `crates/ui/src/components/tree_view_item.rs`

## Implementation evidence

- The diff changes selected background/border tokens, adds the control radius, token border, active background, and a token-width inset focus ring.
- Existing item size, label rendering, disclosure structure, selected/focused/disabled conditions, and indentation remain around the changed style chain.

## Behavior boundary

- Tree expansion state, disclosure action, virtual-list logic, and Project Panel code are not changed by this task diff.

## Validation checklist

- [x] Component diff inspected.
- [x] Rust formatting, UI compilation and Project expanded/collapsed captures passed.
- [ ] UI-50 condition: exhaustive hover/focus/disabled/deep-nesting matrix is NOT VERIFIED.
- [x] Installed-app Project Panel read-only smoke passed.

## Remaining gate

The task implementation gate is complete. The unchecked exhaustive state matrix remains an
explicit UI-50 release-qualification condition.
