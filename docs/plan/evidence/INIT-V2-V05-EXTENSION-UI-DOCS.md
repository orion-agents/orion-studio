# V2-05 — 扩展 API、WIT/ABI、UI 资产与文档

## 阶段目标

完成 init 品牌迁移在以下三面的收口：

1. 扩展 API（Rust crate + WIT/ABI）
2. 面向用户的 UI 文案与资源（菜单、对话框、崩溃提示、文档链接）
3. 仓库元数据与文档链接

## 范围与约束（来自 init-plan-v2.md）

- 区分 Rust crate 名 / WIT namespace / 导出 ABI / 产品名：crate 名（`zed`、`zed_extension_api`）与 WIT `package zed:extension` **保留**（ABI 兼容层），仅改产品名与文档链接。
- 保留上游归属与许可证（README 已正确署名 fork，不动）。
- 图标资源文件名（`zed_predict_onboarding` 等）：S11 已登记为 R-\* 延迟项，本次不动（纯内部资源名，改名有打包/缓存风险）。

## 改动清单（允许文件）

### 面向用户 UI 文案（crates/zed/src）

- `zed/app_menus.rs`：应用菜单名 `Zed`→`Orion Studio`；`About Zed`→`About Orion Studio`；`Zed Repository`→`Orion Studio Repository`；`Documentation` URL `zed.dev/docs`→`orion.dev/docs`；`Join the Team` URL `zed.dev/jobs`→`orion.dev/jobs`。
- `zed/main.rs`：启动失败消息 `Zed failed to launch`→`Orion Studio failed to launch`；窗口打开失败消息与 Linux 通知 `Zed failed to open a window`→`Orion Studio ...` + 文档链接 `zed.dev/docs/linux`→`orion.dev/docs/linux`。
- `zed/zed.rs`：`DOCS_URL`/`STATUS_URL`/`MERCH_URL` 全部 `zed.dev`→`orion.dev`；`About Zed` 标题栏→`About Orion Studio`；Linux/WSL 故障排查文档链接 `zed.dev/docs/{linux,windows}`→`orion.dev/docs/...`。
- `zed/mac_only_instance.rs`：单实例握手串 `Zed Editor {Dev,Nightly,Preview,Stable} Instance Running`→`Orion Studio ...`。
- `zed/move_to_applications.rs`：对话框 `Move Zed to Applications?` / `Zed is running from a temporary location...`→`Orion Studio ...`。
- `zed/reliability.rs`：GPU 遥测上下文描述 `the GPU Zed is running on`→`the GPU Orion Studio is running on`。
- `zed/quick_action_bar/repl_menu.rs`：`ZED_REPL_DOCUMENTATION` 链接 `zed.dev/docs/repl`→`orion.dev/docs/repl`（常量名保留）。
- `main.rs`：`paths_or_urls` 文档注释中 `zed.dev`→`orion.dev`（保留 `zed://` legacy scheme 说明，因运行时仍注册）。

### 技术支持端点（crates/feedback/src/feedback.rs）

- `ZED_REPO_URL`、`REQUEST_FEATURE_URL`、`file_bug_report_url` 的 GitHub 指向：`github.com/zed-industries/zed`→`github.com/orion-agents/orion-studio`（fork 已知 remote）。
- 支持邮箱 `mailto:hi@zed.dev`→`mailto:hi@orion.dev`（与已确立的 orion.dev 品牌域一致；**前置条件**：orion.dev 邮件基础设施须在发布前就绪，登记为 R-V2-05-1）。
- action doc comment 更新为 Orion Studio。

### 扩展 API（crates/extension_api、extensions/test-extension）

- `crates/extension_api/Cargo.toml`：`description`→`APIs for creating extensions for Orion Studio in Rust`；`repository`→`orion-agents/orion-studio`。
- `crates/extension_api/src/extension_api.rs`：crate 文档 `Zed ... [Zed](zed.dev)`→`Orion Studio ... [Orion Studio](orion.dev)`。
- `extensions/test-extension/extension.toml`：`repository`→`orion-agents/orion-studio`。
- **WIT namespace `zed:extension` 全部保留**（since_v0.0.1 … v0.8.0 共 11 个 `.wit`）。理由：ABI 兼容层，重命名会使既有扩展全部失效。`extension_api.rs:53` 与 `README.md:51-57` 已文档化该过渡策略。WIT 内 `/// A Zed worktree.` 等 doc 注释描述继承自上游的 API 语义，随 ABI 一并保留。

## 保留 / 延后项（不猜测）

- `app_menus.rs:313` `twitter.com/zeddotdev`（社交 handle）：Orion 官方 X/Twitter handle 未分配 → **DEFER**，登记 R-V2-05-2。保留原链接避免死链。
- `open_listener.rs` 测试 fixtures 中的 `zed://git/clone/?repo=github.com/zed-industries/zed`（5 处）：测试 `zed://` **legacy scheme 解析**，该 scheme 仍被运行时注册（V2-04），测试保持有效；repo 串为任意示例数据，不动。
- 图标资源文件名：R-\* 延迟项（S11）。
- 顶层 `README.md`：已正确以 fork 身份署名 Zed Industries（S07），非品牌残留，不动。

## 测试和命令结果

- `cargo +stable fmt --all -- --check`：**PASS**。
- `git diff --check`（V2-05 触及文件）：**PASS**。
- `cargo +stable check -p feedback`：**PASS**（44s，含全部依赖干净编译）。
- `cargo +stable check -p zed --bin orion-studio`：**BLOCKED**（Metal Toolchain，同 V2-00/02/03/04）。
- `./script/clippy` 全量：**BLOCKED**（webrtc-sys 断网）。

## 状态

PARTIAL（主二进制 `cargo check -p zed --bin orion-studio` 与全量 `./script/clippy` 因 Metal/webrtc 环境阻塞未验证；feedback crate 已 PASS）。

## 注册延迟项

- R-V2-05-1：`hi@orion.dev` 支持邮箱基础设施须在发布前就绪。
- R-V2-05-2：Orion Studio 官方 X/Twitter handle 未分配，`twitter.com/zeddotdev` 暂保留。
