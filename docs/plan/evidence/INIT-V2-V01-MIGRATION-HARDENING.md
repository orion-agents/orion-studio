# INIT-V2-V01 迁移模块硬化证据

- 阶段：V2-01（数据迁移模块硬化）
- 状态：**DONE**（全部 4 项门禁通过；仅披露环境性未运行项，见 §7）
- 执行分支：`init`
- HEAD：`99023dd28964dc1ef729eca2e606b6194d0ace06`
- 前置：V2-00 审计（INIT-V2-PROGRESS-AUDIT.md）无阻塞
- 时间：2026-08-12

## 1. 修改文件（仅允许路径）

| 文件                                                    | 改动摘要                                                                                                                                                                     |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/paths/src/migration.rs`                         | 硬化：4 处 `let _ = fs::remove_dir_all` 消除；marker 全量 fail-closed；错误带阶段上下文；`try_exists` 替代 `exists()` 防静默；新增清理语义；新增 15 项测试（模块测试 10→25） |
| `crates/paths/src/paths.rs`                             | 新增 `remote_wsl_server_dir_relative_legacy()`（`.zed_wsl_server` 对称映射）+ 测试 `legacy_wsl_server_dir_retained_for_migration`                                            |
| `docs/plan/evidence/INIT-V2-V01-MIGRATION-HARDENING.md` | 本证据                                                                                                                                                                       |

未触碰：启动流程、服务域名、UI、IPC、发布配置、其他 crate。

## 2. marker 状态机（fail-closed）

`read_marker(dir)` 对 `dir/.orion_migration_marker` 的判定：

| marker 状态                                                | 行为                                                                |
| ---------------------------------------------------------- | ------------------------------------------------------------------- |
| 文件不存在                                                 | `Ok(None)` → 按无迁移状态继续                                       |
| 空文件 / 无首行                                            | `Err(MarkerCorrupted)`，停止                                        |
| 首行不以 `orion-migration-marker` 开头                     | `Err(MarkerCorrupted)`，停止                                        |
| 任一行无 `=` 分隔                                          | `Err(MarkerCorrupted)`，停止                                        |
| `schema` 非数字 / 重复字段                                 | `Err(MarkerCorrupted)`，停止                                        |
| 缺少 `schema`/`result`/`source` 任一必填字段               | `Err(MarkerCorrupted)`，停止                                        |
| `schema` ≠ 当前版本                                        | `Ok(IncompatibleVersion{found,expected})`，安全停止（双方目录不动） |
| `schema` 匹配但 `source` ≠ 本次 `old`                      | `Err(MarkerSourceMismatch)`，fail-closed 停止                       |
| `schema` 匹配、`source` 一致、`result=success`             | `Ok(Completed)`，幂等跳过                                           |
| `schema` 匹配、`source` 一致、`result≠success`（中断遗留） | 落入重试路径，重新迁移                                              |

> 语义变化：旧实现把损坏 marker 当作"无 marker"（fail-open，可能继续合并/覆盖）；新实现全部 fail-closed。`target`/`time` 字段仍为审计用途，不参与状态判定。`source` 比较为字符串级（PathBuf 相等）；迁移必须以同一派生函数（`legacy_*_dir()`）传入旧路径，否则视为来源变化。

## 3. 错误语义（来源 / 目标 / 阶段）

| 变体                                                                    | 语义                                                                                                                                                                                                                                            |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Io { source, path, context }`                                          | 文件系统操作失败；`path`=来源或目标路径，`context`=阶段（如 `read legacy directory`、`copy legacy file`、`write migration marker`、`rename temporary directory into place`、`remove stale temporary directory`、`initialize new directory` 等） |
| `NoParent(path)`                                                        | 新目录无父目录，无法做原子 rename                                                                                                                                                                                                               |
| `MarkerCorrupted { path, reason }`                                      | marker 损坏，fail-closed                                                                                                                                                                                                                        |
| `MarkerSourceMismatch { marker_path, recorded_source, current_source }` | 来源变化，fail-closed                                                                                                                                                                                                                           |
| `TempCleanupFailed { source, path }`                                    | 迁移数据步骤成功，但临时目录清理失败（残留目录被显式上报，不静默）                                                                                                                                                                              |
| `CleanupFailed { original, source, path }`                              | 迁移步骤失败 **且** 清理也失败，两个错误都上报                                                                                                                                                                                                  |

所有 `Error::source()` 返回链式错误（Io→io::Error；CleanupFailed→original）。

## 4. 清理语义（替代原 4 处 `let _ =`）

- `cleanup_after_failure(temp, original) -> MigrationError`：清理成功→返回原错误；清理失败→`CleanupFailed`（两者都保留）。
- `cleanup_after_success(temp) -> Result<(), MigrationError>`：迁移数据成功后清理；失败→`TempCleanupFailed`（明确上报残留目录）。
- 并发 rename `AlreadyExists` 分支：先 merge+marker（数据步骤），再按上述语义清理，任何失败都不静默。

## 5. 路径迁移矩阵

