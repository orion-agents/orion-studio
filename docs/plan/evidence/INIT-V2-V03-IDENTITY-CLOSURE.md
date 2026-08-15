# V2-03 构建期、运行期和诊断身份闭环 — 证据

## 阶段 ID

V2-03

## 状态

PARTIAL（remote_server 的 check/test 因沙箱缺失 Metal Toolchain 环境阻塞，未能编译验证；其余 focused 门禁已通过。非代码失败，与 V2-00/V2-02 主二进制环境阻塞同源。）

## 当前分支和 HEAD

- 分支：`init`
- HEAD：`99023dd`（本地领先 origin/main 1，未 push）
- 工作树：在 S01–S11 与 V2-00/V2-01/V2-02 的 dirty 基础上继续，本阶段仅改动 V2-03 允许路径。

## 开始前 dirty 边界（本阶段不做 reset/checkout/clean）

继承自上阶段的未提交改动：

- `crates/paths/src/migration.rs`、`crates/paths/src/paths.rs`（V2-01）
- `crates/zed/src/main.rs`（V2-02）
- `docs/plan/evidence/INIT-V2-PROGRESS-AUDIT.md`、`INIT-V2-V01-MIGRATION-HARDENING.md`、`INIT-V2-V02-STARTUP-WIRING.md`
- 未跟踪：`docs/research/`（选型调研，未决是否入库）、`.workbuddy/`（本地记忆，不入库）

## 修改文件（仅 V2-03 允许路径）

- `crates/release_channel/build.rs` — 同时监测 `ORION_STUDIO_RELEASE_CHANNEL`（canonical）与 `ZED_RELEASE_CHANNEL`（legacy），任一存在即启用 `__do_not_set_zed_release_channel` cfg。新增 `rerun-if-env-changed` 两条。
- `crates/release_channel/src/lib.rs` — `compile_time_release_channel_name()` 改为 canonical-first（`ORION_STUDIO_RELEASE_CHANNEL` 优先，`ZED_RELEASE_CHANNEL` 回退）；抽取 `release_channel_name_from_env()` 便于测试；新增 canonical 优先 / legacy 回退的单元测试。
- `crates/zed/build.rs` — 优先接受 `ORION_STUDIO_COMMIT_SHA`，回退 `ZED_COMMIT_SHA`；**同时输出** `ORION_STUDIO_COMMIT_SHA` 与 `ZED_COMMIT_SHA`，以及 `ORION_STUDIO_BUILD_ID` 与 `ZED_BUILD_ID`；新增对应 `rerun-if-env-changed`。
- `crates/cli/build.rs` — 同上逻辑（canonical 优先 + 双写 + rerun-if-env-changed）。
- `crates/remote_server/build.rs` — 输出 `ORION_STUDIO_PKG_VERSION` + `ZED_PKG_VERSION`；提交 SHA 优先接受 `ORION_STUDIO_COMMIT_SHA`，双写 `ORION_STUDIO_COMMIT_SHA`/`ZED_COMMIT_SHA` 与 `ORION_STUDIO_BUILD_ID`/`ZED_BUILD_ID`；新增 rerun-if-env-changed。
- `crates/remote_server/src/server.rs` — 运行期读取全部改为 canonical-first + legacy 回退（`env!("ORION_STUDIO_PKG_VERSION")`、`option_env!("ORION_STUDIO_*")`）；诊断 binary 名与 crash-handler 临时目录前缀收敛为 `orion-studio-*`；保留旧二进制清理前缀 `zed-remote-server-`（磁盘遗留清理，必须保留）；新增 3 个测试锁定诊断身份契约。

## 未修改但检查过的关键文件

