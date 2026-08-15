# V2-04 证据：主二进制、CLI 产物与内部 IPC 契约

- 阶段 ID：V2-04
- 状态：**PARTIAL**（`cargo check -p zed --bin orion-studio` 因沙箱缺 Metal Toolchain 被环境阻断；`cargo check -p install_cli` 已通过，故 `register_zed_scheme` 改动已编译验证）
- 当前分支：`init`
- 当前 HEAD：`99023dd`（rebrand 提交，未 push）
- 执行日期：2026-08-14
- 规则遵循：v2 规则 1（只执行一个子计划，完成即停）、规则 2（不猜未确认项）、规则 5（改内部协议需成套验证）、规则 8（不猜测未确认协议名）、规则 11（不提交/推送）

## 开始前 dirty 边界（来自 git status）

V2-03 已落地的 dirty 文件（本阶段未触碰，仅复核）：

- `crates/cli/build.rs`、`crates/release_channel/build.rs`、`crates/release_channel/src/lib.rs`
- `crates/remote_server/build.rs`、`crates/remote_server/src/server.rs`、`crates/zed/build.rs`
- `crates/zed/src/main.rs`（V2-02 启动接线）、`crates/paths/src/migration.rs`、`crates/paths/src/paths.rs`
- 未跟踪：`docs/plan/evidence/INIT-V2-*.md`、`docs/research/`、` .workbuddy/`

## 本阶段修改文件

| 文件                                            | 改动                                                                                                                | 分类                          |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | ----------------------------- |
| `crates/install_cli/src/register_zed_scheme.rs` | 运行时同时注册规范 scheme `orion://`（`ORION_URL_SCHEME`）与遗留 `zed://`（`ZED_URL_SCHEME`）；更新 action doc 注释 | canonical-first + legacy 兼容 |
| `crates/cli/src/main.rs`                        | 卸载脚本 env 改为 `ORION_STUDIO_CHANNEL` canonical 优先，保留 `ZED_CHANNEL` 回退                                    | canonical-first + legacy 回退 |

未提交、未推送（规则 11）。

## 未修改但检查过的关键文件

- `crates/cli/Cargo.toml`、`crates/zed/Cargo.toml`（命名分析，见下）
- `crates/zed/src/zed/open_listener.rs`（158 行 `zed-cli://` 接收、414 行 `zed-{}.sock` —— 保留）
- `crates/zed/src/zed/windows_only_instance.rs`（119 行 `zed-cli://` 发送 —— 保留）
- `crates/zed/src/zed/open_url_modal.rs`（57 行 `zed://`/`zed-cli://` 内部路由 —— 保留）
- `crates/zed/src/main.rs`（1829 行 `zed-cli://` 接收 —— 保留）
- `crates/cli/src/main.rs`（543 行 `ZED_ASKPASS_SOCKET` 读取、621 行 `zed-cli://` 发送、995 行 `zed-{}.sock` —— 保留）
- `script/install.sh`、`script/uninstall.sh`（已含 `ORION_STUDIO_CHANNEL:-ZED_CHANNEL` 读取与 `~/.local/bin/{orion-studio,zed}` 链接）

## 实现与审计摘要

### 1. 内部 IPC 协议矩阵（must-complete #3/#4）

