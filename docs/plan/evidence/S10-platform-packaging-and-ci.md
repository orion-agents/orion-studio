# S10 — Platform Packaging, Installer & CI/CD Identity (Evidence)

## 2026-08-15 Finalization Update

> **当前生效状态：** 本节是截至 2026-08-15 的最终本地验收结论，覆盖下方与之冲突的旧结论。下方旧状态、旧命令结果和旧阻塞记录仅作为历史证据保留，不得再将其中的 `PARTIAL`、`PENDING` 或 Metal/WebRTC 阻塞描述当作当前状态。

**Init v1/v2 本地源码收口：`DONE / GO-WITH-CONDITIONS`；macOS Dev 产物：`VERIFIED`；Production release：`NO-GO`。**

- **仓库与交付边界：** 验收基线为 `init@99023dd28964dc1ef729eca2e606b6194d0ace06`；仓库是仅含 2 个 commit 的 shallow checkout，共享工作树仍有大量既有 WorkBuddy/用户改动。本轮没有 stage、commit、push、tag、PR 或 release。
- **品牌门禁：** `./script/check-orion-brand` 最终 **PASS**：扫描 4,217 个文件和 263 个符号链接目标，6,446 个命中全部被逐行 allowlist 解释，`unapproved=0`、`stale=0`、`ambiguous=0`、`errors=0`；allowlist SHA-256 为 `b67e5990a56bdf844acb8779df1ed46204943a55a7911225960d3f320c5dd538`。
- **外部身份契约：** 主 CLI 为 `orion-studio`，短别名为 `orion`；新生成的 deep link、schema URI、provider identity、mention URI、凭据与服务地址均使用 Orion 命名。旧名称只保留在明确的输入兼容、迁移、ABI、上游来源、许可证和测试夹具边界。
- **Workflow fail-closed：** 源文件与生成 YAML 已同步；`release`、`nightly`、`after_release`、`deploy_docs` 分别有 14、12、4、1 个生产副作用 job 绑定 `production` environment，并分别受稳定版、夜间版和 after-release 显式开关约束；相关 xtask 22 个测试 **PASS**。
- **编译与回归：** main `cargo +stable check -p orion-studio --bin orion-studio`、完整 `./script/clippy`、format、workflow generation/check、`git diff --check`、todo/keymap/license 检查、Docker/AppX/XML/entitlements 静态门禁均 **PASS**；`paths` 51 个、`cli` 7 个、`deploy_collab` 7 个测试及约定聚焦测试均 **PASS**。
- **macOS Dev 产物：** `target/aarch64-apple-darwin/release/Orion-Studio-aarch64.dmg` 已生成并通过 `hdiutil verify`；大小 150,297,095 bytes（143 MiB），SHA-256 为 `3fa0c88c88fcc1d1c841adc6a6b338b7aed3815df229164616f33b2a00981f2d`。
- **App 验收：** DMG 内为 `Orion Studio Dev.app`，`CFBundleIdentifier=dev.orion.OrionStudio-Dev`、`CFBundleExecutable=orion-studio`、版本 `1.16.0`，主程序为 arm64；Info.plist lint 与扩展键去重、`codesign --verify --deep --strict`、entitlements 检查均 **PASS**。
- **开发包限制：** 当前仅有 ad-hoc 签名，`TeamIdentifier` 为空，不含 associated-domain entitlement，也未嵌入服务条款或执行 notarization；`spctl` 以状态 3 拒绝该包是预期结果，因此它不是可公开分发的 macOS release。
- **打包稳定性：** `TERM=dumb` 下旧打包工具的彩色输出 panic 已在项目脚本中 fail-safe 规避；重复 plist 扩展会在签名前规范化；许可证内容未变化时保留文件 mtime，最终缓存复跑的 Rust 构建均在约 2 秒内完成。
- **跨平台边界：** Windows canonical CLI、安装器、更新器、scheme 所有权与 workflow 已做源码/交叉检查，但本机不能执行真实 Windows/ISCC/Wine 安装升级卸载 E2E；Windows 更新在 `install/old` 非空的中断恢复仍建议后续增加 transaction journal（P2）。

