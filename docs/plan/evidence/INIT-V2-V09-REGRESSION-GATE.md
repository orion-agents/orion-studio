# INIT-V2-V09 — 全量回归与发布门禁

## 2026-08-15 macOS Preview Release Preparation Update

> **当前生效状态：** 本节覆盖下方与之冲突的历史构建和发布结论。Init v1/v2 源码改造已收口；首个公开渠道限定为 macOS Apple Silicon Preview。源码可进入 PR，公开二进制仍必须通过 Apple Developer ID 签名、公证和干净机器安装启动验证。

**Init v1/v2：`DONE`；Preview 源码：`GO`；未签名本地产物：`LOCAL-ONLY`；公开 Preview 二进制：`NO-GO`（等待 Apple 凭据）。**

- **Git 基线：** `init@99023dd28964dc1ef729eca2e606b6194d0ace06`，刷新后的 `origin/main@08827f9208b4848d62f3faf86ffa15155966d63c`；共同基线为 `d2779c344350bcac5efabf840bbd31ba5b866ab4`，提交前状态为本地 1 ahead / 4 behind。完整 staged 边界为 850 个文件、+35,939 / -11,338；`.workbuddy/` 与 `docs/research/` 不进入发布范围。
- **Post-rebase 验收：** `init` 已无冲突 rebase 到 `origin/main@08827f9208b4848d62f3faf86ffa15155966d63c`，分支关系为 2 ahead / 0 behind。rebase 后重新执行 Preview 15/15、品牌扫描、format、diff、todo/keymap 和 workflow YAML 门禁均 **PASS**；`LK_CUSTOM_WEBRTC=<validated-cache> CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo check --release --target aarch64-apple-darwin -p orion-studio --bin orion-studio` **PASS**，续跑用时 1m01s。
- **Release 编译：** `CARGO_INCREMENTAL=0 cargo build --release -p orion-studio --bin orion-studio --target aarch64-apple-darwin -j 2` **PASS**，用时 59m22s。产物时间戳为 2026-08-15 13:08:36 +0800，大小 410,674,656 bytes，Mach-O 为单一 `arm64`，`LC_BUILD_VERSION minos=11.0`；`script/validate-preview-release mach-o-minimum` **PASS**。
- **本地 App/DMG：** `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 ./script/bundle-mac aarch64-apple-darwin` **PASS**。首次 WebRTC 下载遇到对端 TLS `unexpected-eof`；重试复用两份 SHA-256 一致的本地 WebRTC 缓存后成功。`Orion Studio Dev.app` 约 400 MiB，`CFBundleIdentifier=dev.orion.OrionStudio-Dev`、版本 `1.16.0`、最低 macOS `11.0`、单一 `arm64`，`codesign --verify --deep --strict` **PASS**。
- **本地启动：** 新 App 以当前仓库为启动参数运行，CoreGraphics 确认 PID 39451 存在一个 layer 0、alpha 1、`onscreen=1` 的 1362x809 窗口。迁移只在首个 `MultiWorkspace` 下一帧后启动，因此该观测也证明 UI 已先渲染；AppleScript 窗口计数和 `screencapture` 受当前 macOS 辅助功能/录屏权限影响，不作为失败证据。
- **迁移运行证据：** `~/.config/orion-studio/.orion_migration_marker` 已写入；旧 `~/Library/Application Support/Zed` 保留。数据目录迁移在 GPUI 首帧之后通过后台 executor 扫描并复制，App 窗口在迁移期间保持 onscreen；最终 data marker 在交付前继续观察。
- **Preview workflow：** `.github/workflows/release_preview_macos.yml` 仅接受 `vMAJOR.MINOR.PATCH-pre`，校验 tag/version/channel/main ancestry 和 immutable checkout，固定 Apple Silicon runner，执行空间预检、正式签名、公证、Gatekeeper/stapler/架构/版本/minOS/DMG 校验，只创建或复用 Draft Pre-release，并要求人工 `preview` environment。`./script/test-preview-release` **15/15 PASS**。
- **静态门禁：** `cargo fmt --all -- --check`、`git diff --check`、todo/keymap 检查、workflow 与 issue-template YAML 解析均 **PASS**。`./script/check-orion-brand` **PASS**：扫描 4,225 个文本文件和 263 个符号链接目标，6,157 个品牌命中全部由 6,157 条精确 allowlist 规则解释，`unapproved=0`、`stale=0`、`ambiguous=0`、`errors=0`；allowlist SHA-256 为 `4e46c8050a332893469670a386a8a57aecb0355e3406ff142aca76496983fef2`。
- **磁盘与内存：** 停止高并发链接后以 `cargo clean --profile dev` 和 `cargo clean --profile test` 删除 211,662 个纯生成文件，共 118.6 GiB；源码、Release 产物和用户数据未删除。后续编译统一限制为 2 jobs，编译结束后系统可用内存曾回升到 59% 以上，磁盘保持约 130 GiB 以上可用。
- **本地 DMG 限制：** `Orion-Studio-aarch64.dmg` 大小约 145 MiB，SHA-256 为 `e4d80b7a2ffff643823c3360891afebff6aa6ef3d703a44ee73b524f215e3bce`。它仅有 ad-hoc 签名、无 TeamIdentifier、无 notarization，不得上传到 GitHub Release。
- **公开发布硬门禁：** 当前机器没有有效 codesigning identity，仓库/`preview` environment 也没有 Apple 发布 secrets/variables。必须配置 `MACOS_CERTIFICATE`、`MACOS_CERTIFICATE_PASSWORD`、`APPLE_NOTARIZATION_KEY`、`APPLE_NOTARIZATION_KEY_ID`、`APPLE_NOTARIZATION_ISSUER_ID`、`ORION_STUDIO_MACOS_PROVISIONING_PROFILE_BASE64`，以及 `ORION_STUDIO_MACOS_SIGNING_IDENTITY`、`ORION_STUDIO_MACOS_TEAM_ID`，再由 immutable preview tag 触发 workflow；下载 Draft 产物在干净 Mac 上通过安装、Gatekeeper 与启动验收后，才能人工公开为 Pre-release。

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

