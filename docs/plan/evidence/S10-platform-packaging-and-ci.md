# S10 — Platform Packaging, Installer & CI/CD Identity (Evidence)

**Subplan:** `docs/plan/subplans/10-platform-packaging-and-ci.md`
**Status:** DONE (bundle metadata, installer scripts, platform resources, CI all migrated; two latent packaging defects found and fixed during verification)
**Date:** 2026-08-08 (continued 2026-08-12)

---

## 1. Scope

**Allowed:** `crates/zed/Cargo.toml` bundle metadata, `crates/zed/resources`, `script/bundle-*`, `script/flatpak`, `.github/workflows`.
**Forbidden:** business logic, RPC wire format, service endpoints (→ S08), collab/remote runtime hosts (→ S09).

**Contract (S02):** display `Orion Studio`, slug `orion-studio`, bundle id `dev.orion.OrionStudio*`, URL scheme `orion://` (with `zed://` retained as a compatibility handler), docs/CI links → `orion.dev` / `orion-agents/orion-studio`.

---

## 2. Bundle identifiers (`crates/zed/Cargo.toml`)

| Channel | `identifier` (before) | `identifier` (after) | `name` |
|---|---|---|---|
| Dev | `dev.zed.Zed-Dev` | `dev.orion.OrionStudio-Dev` | `Orion Studio Dev` |
| Nightly | `dev.zed.Zed-Nightly` | `dev.orion.OrionStudio-Nightly` | `Orion Studio Nightly` |
| Preview | `dev.zed.Zed-Preview` | `dev.orion.OrionStudio-Preview` | `Orion Studio Preview` |
| Stable | `dev.zed.Zed` | `dev.orion.OrionStudio` | `Orion Studio` |

`osx_url_schemes` → `["orion", "zed"]` (legacy `zed` handler retained as a compat registration).

---

## 3. Platform resources

| File | Change |
|---|---|
| `crates/zed/resources/flatpak/zed.metainfo.xml.in` | `zed.dev`→`orion.dev`; `github.com/zed-industries/zed`→`orion-agents/orion-studio`; `<developer id="dev.zed">`→`dev.orion`; `caption>Zed`→`Orion Studio` |
| `crates/zed/resources/snap/snapcraft.yaml.in` | `name: zed`→`orion-studio`; `title: Zed`→`Orion Studio`; `apps: zed`→`orion-studio`; `common-id: dev.zed.Zed`→`dev.orion.OrionStudio`; `command: usr/bin/zed`→`usr/bin/orion-studio`; `zed.dev`→`orion.dev` |
| `crates/zed/resources/windows/zed.iss` | `Zed.exe`→`orion-studio.exe`; `Software\Classes\zed`→`Software\Classes\orion`; `www.zed.dev`→`www.orion.dev` |
| `crates/zed/resources/windows/zed.sh` | `zed.exe`→`orion-studio.exe` (both branches) |
| `crates/zed/resources/zed.desktop.in` | `Keywords=zed;`→`orion-studio;`; `x-scheme-handler/zed`→`x-scheme-handler/orion;x-scheme-handler/zed` |

> Note: the desktop file's `Exec`/`TryExec` use `$APP_CLI` (resolved by the installer). The `.desktop` template **filename** `zed.desktop.in` was intentionally NOT renamed — see KEEP-ASSET.

---

## 4. CI / workflows / tooling repo refs

- `.github/workflows/**` (26 files): `zed-industries/zed`→`orion-agents/orion-studio`, `https://zed.dev`→`https://orion.dev` (0 `zed.dev` host refs remain in workflows).
- `script/**` (release/CI tooling): `zed-industries/zed`→`orion-agents/orion-studio` (0 remaining).

---

## 5. Installer scripts — migrated + defects fixed

### 5.1 `script/bundle-linux`
- Editor binary is now built as `release/orion-studio` (S05 `[[bin]]` rename). **Defect found:** the script still referenced `release/zed` at 4 sites (sentry upload, `llvm-objcopy`, `cp` to `libexec`, `ldd`). All corrected to `release/orion-studio`.
- Editor is now staged at `libexec/orion-studio` (the canonical path the CLI discovers — `crates/cli/src/main.rs:952`), with a `libexec/zed-editor` **compat symlink** preserved.
- CLI staged at `bin/orion-studio` (matches `APP_CLI`), with a `bin/zed` **compat symlink**.
- Install dir renamed `zed$suffix.app` → `orion-studio$suffix.app`; archive artifacts `zed-linux-*`→`orion-studio-linux-*`, `zed-remote-server-linux-*`→`orion-studio-remote-server-linux-*`.

