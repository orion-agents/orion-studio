# PIXEL-UI-51 Defaults

Status: SKIPPED-BY-DECISION

## Decision

The user requested a conservative Orion Studio init and did not separately approve replacing
fresh-install color-theme defaults. Orion Pixel Atelier Night and Dawn therefore remain bundled,
selectable opt-in themes.

## Evidence

- `assets/settings/default.json` has no diff from plan base `84ce9a6`.
- The dynamic defaults remain `One Light` and `One Dark`.
- The icon-theme default and registry name remain `Orion Studio (Default)`.
- Existing user settings are not rewritten; there is no database or migration change.
- One, Ayu, Gruvbox and other bundled themes remain selectable.

Users can try either theme through Orion Studio's theme selector and restore the previous theme
through the same selector. A future default switch requires an explicit user decision and fresh
profile validation; it must not be smuggled into another UI or release task.

TASK: UI-51
STATUS: SKIPPED-BY-DECISION
SCOPE CHECK: PASS
BEHAVIOR CHECK: PASS
VISUAL CHECK: N/A
RESOURCE CHECK: N/A