> 阶段 ID：V2-09（init-plan-v2.md §6 最后一阶段）
> 执行日期：2026-08-14（命令实测时间戳）
> 执行分支：`init`
> 当前 HEAD：`99023dd28964dc1ef729eca2e606b6194d0ace06`（init 提交，本地未 push）
> 约束：本阶段**禁止**自动发布、推送或打 tag。发布是独立授权动作。

---

## 0. 交付格式（init-plan-v2.md §7）

| 字段                     | 内容                                                                                                                                                                                                                                     |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 阶段 ID                  | V2-09 全量回归与发布门禁                                                                                                                                                                                                                 |
| 状态                     | **PARTIAL**（可验证门禁通过；全量构建/发布门禁被环境阻塞；残留品牌面未全量迁移）                                                                                                                                                         |
| 当前分支和 HEAD          | `init` @ `99023dd`（本地领先 origin/main 1；未 push）                                                                                                                                                                                    |
| 开始前 dirty 边界        | 50 文件 changed，+1513 / −597（V2-00~V2-08 累积）                                                                                                                                                                                        |
| 本阶段修改文件           | `tooling/xtask/src/tasks/workflows/vars.rs`（仅 rustfmt 行换行修正，无逻辑改动）                                                                                                                                                         |
| 未修改但检查过的关键文件 | `Cargo.toml`、`README.md`、`LICENSE-APACHE`、`LICENSE-GPL`、`crates/paths/src/{paths,migration}.rs`、`crates/cli/src/main.rs`、`crates/zed_env_vars/src/zed_env_vars.rs`、`crates/extension_api/src/extension_api.rs`                    |
| 实现/审计摘要            | 见 §1–§8                                                                                                                                                                                                                                 |
| 测试和命令结果           | 见 §3 门禁表（可验证项全 PASS；环境阻塞项见 §4/§5）                                                                                                                                                                                      |
| 未运行/环境阻塞          | 全量 `./script/clippy --workspace`、主二进制 `cargo check -p zed --bin orion-studio`、全量 `cargo build`、clean worktree 复现、平台签名/notarization、真实服务端点 E2E —— 均 BLOCKED（Metal Toolchain 缺失 + webrtc-sys 预编译下载断网） |
| 遗留风险                 | 残留面向用户 `Zed` 字符串（UI/遥测/设置名/doc 链接）、`cloud.zed.dev` 服务端点、orion.dev 站点未上线、Windows 资源归因、迁移启动接入仅靠单元测试未全量构建验证                                                                           |
| 下一阶段建议             | 在具备 Metal Toolchain + 联网的 CI 复跑全量 clippy/构建；逐项处理 R-V2-09-\* 延迟项（见 §8）；人工复核后单独授权发布                                                                                                                     |
| 回滚方式                 | 仅用可恢复方式：`git stash` / 反向 `git apply`；**禁止** `git reset --hard`、`git checkout --`、`git clean`、删除用户目录                                                                                                                |

