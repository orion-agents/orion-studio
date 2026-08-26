# macOS Apple Silicon Preview

This document defines the supported user-facing scope for the Orion Studio
macOS Apple Silicon Preview. Version-specific changes and the authoritative
SHA-256 digest belong in the corresponding
[GitHub Pre-release](https://github.com/orion-agents/orion-studio/releases).

## Release Scope

| Item | Preview commitment |
| --- | --- |
| Stability | Pre-release software; not Stable |
| Platform | macOS 11.0 or later on Apple Silicon (`arm64`) only |
| Download | GitHub Releases only |
| DMG asset | `Orion-Studio-aarch64.dmg` |
| Installed app | `Orion Studio Preview.app` |
| Integrity | Release-published SHA-256 digest must match |
| macOS trust | Code-signature verification and Gatekeeper assessment must pass |
| Updates | Manual download and verification of each newer Preview |

A DMG that is missing its release digest, fails Gatekeeper, or is distributed
outside the repository's GitHub Releases page is not an approved Preview
artifact.

## Install Safely

Follow the complete [installation guide](../installation.md). The minimum safe
path is:

1. Back up legacy Zed and existing Orion Studio data.
2. Download `Orion-Studio-aarch64.dmg` from a GitHub release marked
   **Pre-release**.
3. Compare its SHA-256 digest with that release's published value.
4. Install `Orion Studio Preview.app` in Applications.
5. Require both `codesign` verification and `spctl` Gatekeeper assessment to
   succeed before first launch.

Do not use quarantine removal or other Gatekeeper bypasses as an installation
step.

## First-Launch Data Migration

On first launch, Orion Studio may copy configuration from `~/.config/zed` and
application data from `~/Library/Application Support/Zed` into Orion Studio's
own directories. Migration runs before Orion Studio initializes persistence or
opens the first workspace and retains the legacy directories.

Quit Zed first and keep an independent backup. A successful migration continues
startup. A conflict or filesystem failure stops startup before Orion Studio
opens its database, leaves both trees intact, and can be retried after the cause
is resolved. Restart once after the first successful launch, and do not delete
the backup or legacy directories until the migrated editor state is reviewed.

## Known Limitations

- macOS 10.15 and earlier, Intel Macs, Linux, and Windows do not have supported
  downloadable Preview packages in this release scope. The Apple Silicon
  artifact has a Mach-O minimum deployment target of macOS 11.0.
- This is pre-release software and may contain incomplete rebranding,
  compatibility defects, crashes, or data-migration defects.
- A large legacy Zed data set can delay the first window and create substantial
  disk I/O. The Preview does not yet provide migration progress or cancellation
  controls.
- Migration fills missing Orion Studio files and accepts byte-for-byte and
  permission-identical files as an idempotent retry. A different existing file
  or entry type is reported as a conflict; neither copy is overwritten.
- If Zed keeps changing the source data, migration retries its stability check
  up to three times and then reports failure without modifying the legacy data.
- No Orion-operated account, subscription, collaboration backend, AI proxy, or
  other hosted online service is offered as part of this Preview. Account and
  collaboration connection attempts fail closed instead of contacting an
  inherited hosted endpoint.
- Hosted telemetry is forced off in Preview and Dev builds. Enabling the local
  telemetry settings does not send metrics, diagnostics, or crash reports to an
  Orion-operated endpoint.
- An Orion extension marketplace is not supported. The Extension Gallery shows
  locally installed and development extensions only; remote search, install,
  upgrade, and automatic extension updates fail closed. Locally installed and
  **Install Dev Extension** workflows remain available.
- Preview does not download a headless remote-server artifact from a hosted
  release service. Direct SSH can reuse an exact matching cached server binary;
  otherwise install the matching server binary manually before connecting.
- Automatic updates are not supported. Users must download and verify each new
  Preview manually.
- Third-party agents, model providers, source-control hosts, and other
  integrations are separately configured and governed by their own terms and
  data practices; their availability is not guaranteed by Orion Studio.
- Commands launched in Orion Studio's integrated terminal are separate programs
  and may ask macOS for privacy-sensitive access. macOS can attribute those
  prompts to Orion Studio as the responsible host. Review the requesting
  command and deny access unless it is expected; Orion Studio itself declares
  only Apple Events, microphone input, and Wasmtime executable-memory release
  entitlements.

## Report a Problem

For a non-sensitive bug, open a
[GitHub Issue](https://github.com/orion-agents/orion-studio/issues/new/choose)
and include:

- the GitHub release tag and DMG SHA-256;
- macOS version and Apple chip model;
- exact reproduction steps and expected versus actual behavior;
- whether this was a fresh install or a migration from Zed; and
- minimal, redacted logs or screenshots.

For a vulnerability or any report containing sensitive security details, use a
private [GitHub Security Advisory](https://github.com/orion-agents/orion-studio/security/advisories/new)
and follow [SECURITY.md](../../SECURITY.md).
