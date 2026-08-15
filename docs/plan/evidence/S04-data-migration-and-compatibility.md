# S04 验收证据：配置、状态、密钥和远程目录迁移

- 子计划：`docs/plan/subplans/04-data-migration-and-compatibility.md`
- 状态：**DONE**
- 执行分支：`init`
- 基线 HEAD：`d2779c3`
- 依赖：S01 DONE、S02 DONE（迁移政策见 §3.2 / §1）、S03 DONE（规范 `APP_NAME` 已落地）

## 修改文件（仅本子计划范围）

| 文件                            | 改动摘要                                                                                                                                                                               |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/paths/src/migration.rs` | **新增**：迁移状态模型、幂等/原子迁移算法、按分类复制（跳过符号链接与特殊文件）、旧目录保留为备份、纯文本 marker（版本/源/目标/时间/结果）。含 10 项测试矩阵                           |
| `crates/paths/src/paths.rs`     | 声明 `mod migration;` 并 `pub use` 公开 `migrate_root` / `MigrationState` / `MigrationError` / `legacy_config_dir` / `legacy_data_dir` / marker 常量（S03 的 `APP_NAME` 改动保持不动） |

未触碰 `crates/cli`、`crates/client`、`crates/collab`、数据库生产迁移、Docker、安装器、UI、服务 endpoint。

## 支持 / 不支持的数据类别（按 S04 §迁移分类）

| 数据                                                  | 处理                                                                                                          | 失败时                                 |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | -------------------------------------- |
| 用户配置（config 根）                                 | 整体原子迁移 + 旧目录保留备份                                                                                 | 报错，保留旧配置                       |
| workspace/session 状态（data 根，含 db）              | 整体原子迁移                                                                                                  | 不阻塞，旧数据可读                     |
| 扩展目录（extensions/remote_extensions）              | 随 data 根迁移                                                                                                | 不删旧目录                             |
| 密钥/凭据                                             | **随用户数据根目录整体迁移**（即 copy 整个根树，含其内部凭据文件）；旧目录保留为备份，**首轮不删除**          | 失败则要求重新认证（旧凭据仍在备份中） |
| cache/temp                                            | 不作为必须迁移数据（可重新生成）                                                                              | —                                      |
| 日志                                                  | 随 data 根迁移，可选                                                                                          | 不影响启动                             |
| remote server 数据（`.zed_server` → `.orion_server`） | 由 S03 保留的 `remote_server_dir_relative_legacy()` 供探测；本模块覆盖根级迁移，远程主机侧按 S03 契约后续处理 | 不自动覆盖远端                         |

> 密钥迁移政策：本合同采用"**用户数据根目录整体原子复制 + 旧目录保留为备份、首轮不删**"的明确定义，满足 S04 BLOCKED 条件中"必须有明确密钥迁移政策"的要求，故 S04 不 BLOCKED。

## 迁移状态模型与 marker 位置

- 状态枚举（`MigrationState`）：`NoLegacyData` / `Completed` / `Migrated` / `IncompatibleVersion { found, expected }`。
- 失败以 `Err(MigrationError)` 表达，且**保证旧数据完整可读**（等价于契约的"迁移失败但旧数据仍可读"）。
- 幂等判定**不依赖"目录存在"**：成功后在 `new` 根写入 marker 文件 `.orion_migration_marker`（纯文本：`orion-migration-marker v1` + `schema=` + `source=` + `target=` + `time=` + `result=success`）。重复启动读 marker；schema 不符 → `IncompatibleVersion` 安全停止；`result=success` → `Completed` 跳过。
- marker 常量：`MIGRATION_MARKER_NAME = ".orion_migration_marker"`、`MIGRATION_SCHEMA_VERSION = 1`。

## 算法（安全迁移，S04 §2）

1. 计算旧根 / 新根（新根来自 S03 的 `config_dir()`/`data_dir()`，旧根由 `legacy_config_dir()`/`legacy_data_dir()` 按旧 `Zed`/`zed` 命名镜像派生）。
2. 新根有成功 marker → `Completed`（幂等，不复制）。
3. 旧根不存在 → 初始化新根 + 写 marker → `NoLegacyData`。
4. 旧根存在且新根不存在 → 在**同父目录**建临时目录 `.orion-studio.migrating-<pid>-<nanos>`，复制树 → 写 marker → **原子 rename** 临时目录到新根；旧根保留。
5. 旧根存在且新根已存在（部分安装）→ **merge 复制**（不覆盖新根已有文件）+ 写 marker。
6. 任一步失败 → 清理临时目录、`Err` 返回，**旧根不变**。
7. 全程**禁止递归删除旧根**；符号链接与特殊文件（socket/fifo/device）跳过不复制，防路径穿越/循环。

## 测试矩阵（实际结果，全部通过 = 13 passed）

| 用例                                                | 覆盖                                                        | 结果 |
| --------------------------------------------------- | ----------------------------------------------------------- | ---- |
| `fresh_install_without_legacy`                      | 新装：无旧目录 → 初始化新根+marker                          | ok   |
| `upgrade_only_legacy_is_migrated_and_old_retained`  | 升级：仅旧目录 → 迁移，旧目录保留                           | ok   |
| `partial_new_directory_is_merged_without_overwrite` | 新旧并存 → merge，不覆盖新文件                              | ok   |
| `repeated_migration_is_idempotent`                  | marker 存在 → 幂等 Completed，不再复制                      | ok   |
| `interrupted_partial_new_retries_successfully`      | 新根为空（上次中断）→ 重试成功                              | ok   |
| `write_failure_preserves_legacy_data`               | 目标父为文件 → 写失败返回 Err，旧数据完好                   | ok   |
| `symlinks_are_not_copied`                           | 符号链接不复制                                              | ok   |
| `secrets_are_copied_and_legacy_retained`            | 凭据随根复制且旧目录保留（政策验证）                        | ok   |
| `incompatible_marker_stops_safely`                  | schema=999 marker → 安全停止，不覆盖                        | ok   |
| `legacy_dirs_use_zed_naming`                        | `legacy_config_dir`/`legacy_data_dir` 按平台用 Zed/zed 命名 | ok   |
| + S03 既有 3 项路径测试                             | 回归                                                        | ok   |

## 失败注入与回滚证据

- `write_failure_preserves_legacy_data`：把新根父路径设为普通文件，使 `create_dir_all` 失败（NotADirectory），断言 `migrate_root` 返回 `Err` 且 `old/settings.json` 内容与存在性不变 → 证明失败不破坏旧数据。
- `incompatible_marker_stops_safely`：预置 schema=999 的 marker，断言返回 `IncompatibleVersion` 且 `new` 未被旧数据覆盖（仅含 1 个文件=marker）→ 证明版本不兼容时安全停止。
- 回滚策略（线上）：恢复上一版客户端即可；旧目录作为备份始终保留，读备份即可，禁止用新 schema 降级覆盖旧数据（本模块首轮不删旧目录，天然满足）。

## 验收命令与真实结果

| 命令                                                  | 结果                 |
| ----------------------------------------------------- | -------------------- |
| `git diff --check`                                    | 通过                 |
| `cargo +stable fmt --all -- --check`                  | 通过（exit 0）       |
| `cargo +stable metadata --no-deps --format-version 1` | 通过                 |
| `cargo +stable test -p paths`                         | 13 passed；0 warning |
| `cargo +stable check -p paths`                        | 通过                 |

## 范围遵从（禁止事项未违反）

- 未实现 UI/CLI 调用（仅库函数 + 测试；app 启动接线留给后续步骤）。
- 未改 `cli`/`client`/`collab`/数据库生产迁移/Docker/安装器/服务 endpoint。
- 未引入新依赖（marker 用纯文本，避免改 Cargo.toml）。
- 未访问真实用户目录 / 真实 cloud / 真实 secret（测试全用临时目录）。
- 未做危险递归删除；旧目录首轮保留。

## 交接

- 支持类别：config/data 根整体迁移（含 db、扩展、日志、凭据）；不支持：自动删除旧目录、覆盖新根已有文件、复制符号链接/特殊文件。
- 状态模型 + marker 位置见上；迁移函数 `migrate_root(old, new)` 已就绪，`legacy_*_dir()` 供 app 启动接线使用。
- 测试矩阵全绿，失败注入/回滚证据齐备。
- **S05 可以开始。**