---

## 1. 全仓库品牌扫描与分类（V2-09 要求 1）

扫描方法：ripgrep 全仓库枚举 `zed\.dev`、`\bZed\b`、`<zed:/zed::` 动作命名空间、`ZED_*` 功能环境变量，逐项判定类别。扫描为**只读枚举**，未做机械替换。

### 1.1 命中规模

| 模式                          | 命中规模         | 分布                                                                                                                                                                              |
| ----------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `zed.dev` / `cloud.zed.dev`   | ~217 处          | `docs/src/*`、`crates/{client,workspace,debugger_ui,remote,docs_preprocessor,settings_content,git,migrator}`、`assets/themes/*/$schema`、`docs/theme/*`、`script/*`、`legal/*.md` |
| `\bZed\b`（crates `*.rs`）    | 数百个文件       | 遥测/UI 字符串、设置/键映射主题名、动作命名空间、doc comments、测试 fixture、Windows 资源归因                                                                                     |
| `\bZed\b`（`docs/src`）       | 数十个文件       | 帮助文档 prose、交叉引用文件名（`configuring-zed.md`）、计费等                                                                                                                    |
| `zed::` / `zed:` 动作命名空间 | 多处（ABI 保留） | `crates/zed_actions`、`crates/extension_api/wit/*`、`crates/settings/src/keymap_file.rs` 等                                                                                       |
| `ZED_*` 功能环境变量          | 4 文件           | `crates/gpui_wgpu/{wgpu_context,wgpu_renderer}.rs`、`crates/zlog/src/zlog.rs`、`crates/zlog/README.md`                                                                            |

### 1.2 分类结论

