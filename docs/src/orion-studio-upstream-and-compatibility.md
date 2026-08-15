---
title: Upstream Attribution and Compatibility
description: How Orion Studio credits its Zed foundation and preserves compatibility identifiers during the migration.
---

# Upstream Attribution and Compatibility

Orion Studio is derived from the open-source Zed editor. The editor code remains
subject to the repository's GPL-3.0-or-later license, while separately licensed
components such as GPUI retain their own license terms. Copyright notices,
contributors, and third-party license notices remain intact.

Orion Studio is the product name shown to users. References to Zed in this
documentation are retained only when they identify an upstream project or an
implemented compatibility contract. They do not imply that Zed Industries
operates, endorses, or provides Orion Studio services.

## Compatibility identifiers

The following identifiers may remain visible in technical examples because the
current source still consumes them:

- `crates/zed` is the historical workspace path that contains the Orion Studio
  application entry point.
- `zed_extension_api`, its Rust alias `zed`, and the WIT namespace
  `zed:extension` remain stable extension ABI identifiers.
- Action identifiers such as `zed::OpenSettings` are internal command IDs used
  by the documentation preprocessor and keymap system.
- Project-local configuration currently remains under `.zed/`, including
  `.zed/settings.json`, `.zed/tasks.json`, and `.zed/debug.json`.
- `ZED_*`, `zed://`, the `zed` CLI alias, and legacy Zed data directories are
  accepted only where the application implements migration or compatibility.
- Historical route filenames such as `configuring-zed.md`, `ai/zed-agent.md`,
  and `account/zed-hosted-models.md` remain in place so existing links do not
  break; their page titles use Orion Studio terminology.
- Third-party repositories, packages, and assets may retain `zed` in their
  proper names.

New instructions use `orion` for the packaged command-line launcher and
`orion-studio` for the application binary or its equivalent launcher alias.
Compatibility names should not be introduced into new integrations.

## Service availability

Orion-hosted account, billing, collaboration, update, telemetry, and model
services are deployment-dependent. This documentation does not assume that an
`orion.dev` endpoint is live. Features that require an Orion service are
available only after an operator deploys and configures the corresponding
endpoint. Local editing, local model providers, provider API keys, external
agents, and other offline or self-hosted workflows do not require an Orion
hosted service unless their own setup says otherwise.
