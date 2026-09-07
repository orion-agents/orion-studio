# PIXEL-UI-40 Agent Shell

Status: DONE

The final decision is to inherit shared Button/Tab/List/surface Chrome without changing the
AgentPanel root. Agent deterministic captures and focused regressions passed; the complete live
interaction matrix remains a UI-50 condition.

## Scope

- Planned direct scope: selected style chains in `crates/agent_ui/src/agent_panel.rs`.
- Shared candidate scope: Button, Tab, ListItem, and surface changes.

## Implementation evidence

- Current `git diff` contains no direct change to `agent_panel.rs`.
- Shared component/surface diffs exist, but the current diff alone does not prove complete Agent Panel shell coverage.

## Behavior boundary

- The 0-diff decision preserves the AgentPanel root hierarchy, surface switching, actions, drag/drop, focus, scrolling, font zoom, connection state, and base-view state.

## Validation checklist

- [x] Confirmed no direct Agent Panel production diff.
- [x] Current source confirms shared Button/Tab/List/surface inheritance.
- [x] Night/Dawn deterministic Agent captures and installed Agent Panel rendered successfully.
- [ ] UI-50 condition: exhaustive zoom/scroll/drag/multi-surface behavior is NOT VERIFIED.

## Remaining gate

The zero-production-diff inheritance decision is accepted. The unchecked exhaustive interaction
matrix remains an explicit UI-50 release-qualification condition.