| 类别                                      | 处置                    | 代表命中                                                                                                                                                                                                                                                           | 依据                                                                                         |
| ----------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| **ABI / 协议兼容（KEEP）**                | 保留                    | `zed::`/`zed:` 动作命名空间、`WIT package zed:extension`、`crates/zed` crate 名、`--zed <PATH>` CLI flag、`zed-cli://`、`zed-*.sock`、`ZED_ASKPASS_SOCKET` 发送端                                                                                                  | S02 兼容契约；V2-04 协议矩阵定为 BLOCKED-keep / KEEP-legacy；改名会破坏升级链路与扩展 ABI    |
| **功能环境变量（KEEP，非品牌）**          | 保留                    | `ZED_DEVICE_ID`、`ZED_LOG`、`ZED_FONTS_GAMMA`、`ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST`                                                                                                                                                                             | GPU/字体渲染功能开关，非用户可见品牌；文档须字面保留这些 token                               |
| **上游归属 / 法务（KEEP）**               | 保留                    | `legal/*.md`、`README.md` 的 "fork of Zed … Zed Industries, Inc."、`assets/themes/*/$schema=zed.dev/schema/themes`（稳定公开 schema）                                                                                                                              | 上游 LICENSE/版权/归因；不得删除                                                             |
| **第三方发行包名（KEEP）**                | 保留                    | `zed-editor`/`zed-git`/`zed-preview`/`zedit`/`zeditor`/`zed-community`、Discord `zed-community`、`ZedIndustries.*` WinGet id、`zedindustries` GitHub org                                                                                                           | 第三方生态命名，非本仓库可控                                                                 |
| **历史迁移数据（KEEP）**                  | 保留                    | `crates/migrator/src/migrator.rs` `"provider":"zed.dev"`、`crates/git/src/repository.rs` `hi@zed.dev`（测试 fixture）                                                                                                                                              | 历史数据/测试样本，改写会破坏兼容                                                            |
| **外部基建引用（DEFERRED，治理延迟）**    | 不臆改，登记 R-\*       | `.github` 中 Sentry `zed-dev`、DO registry `zed`、R2 `zed-open-source-website-assets`、`zed-extensions` org、`zed-extension-cli`、`danger-proxy.zed.dev`、`ZED_*` secret、`zed-zippy` bot、fork-sync `repositories: zed`、Cachix `name: zed`                       | V2-08 已登记为 R-V2-08-\*（15 项）；需治理/基础设施授权                                      |
| **服务端点（DEFERRED，待注册）**          | 不臆改                  | `crates/client/src/zed_urls.rs`（6×）、`cloud.zed.dev`、OAuth/WS 端点                                                                                                                                                                                              | V2-06 定为 BLOCKED：生产域名/OAuth 回调未由人工确认注册                                      |
| **面向用户文档/帮助链接（DEFERRED）**     | 待 orion.dev 上线后扫荡 | `docs/src/*` 中 `zed.dev` 帮助/计费/认证/git/vim 链接、`docs/theme/*` Zed logo/CDN                                                                                                                                                                                 | R-BRAND-URL-SWEEP；orion.dev 站点未上线，链接改了即 404                                      |
| **面向用户 `Zed` 产品字符串（DEFERRED）** | 待产品/法务决策         | `crates/language_models/src/provider/cloud.rs` "Zed account"/"ZedPro"、`crates/ui` "Zed Agent"/"Zed X Copilot"、`crates/agent_ui` `AgentId::from("Zed")`、`crates/settings` `"Zed (Default)"` 键映射/图标主题名、`crates/windows_resources` "Zed Industries, Inc." | 大规模 UI/遥测/设置名改写超出机械替换 prohibition；需产品+法务决策，单列 R-V2-09-USERSTRINGS |
| **文档文件名交叉引用（DEFERRED）**        | 待决策                  | `docs/src/configuring-zed.md` 标题与交叉链接                                                                                                                                                                                                                       | R-V2-09-DOCSFILES；改名会破坏 `git.md` 等交叉引用                                            |

**结论**：canonical 身份链（二进制名、应用名、数据/配置目录、env 前缀、安装/CLI 文档、打包制品名、URL scheme `orion://`、legacy `zed://` 回退、桌面 id `dev.orion.OrionStudio*`）已闭合；但**面向用户 `Zed` 字符串与 `zed.dev` 服务端点/文档链接仍有大面积残留**，属于待治理/待注册延迟项，非本阶段可臆改。

---

## 2. Clean worktree 复现核心构建（V2-09 要求 2）

**状态：BLOCKED（环境）**

- 当前 dirty tree 的 focused pass（见 §3）不能代替 clean proof —— 已如实记录。
- 在隔离 worktree 复现全量核心构建（`cargo build` / `cargo clippy --workspace`）需要：
  1. **Metal Toolchain**（macOS，`gpui_macos` 的 metal shader 编译）—— 沙箱缺失；
  2. **联网下载 webrtc-sys 预编译产物** —— 沙箱断网，构建卡死于预编译拉取。
- 因此 clean-worktree 全量构建与全量 clippy 均无法在本环境取得可接受的成功证据。按 init-plan-v2.md 规则，记录为 **BLOCKED**，不得改写成代码错误，也不得跳过该门禁。
- 复跑建议：在具备 Metal Toolchain + 联网的 CI runner 上执行 `./script/clippy`（即 `cargo clippy --workspace --release --all-targets --all-features -- --deny warnings`）与 `cargo build --release`。

