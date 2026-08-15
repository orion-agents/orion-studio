# V2-08 证据：平台打包 / 安装 / 升级 / CI 品牌收敛

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

> 子计划：`docs/plan/subplans/START-HY3-WORKBUDDY-V2.md` → V2-08
> 前置：V2-00 ~ V2-07 已落地（见各自证据文档）
> 约束：只改允许文件、不破坏 ABI 兼容层、保留 legacy 回退、不猜测未确认协议/外部依赖、环境阻塞如实标记。

## 1. 本次 V2-08 改动清单

### 1.1 打包脚本（制品名生产侧）

| 文件                                | 改动                                                                                                                                                                                              |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `script/bundle-mac`                 | **关键修复**：`:335` 原 `cp .../zed "${app_path}/Contents/MacOS/zed"` → `orion-studio`（主二进制已改名，否则 macOS 打包静默失败）；`:340` gzip `orion-studio-remote-server-macos-$arch_suffix.gz` |
| `script/bundle-freebsd`             | `:161` `orion-studio-remote-server-freebsd-x86_64.gz`                                                                                                                                             |
| `script/bundle-windows.ps1`         | `:150` `orion-studio-remote-server-windows-$Architecture.zip`                                                                                                                                     |
| `script/install-linux`              | `:20-21` `archive="orion-studio-linux-${arch}.tar.gz"`；`ZED_BUNDLE_PATH` → `ORION_STUDIO_BUNDLE_PATH`；`:15` 文案 → Orion Studio                                                                 |
| `script/upload-nightly.ps1`         | `:18` filter `orion-studio-remote-server-windows-*.zip`                                                                                                                                           |
| `script/zed-local`                  | `:197,199` `binaryPath` → `/Applications/Orion Studio.app/Contents/MacOS/orion-studio`（及 Preview 变体）                                                                                         |
| `script/verify-macos-document-icon` | `:7` usage `/path/to/Orion Studio.app`                                                                                                                                                            |
| `nix/build.nix`                     | `homepage`/`changelog` → `orion.dev`；`mainProgram = "zed"` → `"orion-studio"`（保留 `fetchFromGitHub owner="zed-industries"`：第三方 cargo-bundle 构建工具依赖）                                 |
| `Procfile.web`                      | `cd ../zed.dev` → `cd ../orion.dev`                                                                                                                                                               |
| `ci/Dockerfile.namespace`           | clone `orion-agents/orion-studio.git` + `WORKDIR .../orion-studio`                                                                                                                                |

### 1.2 制品名生成源（单一事实来源）

`tooling/xtask/src/tasks/workflows/vars.rs:371-387`：

- `MAC_AARCH64` → `Orion-Studio-aarch64.dmg`、`MAC_X86_64` → `Orion-Studio-x86_64.dmg`
- `LINUX_*` → `orion-studio-linux-*.tar.gz`
- `WINDOWS_*` → `Orion-Studio-*.exe`
- `REMOTE_SERVER_*` → `orion-studio-remote-server-*.{gz,zip}`

### 1.3 制品名消费侧（必须与生产侧同名，否则远程开发找不到二进制）

`crates/remote/src/transport/ssh.rs:818`、`wsl.rs:203`、`docker.rs:225`：拼接 `orion-studio-remote-server-{}-{}` / `orion-studio-remote-server-{}`（与 vars.rs 一致）。

### 1.4 CI 工作流 YAML（制品文件名模式）

`release.yml` / `release_nightly.yml` / `run_bundling.yml` 批量重命名（Python）：

- `zed-linux-*` → `orion-studio-linux-*`
- `zed-remote-server-*` → `orion-studio-remote-server-*`
- `Zed-*.dmg` / `Zed-*.exe` → `Orion-Studio-*.dmg` / `Orion-Studio-*.exe`

> 注：YAML 中的 `name: zed`（Cachix 缓存名，`cachix-action`）**不改** —— 那是外部 Cachix.org 缓存（Zed 的），需自建 `orion-studio` 缓存后才可换（见 §3 延迟项）。

### 1.5 用户可见安装/CLI 文档（与代码制品名一致）

`docs/src/linux.md`、`docs/src/macos.md`、`docs/src/reference/cli.md`：

