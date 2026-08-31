# Orion Code Update v2 基线与 ADR

## 执行状态

| 字段 | 值 |
| --- | --- |
| Task | `UP-PRE-00` |
| Status | `PARTIAL`（等待人工审阅） |
| 记录日期 | 2026-08-31 |
| 外部动作 | `NOT PUSHED / NOT PUBLISHED / NOT SIGNED` |

## 可复现基线

### Orion Studio

- 实施工作树：独立 Orion Studio worktree（本机路径不进入仓库证据）
- 分支：`codex/orion-code-managed-agent`
- 基线 commit：`844117312f53658c4bffed6b6dd7e7117397734f`
- 相对 `origin/main`：behind `0`、ahead `12`
- 开始执行时仅有未跟踪计划文件 `docs/plan/orion-code-managed-sidecar-update-v2-plan.md`
- 用户原始工作树包含自己的分支和未跟踪计划文件；本任务不在其中实施，也不覆盖其内容。

### Orion Code

- 实施工作树：独立 Orion Code worktree（本机路径不进入仓库证据）
- 分支：`codex/orion-code-acp`
- 基线 commit：`de3ea28c66af04e4ab5f9e6829d96925e55a0e28`
- 相对 `origin/main`：behind `0`、ahead `11`
- 开始执行时工作树干净。

## 架构决策

### ADR-U01：ACP 继续作为进程边界

Studio 只通过 ACP v1 启动和使用 Orion Code。更新系统只替换被验证的 sidecar command，不让 Studio 直接导入 Orion Code TypeScript SDK，也不改变 session 数据根目录和 ACP identity。

### ADR-U02：first-party 签名索引是自动更新权威

官方 ACP Registry 继续承担生态发现和最新 Stable 展示。频道、灰度、最低 Studio 版本、暂停、撤回、回滚和索引签名由 Orion first-party update index 表达；Registry 的更高版本不能绕过该控制面。

### ADR-U03：频道与模式分离

持久化模型分别保存 `channel = stable | beta` 和 `mode = automatic | manual`。UI 可以显示 Stable、Beta、Manual 三个简化选择，但 Manual 不作为发布频道。

### ADR-U04：下载、验证、激活是三个事务

下载只能生成 partial；完整校验后才能生成 staged receipt；只有全局 idle barrier 放行后才允许切换 active。任一失败均不得修改 current verified。

### ADR-U05：Stable 只使用平台 archive

Stable sidecar 不依赖用户全局 Node/npm。macOS 首发采用 app-like bundle，逐层 Developer ID 签名、公证、staple，并对最终压缩 bytes 重新计算 SHA-256。无签名凭据时只能生成 `NOT RELEASABLE` candidate。

### ADR-U06：撤回不隐含远程终止

`paused` 只停止新下载和激活；`revoked` 禁止新 session，并在已有工作自然结束后切换到本地安全版本或 Native。普通 update index 不具备静默杀死进行中任务的能力。

### ADR-U07：release receipt 单向绑定 archive

最终 release receipt 是 archive 外部的发布产物，单向绑定最终 archive SHA-256，以及内嵌 manifest、SBOM 和 notices 摘要。archive 内的 manifest 不得反向包含最终 receipt 摘要；否则 receipt 同时绑定最终 archive 时会形成不可求解的循环摘要。Studio 验证签名索引、archive 和内嵌文件；索引生成器在发布侧另行重放验证外部 receipt。

## 第一阶段文件所有权

| 范围 | 独占写入者 | 文件 |
| --- | --- | --- |
| 更新契约、候选解析、持久化状态核心 | 主执行 Agent | `crates/agent_ui/src/orion_code_update.rs`、`crates/agent_ui/src/agent_ui.rs`、`crates/agent_ui/Cargo.toml`、workspace dependency |
| 用户设置 | Settings 子任务 | `crates/settings_content/src/agent.rs`、`crates/agent_settings/src/agent_settings.rs`、`assets/settings/default.json` |
| Orion Code producer contract | Orion Code 子任务 | `docs/architecture/orion-code-update-v1/`、golden fixtures、release scripts |
| 安全下载和版本目录 | 后续 installer 子任务 | `crates/project/src/agent_server_store.rs` 或经审计确认的新窄模块 |
| 更新 UI | 后续 UI 子任务 | `crates/agent_ui/src/agent_registry_ui.rs` |

任一边界需要调整时，先更新本表，避免并行 Agent 修改同一文件。

## 第一阶段门禁

- [x] 两仓分支、commit、ahead/behind、status 已记录。
- [x] 原始脏工作树与隔离实施工作树已区分。
- [ ] first-party index、archive、channel/mode、idle activation、rollback/revoke ADR 已由人工审阅。
- [x] 第一阶段并行文件所有权已冻结。
- [ ] `UP-CON-01` Rust/JSON 契约与 golden fixtures 一致。
- [ ] `UP-ST-01` 设置和 state-v2 迁移测试通过。