---

## 3. 仓库规定检查（V2-09 要求 3）

> 命令均在仓库根目录、分支 `init`、HEAD `99023dd` 实测。
> 时间基准：2026-08-14。cargo 工具链：+stable（1.95.0）。

| #   | 检查          | 命令                                                                                                       | 退出码 | 结果                                                                                                                                                                                                                                                               |
| --- | ------------- | ---------------------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | 格式          | `cargo +stable fmt --all -- --check`                                                                       | 0      | **PASS**（本阶段修正 `vars.rs` 一行换行后通过；此前因 `orion-studio-` 前缀使 `REMOTE_SERVER_WINDOWS_AARCH64` 超宽而失败，exit 1 → 已修）                                                                                                                           |
| 2   | 迁移单测      | `cargo +stable test -p paths`                                                                              | 0      | **PASS — 33 passed**（含 fail-closed/marker/幂等/secret/符号链接等）                                                                                                                                                                                               |
| 3   | CLI 单测      | `cargo +stable test -p cli`                                                                                | 0      | **PASS — 6 passed**（`cli_command_name_is_orion_studio` 等）                                                                                                                                                                                                       |
| 4   | CLI 检查      | `cargo +stable check -p cli`                                                                               | 0      | **PASS**                                                                                                                                                                                                                                                           |
| 5   | 安装 CLI 检查 | `cargo +stable check -p install_cli`                                                                       | 0      | **PASS**                                                                                                                                                                                                                                                           |
| 6   | 扩展 API 检查 | `cargo +stable check -p zed_extension_api`                                                                 | 0      | **PASS**（前序已验证，本次复核目录/包名）                                                                                                                                                                                                                          |
| 7   | 作用域 clippy | `GITHUB_ACTIONS=1 ./script/clippy -p paths -p zed_env_vars -p zed_extension_api -p cli`                    | 0      | **PASS**（`--deny warnings`，4 个 Metal-free crate 全过）                                                                                                                                                                                                          |
| 8   | todo 检查     | `./script/check-todos`                                                                                     | 0      | **PASS**                                                                                                                                                                                                                                                           |
| 9   | keymap 检查   | `./script/check-keymaps`                                                                                   | 0      | **PASS**                                                                                                                                                                                                                                                           |
| 10  | diff 检查     | `git diff --check`                                                                                         | 0      | **PASS**（0 警告行）                                                                                                                                                                                                                                               |
| 11  | 全量 clippy   | `./script/clippy`（=`cargo clippy --workspace --release --all-targets --all-features -- --deny warnings`） | —      | **BLOCKED**（Metal + webrtc 环境；见 §2）                                                                                                                                                                                                                          |
| 12  | 主二进制检查  | `cargo +stable check -p zed --bin orion-studio`                                                            | —      | **BLOCKED**（Metal Toolchain 缺失）                                                                                                                                                                                                                                |
| 13  | 全量构建      | `cargo +stable build --release`                                                                            | —      | **BLOCKED**（webrtc-sys 预编译断网）                                                                                                                                                                                                                               |
| 14  | 密钥扫描      | 仓库 CI 用 gitleaks（若有）；本环境无专属 `check-secrets` 脚本                                             | —      | **PARTIAL**：`git diff --check` 通过 + 人工复核 50 文件 diff 无硬编码密钥；CI 侧 gitleaks 为延迟 CI 门禁（BLOCKED 环境）                                                                                                                                           |
| 15  | 打包检查      | `script/bundle-*`、`script/install.sh`、`script/install-linux` 制品名/路径一致性                           | —      | **PASS（静态复核）**：tarball 内部 `orion-studio$suffix.app`、binary `orion-studio`+legacy 软链 `zed`、`cloud.orion.dev`/`asset=orion-studio`/`ORION_STUDIO_CHANNEL` 与 `crates/remote/src/transport/*` 期望、`tooling/xtask/.../vars.rs` 生成源一致（V2-08 证据） |

