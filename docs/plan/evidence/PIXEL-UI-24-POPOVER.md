# PIXEL-UI-24 Popover

Status: DONE

The final decision is to consume the shared elevation/Chrome layer without a Popover-specific
production edit. Placement, dismissal and focus code therefore remain byte-for-byte unchanged;
the broader manual interaction matrix remains a UI-50 condition.

## Scope

- Planned direct scope: `crates/ui/src/components/popover.rs`.
- Shared candidate scope: UI-11 elevation helpers.

## Implementation evidence

- Current `git diff` contains no direct change to `popover.rs`.
- UI-11 has a visible shared elevation diff, but the current diff alone does not prove Popover inheritance or complete the Popover acceptance criteria.

## Behavior boundary

- The 0-diff decision avoids changing anchor, placement, focus restore, outside-click dismissal, event handlers, or callers.

## Validation checklist

- [x] Confirmed no direct Popover production diff.
- [x] Current source confirms Popover consumes the modified shared elevation path.
- [x] Deterministic Night/Dawn consumers and installed Component Preview passed.
- [ ] UI-50 condition: exhaustive dismissal/focus/edge-placement matrix is NOT VERIFIED.

## Remaining gate

The zero-production-diff inheritance decision is accepted. The unchecked exhaustive interaction
matrix remains an explicit UI-50 release-qualification condition.