- 产品名 `Zed` → `Orion Studio`（prose）
- 命令 `zed` → `orion-studio`（canonical），legacy `zed` 别名在脚本中保留
- 应用包 `Zed.app` → `Orion Studio.app`；桌面 id `dev.zed.Zed` → `dev.orion.OrionStudio`；图标 `orion-studio`
- 数据/配置目录 `~/.local/share/zed` → `~/.local/share/orion-studio`、`~/.config/zed` → `~/.config/orion-studio`、`~/Library/.../Zed` → `.../Orion Studio`
- 下载制品 `zed-linux-*.tar.gz` → `orion-studio-linux-*.tar.gz`；host `cloud.zed.dev` → `cloud.orion.dev`；`asset=zed` → `asset=orion-studio`
- 频道 env `ZED_CHANNEL=preview` → `ORION_STUDIO_CHANNEL=preview`（canonical-first，`ZED_CHANNEL` legacy 回退保留）
- **保留（功能/第三方/ABI，不改）**：`ZED_DEVICE_ID`、`ZED_LOG`、`ZED_FONTS_GAMMA`、`ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST`（代码未改名，文档必须字面匹配）、`zed::` 动作命名空间（ABI）、项目级 `.zed` 文件夹、第三方发行包名（`zed-editor`/`zed-git`/`zed-preview`/`zedit`/`zeditor`）、`--zed <PATH>` CLI flag（V2-04 已定为 BLOCKED-keep 兼容 flag）

### 1.6 GitHub 用户可见模板（修复自有仓库坏链 + 产品文案）

`CODEOWNERS.hold`、`actionlint.yml`、`DISCUSSION_TEMPLATE/feature-requests.yml`、`pull_request_template.md`、`ISSUE_TEMPLATE/{config,10_bug_report,11_crash_report}.yml`：

- 修复自有仓库坏链 `orion-agents/zed` → `orion-agents/orion-studio`（fork 后仓库已改名但模板 URL 未更新，属真实 bug）
- 产品文案 `Zed` → `Orion Studio`
- **保留**：`zed:`/`zed::` 动作命令（命名空间有意保留）、`zedindustries`（外部 Discord 邀请）、`ZedIndustries.{Zed,Preview}`（WinGet 社区包 id，外部）、`.zed`（项目文件夹）、`crates/zed`（crate 路径）、`zed-dev-team`（GitHub 团队 slug，治理延后）

## 2. 关键修复（否则发布会静默失败）

1. **`bundle-mac:335` 主二进制拷贝**：原 `cp .../zed` → `Contents/MacOS/zed`，但主二进制已改名为 `orion-studio`，不修则 macOS 打包产出无主二进制。已改为 `orion-studio`。
2. **制品名生产/消费一致性**：`vars.rs` 生成 `orion-studio-remote-server-*`，而 `crates/remote/{ssh,wsl,docker}.rs` 客户端拼接原 `zed-remote-server-*`。仅改一边会导致远程开发找不到二进制。已同步为 `orion-studio-remote-server-*`。

## 3. 延迟项登记表（R-V2-08-\*）

