# S03 验收证据：运行时身份、路径、环境变量和版本显示

- 子计划：`docs/plan/subplans/03-runtime-identity-and-paths.md`
- 状态：**DONE**
- 执行分支：`init`
- 基线 HEAD：`d2779c3`
- 依赖：S01 DONE、S02 DONE（身份契约已批准，最终值见 S02 §1）

## 修改文件（仅本子计划范围）

| 文件 | 改动摘要 |
|---|---|
| `crates/paths/src/paths.rs` | 规范 `APP_NAME = "Orion Studio"`；新增独立 slug 常量 `APP_NAME_LOWERCASE = "orion-studio"`（Linux/FreeBSD XDG 路径，避免空格）；远程目录规范 `.orion_server` + 保留 `remote_server_dir_relative_legacy()`（`.zed_server`，供 S04 探测）；`.zed_wsl_server` → `.orion_wsl_server`；日志名 `Orion Studio.log`；新增测试模块 |
| `crates/zed_env_vars/src/zed_env_vars.rs` | 新增规范 `ORION_STUDIO_STATELESS`，保留 `ZED_STATELESS` 兼容别名（两者均优先读新变量、回退读旧变量，旧变量不被写回——保留 `zed`/`agent`/`db` 三个外部引用符号）；新增兼容读取测试 |
| `crates/release_channel/src/lib.rs` | docs URL → `orion.dev/docs`；显示名 → `Orion Studio` 系列；release/env 变量改为 `ORION_STUDIO_RELEASE_CHANNEL` / `ORION_STUDIO_APP_VERSION` 并兼容回退旧 `ZED_*`；更新并扩展测试 |

## 规范常量（已改）

- `paths::APP_NAME` = `"Orion Studio"`（派生 macOS/Windows 数据目录的规范源）
- `paths::APP_NAME_LOWERCASE` = `"orion-studio"`（派生 Linux/FreeBSD XDG 配置/数据/缓存目录）
- `release_channel` 文档 URL = `https://orion.dev/docs`
- `release_channel` 显示名 = `Orion Studio` / `Orion Studio Preview` / `Orion Studio Nightly` / `Orion Studio Dev`
- `zed_env_vars::ORION_STUDIO_STATELESS`（规范），`release_channel` 用 `ORION_STUDIO_RELEASE_CHANNEL` / `ORION_STUDIO_APP_VERSION`（规范）

## 兼容输入（保留，未删未写回）

- `paths::remote_server_dir_relative_legacy()` → `.zed_server`（旧远程目录，S04 迁移探测源）
- `zed_env_vars::ZED_STATELESS`（旧环境变量，仅兼容读取，回退到 `ORION_STUDIO_STATELESS`）
- `release_channel` 旧 `ZED_RELEASE_CHANNEL` / `ZED_APP_VERSION`（仅兼容读取回退）
- 各兼容入口均带注释说明移除条件（迁移窗口后 S06/S11 移除）

## 验收命令与真实结果

| 命令 | 结果 |
|---|---|
| `git diff --check`（3 crates） | 通过，无 whitespace 错误 |
| `cargo +stable metadata --no-deps --format-version 1` | 通过（workspace 解析 OK） |
| `cargo +stable fmt --all -- --check` | 通过（exit 0，全工作区格式干净） |
| `cargo +stable test -p paths` | 3 passed（app_name_constants_use_orion、legacy_remote_server_dir_retained_for_migration、data_dir_uses_app_name_on_macos） |
| `cargo +stable test -p zed_env_vars` | 1 passed（canonical_env_takes_precedence_and_legacy_still_reads） |
| `cargo +stable test -p release_channel` | 2 passed（test_display_name_for_release_channel、test_docs_url_for_release_channel） |
| `cargo +stable check -p paths -p zed_env_vars -p release_channel` | 通过（含 gpui 依赖链） |

修复记录：rustc 1.95 将 `std::env::set_var/remove_var` 标记为 `unsafe`，测试原调用未包 `unsafe {}` 导致 E0133；`release_channel` 的 `if let Ok` 误接到 `Option` 导致 E0308。两处均已修正并复测通过。

## 范围遵从（禁止事项未违反）

- 未实现 S04 数据复制/删除/恢复/用户确认。
- 未改 `crates/cli`、服务 URL、协议 scheme、安装器或 UI 文案。
- 未重命名整个 workspace。
- 未写入真实 endpoint、secret 或个人路径。
- 未用全局替换删除 Zed 字符串（仅按范围局部、加兼容层改造）。

## 已知冲突 / 待 S10 处理

- `release_channel::app_id()`（macOS/Wayland）与 Windows `app_identifier()` 仍引用 `zed` / `dev.zed.Zed` / `Zed-Editor-*`，按 S03 禁止项留给 S10（bundle metadata）。S03 仅改运行时显示来源，已在常量旁注明冲突点。
- `paths` 的 macOS/Windows 数据目录由 `APP_NAME="Orion Studio"` 派生，将导致新安装与旧 `Zed/` 目录并存，由 S04 负责探测并迁移。

## 交接

- 规范常量已全部落地，旧值保留为兼容/迁移输入。
- 测试全绿，fmt/check 全绿，改动范围严格收敛于 3 个 crate。
- **S04（数据目录迁移）可以开始。**
