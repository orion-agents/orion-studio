# Preview Support

Support is limited to the latest Orion Studio Preview for **macOS 11.0 or later
on Apple Silicon**, downloaded from this repository's GitHub Releases page.
This document does not promise a response time, service level, or fix for every
report.

## Before Filing an Issue

1. Confirm the Mac uses Apple Silicon, runs macOS 11.0 or later, and the
   artifact is `Orion-Studio-aarch64.dmg` from a release marked
   **Pre-release**.
2. Verify the SHA-256 digest, code signature, architecture, and Gatekeeper
   assessment using the [installation guide](./docs/installation.md).
3. Reproduce the problem with the latest Preview and search existing issues.
4. Remove secrets, source code, personal data, private repository names, and
   unrelated log content from the report.

## File a Non-Sensitive Bug

Use [GitHub Issues](https://github.com/orion-agents/orion-studio/issues/new/choose).
GitHub Discussions is not required or assumed to be enabled.

Include:

- Preview release tag and DMG SHA-256;
- macOS version and Apple chip model;
- fresh install or Zed migration;
- exact steps to reproduce;
- expected and actual behavior; and
- minimal redacted logs, crash output, or screenshots.

For first-launch problems, also say whether Zed was running and whether legacy
data existed in `~/.config/zed` or `~/Library/Application Support/Zed`.

## Supported Scope

The Preview support scope does not include:

- Intel Mac, Linux, or Windows downloadable packages;
- artifacts from mirrors, local shares, or unverified builds;
- an Orion-operated extension marketplace or automatic update service;
- Orion-hosted accounts, subscriptions, collaboration, AI proxying, or other
  online services; or
- availability or behavior of independently configured third-party providers.

For security vulnerabilities or reports that need sensitive details, do not use
a public issue. Follow [SECURITY.md](./SECURITY.md) and submit a private GitHub
Security Advisory.
