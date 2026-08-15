---
title: Code Completions - Orion Studio
description: Orion Studio's code completions from language servers and edit predictions. Configure autocomplete behavior, snippets, and documentation display.
---

# Completions

Orion Studio supports two sources for completions:

1. "Code Completions" provided by Language Servers (LSPs) automatically installed by Orion Studio or via [Orion Studio Language Extensions](languages.md).
2. "Edit Predictions" provided by Zeta or external providers such as [GitHub Copilot](#github-copilot).

## Language Server Code Completions {#code-completions}

When there is an appropriate language server available, Orion Studio will provide completions of variable names, functions, and other symbols in the current file. You can disable these by adding the following to your Orion Studio `settings.json` file:

```json [settings]
"show_completions_on_input": false
```

You can manually trigger completions with `ctrl-space` or by triggering the `editor::ShowCompletions` action from the command palette.

> Note: Using `ctrl-space` in Orion Studio requires disabling the macOS global shortcut.
> Open **System Settings** > **Keyboard** > **Keyboard Shortcut**s >
> **Input Sources** and uncheck **Select the previous input source**.

For more information, see:

- [Configuring Supported Languages](./configuring-languages.md)
- [List of Orion Studio Supported Languages](./languages.md)

## Edit Predictions {#edit-predictions}

Orion Studio has built-in support for predicting multiple edits at a time through [Zeta](https://huggingface.co/zed-industries/zeta). The `zed-industries` namespace and Zeta artifacts come from the upstream Zed project; a hosted Zeta endpoint is available only after an Orion operator configures one.
Edit predictions appear as you type, and most of the time, you can accept them by pressing `tab`.

See the [edit predictions documentation](./ai/edit-prediction.md) for more information on how to setup and configure Orion Studio's edit predictions.
