# Moving Development to a New Machine

Notes for relocating the Orion Studio development environment to a new Mac
(for example, a new Mac mini). The environment has one load-bearing external
dependency — the development SSD — so most of this page is about keeping that
volume working after the move.

## The external SSD is the development volume

The build artifacts and CI runner live on an external NVMe SSD mounted at:

```text
/Volumes/20gbps
```

Two absolute-path conventions depend on this exact mount point:

| Path | Role |
| --- | --- |
| `/Volumes/20gbps/dev/orion-studio/target` | Main checkout's cargo build directory (`~/ai-project/orion-studio/target` is a symlink to it) |
| `/Volumes/20gbps/dev/orion-studio-target-v1.17.2` | `CARGO_TARGET_DIR` used by the `release/v1.17.2` worktree |
| `/Volumes/20gbps/orion-agents/orion-studion-runner` | Self-hosted GitHub Actions runner (optional; macOS release builds default to hosted runners) |

**Keep the volume name `20gbps` unchanged.** If macOS mounts the disk under a
different name (for example `20gbps 1` because the original volume name
collided, or the disk was renamed), every symlink and pinned path above breaks
silently.

### Symptom: cargo fails with `os error 20`

If any cargo command dies with `failed to create directory ...: Not a
directory (os error 20)`, the SSD is **not mounted**. Mount it and re-run:

```sh
ls /Volumes/20gbps && cargo check -p orion-studio
```

Never fall back to creating a real directory at `/Volumes/20gbps` — that masks
the unmounted volume and leaves the target on the internal disk.

## Reusing the build cache

The ~171 GB of incremental build state survives the move **only if**:

1. The new machine is Apple Silicon (aarch64), and
2. `rustup` installs the toolchain version pinned in
   [`rust-toolchain.toml`](../../rust-toolchain.toml) — fingerprints embed the
   compiler version, so a different toolchain forces a full rebuild.

With both satisfied, plug the SSD in, install rustup, and the first
`cargo check` after the move should take minutes, not tens of minutes. A fast
first build is the confirmation that the cache was reused.

## First-boot checklist on the new machine

1. Install [Homebrew](https://brew.sh) and restore packages with
   `brew bundle` (dump the old machine's `Brewfile` with
   `brew bundle dump` before decommissioning it).
2. Install rustup; the pinned toolchain in `rust-toolchain.toml` is selected
   automatically inside the checkout.
3. Install Xcode and open it once so the **Metal toolchain** is provisioned —
   `gpui`'s macOS shaders require it, and `cargo check -p orion-studio` fails
   without it.
4. Authenticate `gh` (`gh auth login`) and confirm SSH keys for GitHub.
5. Clone or copy the checkout to `~/ai-project/orion-studio` and the
   worktrees to `~/ai-project/orion-studio-worktrees` — keeping these
   absolute paths unchanged avoids touching worktree metadata, which records
   the main repository's absolute path.

## Self-hosted runner

A GitHub Actions runner registered to `orion-agents/orion-studio` lives on the
SSD, but **one runner identity can only be online from one machine at a
time**. Before switching machines:

1. On the old machine: stop the service
   (`launchctl bootout gui/$(id -u)/actions.runner.orion-agents-orion-studio.orion-studio-mac-arm64-20gbps`).
2. On the new machine: re-run the runner's `config` step, or re-create the
   launchd plist pointing at the SSD path, keeping the same label
   (`orion-studio-mac-arm64-20gbps`) so workflow `runs-on` references keep
   working.

This is optional: since the hosted-runner default landed, macOS Stable
releases build on GitHub-hosted runners and the self-hosted runner is an
acceleration option only.

## Signing and notarization

The Developer ID Application certificate lives in the login keychain. Export
a `.p12` backup of it (with a strong password) **before** decommissioning the
old machine — it is the only credential in the release chain that cannot be
regenerated without a new issuance from the Apple Developer account. If
notarization uses a `notarytool` keychain profile, re-create it on the new
machine (`xcrun notarytool store-credentials`).

## Service daemons

LaunchAgents that reference absolute paths (for example the local model
gateway proxy) need their plists reviewed after the move; paths under
`/Users/<you>` and `/Volumes/20gbps` survive as long as the username and
volume name are preserved.
