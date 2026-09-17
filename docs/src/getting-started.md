---
title: Getting Started with Orion Studio
description: Get started with Orion Studio, the fast open-source code editor. Essential commands, environment setup, and navigation basics.
---

# Getting Started

Orion Studio is an open-source code editor with built-in collaboration and AI tools.

This guide covers the essential commands, environment setup, and navigation basics.

## Quick Start

### Welcome Page

When you open Orion Studio without a folder, you see the welcome page in the main editor area. The welcome page offers quick actions to open a folder, clone a repository, or view documentation. Once you open a folder or file, the welcome page disappears. If you split the editor into multiple panes, the welcome page appears only in the center pane when empty—other panes show a standard empty state.

To reopen the welcome page, close all items in the center pane or use the command palette to search for "Welcome".

### 1. Open a Project

Open a folder from the command line:

```sh
orion ~/projects/my-app
```

Or use `Cmd+O` (macOS) / `Ctrl+O` (Linux/Windows) to open a folder from within Orion Studio.

By default, new projects open in your current window's threads sidebar. To open in a new window instead, use `orion -n ~/projects/my-app` or press `Cmd+Enter` when selecting from Open Recent. See [Windows & Projects](./windows-and-projects.md) for more details.

### 2. Learn the Essential Commands

| Action          | macOS         | Linux/Windows  |
| --------------- | ------------- | -------------- |
| Command palette | `Cmd+Shift+P` | `Ctrl+Shift+P` |
| Go to file      | `Cmd+P`       | `Ctrl+P`       |
| Go to symbol    | `Cmd+Shift+O` | `Ctrl+Shift+O` |
| Find in project | `Cmd+Shift+F` | `Ctrl+Shift+F` |
| Toggle terminal | `` Ctrl+` ``  | `` Ctrl+` ``   |
| Open settings   | `Cmd+,`       | `Ctrl+,`       |

The command palette (`Cmd+Shift+P`) is your gateway to every action in Orion Studio. If you forget a shortcut, search for it there.

### Panel Layout

Use **Panel Layout > Agentic** from the user menu in the title bar (or the {#action workspace::UseAgenticLayout} action) when you want the Agent Panel and Threads Sidebar next to each other on the left. Use **Panel Layout > Classic** (or {#action workspace::UseClassicLayout}) to restore the editor-oriented layout.

Use **Panel Layout > AI Native** (or the {#action workspace::UseAiNativeLayout} action) for a conversation-first workspace: the Threads Sidebar moves to the left, the current task's conversation fills the center, and a tab-free **Task Environment** panel docks on the right. The Agent Panel keeps running your threads but stays out of the way; file previews stay on the right, and **Open review in Code Workspace** with **Return to task** are the explicit way in and out of full editing.

### 3. Configure Your Editor

Open the Settings Editor with `Cmd+,` (macOS) or `Ctrl+,` (Linux/Windows). Search for any setting and change it directly.

Common first changes:

- **Theme**: Press `Cmd+K Cmd+T` (macOS) or `Ctrl+K Ctrl+T` (Linux/Windows) to open the theme selector
- **Font**: Search for `buffer_font_family` in Settings
- **Format on save**: Search for `format_on_save` and set to `on`

### 4. Set Up Your Language

Orion Studio includes built-in support for many languages. For others, install the extension:

1. Open Extensions with `Cmd+Shift+X` (macOS) or `Ctrl+Shift+X` (Linux/Windows)
2. Search for your language
3. Click Install

See [Languages](./languages.md) for language-specific setup instructions.

### 5. Try AI Features

Orion Studio includes built-in AI assistance. Open the Agent Panel with `Cmd+Shift+A` (macOS) or `Ctrl+Shift+A` (Linux/Windows) to start a conversation, or use `Cmd+Enter` (macOS) / `Ctrl+Enter` (Linux/Windows) for inline assistance.

See [AI Overview](./ai/overview.md) to configure providers and learn what's possible.

## Coming from Another Editor?

We have dedicated guides for switching from other editors:

- [VS Code](./migrate/vs-code.md) — Import settings, map keybindings, find equivalent features
- [IntelliJ IDEA](./migrate/intellij.md) — Adapt to Orion Studio's approach to navigation and refactoring
- [PyCharm](./migrate/pycharm.md) — Set up Python development in Orion Studio
- [WebStorm](./migrate/webstorm.md) — Configure JavaScript/TypeScript workflows
- [RustRover](./migrate/rustrover.md) — Rust development in Orion Studio

You can also enable familiar keybindings:

- **Vim**: Enable `vim_mode` in settings. See [Vim Mode](./vim.md).
- **Helix**: Enable `helix_mode` in settings. See [Helix Mode](./helix.md).

## Join the Community

Orion Studio is open source. Use the Orion Studio GitHub repository to
contribute code, report bugs, or suggest features.

- [GitHub Issues](https://github.com/orion-agents/orion-studio/issues)

The upstream Zed community channels are not Orion Studio support channels. See
[Upstream Attribution and Compatibility](./orion-studio-upstream-and-compatibility.md)
when reporting an issue that may also affect upstream Zed.
