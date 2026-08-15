# INIT-V2 进度审计（V2-00）

- 阶段：V2-00（只读进度审计）
- 执行分支：`init`
- 实际 HEAD：`99023dd28964dc1ef729eca2e606b6194d0ace06`（`rebrand: migrate fork identity from Zed to Orion Studio`，197 文件 / +6053 / -970）
- 计划基线 HEAD：`d2779c3` —— **偏差已记录**：init-plan-v2.md 编写时工作树是 dirty 未提交；2026-08-12 经用户"继续"授权已本地提交（未 push）。内容全部保留，仅从"未跟踪 dirty"变为"已提交"。无 reset/checkout/clean/删除。
- 审计时间：2026-08-12
- 状态：**VERIFIED（含已记录偏差与延迟项）**，V2-01 允许执行

## 1. Git 基线

| 项                                  | 值                                                                |
| ----------------------------------- | ----------------------------------------------------------------- |
| `git branch --show-current`         | `init`                                                            |
| `git rev-parse HEAD`                | `99023dd28964dc1ef729eca2e606b6194d0ace06`                        |
| shallow/grafted                     | shallow 仓库 = true；git log 显示 `(grafted)`（上游历史被截断）   |
| `git status --short`                | 仅 `?? .workbuddy/`、`?? docs/research/`                          |
| `git diff --stat`（工作树 vs HEAD） | 空（无未提交 tracked 改动）                                       |
| `git diff --check`                  | 通过                                                              |
| ahead/behind                        | `init` 领先 `origin/main` 1 个提交（0 behind / 1 ahead），未 push |

未跟踪目录边界：`.workbuddy/`（本地执行记忆，不入库）、`docs/research/`（选型调研，独立未决）、`docs/plan/`（已随 99023dd 提交入库）。`target/` 被 .gitignore 忽略。

## 2. S01–S06 evidence 与源码抽查

| 子计划         | evidence 声明                      | 本轮抽查结果                                                                                                                                                                      |
| -------------- | ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S01 基线盘点   | 7000 行 / 1009 文件端点清单        | VERIFIED（清单与源码一致；master `server_url` 在 `assets/settings/default.json`）                                                                                                 |
| S02 身份契约   | 命名冻结                           | VERIFIED（Orion Studio / orion-studio / ORION*STUDIO*_ / orion.dev / orion:// / dev.orion.OrionStudio_；`crates/paths/src/paths.rs` `APP_NAME` 与 `APP_NAME_LOWERCASE` 实测一致） |
| S03 运行时路径 | APP_NAME 规范 + slug + 远程目录    | VERIFIED（paths.rs 抽查；`remote_server_dir_relative()` = `.orion_server`，legacy = `.zed_server`；测试 13 passed 复跑通过）                                                      |
| S04 数据迁移   | migration.rs 实现 + 13 测试        | PARTIAL：实现与测试真实存在且复跑通过，但**迁移未接入启动流程**（见 §3）；存在 `let _ =` 静默丢弃与 marker fail-open 问题（V2-01 修复）                                           |
| S05 主二进制   | `[[bin]] orion-studio`             | PARTIAL：Cargo.toml `[[bin]] name = "orion-studio"` 已改；**主二进制完整构建无成功证据**（Metal Toolchain 缺失，见 §4）                                                           |
| S06 CLI/协议   | clap 名 orion-studio + 兼容 scheme | PARTIAL：`crates/cli/src/main.rs` clap 名已改，`orion://`/`zed://` 双接受存在；`zed-cli://` 内部协议仍 5 处（V2-04 范围）                                                         |

## 3. 迁移接线核查（P0）

`grep -rn "migrate_root|legacy_config_dir|legacy_data_dir"`（排除 crates/paths）→ **0 命中**。
结论：`crates/paths/src/paths.rs` 只 `pub use` 导出，**应用启动流程未调用迁移**。用户升级时不会自动迁移数据 —— 属 V2-02（启动接线）范围，V2-01 只硬化模块本身，不接入。

## 4. 已实测验证的命令（非转述）

| 命令                                                                                                     | 结果                                                                                                                 |
| -------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `cargo +stable test -p paths`（2026-08-12，HEAD 99023dd）                                                | 13 passed；0 failed                                                                                                  |
| `cargo +stable fmt --all -- --check`                                                                     | 通过（0 diff）                                                                                                       |
| `git diff --check`                                                                                       | 通过                                                                                                                 |
| 早期本轮会话 `cargo check -p client -p cloud_api_client -p http_client -p context_server -p open_router` | 通过（24.70s）                                                                                                       |
| 早期本轮会话 `cargo check -p zed_extension_api`                                                          | 通过（1.57s）                                                                                                        |
| `cargo +stable check -p zed --bin orion-studio`（主二进制）                                              | **BLOCKED（环境）**：gpui_macos 缺 Metal Toolchain；且 webrtc-sys 预编译下载在沙箱断网。非代码失败证据               |
| `./script/clippy`                                                                                        | **NOT VERIFIED（本轮）**：V2-01 门禁中必须执行（`./script/clippy -p paths` 定向跑避开 workspace 的 webrtc 网络依赖） |

## 5. 品牌残留扫描与分类（2026-08-12 实测计数，excl target/.workbuddy/docs/plan）