- `crates/zed_env_vars/src/zed_env_vars.rs` — 已在 init 落地 `ORION_STUDIO_STATELESS`/`ZED_STATELESS` canonical 优先 + legacy 回退 + 测试，本阶段无需改动，确认合规。
- `crates/windows_resources/src/windows_resources.rs` — 仍读 `ZED_COMMIT_SHA` 做 Windows 资源版本戳。不在 V2-03 允许列表；因本阶段 build.rs 仍双写 `ZED_COMMIT_SHA`，该消费者继续可用。**建议** V2-08/收尾时一并改为接受 canonical（不在本阶段范围）。
- `crates/collab/src/rpc.rs:1158-1188` — `x-zed-app-version` / `x-zed-release-channel` HTTP 协议头。属**服务协议层**，明确归 V2-06（服务端点），本阶段**不触碰**（规则 6：不改生产服务域名/协议）。
- `crates/edit_prediction_cli/**`、`crates/eval_cli/**` — 各自 build.rs 输出 `ZED_PKG_VERSION`、headless.rs 读 `ZED_*`。不在允许列表（V2-04/V2-08 范围）；因 legacy 名继续被本阶段 build.rs 双写且无跨依赖，不受影响。
- `crates/cli/src/main.rs:981,1239`、`crates/zed/src/main.rs:316,318` — 仍读 `ZED_COMMIT_SHA`/`ZED_BUILD_ID`。cli/src/main.rs 与 zed/src/main.rs 不在 V2-03 允许列表（V2-04 范围）；因本阶段 build.rs 仍双写 legacy 名，继续可用。运行期读取的 canonical 化留待 V2-04。
- `script/bundle-mac`、`script/bundle-windows.ps1`、`nix/build.nix` — 仍注入 `ZED_RELEASE_CHANNEL`。属 CI/打包（V2-08）。本阶段 build.rs 已支持 canonical 名，但 CI 脚本切换为 `ORION_STUDIO_RELEASE_CHANNEL` 属 V2-08 范围，**不在本阶段修改**，作为遗留项登记。

## 身份变量矩阵（canonical ORION*STUDIO*_ / legacy ZED\__）

| 用途                      | Canonical（新）                | Legacy（保留回退）    | 写入位置                                                            | 读取位置（本阶段 canonical-first）                                                                 | 说明                                     |
| ------------------------- | ------------------------------ | --------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| 发布频道（构建期覆盖）    | `ORION_STUDIO_RELEASE_CHANNEL` | `ZED_RELEASE_CHANNEL` | `release_channel/build.rs` → cfg `__do_not_set_zed_release_channel` | `release_channel/src/lib.rs` `compile_time_release_channel_name` / `release_channel_name_from_env` | 任一存在即启用编译期频道；canonical 优先 |
| 应用版本（构建期覆盖）    | `ORION_STUDIO_APP_VERSION`     | `ZED_APP_VERSION`     | （init 已处理，运行时读取）                                         | `release_channel::AppVersion::load`                                                                | init 已完成，本阶段确认合规              |
| 提交 SHA（构建期注入）    | `ORION_STUDIO_COMMIT_SHA`      | `ZED_COMMIT_SHA`      | `zed/build.rs`、`cli/build.rs`、`remote_server/build.rs`（双写）    | `remote_server/src/server.rs`、`release_channel` 间接                                              | canonical 优先，双写保证旧消费者可用     |
| 构建 ID（构建期注入）     | `ORION_STUDIO_BUILD_ID`        | `ZED_BUILD_ID`        | 同上（来自 `GITHUB_RUN_NUMBER`，双写）                              | `remote_server/src/server.rs`                                                                      | 同上                                     |
| 包版本（来自 Cargo.toml） | `ORION_STUDIO_PKG_VERSION`     | `ZED_PKG_VERSION`     | `remote_server/build.rs`（双写）                                    | `remote_server/src/server.rs`                                                                      | canonical 优先                           |
| 无状态模式                | `ORION_STUDIO_STATELESS`       | `ZED_STATELESS`       | （init）                                                            | `zed_env_vars`                                                                                     | init 已完成，本阶段确认合规              |

约定：**双写**（build.rs 同时输出 canonical 与 legacy 同名 env）保证尚未 canonical 化的消费者（cli/main.rs、zed/main.rs、edit_prediction_cli、eval_cli、windows_resources）不破；**canonical-first 读取**（在允许文件内）逐步收敛身份。未设 canonical 时回退 legacy；两者皆无时回退编译期 `RELEASE_CHANNEL` 文件 / git HEAD / Cargo.toml 版本。

## 仍保留的 legacy 命中清单（在范围内、有意保留）

