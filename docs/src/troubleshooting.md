---
title: Troubleshooting
description: "Common issues and solutions for Orion Studio on all platforms."
---

# Troubleshooting

This guide covers common troubleshooting techniques for Orion Studio.
Sometimes you'll be able to identify and resolve issues on your own using this information.
Other times, troubleshooting means gathering the right information (logs, profiles, or reproduction steps) to help us diagnose and fix the problem.

> **Note**: To open the command palette, use `cmd-shift-p` on macOS or `ctrl-shift-p` on Windows / Linux.

## Retrieve Orion Studio and System Information

When reporting issues or seeking help, it's useful to know your Orion Studio version and system specifications. You can retrieve this information using the following actions from the command palette:

- {#action zed::About}: Find your Orion Studio version number
- {#action zed::CopySystemSpecsIntoClipboard}: Populate your clipboard with Orion Studio version number, operating system version, and hardware specs
- {#action zed::CopyInstalledExtensionsIntoClipboard}: Populate your clipboard with a list of your installed extensions and versions

## Orion Studio Log

Often, a good first place to look when troubleshooting any issue in Orion Studio is the Orion Studio log, which might contain clues about what's going wrong.
You can review the most recent 1000 lines of the log by running the {#action zed::OpenLog} action from the command palette.
If you want to view the full file, you can reveal it in your operating system's native file manager via {#action zed::RevealLogInFileManager} from the command palette.

You'll find the Orion Studio log in the respective location on each operating system:

- macOS: `~/Library/Logs/Orion Studio/Orion Studio.log`
- Windows: `C:\Users\YOU\AppData\Local\Orion Studio\logs\Orion Studio.log`
- Linux: `~/.local/share/orion-studio/logs/Orion Studio.log` or `$XDG_DATA_HOME/orion-studio/logs/Orion Studio.log`

> **Note:** In some cases, it might be useful to monitor the log live, such as when [developing an Orion Studio extension](./extensions/developing-extensions.md).
> Example: `tail -f ~/Library/Logs/Orion Studio/Orion Studio.log`

The log may contain enough context to help you debug the issue yourself, or you may find specific errors that are useful when filing a [GitHub issue](https://github.com/orion-agents/orion-studio/issues/new/choose).

## Performance Issues (Profiling)

If you're running into performance issues in Orion Studio (hitches, hangs, or general unresponsiveness), having a performance profile attached to your issue will help us zero in on what is getting stuck.

### macOS

Xcode Instruments (which comes bundled with your [Xcode](https://apps.apple.com/us/app/xcode/id497799835) download) is the standard tool for profiling on macOS.

1. With Orion Studio running, open Instruments
1. Select `Time Profiler` as the profiling template
1. In the `Time Profiler` configuration, set the target to the running Orion Studio process
1. Start recording
1. Perform the action in Orion Studio that causes performance issues
1. Stop recording
1. Save the trace file
1. Compress the trace file into a zip archive
1. File a [GitHub issue](https://github.com/orion-agents/orion-studio/issues/new/choose) with the trace zip attached

<!--### Windows-->

<!--### Linux-->

## Startup and Workspace Issues

Orion Studio creates local SQLite databases to persist data relating to its workspace and your projects. These databases store, for instance, the tabs and panes you have open in a project, the scroll position of each open file, the list of all projects you've opened (for the recent projects modal picker), etc. You can find and explore these databases in the following locations:

- macOS: `~/Library/Application Support/Orion Studio/db`
- Linux and FreeBSD: `~/.local/share/orion-studio/db` (or under `$XDG_DATA_HOME/orion-studio` or `$FLATPAK_XDG_DATA_HOME/orion-studio`)
- Windows: `%LOCALAPPDATA%\Orion Studio\db`

The naming convention of these databases takes the form `0-<release_channel>`:

- Stable: `0-stable`
- Preview: `0-preview`
- Nightly: `0-nightly`
- Dev: `0-dev`

While rare, we've seen a few cases where workspace databases became corrupted, which prevented Orion Studio from starting.
If you're experiencing startup issues, you can test whether it's workspace-related by temporarily moving the database from its location, then trying to start Orion Studio again.

> **Note**: Moving the workspace database will cause Orion Studio to create a fresh one.
> Your recent projects, open tabs, etc. will be reset to "factory".

If your issue persists after regenerating the database, please [file an issue](https://github.com/orion-agents/orion-studio/issues/new/choose).

## Language Server Issues

If you're experiencing language-server related issues, such as stale diagnostics or issues jumping to definitions, restarting the language server via {#action editor::RestartLanguageServer} from the command palette will often resolve the issue.

## Agent Error Messages

### "Max tokens reached"

You see this error when the agent's response exceeds the model's maximum token limit. This happens when:

- The agent generates an extremely long response
- The conversation context plus the response exceeds the model's capacity
- Tool outputs are large and consume the available token budget

**To resolve this:**

1. Start a new thread to reduce context size
2. Use a model with a larger token limit in AI settings
3. Break your request into smaller, focused tasks
4. Clear tool outputs or previous messages using the thread controls

The token limit varies by model—check your model provider's documentation for specific limits.
