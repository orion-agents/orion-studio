---
title: Helix Mode - Orion Studio
description: Helix-style keybindings and modal editing in Orion Studio. Selection-first editing built on top of Vim mode.
---

# Helix Mode

_Work in progress. Not all Helix keybindings are implemented yet._

Orion Studio's Helix mode is an emulation layer that brings Helix-style keybindings and modal editing to Orion Studio. It builds upon Orion Studio's [Vim mode](./vim.md), so much of the core functionality is shared. Enabling `helix_mode` will also enable `vim_mode`.

For a guide on Vim-related features that are also available in Helix mode, please refer to our [Vim mode documentation](./vim.md).

To check the current status of Helix mode or request a missing feature, search the
[Helix issues](https://github.com/orion-agents/orion-studio/issues?q=is%3Aissue+helix)
before opening a new issue.

For a detailed list of Helix's default keybindings, please visit the [official Helix documentation](https://docs.helix-editor.com/keymap.html).

## Core differences

Any text object that works with `m i` or `m a` also works with `]` and `[`, so for example `] (` selects the next pair of parentheses after the cursor.