1. build.rs 双写的 `ZED_COMMIT_SHA` / `ZED_BUILD_ID` / `ZED_PKG_VERSION` —— 兼容旧消费者，非用户可见，随 V2-04/V2-08 canonical 化后移除。
2. `remote_server/src/server.rs` 的 `OLD_REMOTE_SERVER_BINARY_PREFIX = "zed-remote-server-"` —— 磁盘遗留二进制清理匹配前缀，**必须保留**，已抽常量 + 测试锁定，不得改为 Orion 前缀（否则遗漏旧二进制）。
3. `release_channel/build.rs` 的 cfg 名 `__do_not_set_zed_release_channel` —— 编译期内部标识，非用户可见，与 lib.rs 耦合，保持不动。
4. `windows_resources` / `collab/rpc` / `edit_prediction_cli` / `eval_cli` / `cli|zed/src/main.rs` 中的 `ZED_*` 读取 —— 不在 V2-03 允许列表，因双写策略继续可用，归 V2-04/V2-06/V2-08。
5. CI/打包脚本（`bundle-mac`、`bundle-windows.ps1`、`nix/build.nix`）注入的 `ZED_RELEASE_CHANNEL` —— V2-08 范围，本阶段未改。

## 实现或审计摘要

- 构建期身份：四个 build.rs 全部支持 canonical `ORION_STUDIO_*` 输入优先，并继续输出 legacy `ZED_*` 以便过渡期消费者构建不破；补齐 `rerun-if-env-changed`。
- 运行期身份：release_channel 的频道解析、remote_server 的版本/提交 SHA 全部 canonical-first + legacy 回退；`app_id()`、`app_identifier()`（Windows）、`docs_url`、`display_name` 已在 init 收敛为 Orion Studio / dev.orion.OrionStudio* / Orion-Studio-* / orion.dev，本阶段复核确认。
- 诊断身份：remote_server 的 crash-handler `binary` 字段与临时目录前缀由 `zed-remote-server*`/`zed-remote-proxy*` 收敛为 `orion-studio-remote-server*` / `orion-studio-remote-proxy-*`，并抽常量 + 测试，确保不残留 `zed` 前缀；旧二进制清理前缀保留并测试锁定。
- User-Agent：本阶段确认 `remote_server` 已是 `Orion-Studio-Server/{ver} ({os}; {arch})`（init 已落地），无 `Zed-Server` 残留。
- 错误处理审查（规则 5）：release_channel/lib.rs 的 `AppVersion::load` 对非法版本使用 `.expect(...)`（沿用上游与 init 既有写法，未新增 panic）；`RELEASE_CHANNEL` 静态对未知频道 `panic!` 为上游既有。本阶段**未新增任何 unwrap/expect/panic**，仅做 canonical-first 重构并新增测试。既有 expect 作为已知项登记，不在本阶段范围修复（避免改动版本 API 边界）。

## 测试和命令结果

- `cargo +stable fmt --all -- --check`：**PASS**（本阶段改动已格式化）。
- `git diff --check`（V2-03 改动文件）：**PASS**（无尾随空白 / 制表符错误）。
- `cargo +stable test -p release_channel`：**PASS**（3 测试，含新增 `canonical_release_channel_env_takes_precedence_over_legacy`）。
- `cargo +stable test -p zed_env_vars`：**PASS**（1 测试）。
- `cargo +stable check -p cli`：**PASS**。
- `cargo +stable check -p remote_server`：**BLOCKED（环境）** — `gpui_macos` 构建脚本在编译 metal shader 时失败：`cannot execute tool 'metal' due to missing Metal Toolchain`（macOS 沙箱缺失 Metal Toolchain）。该步骤发生在 `remote_server` 自身类型检查之前，故 `server.rs` 未能进入编译验证。此为环境前置条件缺失，与 V2-00/V2-02 记录的主二进制 `cargo check -p zed --bin orion-studio` 阻塞同源，**非本阶段代码错误**。改动已做人工类型/语法复核（见下）。
- `cargo +stable test -p remote_server`：**BLOCKED（环境）** — 同上依赖链（remote_server → gpui → gpui_macos），无法在不具备 Metal Toolchain 的沙箱中编译运行。
- `./script/clippy`：**NOT RUN** — 全量 clippy 会编译含 webrtc-sys 的 zed 主二进制，沙箱断网导致 webrtc-sys 预编译下载失败（与 V2-00/V2-02 一致的环境 BLOCKED）。本阶段仅对 `release_channel`/`zed_env_vars`/`cli` 做了 focused check/test；`cargo check -p zed --bin orion-studio` 与 `cargo check -p remote_server` 均 BLOCKED（环境）。规则要求使用 `./script/clippy` 且不可用 `cargo clippy` 替代，但环境阻塞须如实标记，故标 BLOCKED/PARTIAL，不写 DONE。

