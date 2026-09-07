# PIXEL-UI-01 Design Contract

Status: DONE

The implementation at `1fadd0ca` follows this contract. Final automated and conditional manual
gate results are recorded in `PIXEL-UI-EXECUTION-RECORD.md` and UI-50.

The user's instruction to complete the Orion Pixel Atelier objective adopts the recommended
v1 decisions from the master plan. WorkBuddy and sub-agents must not invent alternatives.

## Identity

- Family: `Orion Pixel Atelier`
- Dark variant: `Orion Pixel Atelier Night`
- Light variant: `Orion Pixel Atelier Dawn`
- Author: `Orion Studio`
- Default pixel font: none
- Existing UI, buffer, terminal and Agent fonts remain unchanged
- Existing icon theme name: `Orion Studio (Default)`

## Visual direction

Orion Pixel Atelier is a refined productivity UI, not a game overlay. Pixel character comes
from integer rhythm, crisp silhouettes, limited accents, hard boundaries and restrained
elevation. It does not use CRT scanlines, noise, chromatic aberration, full-screen textures,
bitmap scaling or a new spatial layout.

Editor and terminal text remain high-resolution and undecorated.

## Geometry

| Token | Value | Use |
| --- | ---: | --- |
| space-1 | 4px | icon/text micro gap |
| space-2 | 8px | normal control padding |
| space-3 | 12px | card padding |
| space-4 | 16px | section gap |
| space-6 | 24px | large empty/modal gap |
| radius-none | 0px | joined tabs and separators |
| radius-control | 2px | buttons, inputs, chips |
| radius-surface | 2px | cards, menus, popovers |
| radius-modal | 4px | large modal surfaces |
| border-default | 1px | borders and separators |
| focus-ring | 2px | keyboard focus without reflow |
| shadow-elevated | 2px 2px 0 | menu/popover/notification |
| shadow-modal | 4px 4px 0 | modal |

Pixel Chrome is global Orion Studio geometry. It also applies when a user chooses One, Ayu
or Gruvbox. Runtime Classic/Pixel geometry switching is a separate future plan. Theme-name
branches are forbidden.

## Night palette

| Semantic role | Value |
| --- | --- |
| app background | `#0B0F14` |
| surface | `#111821` |
| elevated surface | `#18212C` |
| editor background | `#0E141B` |
| border variant | `#263446` |
| border | `#3C516B` |
| text | `#D6E2F0` |
| text muted | `#A3B6C8` |
| accent | `#62D6C5` |
| secondary accent | `#8DA6FF` |
| success | `#7BD88F` |
| warning | `#F4C56A` |
| error | `#F27878` |

## Dawn palette

| Semantic role | Value |
| --- | --- |
| app background | `#F2EEE6` |
| surface | `#FFFDF8` |
| elevated surface | `#E9E3D8` |
| editor background | `#FAF7F0` |
| border variant | `#C8BDAE` |
| border | `#7D7061` |
| text | `#25221E` |
| text muted | `#746B61` |
| accent | `#237C72` |
| secondary accent | `#4F65B8` |
| success | `#337A48` |
| warning | `#9A6419` |
| error | `#B74343` |

## State contract

| State | Required evidence |
| --- | --- |
| default | surface, text and icon are readable |
| hover | background changes without movement or reflow |
| active | stronger than hover and preserves contrast |
| selected | accent plus border/icon/shape cue |
| focused | visible 2px equivalent ring without changing box size |
| disabled | lower emphasis but still legible; behavior unchanged |
| error | error border plus text or icon; not color-only |
| drag target | existing drop behavior retained; only styling changes |

The matrix applies to Button, Tab, ListItem, TreeViewItem, InputField, Toggle, Chip, Banner,
Popover, Context Menu, Modal and Agent cards.

## Icon pilot allowlist

File icons:

- `assets/icons/file_icons/folder.svg`
- `assets/icons/file_icons/folder_open.svg`
- `assets/icons/file_icons/file.svg`
- `assets/icons/file_icons/chevron_right.svg`
- `assets/icons/file_icons/chevron_down.svg`

Product icons:

- `assets/icons/ai_orion.svg`
- `assets/icons/magnifying_glass.svg`
- `assets/icons/settings.svg`
- `assets/icons/terminal.svg`
- `assets/icons/git_branch.svg`
- `assets/icons/close.svg`
- `assets/icons/check.svg`
- `assets/icons/warning.svg`
- `assets/icons/x_circle.svg`

Rules:

- Keep filenames, `IconName`, action semantics and call sites.
- Use the existing 16x16 viewBox and integer-aligned silhouettes.
- Prefer hard steps and continuous color clusters over curves and random pixel noise.
- Verify 10, 12, 14 and 16px rendering.
- Do not add a second icon-theme selector or modify registry/schema behavior.
- Expansion beyond the allowlist requires a separate reviewed batch.

## Accessibility

- Body text target: APCA Lc 75.
- General UI text target: APCA Lc 60.
- Large text/support graphics minimum: APCA Lc 45.
- Focus is always visible.
- Git, diagnostic and task states are not red/green-only.
- ARIA role, label, toolbar and tab order remain unchanged.
- Reduced motion remains respected; v1 adds no new global animation system.

### Night accessibility correction

The initial Night muted value `#8FA3B8` measured approximately Lc 51 against the app and
editor backgrounds with Orion Studio's existing APCA 0.0.98G-4g implementation. Execution
therefore raises `text.muted` to `#A3B6C8`, and raises placeholder, disabled, muted-icon,
line-number and terminal-dim roles to at least the Lc 45 support threshold. This is an
accessibility correction to the frozen palette, not a new visual direction.

## Approval boundary

This contract approves implementation of themes, icon pilot and global Pixel Chrome. It does
not pre-approve switching fresh-install defaults. UI-51 remains gated on UI-50 results.
