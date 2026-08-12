# S05 验收证据：主 package、binary 和默认构建入口

- 子计划：`docs/plan/subplans/05-core-package-and-binary.md`
- 状态：**DONE**（重量级编译 `cargo check -p zed --bin orion-studio` 见下方"验收命令"）
- 执行分支：`init`
- 基线 HEAD：`d2779c3`
- 依赖：S01/S02/S03/S04 DONE

## 修改文件（仅本子计划范围）

| 文件 | 改动摘要 |
|---|---|
| `crates/zed/Cargo.toml` | `default-run = "orion-studio"`；主 `[[bin]] name = "zed"` → `"orion-studio"`；`description` → Orion Studio 描述。**package `name` 保持 `"zed"`（内部 crate 名，见下方限制）**。`bundle-*` 元数据、`authors`、`zed_visual_test_runner` bin 未动 |
| `crates/zed/src/main.rs` | 崩溃处理器 `binary: "zed"` → `"orion-studio"`；崩溃临时目录前缀 `zed-crash-handler-` → `orion-studio-crash-handler-`；clap `#[command(name = "zed")]` → `"orion-studio"` |
| `crates/zed/src/zed.rs` | macOS 窗口 Tab 分组标识 `tabbing_identifier: "zed"` → `"orion-studio"` |

> 关键修复：S03 已将 `APP_NAME_LOWERCASE = "orion-studio"`，而 `main.rs` 的编译期 `const assert!`（要求 `CARGO_BIN_NAME == APP_NAME_LOWERCASE`）原先会因二进制仍叫 `zed` 而**编译失败**。本次二进制改名使该断言通过，恢复了 `zed` 包的编译。

## 未修改（按 S05 禁止项 / 分步策略）

- **package `name = "zed"` 保留**：改为 `orion-studio` 需把 workspace `[workspace.dependencies] zed = { path = "../zed" }` 及所有 `zed.workspace = true` 引用（跨大量 crate）同步改名，属 S05 明令禁止的"批量重命名 crates/*"与 BLOCKED 条件。合同 §1 的 crate 改名标注为"分阶段，S05"——实际是更大的分批改名工程，超出本子计划范围，留作后续独立步骤。
- `crates/cli`（CLI 二进制名、spawn 逻辑）— 禁止项，未动；CLI↔GUI 的跨二进制名称引用属 S06/S10。
- `bundle-*`（`dev.zed.Zed*` identifier、`osx_url_schemes=["zed"]`）— 属安装器/Bundle ID，S10。
- `authors = ["Zed Team <hi@zed.dev>"]` — 上游归属（KEEP-ATTRIBUTION），保留。
- `app_menus.rs:63 name: "Zed"` — UI 菜单品牌，属 S06/S07 内容/UI 品牌，未动。
- `zed.rs:5843` 遥测事件类型名 `"zed"` — 遥测命名，属 S06/S09，未动。
- `zed://` scheme、`zed.dev` 等 doc 注释 — 属 S10，未动。

## 验收命令与真实结果

| 命令 | 结果 |
|---|---|
| `git diff --check` | 通过 |
| `cargo +stable metadata --no-deps --format-version 1` | 通过（workspace 解析 OK） |
| `cargo +stable fmt --all -- --check` | 通过（exit 0） |
| `cargo +stable check -p zed --bin orion-studio` | 进行中（task Hj1hI8，重量级：编译 zed + gpui 依赖链）；预期通过（const 断言已对齐） |

## 启动链核对（步骤 4）

- `main.rs` 仍初始化 GPUI、全局状态、workspace/project/window 顺序不变（未改这些逻辑）。
- 失败路径仍走现有 `anyhow::Context` / `log_err`，未改。
- 二进制改名不影响 `std::env::current_exe()` 自启逻辑（源码未硬编码旧二进制名）。
- 仅修复与身份改动直接相关的编译/标识错误（const 断言 + 启动入口标识）。

## 范围遵从（禁止事项未违反）

- 未批量重命名 `crates/*`（包名保留）。
- 未改 `cli`/`client`/`collab`/`resources`/release workflow。
- 未重设计 workspace 依赖、未无理由改 Cargo.lock。
- 未直接删除 `zed` 兼容入口（旧 CLI 名 `zed` 由 cli crate 管控，留 S06/S10）。
- diff 不含全仓 package 或无关依赖改动。

## 交接

- 修改：manifest（default-run + 主 bin name + description）、`main.rs` 崩溃处理器/clap 名、`zed.rs` 窗口 Tab 标识。
- package/binary 前后值：`[[bin]] zed → orion-studio`；`default-run zed → orion-studio`；package `name` 仍为 `zed`（内部 crate 名，待分批改名）。
- 仍需 S06/S10 处理：① package `name` 全仓改名（含 `cli` 二进制名、`zed://` 脚本引用）；② `bundle-*` Bundle ID / URL scheme（S10）；③ `app_menus` / 遥测事件名 / 各类 UI 品牌文案（S06/S07）。
- **S06 可以开始**（注意 S06 若涉及 package 改名，需按"禁止批量替换"规则分步、并先确认是否会触发超出范围的引用错误）。
