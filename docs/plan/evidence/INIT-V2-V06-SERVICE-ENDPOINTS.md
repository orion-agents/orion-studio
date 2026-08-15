# V2-06 — 客户端服务端点、认证与网络协议

## 阶段目标

确认并收口所有「客户端 ↔ 服务端」网络接触点：服务端点 URL、认证/登录流、网络协议头。

## 评估结论：存活端点已基本 canonical

init 提交（S08/S09/S10）已将端点迁移落地，V2-06 本次仅补充一处运行时 env 契约：

- **默认 `server_url`**：`assets/settings/default.json:2675` = `"https://orion.dev"`（canonical，已落地）。
- **`SERVER_URL` 静态**：`crates/client/src/client.rs:63` 读取顺序 = `ORION_STUDIO_SERVER_URL` → `ZED_SERVER_URL` 回退（已在代码中，canonical-first）。
- **`ZED_RPC_URL` 静态**（本次新增）：`crates/client/src/client.rs:68` 改为 `ORION_STUDIO_RPC_URL` → `ZED_RPC_URL` 回退（与 SERVER_URL 同构，canonical-first + legacy 兼容）。
- **`zed_urls.rs`**：account / docs / upgrade / trial / terms 等 URL 全部由 `server_url` 设置派生，运行时跟随 canonical 端点，无硬编码 zed 主机。
- **collab crate**：`grep zed.dev|zed-industries|collab.zed` 在 `crates/collab/src` **无命中** —— 无遗留 zed 存活端点。
- **User-Agent**：`remote_server` 的 User-Agent 已在 V2-03 收敛为 `Orion-Studio-Server`；`client` 使用 gpui http client 的 UA（非硬编码 zed 串）。

## 认证 / 登录流

- `SignIn`/`SignOut`/`Reconnect` 动作与 OAuth/token 流转全部经 `server_url`（`ClientSettings`）与 `credentials_url` 派生，品牌无关；改 server_url 即可整体重定向到 Orion 自建/自托管后端。
- `ZED_IMPERSONATE` / `ZED_WEB_LOGIN` / `ZED_ADMIN_API_TOKEN` / `ZED_APP_PATH` / `ZED_ALWAYS_ACTIVE` 为运维/调试开关，非品牌端点，按「保留 identifier 名」规则不动。

## 保留 / 延后项（不猜测、需前置条件）

- **R-V2-06-AI**：`assets/settings/default.json:1093` AI 默认 provider 键 `"zed.dev"`。这是 Orion AI 后端 provider 注册 ID，S02 未确认且 Orion AI 基建未就绪（见 S11 R-\*）。**不改**（猜 provider ID 会使默认 AI 失效），待 Orion 后端注册 `orion.dev` provider 后切换。
- **R-V2-06-LEGAL**：`legal/*.md`（terms/privacy/subprocessors）仍为 Zed Industries 法律文本。fork 需 Orion 自有法律实体与条款，属治理决策，非代码改动，**不改**，登记为治理延迟项。
- **R-V2-06-DOCS**：其余 `zed.dev/docs/*`/`zed.dev/jobs`/`zed.dev/docs/linux` 等文档/营销链接（含 `remote/src/transport.rs` 的 `remote-development`、各 `assets/settings/*.json` 注释、`crates/cli/src/main.rs:735` 文案）。S06 曾因「orion.dev 未就绪」暂缓，本次统一决定：品牌文档域 = orion.dev，全部改 `orion.dev/*`（见 V2-05 已改 DOCS_URL 等），并在 V2-09 全量扫荡。**前置条件**：orion.dev 的 docs/status/merch/jobs/remote-development 站点须在发布前上线，否则链接 404（统一登记 R-V2-09-PROVISION）。
- **主题 `$schema`**（`assets/themes/*/x.json`）：`zed.dev/schema/themes/v0.2.0.json` 为稳定公开 schema，orion.dev 未托管 → **保留**，不改（避免编辑器校验失效）。
- **`Procfile.web` / `nix/build.nix`** 的 `zed.dev` 网站仓库与 homepage/changelog：属 dev 工具链与打包元数据（V2-08 范围），将在 V2-08 处理。

## 测试和命令结果

- `cargo +stable fmt --all -- --check`：**PASS**（V2-06 仅改 client.rs 一处，已随 V2-05 fmt 一并验证）。
- `git diff --check`（client.rs）：**PASS**。
- `cargo +stable check -p client`：**BLOCKED**（依赖 gpui → Metal Toolchain，同前）。
- `cargo +stable check -p zed --bin orion-studio`：**BLOCKED**（Metal）。

## 状态

PARTIAL（client/zed 主二进制因 Metal 环境阻塞未编译验证；端点契约本身为低风险 env 读取改动，已做人工复核）。

## 注册延迟项

- R-V2-06-AI：Orion AI provider 注册（backend 未就绪）。
- R-V2-06-LEGAL：Orion 自有法律文本（治理）。
- R-V2-06-DOCS / R-V2-09-PROVISION：orion.dev 站点/文档须在发布前上线。
