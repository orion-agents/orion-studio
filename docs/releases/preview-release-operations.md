# macOS Apple Silicon Preview Release Operations

This runbook covers the first public Orion Studio Preview. It does not authorize
a Stable release or any hosted Orion service.

## Required GitHub Runner

Use a dedicated, trusted Apple Silicon Mac. Do not attach a personal workstation
that handles untrusted pull requests.

1. Open the repository's **Settings → Actions → Runners** page.
2. Add a macOS ARM64 self-hosted runner by following GitHub's generated commands.
3. Add the custom label `orion-studio-macos-release`.
4. Keep the default `self-hosted`, `macOS`, and `ARM64` labels.
5. Provide at least 100 GiB free before a release starts and at least 90 GiB after checkout.
6. Restrict the runner to this trusted repository and the manual Preview workflow.

The release job requires all four labels. A standard GitHub `macos-15` runner has
only 14 GiB of storage and is not a supported Orion Studio release builder.
The larger safety margin is intentional: a qualification run that retained
Debug, host Release, and arm64 Release profiles peaked at 89 GiB in `target`.
The workflow always runs `cargo clean`, but it must also tolerate stale caches
left by an interrupted prior run.

## Apple Inputs

Store these as secrets in the protected `preview` environment:

- `MACOS_CERTIFICATE`: base64-encoded Developer ID Application `.p12` file;
- `MACOS_CERTIFICATE_PASSWORD`: password used when exporting the `.p12` file;
- `APPLE_NOTARIZATION_KEY`: App Store Connect API private key contents;
- `APPLE_NOTARIZATION_KEY_ID`: API key ID;
- `APPLE_NOTARIZATION_ISSUER_ID`: API issuer ID.

Store these as variables in the same environment:

- `ORION_STUDIO_MACOS_SIGNING_IDENTITY`: the complete
  `Developer ID Application: ... (TEAMID)` identity;
- `ORION_STUDIO_MACOS_TEAM_ID`: the ten-character Apple Team ID.

Store `ORION_STUDIO_RELEASE_SSH_ALLOWED_SIGNERS_BASE64` as a repository variable.
Its decoded value must be a Git SSH `allowedSignersFile` containing only approved
release-tag signers. Public signing keys are not secrets, but changes to this variable
must be reviewed like release code.

`ORION_STUDIO_MACOS_PROVISIONING_PROFILE_BASE64` is optional. The current app
does not claim restricted entitlements, so Developer ID distribution does not
require a provisioning profile. Add one only when a restricted capability is
introduced. The workflow then verifies its bundle ID, Team ID, expiry, and
embedded contents.

Keep at least one independent required reviewer on the `preview` environment.
Never store certificate passwords or API private keys in repository variables,
workflow inputs, release notes, or logs.

## Prepare the Immutable Source

The workflow accepts only `vMAJOR.MINOR.PATCH-pre`. The tag commit must:

- use an annotated SSH-signed tag whose signer appears in the configured allowed-signers file;
- be an ancestor of `origin/main`;
- contain the matching `orion-studio` package version;
- contain `preview` in `crates/zed/RELEASE_CHANNEL`; and
- remain immutable for the complete run.

Run the local source gates before dispatch:

```sh
./script/test-orion-brand
./script/check-orion-brand
./script/test-uninstall
./script/test-preview-release
./script/check-secrets
./script/check-macos-entitlements
cargo fmt --all -- --check
git diff --check
```

Create a new signed tag only after those gates pass. Do not move an existing tag:

```sh
git config gpg.format ssh
git config user.signingkey ~/.ssh/your_release_signing_key
git tag -s -m 'Orion Studio Preview v1.16.1-pre' v1.16.1-pre RELEASE_COMMIT_SHA
git verify-tag v1.16.1-pre
git push origin refs/tags/v1.16.1-pre
```

## Build the Draft

1. Open **Actions → Release macOS Apple Silicon Preview (Bootstrap)**.
2. Select `main`, enter the immutable Preview tag, and dispatch.
3. Review the validated commit, version, channel, and runner capacity.
4. Approve the `preview` environment only when the displayed values match.
5. Wait for build, Developer ID signing, notarization, stapling, Gatekeeper,
   architecture, minimum macOS version, DMG, and checksum verification.

After the verified assets are uploaded to the workflow artifact store, the
self-hosted runner runs `cargo clean` even when an earlier build step failed.
Keep the Cargo registry/tool cache, but do not retain workspace `target`
directories between releases; they can grow by tens of GiB per target/profile.

The workflow may create or reuse a Draft Pre-release. It only accepts:

- `Orion-Studio-aarch64.dmg`;
- `SHA256SUMS`.

Do not upload a locally ad-hoc-signed DMG or replace a published asset.

## Clean-Mac Acceptance

Download both Draft assets on a separate Apple Silicon Mac without the repository
or build cache. Verify them before opening the DMG:

```sh
shasum -a 256 -c SHA256SUMS
hdiutil verify Orion-Studio-aarch64.dmg
xcrun stapler validate Orion-Studio-aarch64.dmg
spctl --assess --type open --context context:primary-signature --verbose=2 \
  Orion-Studio-aarch64.dmg
```

Then install `Orion Studio Preview.app` and verify:

```sh
codesign --verify --deep --strict --verbose=2 \
  '/Applications/Orion Studio Preview.app'
spctl --assess --type execute --verbose=2 \
  '/Applications/Orion Studio Preview.app'
```

Complete first launch, open a folder, run the `orion` CLI, test an `orion://`
link, and verify migration with a backed-up legacy Zed profile. Confirm the old
Zed data is retained and that Orion Studio can be removed without deleting it.

## Publish or Roll Back

Publish the Draft as a Pre-release only after the clean-Mac acceptance record is
attached to the release. If any gate fails:

- keep the release as Draft;
- preserve logs and notarization submission identifiers;
- do not move or reuse the tag;
- fix the issue on `main` and create a new patch Preview tag.