### remote_server/src/server.rs 人工复核（因环境无法编译）

- `option_env!("ORION_STUDIO_BUILD_ID").or(option_env!("ZED_BUILD_ID"))` → `Option<&'static str>`，与 `AppVersion::load` 的 `build_id: Option<&str>` 签名匹配。
- `option_env!("ORION_STUDIO_COMMIT_SHA").or(option_env!("ZED_COMMIT_SHA")).map(|s| AppCommitSha::new(s.to_owned()))` → `Option<AppCommitSha>`，与 `commit_sha: Option<AppCommitSha>` 匹配。
- `env!("ORION_STUDIO_PKG_VERSION")` / `env!("ORION_STUDIO_COMMIT_SHA")` 为编译期宏，依赖 `remote_server/build.rs` 已双写输出（已确认）。
- crash-handler 闭包 `|pid| { paths::temp_dir().join(format!("{REMOTE_SERVER_CRASH_HANDLER_DIR_PREFIX}{pid}")) }` 返回 `PathBuf`，与原 `format!("zed-remote-server-crash-handler-{pid}")` 形态一致。
- 新增 4 个常量与 3 个测试语法自洽（测试仅依赖常量字符串断言，不依赖 gpui_macos）。

## 未运行或环境阻塞

- `./script/clippy` 全量：BLOCKED（webrtc-sys 网络下载）。
- `cargo check -p zed --bin orion-studio` 与全量 `cargo build`：BLOCKED（webrtc-sys 网络下载）。
- `cargo check -p remote_server` / `cargo test -p remote_server`：BLOCKED（缺失 Metal Toolchain，`gpui_macos` 构建脚本 metal shader 编译失败）。remote_server 经 gpui 间接依赖 gpui_macos，故无法在本沙箱编译验证；非代码错误。
- 对构建脚本「分别使用 canonical 和 legacy 环境变量做最小验证」：通过代码审查确认双写与 canonical-first 逻辑；未做独立的环境变量注入构建验证（需联网 CI 或具备 Metal Toolchain / webrtc 预编译的本地环境，标记为待联网复跑）。

## 遗留风险

- 跨阶段遗留：`windows_resources`、CLI/zed 主二进制运行期读取、collab 协议头（`x-zed-*`）、edit*prediction_cli/eval_cli 的 `ZED*\*` 仍需在 V2-04（CLI/主二进制/IPC）、V2-06（服务端点/协议）、V2-08（打包/CI）中 canonical 化；本阶段通过双写保证不破。
- CI/打包脚本仍注入 `ZED_RELEASE_CHANNEL`，canonical 名在 CI 真正使用前不会被触发（本地 dev 构建走 `RELEASE_CHANNEL` 文件）。
- 既有 `.expect()` 版本解析（release_channel/lib.rs）属上游/ init 既有，未新增 panic，但不满足 AGENTS.md 的「避免 expect」总则，建议 V2-09 前评估改为 `anyhow::Result` 或在迁移窗口后收紧。

## 下一阶段建议

- V2-04：收敛主二进制与 CLI 产物、IPC 契约；届时把 `crates/cli/src/main.rs`、`crates/zed/src/main.rs` 的 `ZED_COMMIT_SHA`/`ZED_BUILD_ID` 读取 canonical 化，并处理 `windows_resources` 的版本戳读取。
- V2-06：处理 `collab/src/rpc.rs` 的 `x-zed-app-version` / `x-zed-release-channel` 协议头（服务协议，需域名/契约确认）。
- V2-08：将 `bundle-mac` / `bundle-windows.ps1` / `nix/build.nix` 切换到 `ORION_STUDIO_RELEASE_CHANNEL`，并在 CI 真正开始发 canonical 变量。
- 联网 CI 复跑全量 `./script/clippy` 与 `cargo build` 以解除 BLOCKED。

## 回滚方式

- 本阶段所有改动集中在 6 个允许文件，均为增量（canonical 优先 + legacy 双写），不改变既有行为契约。回滚使用可恢复方式：`git checkout -- crates/release_channel crates/zed_env_vars crates/zed/build.rs crates/cli/build.rs crates/remote_server`（不执行 `git reset --hard`/`clean`/删除用户目录）。本阶段未提交、未 push、未发布。