| 模式                | 计数 | 分类                                                                                                                                                                                                                                  |
| ------------------- | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Zed`               | 1730 | 绝大多数为上游归属/法律/LICENCE/注释/doc 链接 → KEEP-ATTRIBUTION；少量用户可见文案 → 已由 S07 处理或待 V2-05                                                                                                                          |
| `zed.dev`（含子域） | 217  | 代码内用户可见文档/帮助/状态/商店/schema 链接 → 延迟（或ion.dev 站点建好再扫）；`assets/settings/default.json` 主开关已是 `orion.dev`                                                                                                 |
| `cloud.zed.dev`     | 3    | `crates/sandbox/src/windows_wsl.rs:85,121`（WSL sandbox 下载 Zed 发布产物端点）+ `tooling/xtask/.../after_release.rs:65`（发布刷新端点）→ 真实活动端点残留，属 V2-06/08 服务端点范围，**延迟**                                        |
| `ZED_`              | 878  | 绝大多数为 env 变量前缀，属于既定兼容层（`ZED_*` 回退保留）→ KEEP-COMPAT；另有 `ZED_CHANNEL`/`ZED_ASKPASS_SOCKET` 等内部协议变量 → V2-04 协议矩阵                                                                                     |
| `zed-cli://`        | 5    | `open_listener.rs`、`windows_only_instance.rs`、`open_url_modal.rs`、`zed/src/main.rs:1819`、`cli/src/main.rs:621` → 内部 CLI IPC 协议；orion-cli:// 是否取代未定（S02 明示不得自行猜测）→ **BLOCKED 待人工契约**，V2-04 处理         |
| `zed-*.sock`        | 3    | 全部为 `crates/sandbox/src/linux_bubblewrap.rs` 测试内 `/tmp/zed-proxy.sock` → 测试 fixture；运行期 socket 命名属 V2-04                                                                                                               |
| `dev.zed`           | 4    | `cli/src/main.rs:1137,1158`（有意兼容检查 `dev.zed.Zed` ↔ `dev.orion.OrionStudio`，S06 产物）→ KEEP-COMPAT；`zed/src/main.rs:174` 通知 ID `dev.zed.Oops` → 延迟；`gpui/examples/system_notifications.rs:100` 示例身份 → 测试 fixture |
| `Zed-Server`        | 0    | 已全部迁移为 `Orion-Studio-Server`（remote_server User-Agent）                                                                                                                                                                        |
| `zed_wsl_server`    | 1    | `crates/util/src/shell.rs:704` 注释（dev 命令示例）→ KEEP-COMMENT；`paths.rs` 已有 canonical `.orion_wsl_server`，**缺 legacy `.zed_wsl_server` 对称映射** → V2-01 补齐                                                               |

## 6. migration.rs 现状问题清单（供 V2-01）

1. **`let _ = fs::remove_dir_all(&temp)` × 4**（第 187/191/200/206 行）——违反 AGENTS.md 错误处理规则，临时目录清理失败被静默丢弃。
2. **marker fail-open**：`read_marker` 对损坏/空/无头 marker 返回 `Ok(None)`（当作无 marker 继续迁移）；`schema`/`result` 缺失时用 `unwrap_or(0)`/`unwrap_or_default()` 兜底；重复字段 last-wins；**source 字段被解析后丢弃，来源变化不校验**——都可能把异常状态当成"无迁移状态"继续合并/覆盖。
3. 错误无"阶段/来源/目标"上下文（只有 path）。
4. 路径矩阵缺 WSL legacy（`.zed_wsl_server`）；symlink/特殊文件测试只覆盖顶层 symlink。
5. secrets 权限保留无测试；权限保留依赖 `fs::copy` 的 unix 行为，需测试锁定。
6. 生产代码无 unwrap/expect（`unwrap_or` 仅用于时间戳，可接受）；V2-01 不得新增。

## 7. V2-01 允许修改边界（精确清单）

- `crates/paths/src/migration.rs`
- `crates/paths/src/paths.rs`（仅限：新增 `remote_wsl_server_dir_relative_legacy()` 对称映射 + 相关测试；不动其他路径函数）
- `crates/paths/src/lib.rs` —— **不存在**（库根为 `paths.rs`，`[lib] path = "src/paths.rs"`），无需创建
- `crates/paths` 中与迁移直接相关的现有测试（`migration.rs` 内 `mod tests` + `paths.rs` 内 `mod tests`）
- `docs/plan/evidence/INIT-V2-V01-MIGRATION-HARDENING.md`（V2-01 证据）

V2-01 不接入启动流程、不改服务域名/UI/IPC/发布配置、不做品牌全局替换。

## 8. 结论

| 项           | 结论                                                                   |
| ------------ | ---------------------------------------------------------------------- |
| S01–S02 契约 | VERIFIED                                                               |
| S03 路径     | VERIFIED（测试复跑通过）                                               |
| S04 迁移模块 | PARTIAL（实现真实、测试通过；未接线 + 错误处理/marker 需硬化 → V2-01） |
| S05 主二进制 | PARTIAL/BLOCKED（构建缺 Metal Toolchain，环境）                        |
| S06 CLI/协议 | PARTIAL（zed-cli:// 协议契约未定 → BLOCKED 待人工，V2-04）             |
| 服务端点     | PARTIAL（cloud.zed.dev 3 处残留 → V2-06/08 延迟）                      |
| V2-01 前置   | **无阻塞**（审计本身未修改任何源代码；`git diff --check` 通过）        |

→ V2-01（迁移模块硬化）可以执行。