**本阶段真实捕获**：`cargo fmt --check` 在 `tooling/xtask/src/tasks/workflows/vars.rs:386` 因 `orion-studio-` 前缀使 `REMOTE_SERVER_WINDOWS_AARCH64` 常量超宽而失败（exit 1）。已就地换行修正（对齐 rustfmt），复测 exit 0。这是 V2-08 引入、V2-09 门禁兜住的格式违规。

---

## 4. 首装/升级/迁移/回滚验证（V2-09 要求 4）

| 场景                                        | 证据                                                                                                                                                                                                                                                       | 状态                                                                      |
| ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| 首装（无旧目录）                            | `migration::tests::fresh_install_without_legacy`：无旧目录时启动行为不变                                                                                                                                                                                   | VERIFIED（单元）                                                          |
| 旧版本升级（有旧 `~/.config/zed` 等）       | `migrate_legacy_dirs_*` 系列：仅迁移存在的 root、旧目录保留、幂等                                                                                                                                                                                          | VERIFIED（单元）                                                          |
| 重复启动 / 幂等                             | `migrate_legacy_dirs_is_idempotent_across_runs`、`repeated_migration_is_idempotent`、`no_temp_directory_left_after_repeated_runs`                                                                                                                          | VERIFIED（单元）                                                          |
| 迁移失败 / fail-closed                      | `empty_marker_fails_closed`、`duplicate_marker_field_fails_closed`、`marker_source_mismatch_fails_closed`、`interrupted_marker_reattempts_migration`、`migrate_legacy_dirs_propagates_failure_and_preserves_legacy`、`write_failure_preserves_legacy_data` | VERIFIED（单元）                                                          |
| 回滚到旧目录                                | 旧目录在迁移前后均保留（`*_old_retained` 测试、`upgrade_only_legacy_is_migrated_and_old_retained`）                                                                                                                                                        | VERIFIED（单元）                                                          |
| 旧入口兼容（`zed://`、`zed` 软链、`--zed`） | `register_zed_scheme` 双注册 `orion://`+`zed://`（V2-04）；`install.sh` 建 `zed` 软链；`--zed <PATH>` flag 保留                                                                                                                                            | VERIFIED（代码静态） / 安装产物实测为 PARTIAL（需真实安装环境，环境阻塞） |
| 迁移接入启动流程（运行时）                  | `crates/zed/src/main.rs` 接入 `migrate_legacy_user_data` + 失败硬退出（V2-02 证据 `INIT-V2-V02-STARTUP-WIRING.md`）                                                                                                                                        | VERIFIED（单元/集成）÷ 全量构建验证 BLOCKED（Metal）                      |

> 说明：迁移逻辑有 33 个单元/集成测试覆盖上述场景，但未在**全量构建产物**上跑过真实首装/升级（受 Metal 环境阻塞）。结论为「逻辑 VERIFIED，端到端 PARTIAL」。

---

## 5. 平台目标验证（V2-09 要求 5）

| 平台                                   | 声明支持 | 本环境状态                                                                                                                      |
| -------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------- |
| macOS（bundle / Metal / notarization） | 是       | **BLOCKED**：Metal Toolchain 缺失，无法编译 `gpui_macos` / 打包 `.app` / notarization 需账号授权                                |
| Windows（app id / 单实例 / 签名）      | 是       | **BLOCKED**：无 Windows 构建环境；`crates/windows_resources` "Zed Industries, Inc." 归因待法务/签名决策（R-V2-09-WINATTR）      |
| Linux（deb/rpm/AppImage/Flatpak/wasm） | 是       | **PARTIAL**：`script/bundle-linux`/`install-linux` 静态复核通过；wasm32-wasip2 扩展编译检查需工具链（V2-05 要求），本环境未验证 |
| wasm（扩展 ABI `zed:extension`）       | 是       | **BLOCKED/PARTIAL**：`zed_extension_api` check 通过；`test-extension` wasm32-wasip2 真实编译需工具链，未实测                    |

