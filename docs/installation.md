# Install Orion Studio on macOS Apple Silicon

Stable Orion Studio is distributed for **macOS 11.0 or later on Apple Silicon
(arm64)** only. Download it from
[GitHub Releases](https://github.com/orion-agents/orion-studio/releases); no
other download site or package manager is supported.

## Before You Download

Confirm that:

- the Mac uses Apple Silicon;
- the Mac runs macOS 11.0 or later;
- the GitHub release has an exact Stable version tag such as `v1.17.0` and is
  not marked **Pre-release**;
- the release contains `Orion-Studio-aarch64.dmg` and publishes its SHA-256
  digest; and
- you have read the release entry and the
  [release policy](./releases/release-policy.md).

Do not install an Intel, Linux, Windows, unsigned, locally shared, or
third-party-mirrored artifact as though it were the supported Stable release.

## Back Up Before First Launch

Quit Zed completely before backing up or starting Orion Studio. In Finder,
choose **Go > Go to Folder** and copy any existing directories that matter to a
separate backup location:

- `~/.config/zed`
- `~/Library/Application Support/Zed`
- `~/.config/orion-studio` if an earlier Orion Studio build was used
- `~/Library/Application Support/Orion Studio` if an earlier Orion Studio build
  was used

Before Orion Studio opens its database or first workspace, it checks for legacy
Zed configuration and application data. When legacy data exists, Orion Studio
copies it into its own directories and does not overwrite a different existing
Orion Studio file. The legacy Zed directories are retained, but a separate
backup is still required. A large legacy data directory can delay the first
window and produce sustained disk activity. Do not run Zed or another Orion
Studio process concurrently with this migration, and do not delete the legacy
data until Orion Studio has started successfully, has been restarted, and the
migrated settings and projects have been checked. If both products contain a
different file at the same relative path, Orion Studio stops before opening its
database and reports the conflict instead of choosing either copy.

## Download and Verify SHA-256

Download `Orion-Studio-aarch64.dmg` from the selected GitHub Stable release. In
Terminal, calculate its digest:

```sh
cd ~/Downloads
shasum -a 256 "Orion-Studio-aarch64.dmg"
```

Compare all 64 hexadecimal characters with the SHA-256 value published in that
same GitHub release. The filename and digest must both match. Do not continue if
the release omits a digest or if the values differ.

## Install and Verify Gatekeeper

1. Open `Orion-Studio-aarch64.dmg`.
2. Drag **Orion Studio.app** into **Applications**.
3. Eject the disk image.
4. Before first launch, run:

```sh
codesign --verify --deep --strict --verbose=2 "/Applications/Orion Studio.app"
spctl --assess --type execute --verbose=4 "/Applications/Orion Studio.app"
stapler validate "/Applications/Orion Studio.app"
file "/Applications/Orion Studio.app/Contents/MacOS/orion-studio"
```

`codesign` must complete successfully, `spctl` must report that the app is
accepted, `stapler` must validate the ticket, and `file` must identify an
`arm64` executable. If any check fails,
do not bypass Gatekeeper, remove quarantine attributes, or force the app open.
Delete the app and DMG, then report the release URL and command output through
the [support process](../SUPPORT.md).

## First Launch and Migration

Open **Orion Studio** from Applications. If legacy Zed data is present,
migration completes before Orion Studio initializes persistence or displays the
first workspace. Keep the Mac awake and avoid launching additional Orion Studio
or Zed processes while it runs.

A successful import leaves the original Zed data unchanged. A migration error
stops startup before Orion Studio opens its database, preserves both data trees,
and leaves the operation retryable after the conflict or filesystem problem is
resolved. Restart once after the first successful launch and verify the migrated
settings and projects before removing any backup.

If the first window does not appear or a migration error is shown:

1. Do not repeatedly launch more copies of the app.
2. Check `~/Library/Logs/Orion Studio/Orion Studio.log` for startup or migration
   errors.
3. Confirm that the Zed backup remains readable.
4. File an issue with the release tag, macOS version, Apple chip model, relevant
   redacted log lines, and whether Zed was running.

Never attach source code, credentials, access tokens, private project names, or
an entire unredacted log to a public issue.

## Updating

Automatic updates are not supported. For each update, download the newer Stable
release from GitHub Releases, verify its new SHA-256 digest and
Gatekeeper status, quit Orion Studio, and replace the app in Applications.

## Uninstalling

1. Quit Orion Studio.
2. Move **Orion Studio.app** from Applications to the Trash.
3. Empty the Trash only when recovery is no longer needed.

Removing the app does not remove user data. To remove Orion Studio data as a
separate, irreversible step, first back it up and then use Finder's **Go to
Folder** to review these locations before moving them to the Trash:

- `~/.config/orion-studio`
- `~/Library/Application Support/Orion Studio`
- `~/Library/Caches/Orion Studio`
- `~/Library/Logs/Orion Studio`
- `~/.local/state/Orion Studio`

Do not remove the Zed directories during Orion Studio uninstall; they belong to
the separately installed upstream application.

## Getting Help

Use [GitHub Issues](https://github.com/orion-agents/orion-studio/issues/new/choose)
for reproducible, non-sensitive release problems. Follow
[SECURITY.md](../SECURITY.md) for vulnerabilities or sensitive reports. GitHub
Discussions is not required or assumed to be enabled.
