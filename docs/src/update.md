---
title: Update Orion Studio
description: "Update Orion Studio from a release bundle, package manager, or source checkout, with optional automatic updates when a release service is configured."
---

# Update Orion Studio

How you update Orion Studio depends on how it was installed. A packaged build
can use the built-in updater only when its distributor configures a compatible
release service. A local source build does not imply that such a service is
available.

## Auto-updates

The `auto_update` setting defaults to `true`. When the running build has a
configured release service, Orion Studio can check for a matching release,
download it in the background, and apply it on restart.

If no release service is configured, update with the same channel that supplied
the application:

- replace a downloaded release bundle with a newer bundle;
- use the package manager that installed Orion Studio; or
- update the repository checkout and rebuild it by following
  [Installation](./installation.md).

No public `orion.dev` update service is assumed to be deployed.

## How to check your current version

To check which version of Orion Studio you're using:

Open the Command Palette (Cmd+Shift+P on macOS, Ctrl+Shift+P on Linux/Windows).

Type and select {#action zed::About}. The `zed::` prefix is a retained internal
action namespace, not the product brand. A modal will show the current version.

## How to control update behavior

To turn off update checks for a configured release service, open the Settings
Editor (Cmd , on macOS) and find `Auto Update` under General Settings, or set
`"auto_update": false` in your settings file.

Distributors can disable the built-in updater and provide package-specific
instructions with `ORION_STUDIO_UPDATE_EXPLANATION`. The legacy
`ZED_UPDATE_EXPLANATION` environment variable remains supported for compatible
build pipelines.