按 init-plan-v2.md："缺少 Metal Toolchain、wasm 工具链或签名环境时清楚列为 BLOCKED" —— 已如上标注。

---

## 6. LICENSE / NOTICE / 版权 / 版本核对（V2-09 要求 6）

| 项            | 状态    | 说明                                                                                                                           |
| ------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------ |
| 主许可证      | PASS    | GPL-3.0-or-later（`LICENSE-GPL` 存在）；`LICENSE-APACHE` 一并保留（上游双许可）                                                |
| 上游归属      | PASS    | `README.md` 顶部："Orion Studio is a fork of [Zed](https://zed.dev), built on … the work of Zed Industries, Inc." 归因完整保留 |
| NOTICE 文件   | N/A     | 上游 Zed 无 NOTICE；本 fork 不新增 NOTICE（无额外第三方归属需单列）                                                            |
| 版权头        | PASS    | 源码文件未批量改写版权（保持上游 + 新增贡献）；未删除上游 LICENSE/版权                                                         |
| 版本号        | PASS    | `Cargo.toml` `version = "0.61"`（沿用上游 fork 基线，未擅自 bump）                                                             |
| release notes | PARTIAL | `docs/src/development/release-notes.md` 仍含 `zed.dev` 历史链接（R-BRAND-URL-SWEEP 范畴），非 blocker                          |

---

## 7. Go / No-Go 表（V2-09 要求 7）

判定规则（init-plan-v2.md §6）：只要主二进制、迁移接入、安装产物、关键服务端点或协议兼容**没有证据**，结论即 NO-GO。

| 维度                                        | 证据强度                                     | Go/No-Go                             |
| ------------------------------------------- | -------------------------------------------- | ------------------------------------ |
| 二进制/应用名/路径/环境变量 canonical 闭环  | VERIFIED（代码+单测）                        | ✅ Go（核心身份）                    |
| 安装产物 / CLI / 打包制品名一致性           | VERIFIED（静态复核） ÷ 真实安装 PARTIAL      | ⚠️ 条件 Go                           |
| 迁移逻辑（首装/升级/幂等/失败/回滚）        | VERIFIED（33 单测）                          | ✅ Go（逻辑）                        |
| 迁移接入启动流程（运行时）                  | VERIFIED（单元/集成） ÷ 全量构建验证 BLOCKED | ⚠️ 条件 Go                           |
| 旧入口/协议兼容（`zed://`/`zed`/ABI）       | VERIFIED（代码静态）                         | ✅ Go（兼容窗口）                    |
| 主二进制全量构建                            | **BLOCKED（Metal）**                         | ❌ No-Go                             |
| 全量 clippy / lint                          | **BLOCKED（Metal+webrtc）**                  | ❌ No-Go                             |
| 关键服务端点（`cloud.zed.dev`/OAuth/WS）    | **DEFERRED（域名未注册）**                   | ❌ No-Go                             |
| 面向用户 `Zed` 字符串残留（UI/遥测/设置名） | **DEFERRED（待产品/法务）**                  | ❌ No-Go（发布前必须处理或显式豁免） |
| 平台签名/notarization                       | **BLOCKED/PARTIAL**                          | ❌ No-Go                             |
| 密钥扫描（CI gitleaks）                     | PARTIAL（本地静态复核）                      | ⚠️ 条件 Go                           |

### 结论：**NO-GO（发布）**

- **核心 rebrand 身份链已闭合且单测/静态证据充分**（Go 条件满足）。
- 但存在硬性 No-Go 项：主二进制全量构建被 Metal 阻塞、全量 clippy 被 Metal+webrtc 阻塞、关键服务端点 `cloud.zed.dev` 未迁移（域名未注册）、面向用户 `Zed` 字符串大面积残留（待决策）。
- 因此**当前不可发布、不可打 tag、不可推送生产**。发布需先在合规 CI 复跑全量构建/clippy，并完成 §8 延迟项。