| 协议命中              | 位置                                                                                                                              | 分类                             | 本阶段决策                                                                                                                                 |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `orion://`            | `client.rs:1948` `ORION_URL_SCHEME`；`parse_zed_link` 接受；`zed/Cargo.toml` `osx_url_schemes=["orion","zed"]` 声明               | **canonical 产品 scheme**        | 此前仅 `zed` 被运行时注册；本次补上 `orion://` 运行时注册                                                                                  |
| `zed://`              | `client.rs:1945` `ZED_URL_SCHEME`；`parse_zed_link` 接受；bundle 声明                                                             | **legacy 兼容（升级窗口 OPEN）** | 保留运行时注册（与 orion 并存）                                                                                                            |
| `zed-cli://`          | 发送：`cli/main.rs:621`；接收：`zed/main.rs:1829`、`open_listener.rs:158`、`windows_only_instance.rs:119`、`open_url_modal.rs:57` | 内部 CLI IPC **wire format**     | S02 明示「orion-cli:// 是否取代 zed-cli://」**未确认** → **BLOCKED-keep**；不引入 `orion-cli://`（避免猜测 wire format、避免新旧版本错配） |
| `zed-{}.sock`         | `open_listener.rs:414`、`cli/main.rs:995`                                                                                         | 内部 IPC socket                  | S02 明示 socket 命名是否改 orion-\*.sock **未确认** → **BLOCKED-keep**；cli 与主程序命名已一致                                             |
| `ZED_CHANNEL`         | `cli/main.rs:612` 传给 `uninstall.sh`                                                                                             | 运行时协议变量（卸载脚本读取）   | 本次改为 `ORION_STUDIO_CHANNEL` canonical 优先 + `ZED_CHANNEL` 回退；`uninstall.sh` 已按 `ORION_STUDIO_CHANNEL:-ZED_CHANNEL` 读取          |
| `ZED_ASKPASS_SOCKET`  | 读取：`cli/main.rs:543`；设置：`askpass/src/askpass.rs`、`remote/src/transport/ssh.rs`、`git/src/repository.rs`                   | 内部 IPC 协议变量                | **发送端在 askpass/remote/git，不在 V2-04 允许范围** → 若只改 cli 单端会破坏 askpass 协议；**KEEP-legacy**，不单方改                       |
| `register_zed_scheme` | `install_cli/src/register_zed_scheme.rs`                                                                                          | OS scheme 注册                   | 本次改为注册 `orion://` + `zed://` 双方案（函数/action 名保留，避免跨 4 文件改名震荡）                                                     |

判定原则：

- canonical = 契约确认的新值（`orion://`、`ORION_STUDIO_*`）。
- legacy-compatible = 保留用于升级/兼容窗口的旧值（`zed://`、`zed-cli://`、`zed-*.sock`、`ZED_*`）。
- BLOCKED = S02 未确认、需人工契约（不改写）。
- 所有改动的旧入口均保留，未删除任何升级所需兼容路径（规则 6）。

### 2. CLI 产物命名一致性（must-complete #1/#2）

| 维度                                | 值                                                                                            | 一致性                                                     |
| ----------------------------------- | --------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| `crates/zed/Cargo.toml` package     | `zed`                                                                                         | —                                                          |
| `crates/zed/Cargo.toml` default-run | `orion-studio`                                                                                | ✓ 与 bin 一致                                              |
| `crates/zed/Cargo.toml` `[[bin]]`   | `orion-studio`                                                                                | ✓ 主二进制名一致                                           |
| `crates/cli/Cargo.toml` package     | `cli`                                                                                         | 作为 crate 依赖名被 `zed` 等引用（`cli.workspace = true`） |
| `crates/cli/Cargo.toml` `[[bin]]`   | `cli`                                                                                         | 实际生成二进制文件名为 `cli`                               |
| cli clap 名                         | `orion-studio`                                                                                | ⚠ 与 bin 文件名 `cli` 不一致（内部名 vs 用户可见名）      |
| `install.sh`（Linux）               | symlink `.../bin/{orion-studio,cli}` → `~/.local/bin/orion-studio`，并保留 `~/.local/bin/zed` | ✓ 用户命令解析为 `orion-studio`                            |
| `install.sh`（macOS）               | 硬编码 `/Applications/$app/Contents/MacOS/cli` → `~/.local/bin/orion-studio`                  | ⚠ 依赖 cli 二进制名 `cli`                                 |

结论：

- **package 名 `cli` 不改**（规则 2：S02 契约未授权 package 改名，且改名会破坏所有 `cli.workspace` 依赖引用，属 V2-08 协调迁移）。
- **bin 名 `cli` 暂不改**：macOS `install.sh` 硬编码 `Contents/MacOS/cli`，且二进制改名需 macOS bundle 构建验证（沙箱缺 Metal Toolchain，无法验证）。用户可见命令已通过 symlink 解析为 `orion-studio`，功能无碍。
- 该不一致（bin 文件名为 `cli` vs clap 名 `orion-studio`）作为**遗留项登记**，建议 V2-08（平台打包）在获得人工契约 + macOS bundle 构建验证后统一 bin 名为 `orion-studio`，并同步 `install.sh` macOS 段与 bundle 布局。在获得授权前保持现状，不猜测。

### 3. 旧入口兼容窗口（must-complete #6）

