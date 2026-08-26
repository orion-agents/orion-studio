---
title: Installing Extensions
description: "Install and manage Orion Studio extensions from a configured registry or local development checkout."
---

# Installing Extensions {#installing-extensions}

Extensions add functionality to Orion Studio, including languages, themes, and
AI tools. Browse and install them from the Extension Gallery when the build has
a configured registry. No public Orion extension catalog is assumed to be
deployed.

Open the Extension Gallery with {#kb zed::Extensions}, or select "Orion Studio > Extensions" from the menu bar.

## Preview and Dev builds

Preview and Dev builds run the Extension Gallery in local-only mode. They list
installed and development extensions without contacting an inherited registry.
Remote search, install, upgrade, version history, recommendations, and automatic
extension updates are unavailable and fail closed. You can still configure,
rebuild, or uninstall a local extension and use **Install Dev Extension**.

An extension can make its own network requests when its declared capabilities
allow that. The local-only Gallery boundary does not turn third-party extensions
into offline or Orion-operated software.

For an unpublished extension, use **Install Dev Extension** and select its local
checkout. See [Developing Extensions](./developing-extensions.md).

## Installation Location

- On macOS, extensions are installed in `~/Library/Application Support/Orion Studio/extensions`.
- On Linux, they are installed in either `$XDG_DATA_HOME/orion-studio/extensions` or `~/.local/share/orion-studio/extensions`.
- On Windows, the directory is `%LOCALAPPDATA%\Orion Studio\extensions`.

This directory contains two subdirectories:

- `installed`, which contains the source code for each extension.
- `work` which contains files created by the extension itself, such as downloaded language servers.

## Auto-installing

To automate extension installation/uninstallation see the docs for [auto_install_extensions](../reference/all-settings.md#auto-install-extensions).
