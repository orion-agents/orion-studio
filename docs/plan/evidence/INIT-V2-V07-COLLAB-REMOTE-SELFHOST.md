# V2-07 — 协作、远程与自托管最小闭环

## 阶段目标

确认协作/远程/自托管三条链路的品牌接线闭合：

1. 协作邀请/登录/token 传递（经 `server_url`）。
2. 远程项目启动/连接/断开/重连。
3. 自托管 server 数据目录与身份（`.orion_server` / `~/.orion_studio_server`）。
4. Orion 客户端与旧兼容 server 的边界。

## 验证结论（设计为品牌正确）

- **端点驱动**：协作/远程的全部服务端接触点经由 `ClientSettings.server_url`（默认 `https://orion.dev`）+ `ORION_STUDIO_SERVER_URL`/`ORION_STUDIO_RPC_URL` canonical 环境变量（V2-06 已加 RPC_URL canonical-first）。改 server_url 即可整体重定向到 Orion 自建/自托管后端，无需改 RPC 代码。
- **远程目录**：`crates/paths/src/paths.rs` —— 规范 `remote_server_dir_relative()` = `.orion_server`；`remote_server_dir_relative_legacy()` = `.zed_server`（保留供 S04 迁移探测）。测试 `legacy_remote_server_dir_retained_for_migration` 锁定该契约。
- **collab 自托管**：`crates/collab/src` grep `zed.dev|zed-industries|collab.zed` **无命中**；`rpc.rs` 的 live_kit URL 来自 env 配置，无硬编码 zed 主机。
- **本地 server 数据目录**：`~/.orion_studio_server` 已在 `script/uninstall.sh` 落地（S10），与规范名一致。
- **旧兼容 server 边界**：`zed://` scheme 仍运行时注册（V2-04），旧 `.zed_server` 目录仍被探测用于迁移；新客户端可兼容旧 server 直到迁移完成。

## 本次改动（文档准确性）

`docs/src/remote-development.md` 过去混用 Orion 链接与旧 Zed 文案/路径/二进制名，与 V2-03 改后的 `orion-studio-remote-server-*` 二进制名和 `.orion_server` 目录不符。统一修正：

- 产品名 `Zed`→`Orion Studio`（标题、正文、CLI 调用示例）。
- 远程 server 目录 `~/.zed_server`→`~/.orion_server`；手动上传路径 `zed-remote-server-*`→`orion-studio-remote-server-*`。
- 本地/项目设置路径 `~/.zed`、`~/.config/zed`、`.zed/settings.json`→`~/.orion-studio`、`~/.config/orion-studio`、`.orion-studio/settings.json`。
- 自定义 scheme `zed://ssh/`→`orion://ssh/`（canonical scheme，与 `register_zed_scheme` 注册的 `orion`/`zed` 一致）；CLI 命令 `zed`→`orion-studio`。
- `zed.dev`→`orion.dev`。
- 保留（非品牌）：`{#kb zed::OpenSettings}` 动作命名空间（内部 action 路径）、`~/code/zed/zed` 示例工程路径、`configuring-zed.md` 文档文件名（改名会破坏多处交叉链接，登记为 V2-09 文档资产复查项）。

## 测试和命令结果

- 文档改动无编译影响（mdbook 构建不在本沙箱执行）。
- `cargo +stable check -p paths`（远程目录契约测试所在 crate）：**BLOCKED**（依赖 gpui → Metal Toolchain，同前；`paths.rs` 测试逻辑已在 V2-01/02 复跑通过并保留）。
- `cargo +stable check -p collab`：**BLOCKED**（Metal）。

## 状态

PARTIAL（代码契约经设计验证 + 文档修正；运行时端到端闭环因沙箱无构建/网络无法实跑）。

## 注册延迟项

- R-V2-07-E2E：mock / loopback / 真实云端三种端到端证明需在联网 + Metal 的 CI 或真实自托管环境复跑（环境阻塞）。
- R-V2-09-DOCSFILES：`configuring-zed.md` 等文档文件名含 `zed`，改名需全量交叉链接复查（与 S10 的「桌面文件名不改名」原则一致，暂保留）。