---

## 8. 延迟项登记表（R-V2-09-\*）

| ID                  | 项                                                                                                                 | 类别          | 阻塞方                                        | 建议                                     |
| ------------------- | ------------------------------------------------------------------------------------------------------------------ | ------------- | --------------------------------------------- | ---------------------------------------- |
| R-V2-09-USERSTRINGS | 面向用户 `Zed` 产品字符串（UI/遥测/设置名/`AgentId::from("Zed")`/`"Zed (Default)"`/`"Zed Industries, Inc."`）      | 产品+法务     | 需决策是否全量改 Orion + 遥测 schema 向后兼容 | 单列迁移 PR，逐 crate 审查，禁止机械替换 |
| R-V2-09-ENDPOINTS   | `cloud.zed.dev`/OAuth/WS 服务端点                                                                                  | 基础设施      | 需注册 `orion.dev` 域名、OAuth 回调、云端 API | V2-06 已定 BLOCKED，待人工确认契约       |
| R-V2-09-URLSWEEP    | `docs/src`/`docs/theme` 中 `zed.dev` 帮助/计费/认证/logo/CDN 链接 ~60+                                             | 站点          | orion.dev 站点上线                            | 站点上线后批量扫荡替换                   |
| R-V2-09-DOCSFILES   | `configuring-zed.md` 文件名与交叉引用                                                                              | 文档          | 改名会破坏 `git.md` 等引用                    | 随 URL 扫荡一并处理                      |
| R-V2-09-WINATTR     | Windows 资源 `Zed Industries, Inc.` 归因                                                                           | 法务/签名     | 公司实体决策 + 签名环境                       | 与签名/notarization 一并决策             |
| R-V2-08-\*（15 项） | `.github` 外部基建（Sentry/DO/R2/EXTORG/EXTCLI/DANGER/SECRETS/BOT/FORKSYNC/WINGET/DISCORD/TEAM/PKG/ZEDPRO/CACHIX） | 治理/基础设施 | 外部账号授权                                  | 见 `INIT-V2-V08-PACKAGING-CI.md`         |
| R-V2-09-CI-FULL     | 全量 `./script/clippy` + `cargo build --release` + gitleaks + check-licenses                                       | CI 环境       | Metal Toolchain + 联网 + 签名                 | 在合规 CI runner 复跑                    |

> 延续 V2-00~V2-08 既有 R-\* 登记（R-V2-05-1 邮箱基建、R-V2-05-2 twitter、R-V2-06-AI provider、R-V2-06-LEGAL、R-V2-06-DOCS/PROVISION、R-V2-07-E2E 等），不在本表重复。

---

## 9. 禁止自动发布/push/tag（V2-09 强制）

- 本阶段**未执行**任何 `git commit` / `git push` / `git tag` / 发布包 / 修改生产数据。
- V2-00~V2-09 全部改动仍停留在工作树（50 文件，+1513/−597，含本阶段 `vars.rs` 格式修正），**未提交、未推送**（本地 `init` 分支领先 origin/main 1，待用户授权）。
- 发布是独立授权动作：须在合规 CI 复跑全量构建/clippy 并关闭 §8 延迟项后，由用户另行授权执行。

---

## 10. 下一步建议

1. **（用户授权后）提交当前工作树**到 `init` 分支（不 push），或先 push 到 fork 远端供 CI 使用。
2. **在具备 Metal Toolchain + 联网的 CI** 复跑：`./script/clippy`、`cargo build --release`、gitleaks、`check-licenses`，关闭 R-V2-09-CI-FULL。
3. **处理 R-V2-09-USERSTRINGS**：产品/法务确认后，逐 crate 改面向用户 `Zed` 字符串（遥测 schema 需向后兼容策略）。
4. **注册并迁移服务端点**（R-V2-09-ENDPOINTS）与 **orion.dev 站点上线 + URL 扫荡**（R-V2-09-URLSWEEP / DOCSFILES）。
5. 上述关闭后，由用户单独授权发布/tag。