| ID               | 项                                                                                                                                                                                       | 现状                                    | 处置                                                             |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- | ---------------------------------------------------------------- |
| R-V2-08-CACHIX   | `cachix-action` 缓存名 `zed`                                                                                                                                                             | 外部 Cachix.org 缓存                    | 自建 `orion-studio` Cachix 缓存后替换                            |
| R-V2-08-SENTRY   | `SENTRY_ORG: zed-dev` / `SENTRY_PROJECT: zed` / `ZED_MINIDUMP_ENDPOINT` / `ZED_SENTRY_MINIDUMP_ENDPOINT`                                                                                 | 外部 Sentry org/project                 | 建 `orion-dev` Sentry org 后替换                                 |
| R-V2-08-DO       | `registry.digitalocean.com/zed/collab`                                                                                                                                                   | 外部 DO 容器 registry                   | 建自有 registry/repo 后替换                                      |
| R-V2-08-R2       | `zed-open-source-website-assets`                                                                                                                                                         | 外部 R2 bucket（install.sh 等静态资源） | 建 `orion-studio-*` bucket 后替换                                |
| R-V2-08-EXTORG   | `github.repository_owner == 'zed-extensions'` / `org: 'zed-extensions'` / `owner: zed-extensions` / `repository: zed-extensions/...`                                                     | 上游扩展发布 org                        | 建 `orion-extensions` 或自有扩展发布 infra 后替换                |
| R-V2-08-EXTCLI   | `zed-extension-cli` 下载（`zed-extension-cli.nyc3.digitaloceanspaces.com`）、`huacnlee/zed-extension-action`、`ZED_EXTENSION_CLI_SHA`                                                    | 外部扩展 CLI 工具                       | 自建/镜像后替换                                                  |
| R-V2-08-DANGER   | `danger-proxy.zed.dev`                                                                                                                                                                   | 外部 Danger CI 代理                     | 建 `danger-proxy.orion.dev` 后替换                               |
| R-V2-08-SECRETS  | `ZED_ZIPPY_APP_ID`/`ZED_ZIPPY_APP_PRIVATE_KEY`/`ZED_COMMUNITY_BOT_APP_ID`/`ZED_CLIENT_CHECKSUM_SEED`/`ZED_DEV_REVALIDATE_TOKEN`/`ZED_CLOUD_PROVIDER_ADDITIONAL_MODELS_JSON` 等 secret 名 | 仓库 secret（值由 Orion 运维提供）      | 重命名需在 GitHub repo settings 同步改 secret 名 → 治理决策      |
| R-V2-08-BOT      | `zed-zippy[bot]` 机器人身份                                                                                                                                                              | 外部 GitHub App 机器人                  | 建自有机器人后替换                                               |
| R-V2-08-FORKSYNC | `pushFilter: -zed-editor-[0-9.]*`、`repositories: zed`、`repo: 'zed'`、扩展 rollout “from the main Zed repository”                                                                       | 上游 fork 同步配置                      | 保留（保持与上游同步能力），或切换为 Orion 自有 bot 监听         |
| R-V2-08-WINGET   | `ZedIndustries.Zed` / `ZedIndustries.Zed.Preview`                                                                                                                                        | WinGet 社区包 id（外部维护）            | 提交 Orion 自有 WinGet 包后替换                                  |
| R-V2-08-DISCORD  | `discord.com/invite/zedindustries`                                                                                                                                                       | 外部 Discord 邀请                       | 建 Orion 社区 Discord 后替换                                     |
| R-V2-08-TEAM     | `@orion-agents/zed-dev-team` CODEOWNERS 团队 slug                                                                                                                                        | GitHub 团队 slug（需实际改名）          | 治理决策：GitHub 团队改名 `orion-dev-team` 后替换                |
| R-V2-08-PKG      | linux/macos 文档“通过包管理器安装”章节列举 `zed-editor`/`zed-git` 等第三方包                                                                                                             | 第三方包装的是上游 Zed，非 Orion        | 建 Orion 自有发行包（Homebrew cask `orion-studio` 等）后改写章节 |
| R-V2-08-ZEDPRO   | issue 模板 `Anthropic via ZedPro`                                                                                                                                                        | 上游商业 AI 服务名                      | 随 R-V2-06-AI（Orion AI 基建）决策后替换                         |

## 4. 验证状态

- ✅ `script/install-linux` 文案/制品名/通道 env 已对齐 `orion-studio`（与 `install.sh`/`bundle-linux` 实际行为一致）。
- ✅ 文档与代码制品名/路径/桌面 id 已闭环（`vars.rs` ↔ `bundle-*` ↔ `transport/*` ↔ `install.sh` ↔ `docs/*`）。
- ✅ 3 个 CI YAML 制品文件名模式重命名已执行并复核 CLEAN（不影响 `name: zed` Cachix 缓存）。
- ⚠️ **环境阻塞（沙箱，非代码）**：含 `gpui` 依赖的 crate 类型检查（`cargo check -p zed --bin orion-studio`、`remote_server`、`client`、`paths`）被 macOS **Metal Toolchain** 缺失阻断；全量 `cargo build`/`clippy` 被 `webrtc-sys` 预编译下载（断网）阻断。需在具备 Metal Toolchain + 联网的 CI 复跑。
- ⏸️ `.github` 外部 infra 引用（§3）按“不猜测未确认依赖”原则**有意保留**，登记为延迟项，不在本子计划臆改。

## 5. 结论

V2-08 范围内“自有可控”的打包/安装/CI/文档品牌收敛已完成且内部一致；所有指向外部 Zed 基建的引用均作为治理/基础设施延迟项登记，未做破坏性臆改。符合 V2-00 审计的“改动收敛在配置/品牌/鉴权/服务端点，保持贴近主线”策略。