### 5.2 `script/install.sh`
- `appid` → `dev.orion.OrionStudio*` (was `dev.zed.Zed*`) — **critical**: must match the desktop filename shipped by `bundle-linux`.
- Install dir `~/.local/zed$suffix.app` → `~/.local/orion-studio$suffix.app`; `src_dir` corrected to the new path (latent bug fixed).
- CLI symlink → `~/.local/bin/orion-studio` (+ legacy `bin/zed` alias).
- Desktop `Exec=`/`Icon=` substitution retargeted to `orion-studio` / `orion-studio.png`.
- Download endpoints `cloud.zed.dev`→`cloud.orion.dev`, `asset=zed`→`asset=orion-studio`; temp artifact renamed.
- Env-var contract: `ORION_STUDIO_CHANNEL` / `ORION_STUDIO_VERSION` / `ORION_STUDIO_BUNDLE_PATH` are now the canonical inputs, with `ZED_CHANNEL` / `ZED_VERSION` / `ZED_BUNDLE_PATH` kept as **read-only fallbacks**.

### 5.3 `script/uninstall.sh`
- `appid` and macOS `.app` names (`Zed.app`→`Orion Studio.app`, etc.) → `dev.orion.OrionStudio*` / `Orion Studio*.app`.
- Install dir, `~/.local/bin` symlink, config dir (`~/.config/orion-studio`), data dir (`~/.local/share/orion-studio`), socket, and server data dir (`~/.orion_studio_server`) all updated. Legacy `zed` variants are removed alongside for clean transitional uninstalls.
- Channel env var accepts `ORION_STUDIO_CHANNEL` with `ZED_CHANNEL` fallback.

All three scripts pass `bash -n` syntax checks.

---

## 6. Cross-platform app identity correction (found during S11 verification)

`crates/release_channel/src/lib.rs` was missed by the original S10 pass (it lived in a different crate). Its doc comment explicitly states the app-id *"has to match the bundle identifier for Orion Studio on macOS"*, but it still returned `dev.zed.Zed*`. Corrected:

- `app_id()` (Wayland/X11 + must-match macOS bundle) → `dev.orion.OrionStudio*`.
- `app_identifier()` (Windows AppUserModelID) → `Orion-Studio-*`.
- App-referential doc comments → `Orion Studio`.

This restores consistency between the Cargo.toml bundle ids, flatpak/snap/metainfo ids, and the runtime app id.

---

## 7. KEEP-* taxonomy

| Category | Items | Rationale |
|---|---|---|
| **KEEP-ASSET** (follow-up design task) | `app-icon*.png` glyph/content; `zed.desktop.in` filename; `zed.metainfo.xml.in` filename | Binary/asset filename rename + new Orion icon artwork is a design deliverable, out of safe string-migration scope. Rendered output is correctly branded. |
| **KEEP-INTERNAL** (deployment/code-signing) | `crates/zed/contents/*/embedded.provisionprofile` (bundle id `dev.zed.Zed`); `bundle-linux` sentry `-p zed -o zed-dev`; cargo package name `zed` | Provisioning profiles require re-signing with new identities; Sentry project/org is deployment config; the Rust package name rename is a separate, larger refactor. |
| **KEEP-COMPAT** (intentional) | `bin/zed` & `libexec/zed-editor` symlinks; `osx_url_schemes` includes `zed`; `zed://` handler; `ZED_*` env fallbacks; cli.rs `dev.zed.Zed` detection | Preserves legacy behavior during the migration window per S02. |

---

## 8. Verification

- `bash -n script/{install,uninstall,bundle-linux}` → all OK.
- `cargo fmt --all -- --check` → 0 diffs (incl. the `release_channel` correction).
- `git diff --check` on `script/` + `release_channel` → clean.
- Full package build / `.app` / `.deb` / `.dmg` generation **not run** (no packaging toolchain + network in sandbox) — SKIP with rationale; the script logic was validated by inspection + syntax check.

---

## 9. Residuals & handoff

- **R-ICON-ASSET:** design a new Orion icon; rename `app-icon*.png` → `orion-studio*.png` and the `zed.desktop.in` / `zed.metainfo.xml.in` filenames (follow-up design + rename pass).
- **R-PROVISIONPROFILE:** regenerate macOS provisioning profiles under `dev.orion.OrionStudio*` and re-sign (deployment/code-signing).
- **R-SENTRY:** point `bundle-linux` Sentry upload at the Orion Sentry org/project (`-o orion -p orion-studio`) once that exists.
- **Handoff to S11:** the unified residual scan and release gate.