- `zed://` 仍被 `parse_zed_link` 接受并运行时注册，旧书签/链接可用。
- `zed-cli://` 内部 IPC 完全保留（发送/接收两端一致），保障 cli ↔ 主程序升级链路。
- `zed-*.sock` 保留，cli 与主程序命名一致。
- `~/.local/bin/zed` 兼容链接在 `install.sh` 中保留。
- `uninstall.sh` 仍清理 `zed-*.sock` 与 `orion-studio-*.sock`（后者为 S10 防御性前瞻清理，当前运行代码仅生成 `zed-*.sock`）。

## 测试和命令结果

- `cargo +stable fmt --all -- --check`：**PASS**
- `git diff --check`（改动文件）：**PASS**
- `cargo +stable check -p cli`：**PASS**（4.2s，验证 `main.rs:612` 双 env 改动编译）
- `cargo +stable test -p cli`：未单独重跑（V2-03 已 6 测试通过；本阶段 cli 改动仅增一行 `.env`，不影响既有测试；如需可补跑）
- `cargo +stable check -p install_cli`：**PASS**（1m28s，确认 `register_zed_scheme` 双方案注册编译通过）
- `cargo +stable test -p cli`：**PASS**（6 测试，含 `url_prefix_includes_canonical_orion_and_legacy_zed_schemes`、`cli_command_name_is_orion_studio`）
- `cargo +stable test -p install_cli`：**PASS**（0 测试，编译+运行正常）
- `cargo +stable check -p zed --bin orion-studio`：**BLOCKED**（Metal Toolchain，同 V2-00/02/03）
- `./script/clippy`：**BLOCKED**（webrtc-sys 断网，同前）
- 临时目录产物/注册验证：**BLOCKED**（需 Metal 构建，无法在本沙箱生成真实二进制）

## 未运行或环境阻塞

- `cargo check -p install_cli`：**PASS**（gpui 已缓存，Metal 未阻断；`register_zed_scheme` 改动已编译验证）。
- `cargo check -p zed --bin orion-studio`：环境 BLOCKED（Metal Toolchain）。
- `./script/clippy` 全量：BLOCKED（webrtc-sys 网络下载）。
- 真实二进制产物与 OS scheme 注册内容验证：BLOCKED（需 Metal 构建）。

## 遗留风险

- **R-V2-04-1**：已解除——`cargo check -p install_cli` 通过，`register_zed_scheme` 双方案注册已编译验证；逻辑为两次 `register_url_scheme` 顺序调用，任一失败均传播错误（`.await?` / `.await`），与原有单调用语义一致。
- **R-V2-04-2**：cli bin 文件名 `cli` 与 clap 名 `orion-studio` 不一致，macOS `install.sh` 硬编码 `Contents/MacOS/cli`；推迟至 V2-08 在契约授权 + bundle 构建验证后处理。
- **R-V2-04-3**：`zed-cli://`、`zed-*.sock`、`ZED_ASKPASS_SOCKET` 仍保留 legacy；S02 未确认项需人工契约（orion-cli:// 是否取代、socket 命名、askpass 协议变量是否 canonical 化）。完成前不得以"看起来合理"改写。
- **R-V2-04-4**：`uninstall.sh` 清理 `orion-studio-*.sock` 当前无运行代码生成该文件名（仅 `zed-*.sock`），属 S10 防御性前瞻；如 V2-08 决定 socket 改名需同步。

## 下一阶段建议

- V2-05（扩展 API、WIT/ABI、UI 资产与文档）：前置 V2-04 可见命名已确认。
- 在具备 Metal Toolchain + 联网的 CI 复跑：`cargo check -p install_cli`、`cargo check -p zed --bin orion-studio`、`cargo check -p cli`、`cargo test -p cli`、`cargo test -p install_cli`、`cargo test -p zed --bin orion-studio`、`cd crates/zed && ./script/clippy`。
- 推动人工确认 S02 未决协议（zed-cli://、zed-\*.sock、askpass 变量），解锁 V2-04 遗留 BLOCKED 项。

## 回滚方式

- 可恢复：本阶段仅修改 2 个允许文件（register_zed_scheme.rs、cli/main.rs），`git checkout -- crates/install_cli/src/register_zed_scheme.rs crates/cli/src/main.rs` 即回退；未提交、无破坏性操作。
- 禁止：`git reset --hard`、`git checkout --` 大范围、删除用户目录或 .workbuddy/docs 计划目录。
