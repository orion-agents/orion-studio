# INIT-V2-V02 启动流程接入迁移证据

- 阶段：V2-02（启动流程接入迁移）
- 执行分支：`init`
- HEAD：`99023dd28964dc1ef729eca2e606b6194d0ace06`
- 前置：V2-01 完成（paths 测试 29 passed + clippy 通过）
- 时间：2026-08-12
- 状态：**DONE**（主二进制 focused check 环境阻塞，见 §6，非代码失败）

## 1. 修改文件（仅允许路径）

| 文件                                               | 改动摘要                                                                                                                                                  |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/paths/src/migration.rs`                    | 新增 `MigrationOutcome`、`migrate_legacy_user_data()`（启动入口）、`migrate_legacy_dirs()`（可测试编排）、`migrate_root_if_legacy_exists()`；模块文档更新 |
| `crates/paths/src/paths.rs`                        | `pub use` 导出新入口（`MigrationOutcome`/`migrate_legacy_dirs`/`migrate_legacy_user_data`）                                                               |
| `crates/zed/src/main.rs`                           | 在 `init_paths()` 前调用 `paths::migrate_legacy_user_data()`；失败 → `eprintln` + `process::exit(1)`                                                      |
| `docs/plan/evidence/INIT-V2-V02-STARTUP-WIRING.md` | 本证据                                                                                                                                                    |

未触碰：UI 文案、服务域名、IPC、发布配置、其他 crate。

## 2. 首次读取点定位（真实调用链）

`crates/zed/src/main.rs::main()` 启动顺序（相关段）：

1. 各种 early-return（etw / printenv / dump_all_actions）——均不读目录。
2. `paths::set_custom_data_dir(dir)`（L273，若有 `--user-data-dir`）。
3. **`init_paths()`（L287）——首个真实消费者**：内部（L1656-1674）对 `config_dir`、`extensions_dir`、`languages_dir`、`debug_adapters_dir`、`database_dir`、`logs_dir`、`temp_dir`、`hang_traces_dir` 逐个 `create_dir_all`，是这些目录第一次被读取/创建。
4. 之后才 `zlog::init()` → 设置加载 → 数据库打开 → 工作区打开。

结论：迁移必须插在 `set_custom_data_dir`（自定义目录已生效）之后、`init_paths()`（首次读目录）之前。

## 3. 接入设计与行为

```rust
// crates/zed/src/main.rs（init_paths 之前）
if let Err(error) = paths::migrate_legacy_user_data() {
    eprintln!("Failed to migrate legacy Zed data: {error}");
    process::exit(1);
}
```

- **单次执行**：整个进程只有这一处调用；后续消费者通过 `paths::*` 的 `OnceLock` 取同一值，不存在多个消费者各自跑迁移。
- **无旧目录时启动行为不变**：`migrate_legacy_dirs` 只在 legacy 目录 `try_exists()` 为真时调用 `migrate_root`；全新安装两个根都跳过，不创建目录、不写 marker、零副作用（`init_paths` 照常创建）。
- **有旧目录时只执行一次、重复启动幂等**：首次迁移写 `.orion_migration_marker`；后续启动 `read_marker` → `Completed` 跳过。
- **失败行为**：`migrate_legacy_user_data()` 返回带上下文的 `MigrationError`（阶段 + 路径）；main.rs 打印 stderr 并以 exit 1 终止——**阻止以空的新目录继续运行**（旧目录从未删除，可重试）。
- **部分失败语义**：config 根先迁、data 根后迁；config 成功而 data 失败时返回 Err 并终止，config 已写 marker，下次启动自动跳过 config、只重试 data。

## 4. 编排函数（crates/paths/src/migration.rs）

- `migrate_legacy_user_data() -> Result<MigrationOutcome, MigrationError>`：解析 `legacy_config_dir()`/`legacy_data_dir()` 与 `config_dir()`/`data_dir()`，委托 `migrate_legacy_dirs`。
- `migrate_legacy_dirs(legacy_config, new_config, legacy_data, new_data)`：逐根独立处理；legacy 为 `None` 或不存在 → 该根跳过（outcome `None`）；任一失败 → `Err` 传播。
- `MigrationOutcome { config: Option<MigrationState>, data: Option<MigrationState> }`。
- 单根探测用 `try_exists()`（V2-01 语义），探测失败（如 ENOTDIR/权限）→ 带上下文错误，不静默当"不存在"。

## 5. 测试（33 项，全部通过，2 次复跑）

`cargo +stable test -p paths` → **33 passed; 0 failed**（×2）。

新增 4 项（全部隔离临时目录，清理用 `fs::remove_dir_all(&root).unwrap()`——清理失败可见）：

| 用例                                                          | 覆盖                                                                                  |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `migrate_legacy_dirs_migrates_existing_roots_only`            | 仅 data 有 legacy → 只迁 data（`Migrated`），config 根完全不动（`None` + 目录未创建） |
| `migrate_legacy_dirs_no_legacy_does_nothing`                  | 无 legacy → 两个根均为 `None`，新目录不创建（全新安装零副作用）                       |
| `migrate_legacy_dirs_is_idempotent_across_runs`               | 首跑 `Migrated` → 二跑 `Completed`；目录内容不重复（settings.json + marker = 2 项）   |
| `migrate_legacy_dirs_propagates_failure_and_preserves_legacy` | data 根目标被文件阻塞 → `Err`；config 根已迁内容保留、legacy 数据未动（部分失败语义） |

## 6. 验收命令与真实结果

| 命令                                                              | 结果                                                              |
| ----------------------------------------------------------------- | ----------------------------------------------------------------- |
| `cargo +stable fmt --all -- --check`                              | PASS                                                              |
| `cargo +stable test -p paths`                                     | PASS（33 passed ×2）                                              |
| `./script/clippy -p paths`                                        | PASS（`--all-targets --all-features -- --deny warnings`，exit 0） |
| `cargo +stable check -p paths`                                    | PASS                                                              |
| `cargo +stable check -p zed --bin orion-studio`（受影响主二进制） | **BLOCKED（环境）**：见下                                         |
| `git diff --check`                                                | PASS                                                              |

主二进制 check 环境阻塞详情：`webrtc-sys`（LiveKit 原生依赖）build.rs 需联网下载预编译产物，沙箱断网（TLS `close_notify`）；另有 gpui_macos Metal Toolchain 前置缺失。该命令在本轮以后台任务复跑确认，结果见 §7；**main.rs 的改动属增量（仅新增调用 + 错误处理，未改签名），且 `paths` 公共 API 为纯新增**，依赖方不受破坏性影响。

## 7. 未运行 / 环境阻塞（如实披露）

- `cargo +stable check -p zed --bin orion-studio`（受影响主二进制）：**确认 BLOCKED（环境）**。本轮后台复跑 10m01s：所有前置 Rust 依赖编译后被 `webrtc-sys`（livekit 原生依赖）build.rs 阻断——`Failed to write WebRTC download to temporary file / operation timed out`（沙箱断网，TLS/超时）。依赖构建先于 zed crate，故 **main.rs 未被执行 cargo 类型检查**；main.rs 改动为纯增量（新增调用 + 错误处理，未改任何现有签名），且新 API 已由 `cargo check -p paths` 全量验证。主二进制编译证据待联网 CI / V2-09。
- workspace 全量 `./script/clippy`：未跑（同上 webrtc-sys 网络阻塞；本轮改动仅 paths + main.rs，定向 clippy 已覆盖全部改动代码）。
- 真实用户目录迁移的端到端验证：未执行（本机未以真实 `~/Library/Application Support/Zed` 跑迁移；属 V2-07/V2-09 范围）。

## 8. 遗留风险

1. 迁移失败会硬退出（exit 1）——符合"阻止以空目录继续运行"的契约；未来若需降级为"警告但继续"，需另行决策（本阶段不猜）。
2. `--user-data-dir` 自定义目录下仍会迁移 legacy 数据（merge 不覆盖既有文件，fail-safe）——行为已文档化，未额外加开关。
3. Windows ACL 不在 `fs::copy` 复制范围内（V2-01 已记录，平台专项）。
4. 主二进制构建证据依赖联网 CI（R-WEBRTC-COMPILE）。

## 9. 交接

- 改动：`crates/zed/src/main.rs`（+10 行接线）、`crates/paths/src/migration.rs`（+编排入口/结果类型/4 测试）、`crates/paths/src/paths.rs`（导出）。
- 门禁：fmt / test(33) / clippy(-p paths) / check(-p paths) / diff-check 全 PASS。
- 回滚：可逆——`git restore crates/zed/src/main.rs crates/paths/src/migration.rs crates/paths/src/paths.rs`（未提交、未动用户数据、无破坏性命令）。
- 下一步：V2-03（构建期/运行期/诊断身份闭环）——前置（V2-00 完成）已满足。