Production release 必须保持 **NO-GO**：Orion DNS、cloud/collab/OAuth、Cloudflare、Sentry、Apple/Windows 签名与公证凭据、publisher/store/Winget、真实 Windows/Linux packaging E2E、GitHub protected Environments/rulesets/reviewers，以及 trademark/privacy/terms 等法务审批均未完成。任何本地 PASS 或 Dev DMG 都不能替代这些生产、平台、治理与法务门禁。

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

| Channel | `identifier` (before) | `identifier` (after)            | `name`                 |
| ------- | --------------------- | ------------------------------- | ---------------------- |
| Dev     | `dev.zed.Zed-Dev`     | `dev.orion.OrionStudio-Dev`     | `Orion Studio Dev`     |
| Nightly | `dev.zed.Zed-Nightly` | `dev.orion.OrionStudio-Nightly` | `Orion Studio Nightly` |
| Preview | `dev.zed.Zed-Preview` | `dev.orion.OrionStudio-Preview` | `Orion Studio Preview` |
| Stable  | `dev.zed.Zed`         | `dev.orion.OrionStudio`         | `Orion Studio`         |

`osx_url_schemes` → `["orion", "zed"]` (legacy `zed` handler retained as a compat registration).

---

## 3. Platform resources

| File                                               | Change                                                                                                                                                                                                      |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/zed/resources/flatpak/zed.metainfo.xml.in` | `zed.dev`→`orion.dev`; `github.com/zed-industries/zed`→`orion-agents/orion-studio`; `<developer id="dev.zed">`→`dev.orion`; `caption>Zed`→`Orion Studio`                                                    |
| `crates/zed/resources/snap/snapcraft.yaml.in`      | `name: zed`→`orion-studio`; `title: Zed`→`Orion Studio`; `apps: zed`→`orion-studio`; `common-id: dev.zed.Zed`→`dev.orion.OrionStudio`; `command: usr/bin/zed`→`usr/bin/orion-studio`; `zed.dev`→`orion.dev` |
| `crates/zed/resources/windows/zed.iss`             | `Zed.exe`→`orion-studio.exe`; `Software\Classes\zed`→`Software\Classes\orion`; `www.zed.dev`→`www.orion.dev`                                                                                                |
| `crates/zed/resources/windows/zed.sh`              | `zed.exe`→`orion-studio.exe` (both branches)                                                                                                                                                                |
| `crates/zed/resources/zed.desktop.in`              | `Keywords=zed;`→`orion-studio;`; `x-scheme-handler/zed`→`x-scheme-handler/orion;x-scheme-handler/zed`                                                                                                       |

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

`crates/release_channel/src/lib.rs` was missed by the original S10 pass (it lived in a different crate). Its doc comment explicitly states the app-id _"has to match the bundle identifier for Orion Studio on macOS"_, but it still returned `dev.zed.Zed*`. Corrected:

- `app_id()` (Wayland/X11 + must-match macOS bundle) → `dev.orion.OrionStudio*`.
- `app_identifier()` (Windows AppUserModelID) → `Orion-Studio-*`.
- App-referential doc comments → `Orion Studio`.

This restores consistency between the Cargo.toml bundle ids, flatpak/snap/metainfo ids, and the runtime app id.

---

## 7. KEEP-\* taxonomy

| Category                                    | Items                                                                                                                                                | Rationale                                                                                                                                                           |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **KEEP-ASSET** (follow-up design task)      | `app-icon*.png` glyph/content; `zed.desktop.in` filename; `zed.metainfo.xml.in` filename                                                             | Binary/asset filename rename + new Orion icon artwork is a design deliverable, out of safe string-migration scope. Rendered output is correctly branded.            |
| **KEEP-INTERNAL** (deployment/code-signing) | `crates/zed/contents/*/embedded.provisionprofile` (bundle id `dev.zed.Zed`); `bundle-linux` sentry `-p zed -o zed-dev`; cargo package name `zed`     | Provisioning profiles require re-signing with new identities; Sentry project/org is deployment config; the Rust package name rename is a separate, larger refactor. |
| **KEEP-COMPAT** (intentional)               | `bin/zed` & `libexec/zed-editor` symlinks; `osx_url_schemes` includes `zed`; `zed://` handler; `ZED_*` env fallbacks; cli.rs `dev.zed.Zed` detection | Preserves legacy behavior during the migration window per S02.                                                                                                      |

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