| 路径类别                       | 旧（legacy）                                      | 新（canonical）                              | 处理                                                                                      |
| ------------------------------ | ------------------------------------------------- | -------------------------------------------- | ----------------------------------------------------------------------------------------- |
| config（macOS）                | `~/.config/zed`                                   | `~/.config/orion-studio`                     | `legacy_config_dir()`/`config_dir()`                                                      |
| config（Linux/FreeBSD）        | `$XDG_CONFIG_HOME/zed`                            | `$XDG_CONFIG_HOME/orion-studio`              | 同上                                                                                      |
| config（Windows）              | `%APPDATA%\Zed`                                   | `%APPDATA%\Orion Studio`                     | 同上                                                                                      |
| data（macOS）                  | `~/Library/Application Support/Zed`               | `~/Library/Application Support/Orion Studio` | `legacy_data_dir()`/`data_dir()`                                                          |
| data（Linux/FreeBSD）          | `$XDG_DATA_HOME/zed`                              | `$XDG_DATA_HOME/orion-studio`                | 同上                                                                                      |
| data（Windows）                | `%LOCALAPPDATA%\Zed`                              | `%LOCALAPPDATA%\Orion Studio`                | 同上                                                                                      |
| Flatpak                        | `FLATPAK_XDG_CONFIG_HOME`/`FLATPAK_XDG_DATA_HOME` | 同一环境变量                                 | 新旧对称（均不再 join）                                                                   |
| SSH server                     | `.zed_server`                                     | `.orion_server`                              | `remote_server_dir_relative_legacy()` 保留探测                                            |
| WSL server                     | `.zed_wsl_server`                                 | `.orion_wsl_server`                          | **本轮补齐** `remote_wsl_server_dir_relative_legacy()`（此前缺失，V2-00 审计登记）        |
| 符号链接                       | —                                                 | —                                            | 递归复制中逐条目跳过（含嵌套目录内），新增 `nested_symlink_is_not_copied`                 |
| 特殊文件（socket/fifo/device） | —                                                 | —                                            | 跳过不复制，新增 `socket_file_is_not_copied`（UnixListener 构造真实 socket）              |
| 文件权限                       | —                                                 | —                                            | `fs::copy` 在 unix 保留权限位；新增 `copied_files_preserve_permissions`（0o600 校验）锁定 |

secrets 政策：凭据随数据根整体复制，内容从不打印（错误只含路径）；旧目录首轮保留为备份（`upgrade_only_legacy_is_migrated_and_old_retained`、`secrets_are_copied_and_legacy_retained` 断言）。

## 6. 测试（29 项，全部通过，3 次连续复跑稳定）

`cargo +stable test -p paths` → **29 passed; 0 failed**（×3 次复跑）。

新增 16 项：`empty_marker_fails_closed`、`unrecognized_marker_header_fails_closed`、`duplicate_marker_field_fails_closed`、`marker_missing_required_field_fails_closed`、`marker_source_mismatch_fails_closed`、`interrupted_marker_reattempts_migration`、`failed_copy_cleans_temp_directory`、`no_temp_directory_left_after_repeated_runs`、`cleanup_after_failure_returns_original_when_cleanup_succeeds`、`cleanup_after_failure_reports_cleanup_errors`、`cleanup_after_success_removes_temp`、`cleanup_after_success_reports_leftover_temp`、`nested_symlink_is_not_copied`(unix)、`socket_file_is_not_copied`(unix)、`copied_files_preserve_permissions`(unix)（以上 15 项在 migration 模块，10→25）、`legacy_wsl_server_dir_retained_for_migration`（paths 模块 +1）。

原有 13 项全部保持通过（含 S03 的 3 项路径测试、S04 的 10 项迁移测试；`incompatible_marker_stops_safely` 在 schema 检查先于 source 检查的语义下依然成立）。

> 注：首轮复跑出现过一次 28/1（socket 测试路径超 SUN_LEN），已改为紧凑根路径；随后 3 次连续全绿。

## 7. 验收命令与真实结果

| 命令                                 | 结果                                                                                                                          |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `cargo +stable fmt --all -- --check` | PASS（exit 0）                                                                                                                |
| `cargo +stable test -p paths`        | PASS（29 passed ×3 复跑）                                                                                                     |
| `./script/clippy -p paths`           | PASS（release + `--all-targets --all-features -- --deny warnings`，exit 0；脚本随 `which cargo-machete` 未装而按设计 exit 0） |
| `git diff --check`                   | PASS                                                                                                                          |

未运行 / 环境阻塞（如实披露，不视为通过）：

- **workspace 全量 `./script/clippy`（不带 `-p`）未跑**：沙箱网络阻断 `webrtc-sys` 预编译下载，release 全量会卡在原生依赖；本轮仅改动 `paths` crate，定向 clippy 已覆盖全部改动代码。workspace 全量留给联网 CI / V2-09。
- `cargo +stable check -p zed --bin orion-studio`：仍 BLOCKED（gpui_macos 缺 Metal Toolchain，环境前置，非代码回归）。

## 8. 遗留风险 / 边界（非阻塞，已记录）

1. 目录/文件同名冲突（old 有文件 `a`、new 有目录 `a`）：merge 时 `dest.exists()` 为真则跳过，不覆盖也不报错——fail-safe（旧目录保留数据），但该文件不迁移；v1 接受并文档化，未来可升级为显式冲突上报。
2. Windows 上 `fs::copy` 不复制 ACL/安全描述符；unix 权限位有测试锁定。Windows ACL 迁移属后续平台专项。
3. `source` 比较为字符串级；调用方必须始终用同一派生（`legacy_*_dir()`）——已写入迁移函数文档。
4. marker 是纯文本明文；不含 secret 内容（只有路径与时间戳）。
5. 迁移仍未接入启动流程——V2-02（启动接线）前置条件已满足（paths 测试 + clippy 通过）。

## 9. 交接

- 改动：`crates/paths/src/migration.rs`（硬化 + 16 新测试）、`crates/paths/src/paths.rs`（+WSL legacy 对称映射 + 1 测试）。
- 门禁：fmt / test(29) / clippy(-p paths) / diff-check 全 PASS。
- 回滚：可逆——`git restore crates/paths/src/migration.rs crates/paths/src/paths.rs` 即回滚本阶段（未提交、未改其他文件、未动用户数据）。
- 下一步：V2-02（启动流程接入迁移）——前置已满足。
