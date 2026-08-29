# Orion Code ACP 托管 Sidecar 与默认 Agent 开发计划

## 0. 计划信息

| 项目 | 内容 |
| --- | --- |
| 计划状态 | `PREVIEW_IMPLEMENTED / REAL_APP_AND_STABLE_RELEASE_GATED` |
| 编写日期 | 2026-08-29 |
| 实施开始 | 2026-08-29 |
| 最后验证 | 2026-08-30 |
| 主目标 | 通过 ACP v1 将 Orion Code 接入 Orion Studio，由 Studio 私有托管 sidecar，并在全新安装中默认启用 |
| Orion Studio 仓库 | `/Users/hope/ai-project/orion-studio` |
| Orion Code 仓库 | `/Users/hope/ai-project/orion-code` |
| Studio 实施分支 | `codex/orion-code-managed-agent`，base `27de074ad6c40c407aa7550db42c22c4f22c33ff` |
| Code 实施分支 | `codex/orion-code-acp`，base `81bf78a575112832a6dba405b5c53c6bf8fa758e` |
| 协议边界 | `Orion Studio -> ACP JSON-RPC/stdin/stdout -> Orion Code` |
| Preview 分发 | Studio 私有目录中的 Registry npx 安装 |
| Stable 分发 | 带版本、SHA-256 和平台签名的 Registry archive |
| SDK 状态 | 后续演进项，不是当前 ACP 开发前置条件 |

> [!IMPORTANT]
> 本文只授权按任务卡实施代码、测试和本地集成。它不授权 npm publish、GitHub Release、ACP Registry 提交、签名、公证、push、PR 或合并；这些外部动作必须由用户另行明确授权。

> [!NOTE]
> 当前已完成 Preview 代码实现、相关自动化门禁和本地 npm tarball 隔离安装验证。真实 Orion Studio App 的 clean-home 端到端验收，以及 Stable archive、签名、公证和公开发布仍是独立门禁；因此本文不能标记为完整 Stable 发布完成。

> [!IMPORTANT]
> 编写本计划时两个仓库都存在用户未提交工作。执行者不得使用 `git clean`、`git reset --hard`、`git checkout --` 或其他覆盖、删除现有改动的命令。实施前必须使用经人工确认的基线创建隔离分支或 worktree。

## 1. 最终产品定义

安装 Orion Studio 后，全新用户应获得以下体验：

1. 首次引导明确展示 `Orion Code — Recommended`，默认选中，但在下载可执行代码前显示来源、用途和预计下载量。
2. 用户继续引导即表示允许 Studio 将 Orion Code 安装到自身私有数据目录；不得修改全局 npm、系统 PATH 或管理员目录。
3. 安装完成后必须先校验版本、完整性并完成 ACP `initialize` 健康检查，成功后才能将 Orion Code 设为默认 Agent。
4. 用户创建第一个 Agent Thread 时，实际后端是 Orion Code；Agent Panel 保持 Orion Studio 原有 UI 和交互。
5. 下载、安装、启动、握手或配置失败时，自动回退内置 `Orion Agent`，同时提供可见错误、重试和诊断入口。
6. 老用户保留恢复线程、Workspace 选择和全局最近使用 Agent，不被自动覆盖。
7. 用户主动移除或拒绝 Orion Code 后，后续启动不得自动重装；只有用户主动重新启用时才安装。
8. 更新失败时继续使用上一份已验证 sidecar，不留下半安装目录，也不阻断 Orion Studio 启动。

最终架构：

```text
Orion Studio Agent Panel
        |
        | ACP v1 / JSON-RPC / stdin + stdout
        v
Orion Code ACP Adapter (`orion-code` bin / `orion acp` alias)
        |
        | 进程内 TypeScript 调用
        v
现有 Orion Code Runtime
        |
        +-- Models / Tools / Skills / MCP
        +-- Sessions / Replay / Permissions
        +-- 后续可替换为 Orion Code SDK，ACP 契约不变
```

## 2. 已确认的架构决策

| ID | 决策 | 约束理由 |
| --- | --- | --- |
| D-01 | 使用 ACP v1 作为 Studio 与 Code 的唯一集成协议 | ACP v1 是当前稳定协议；v2 仍是 Draft |
| D-02 | Orion Studio 不直接加载 TypeScript SDK | Studio 是 Rust/GPUI；ACP 子进程边界可独立更新并隔离故障 |
| D-03 | 当前先实现 `orion acp`，不等待 SDK 抽取 | 现有 `createOrionRuntime()` 和版本化 runtime protocol 已可作为内部接口缝 |
| D-04 | ACP adapter 只做协议映射，不承载 Agent 业务逻辑 | 后续抽取 SDK 时只替换 adapter 下层调用，不影响 Studio |
| D-05 | 禁止安装时执行全局 `npm install -g` | 避免 PATH、权限、Node 版本、卸载和供应链漂移问题 |
| D-06 | Preview 复用 Studio 的 Registry npx 私有安装 | 改动最少，可先验证协议和产品体验 |
| D-07 | Stable 使用按平台 archive，并要求 SHA-256 | 版本确定、更新可回滚，不依赖用户系统 Node 或 npm |
| D-08 | 只在全新安装且 sidecar 已就绪时应用产品默认 | 不覆盖老用户选择，不制造“选中但不可用”状态 |
| D-09 | 保留内置 `Orion Agent` 作为始终可用的 fallback | 外部安装、网络、模型配置和 sidecar 都可能失败 |
| D-10 | 不修改 `Agent::default()` 的全局语义 | 避免破坏协作 Workspace、未安装场景和历史序列化状态 |
| D-11 | 默认 Agent 与默认模型分离 | 外部 Orion Code 的模型、认证和配置由 Orion Code/ACP session options 管理 |
| D-12 | Stable 前必须分离共享配置与可变运行数据 | 避免 Orion Code CLI 与 Studio sidecar 并发访问同一会话/数据库目录 |
| D-13 | npm 包提供独立 `orion-code` ACP bin，`orion acp` 只作为人工兼容别名 | Studio 当前按 npm 包的非 scope 名选择 bin；独立入口可在 CLI 初始化前保证 stdout 纯净 |
| D-14 | ACP adapter 依赖一个窄的 runtime port，而不是散落调用内部模块 | 当前 runtime 先实现该 port；未来 SDK 只替换 port 实现，不改变 ACP 或 Studio |
| D-15 | 新 sidecar 先验证再晋升，且始终保留上一份 verified 版本 | 避免下载成功但握手失败后，旧版已被清理而无法回滚 |

官方参考：

- [ACP Introduction](https://agentclientprotocol.com/get-started/introduction)
- [ACP v1 Initialization](https://agentclientprotocol.com/protocol/v1/initialization)
- [ACP v1 Session Setup](https://agentclientprotocol.com/protocol/v1/session-setup)
- [ACP v1 Prompt Turn](https://agentclientprotocol.com/protocol/v1/prompt-turn)
- [ACP Session Close](https://agentclientprotocol.com/rfds/session-close)
- [ACP TypeScript SDK](https://agentclientprotocol.com/libraries/typescript)
- [ACP Registry](https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json)

实现时使用 TypeScript SDK 当前推荐的 fluent `agent()` API，不得新写基于已弃用 `AgentSideConnection` 的实现。依赖版本必须在任务开始时重新查询并以精确版本写入 lockfile；计划编写时观察到的 npm `latest` 仅是基线，不得当成永久版本。

## 3. 当前代码基线与缺口

### 3.1 Orion Studio 已有能力

- `crates/zed/src/main.rs:704` 已能判断 `is_new_install`。
- `crates/zed/src/main.rs:787-799` 已初始化 `AgentRegistryStore` 和 Agent UI。
- `crates/project/src/agent_registry_store.rs:38-57` 已支持 Registry binary 与 npx 两类 Agent。
- `crates/project/src/agent_server_store.rs:1213-1325` 已支持版本化 archive 缓存、SHA-256 下载校验和相对命令启动。
- `crates/project/src/agent_server_store.rs:1387-1423` 已支持在 Studio 私有目录中运行 npm install，并使用 Studio 托管 Node。
- `crates/paths/src/paths.rs:460-465` 将外部 Agent 放在 Studio 的 `external_agents` 数据目录。
- `crates/onboarding/src/basics_page.rs:542-600` 已有 Featured Agent 安装和选择入口。
- `crates/agent_ui/src/agent_panel.rs:1430-1455` 已实现恢复线程、Workspace 与全局最近使用 Agent 的优先级。
- `crates/agent_ui/src/agent_panel.rs:1670-1686` 已能在外部 Agent 不存在时回退内置 Agent。

### 3.2 Orion Studio 当前缺口

- Featured Agent 列表中没有 `orion-code`。
- 首次安装标志尚未驱动 Orion Code 安装状态机。
- 当前默认仍是 `Agent::NativeAgent`，没有“产品首选 Agent”回退层。
- Registry 安装按钮派发 `SelectAgent`，但非空白草稿状态下不保证立即持久化为默认。
- npx Registry 版本被转换为 `0.0.0 - <registry-version>` 上限范围，不是不可变精确 pin；实际解析版本可能更旧。
- 首次下载失败、用户拒绝、用户卸载、安装中断和更新回滚尚未形成 Orion Code 专用产品状态。
- 当前 Registry URL 是外部官方列表；Stable 默认 Agent 不应在无缓存首次启动时只依赖远程列表才能知道自身描述符。

### 3.3 Orion Code 已有能力

- `src/index.ts:1-7` 已声明一个受支持的产品 runtime 公共边界。
- `src/index.ts:62-107` 已公开 `createOrionRuntime()`、dispatch、interrupt、replay 和 close。
- `src/runtime/product-bootstrap.ts` 已集中产品 runtime 启动、session 与 runner 组装，可作为 ACP runtime port 的首个实现入口。
- `src/runtime/thread-runtime.ts` 已有 dispatch、订阅和 replay 事实源；`src/runtime/thread-ui-adapter.ts` 已有 UI 事件归一化逻辑。
- `src/runtime/orion-session-runner.ts` 与 `src/runtime/session-storage.ts` 已覆盖执行和持久 session 的核心路径。
- `src/runtime/protocol/runtime-protocol-v1.ts:69-144` 已定义 initialize、thread、turn 和 approval 命令。
- runtime 已产生文本、工具、权限、计划、状态、session 和子任务相关事件，可作为 ACP update 映射来源。
- `ORION_CODE_DISABLE_ENV_FILES=1` 已可禁止 CLI launcher 自动读取全局与项目 `.env`。
- Node 支持范围包含 Studio 托管的 Node 24。

### 3.4 Orion Code 当前缺口

- `src/cli.ts` 没有 `acp` 子命令。
- `package.json` 和 lockfile 中没有 `@agentclientprotocol/sdk`。
- 当前 npm 包根是 CommonJS，而官方 ACP TypeScript SDK 是 ESM；不能为了 ACP 改写整个包的模块格式。
- 没有 JSON-RPC/stdin/stdout ACP server、session manager 或 capability 声明。
- 当前公共 runtime 门面主要提供 replay，没有面向 adapter 的实时订阅接口。
- 当前配置、usage、history、projects、sessions 和数据库集中在一个 config root，CLI 与 sidecar 并发风险没有完整产品验证。
- 当前 npm 包只提供 `orion` CLI，没有独立 sidecar archive、平台签名清单或 ACP 兼容收据。

### 3.5 编写计划时的工作树约束

- Orion Studio 当前位于 `ui/orion-pixel-atelier`，相对 `origin/main` ahead，且已有用户未跟踪计划文档。
- Orion Code 当前位于 `v0.3.1`，存在多项用户正在进行的 Web、session 和测试修改。
- 上述状态只用于说明不能直接在当前工作树实施。每个执行任务都必须重新记录 branch、HEAD、ahead/behind、status 和目标基线。

## 4. 范围

### 4.1 本计划包含

- Orion Code 的独立 `orion-code` npm bin、`orion acp` 兼容入口与 ACP v1 server。
- ACP session、prompt、stream、tool call、permission、cancel、load/replay 和 shutdown 映射。
- Orion Studio 的 Orion Code 描述符、私有安装、更新、卸载、健康检查和诊断。
- 全新安装默认启用、老用户选择保留和 Native Agent fallback。
- npx Preview、平台 archive Stable、SHA、签名和发布门禁。
- 两仓单元测试、进程测试、集成测试、干净用户目录与真实 App 验收。
- 后续 SDK 抽取所需的最小内部接口缝。

### 4.2 本计划不包含

- 立即将 Orion Code 抽成独立 SDK 仓库或 npm workspace。
- 将 Orion Code 业务逻辑重写为 Rust 或合并进 Orion Studio 进程。
- 用 `orion web`、SSE、WebSocket 或 `orion -p` 代替 ACP。
- ACP v2 Draft、远程 ACP、Cloud Agent 或协作 Workspace 外部 Agent 支持。
- 将完整开发 `node_modules` 复制进 `.app`。
- 静默下载、静默执行或绕过用户权限确认。
- 自动迁移、删除或合并用户现有 `~/.orion-code` 数据。
- Apple Developer ID、Notary、npm、GitHub Release 或 Registry 的实际发布操作。

## 5. 目标契约

### 5.1 稳定身份

| 字段 | 目标值 |
| --- | --- |
| Registry/Agent ID | `orion-code` |
| 用户显示名 | `Orion Code` |
| npm Registry 启动入口 | package bin `orion-code` -> `bin/orion-code-acp` |
| 人工/兼容 CLI 入口 | `orion acp`，调用同一 server main |
| Stable archive 入口 | `./bin/orion-code-acp` |
| ACP implementation name | `orion-code` |
| ACP implementation title | `Orion Code` |
| 默认协议 | ACP v1 |
| Studio fallback | `Orion Agent` / `NativeAgent` |

这些标识一旦进入 Preview 不得随意改变；确需改变时必须提供 alias、迁移和兼容测试。

### 5.2 ACP 最低能力

ACP 官方基线要求 Agent 支持 `session/new`、`session/prompt`、`session/cancel` 和 `session/update`。Orion Code 要成为默认 Agent，还必须支持会话重启，因此本项目的最低门槛更高：

| ACP 能力 | Orion Code 映射 | Preview | Stable |
| --- | --- | --- | --- |
| `initialize` | 返回 protocol、agentInfo、capabilities、auth/config 状态 | 必须 | 必须 |
| `session/new` | 创建唯一 session ID 和一个 runtime owner | 必须 | 必须 |
| `session/prompt` | 内容块归一化后 dispatch `turn.start` | 必须 | 必须 |
| `session/update` | runtime 事件映射为文本、计划、工具和状态更新 | 必须 | 必须 |
| `session/cancel` | 调用 runtime interrupt/AbortSignal，等待可观测停止 | 必须 | 必须 |
| Permission request | 映射 Orion Code permission request/response | 必须 | 必须 |
| Tool call/update | 映射工具标题、状态、输入摘要、结果和 diff | 必须 | 必须 |
| `session/load` | 打开持久 session 并按顺序 replay 历史 | Beta 前 | 必须 |
| `session/close` | cancel 后释放 runtime 与订阅 | 可选 | 必须 |
| Session modes/config | 映射 build/plan/auto/goal 与模型选项 | 可选 | 必须 |
| `session/list/delete/resume` | 只有 Studio 实际消费且测试完成后才声明 | 不声明 | 按需 |
| additional directories | 未完成多根目录 containment 前不得声明 | 不声明 | 按需 |

能力声明必须反映真实实现；不支持的能力应省略，而不是声明后静默忽略。

首个可用版本还必须遵守以下内容与认证边界：

- `session/prompt` 至少接受 ACP Text 与 Resource Link；Image、Audio 和 Embedded Resource 在完成真实映射与测试前返回明确的 unsupported 错误。
- 若 Orion Code 继续使用自身已有 provider 配置，`initialize` 的认证方法保持为空；不得制造一个 Studio 实际无法完成的假认证流程。
- 非空 `mcpServers` 不得静默忽略。只有完成逐 session 建连、关闭、stdio command/path/env 校验后才能接受；否则请求失败且 capability 保持未声明。
- `session/prompt` 只有在对应 turn 到达 durable completed、cancelled 或 failed 终态后才返回 stop reason，不能在 dispatch 成功时提前返回。

### 5.3 ACP adapter 内部 runtime port

先在 Orion Code 内部冻结以下职责边界；具体类型名可在 CON-01 中调整，但不得扩大职责：

```ts
interface OrionAcpRuntimePort {
  createSession(input: CreateSessionInput): Promise<SessionDescriptor>;
  loadSession(input: LoadSessionInput): Promise<LoadedSession>;
  prompt(input: PromptInput, observer: RuntimeObserver): Promise<PromptStop>;
  cancel(sessionId: string): Promise<void>;
  closeSession(sessionId: string): Promise<void>;
  close(): Promise<void>;
}
```

- 第一版由现有 product runtime、thread runtime、session runner 和 session storage 组合实现。
- ACP handler 只能依赖此 port 与 ACP 类型，不得直接依赖 TUI/Web view model。
- 后续 Orion Code SDK 只需提供另一个 port 实现；Studio 的 ACP client、Agent ID、session 持久化语义和分发描述符不变。
- port 的 `prompt` observer 接收有序 runtime 事件；ACP mapper 负责差量、ID 和协议形状，runtime 不反向依赖 ACP。

### 5.4 默认 Agent 选择算法

```text
1. 当前恢复线程绑定且可用的 Agent
2. 当前 Workspace 已保存且可用的 selected_agent
3. 全局最近使用且可用的 Agent
4. 仅全新安装：已同意、已安装、版本兼容、ACP 健康检查通过的 Orion Code
5. NativeAgent
```

附加规则：

- 协作 Workspace 继续强制 `NativeAgent`。
- `Agent::default()` 继续是 `NativeAgent`。
- 安装成功不等于默认成功；必须在健康检查通过后以一个原子状态更新完成。
- 用户主动选择其他 Agent 后，后续 Workspace 必须遵循现有持久化优先级。
- 用户移除 Orion Code 后，所有新的空白 Workspace 回退 Native；历史 Orion Code thread 可保留绑定信息，以便以后重装恢复。

### 5.5 安装状态机

```text
not_offered
  -> offered
      -> declined
      -> installing
          -> verifying
              -> ready
              -> failed
          -> failed

ready -> updating -> ready
ready -> removed
failed -> retrying -> installing
declined/removed -> 仅用户主动操作 -> installing
```

每个状态都必须持久化足以防止重复下载，但不得把临时失败永久解释为用户拒绝。

持久化采用 Studio `KeyValueStore` 的独立 namespace `orion-code-bootstrap`、key `state-v1`，不要把产品状态塞进 `agent_servers` entry。最小 durable record：

```json
{
  "schemaVersion": 1,
  "choice": "accepted",
  "source": "archive",
  "lastAttemptedVersion": "0.3.2",
  "lastVerifiedVersion": "0.3.1",
  "lastErrorKind": null
}
```

- `choice` 枚举只能是 `not_asked`、`accepted`、`declined`、`removed`；`source` 只能是 `npx`、`archive` 或 `null`。
- `installing`、`verifying`、`updating` 是内存中的瞬态；重启发现 accepted 但没有 verified receipt 时进入可重试失败态，不无限自动下载。
- `declined` 与 `removed` 必须 durable；删除 `agent_servers.orion-code` 不能同时删除这个选择记录。
- 关键 KVP 写入必须 await 并把错误反馈到 UI；不得只调用 fire-and-forget 的日志写入后就改变默认 Agent。
- record 不保存 token、完整路径、prompt、配置内容或临时 staging 路径。

## 6. 运行时、数据与安全边界

### 6.1 进程与输出

- ACP 模式必须在 CLI banner、dotenv 诊断和普通日志之前分流。
- stdout 只能写 ACP JSON-RPC 帧及分隔换行；任何 banner、debug、warning、stack trace 都写 stderr。
- EOF、SIGINT、SIGTERM、parent exit 和 Studio 主动断开都必须触发 runtime close。
- close 必须幂等；重复 cancel/close 不得 panic 或写损坏 session。
- 未捕获异常必须写脱敏 stderr、结束受影响请求，并以非零退出码结束；不得向 stdout 写裸错误。

### 6.2 工作目录与工具权限

- `cwd` 必须是绝对路径，解析后作为项目 containment 根。
- 相对路径、符号链接和额外目录必须遵循 Orion Code 现有 workspace containment。
- Preview 不声明 additional directories；Stable 如需声明，必须先补多根 containment 测试。
- 写文件、执行命令和其他高风险工具沿用 Orion Code policy，但 `ask` 决策必须通过 ACP permission request 回到 Studio UI。
- ACP 连接断开、permission request 超时或无法解析时默认拒绝，不得默认允许。
- adapter 只报告真实 sandbox/enforcement 状态，不得把“有确认框”描述成“已沙箱化”。

### 6.3 环境变量

- Studio 启动 sidecar 时设置 `ORION_CODE_DISABLE_ENV_FILES=1`。
- 不自动读取项目 `.env`、包内 `.env` 或 `~/.orion-code.env`。
- 模型密钥和其他敏感值只通过明确的 Orion Code 配置/认证流程或 Studio Agent 设置 env 传入。
- 日志、telemetry、安装 receipt 和测试 fixture 不得包含密钥、完整 prompt、用户文件内容或个人目录列表。

### 6.4 配置与运行数据

Stable 前必须形成两个逻辑根：

```text
Orion Code user config
  - 模型/provider/MCP/用户设置
  - 默认仍由 Orion Code 所有

Orion Studio managed Orion Code data
  - sidecar sessions/cache/logs/receipts
  - 位于 Orion Studio Application Support 下
```

实施建议是在 Orion Code 增加独立的 `ORION_CODE_DATA_DIR`，而保留 `ORION_CODE_CONFIG_DIR` 的现有语义。Studio 只设置 data dir，不隐式复制用户配置。CLI 与 Studio 同时运行、同一项目多窗口和异常退出必须有并发测试；在该门禁通过前不得宣称 session 可安全共享。

除数据库自身 file lock 外，每个可写 session 还必须有跨进程生命周期租约：

- 租约至少记录 PID、进程启动时间、随机 owner token、sidecar version 和 session ID，并以原子创建/替换方式写入 managed data dir。
- 检测到仍存活且 token 匹配的 owner 时返回明确 `busy`，不得同时打开同一可写 session。
- 只有确认 owner 进程已不存在、重新取得锁且 durable state 可恢复时才能回收 stale lease；不得仅凭 PID 或文件年龄删除锁。
- cancel、close、EOF 与异常退出路径都要验证 lease 释放或可恢复；只读 replay 不应冒充可写 owner。

## 7. 分发策略

### 7.1 Preview：Registry npx

目标 Registry 描述：

```json
{
  "id": "orion-code",
  "name": "Orion Code",
  "version": "${ORION_CODE_VERSION}",
  "distribution": {
    "npx": {
      "package": "@orion-agents/orion-code@${ORION_CODE_VERSION}",
      "args": [],
      "env": {
        "ORION_CODE_DISABLE_ENV_FILES": "1"
      }
    }
  }
}
```

约束：

- Registry 实际提交是外部发布动作，必须单独授权。
- Orion Code `package.json` 必须同时保留 `orion` 普通 CLI bin，并增加与非 scope 包名一致的 `orion-code` bin 指向专用 `bin/orion-code-acp`。Studio 当前在多 bin 包中按非 scope 包名选择可执行文件，因此 Registry 不应再依赖 `args: ["acp"]` 绕过普通 CLI 初始化。
- 专用 ACP launcher 在加载 dotenv、banner、TUI/Web 或普通 CLI parser 前直接进入 server main；`orion acp` 只是复用同一 main 的人工调试别名。
- Studio 当前会把精确 semver 转换为上限范围；Preview 每次启动必须通过 `agentInfo.version` 或等价健康信息记录实际解析版本。
- 解析版本与 Registry args、能力或最低兼容版本不匹配时不得设为默认。
- npx 只用于 Dev/Preview 验证；Stable 默认启用不依赖该路径。
- 安装目录只能是 Studio `external_agents/registry/npx/orion-code`。

### 7.2 Stable：签名平台 archive

建议 archive 结构：

```text
orion-code-acp/
  manifest.json
  LICENSE
  THIRD_PARTY_NOTICES
  bin/orion-code-acp
  runtime/node
  app/dist/acp/
  app/node_modules/production-only/
```

`manifest.json` 至少包含：

- Orion Code version、git SHA、构建时间和目标平台。
- ACP protocol version 与最小 Studio 兼容版本。
- Node version、ABI 和所有 native module 清单。
- archive 内文件 SHA-256 清单。
- license/SBOM 位置。

平台矩阵：

- `darwin-aarch64`
- `darwin-x86_64`
- `linux-aarch64`
- `linux-x86_64`
- `windows-x86_64`
- Windows arm64 只有在真实构建与 E2E 具备后才声明。

首个 Stable 发布门禁以 `darwin-aarch64` 为必达目标，其余平台按“有构建、有 ACP smoke、有安装测试、有签名证据才启用”的原则逐项开放。缺少某个平台证据时只禁用该 target，不阻塞已经满足全部门禁的平台，也不得在 descriptor 中谎报支持。

macOS 必须对内到外签名 Node、`.node`、`.dylib`、PTY helper 和 launcher，再生成 archive 与最终 SHA-256。任何签名、公证或发布凭据缺失都必须记录为 release `BLOCKED`，不能用 ad-hoc 签名替代公开发布证据。

下载到 App 之外的 sidecar 是独立分发物，不能借用 Orion Studio.app 的签名结论；其 launcher、嵌入 Node 与 native modules 必须单独完成签名、公证和 Gatekeeper 验证。把 sidecar 直接嵌入 App bundle 属于另一个发布方案，本计划不在实施中临时切换。

### 7.3 First-party 描述符

Stable 前，Orion Studio 应随 App 携带一份只包含 `orion-code` ID、兼容范围和受信下载元数据的 first-party descriptor，远程 ACP Registry 用于发现更新和生态展示。目的：

- 首次启动不必先下载整个远程 Registry 才知道产品默认 Agent 是谁。
- 远程 Registry 暂时不可用时仍可显示准确状态和 fallback。
- descriptor 不代表 sidecar 已安装，也不得绕过下载、SHA 和用户同意。

## 8. 执行总览与依赖

| 任务 | 仓库 | 依赖 | 产物 |
| --- | --- | --- | --- |
| PRE-00 | 两仓 | 无 | 隔离基线与执行记录 |
| CON-01 | Orion Code | PRE-00 | ACP v1 映射契约与 golden fixtures |
| OC-01 | Orion Code | CON-01 | 独立 `orion-code` bin、`orion acp` alias 与 SDK 依赖 |
| OC-02 | Orion Code | OC-01 | ACP transport/lifecycle |
| OC-03 | Orion Code | OC-02 | session/new、prompt、cancel、close |
| OC-04 | Orion Code | OC-03 | runtime event/update/tool mapping |
| OC-05 | Orion Code | OC-03 | permission 与 MCP/session config 映射 |
| OC-06 | Orion Code | OC-04 | load/replay 与持久化恢复 |
| OC-07 | Orion Code | OC-03 | config/data/env 隔离 |
| OC-08 | Orion Code | OC-04..07 | npx Preview package 与测试收据 |
| OS-01 | Orion Studio | CON-01 | Orion Code descriptor 与 Registry 展示 |
| OS-02 | Orion Studio | OS-01 | 私有安装/健康/更新/卸载状态 |
| OS-03 | Orion Studio | OS-02 | 首次引导同意与进度 UI |
| OS-04 | Orion Studio | OS-02 | 默认 Agent 优先级与持久化 |
| OS-05 | Orion Studio | OS-02 | 诊断、错误、重试与 fallback UI |
| OS-06 | Orion Studio | OS-03..05 | Rust/GPUI 自动化测试 |
| INT-01 | 两仓 | OC-06、OS-02 | local custom ACP 联调 |
| INT-02 | 两仓 | OC-08、OS-06 | clean-home npx Preview 联调 |
| REL-01 | Orion Code | INT-02 | 平台 archive 构建与签名流水线 |
| INT-03 | 两仓 | REL-01 | archive 安装、更新、回滚联调 |
| INT-04 | Orion Studio | INT-03 | 全新安装默认体验真实 App 验收 |
| REL-02 | 两仓 | INT-04 | 发布候选与 GO/NO-GO 收据 |

可并行边界：

- CON-01 冻结后，OC-01/OC-02 与 OS-01 可以并行。
- OC-03 完成后，OC-04、OC-05、OC-07 可以并行，但不得修改相同 adapter/session manager 文件。
- OS-02 的安装状态接口冻结后，OS-03、OS-04、OS-05 可以分文件并行。
- 同一台 16 GB macOS 主机只允许一个重型 Rust build lane；使用 `CARGO_BUILD_JOBS=2`，不得与 sidecar 多平台打包并发。

## 9. 详细任务卡

### PRE-00：隔离基线与执行保护

**目标：** 在不接触用户当前脏工作树的前提下建立两仓开发基线。

**允许操作：** 只读 Git 检查；经人工确认后创建新分支/worktree。

**步骤：**

1. 在两个仓库分别记录 `git status --short --branch`、HEAD、remote、ahead/behind。
2. 列出所有 modified、staged、untracked 文件，标记其所有权为 `USER/WIP`。
3. 请求人工确认 Studio 和 Code 的目标 base ref；不得由执行模型根据分支名自行猜测。
4. 在独立 worktree 创建实现分支。建议名称：
   - Studio：`codex/orion-code-managed-agent`
   - Code：`codex/orion-code-acp`
5. 在每个 worktree 再次确认 clean status；若不是 clean，停止。

**证据：** 两仓 baseline SHA、branch、worktree 路径、status 和人工确认的 base ref。

**BLOCKED：** base ref 未确认、worktree 不干净、目标文件与用户改动重叠且不能安全隔离。

### CON-01：冻结 ACP v1 映射契约

**仓库：** Orion Code 为主；Studio 只消费 fixture。

**允许文件：** Orion Code 新增 `docs/architecture/orion-code-acp-v1.md`、`tests/fixtures/acp-v1/`；不改 runtime 业务实现。

**步骤：**

1. 重新查询 `@agentclientprotocol/sdk` stable 版本并记录来源。
2. 固定使用 ACP protocol v1；列出实际声明与明确不声明的 capabilities。
3. 定义 ACP method 到 Orion runtime command/event 的一对一映射。
4. 定义文本 chunk、message ID、tool call ID、permission request ID 和 session ID 的稳定生成规则。
5. 明确 runtime 文本事件是累计值还是 delta、turn durable terminal event 与 ACP stop reason 的一对一规则。
6. 定义错误码：无效 cwd、未知 session、busy、unsupported content、cancelled、permission denied、runtime failed、configuration missing。
7. 定义跨进程 session lease、active owner、stale recovery 和 load 后所有权语义。
8. 生成不含敏感内容的 JSONL golden transcript：initialize、新 session、prompt、工具、允许/拒绝、cancel、load、close。
9. 明确 stdout/stderr 和 EOF/signal 行为。

**完成标准：** Studio 和 Code 维护者均可仅凭契约实现各自测试替身；无未解释 capability。

**BLOCKED：** Studio 当前 ACP v1 类型与官方 SDK v1 无法表达同一能力，或需要启用 v2 Draft 才能完成核心路径。

### OC-01：增加独立 ACP bin 与 `orion acp` 入口

**允许文件：** `package.json`、`npm-shrinkwrap.json`、相关 TypeScript build 配置、`src/cli.ts`、`bin/orion`、新 `bin/orion-code-acp`、新 `src/acp/`、对应测试。

**禁止：** 修改 TUI/Web 视觉层；抽取独立 SDK；全局安装依赖。

**步骤：**

1. 使用精确版本加入 `@agentclientprotocol/sdk`，更新 shrinkwrap。
2. 在 `package.json.bin` 保留 `orion -> bin/orion`，新增 `orion-code -> bin/orion-code-acp`；用测试锁定 Studio `read_package_executable` 所需的命名契约。
3. `bin/orion-code-acp` 不经过普通 CLI 的 dotenv、banner 或 UI bootstrap，直接加载 ACP server main；`orion acp` 在任何普通输出前分流到同一 main。
4. 使用官方 fluent `agent()` API；不得使用已弃用 connection classes。
5. 将 ACP 实现隔离为 NodeNext/ESM 输出，例如 `.mjs` 或带局部 `type: module` 的 `dist/acp`；不得把整个现有 CommonJS 包一次性改成 ESM。
6. `orion acp --help` 和单独的版本探针必须可测试，但 Registry 启动的纯 `orion-code` 不输出非协议文本。

**验证：**

```bash
npm run build:server
npm test -- --runInBand tests/acp-cli-routing.test.ts
```

**完成标准：** `orion-code` bin 与 `orion acp` 进入同一独立代码路径；现有 CLI help/版本/TUI/Web 测试不回归；根包模块格式不变。

### OC-02：实现 ACP transport 与进程生命周期

**允许文件：** `src/acp/server.ts`、`src/acp/transport.ts`、`src/acp/logging.ts`、`src/acp/runtime-port.ts`、测试。

**步骤：**

1. 建立 stdin/stdout JSON-RPC transport。
2. 注册 initialize handler，返回 v1、`agentInfo` 与最小 capabilities。
3. stderr 使用结构化、脱敏日志；stdout 写入必须集中在 transport writer。
4. 处理 malformed frame、unknown method、EOF、SIGINT、SIGTERM 和 writer failure。
5. 建立 server shutdown coordinator，保证所有 runtime 最多关闭一次。
6. 固定 `OrionAcpRuntimePort` 接口；handler 不得直接 import TUI/Web 或具体 session storage 实现。
7. 在没有独立认证流程时返回空 auth methods；只声明已通过 handler 与 transcript 测试的 capability。

**验证：**

```bash
npm test -- --runInBand tests/acp-transport.test.ts tests/acp-lifecycle.test.ts
```

**完成标准：** golden initialize transcript 通过；向 stderr 写日志不会污染 stdout；关闭测试无悬挂 handle。

### OC-03：实现 session/new、prompt、cancel 与 close

**允许文件：** `src/acp/session-manager.ts`、`src/acp/runtime-adapter.ts`、必要的公共 runtime 门面、小范围测试。

**步骤：**

1. 一个 ACP session 对应一个明确 owner 的 Orion runtime；使用 map 管理生命周期。
2. `session/new` 要求 cwd 为绝对、存在且为目录，执行 realpath/canonicalize 后作为 containment 根；生成稳定且不冲突的 session ID。
3. 为可写 session 获取带 PID、进程启动时间、随机 token 和 sidecar version 的跨进程 lease；活跃 owner 返回 `busy`，不得抢占。
4. 将 ACP Text 与 Resource Link content blocks 归一化为 runtime `turn.start` 输入；Image、Audio、Embedded Resource 等未实现 block 返回明确错误。
5. `prompt` 注册 observer 后再 dispatch，持续输出 update，并等待 durable terminal event 后才返回 stop reason，避免漏掉同步首事件或提前结束。
6. 处理同 session busy、follow-up 和 steer 行为，不得丢弃输入。
7. `session/cancel` 直接调用当前 session 的 runtime interrupt/AbortSignal，不得复用 CLI“双击退出”语义；同时拒绝未完成 permission，并忽略迟到 response。
8. cancel 等待 durable cancelled/idle 终态；`session/close` 再取消事件订阅、close runtime、释放 lease 与 map entry。

**完成标准：** 两个 session 可独立并行；cancel 不影响另一 session；重复 close 幂等。

### OC-04：实现 runtime event 到 ACP update 的映射

**允许文件：** `src/acp/event-mapper.ts`、公共 runtime 的最小订阅接口、测试 fixtures。

**步骤：**

1. 为公共 runtime 增加 adapter 可消费的实时事件订阅或 AsyncIterable；保留 replay 接口。
2. 映射 user/agent text，并保持 chunk 顺序和 message ID 稳定。若 `ThreadUiAdapter` 给出累计文本，mapper 必须按已发送 UTF-16 长度切出 suffix；不得把完整累计文本重复当 delta 发送。
3. 映射 processing/status、plan、tool started/update/finished、diff/edit preview。
4. 同一个工具从 `tool_call`、permission request 到 `tool_call_update` 必须复用稳定 `toolCallId`；turn、message 和 permission ID 同样要有确定来源。
5. tool result 过大时使用摘要和可引用内容，不在单个 frame 无界复制。
6. 未识别的内部事件写 debug stderr；不得导致整个 turn 崩溃，也不得伪装成已支持 ACP update。

**完成标准：** TUI/Web/runtime 与 ACP 使用同一事件事实来源；golden transcript 顺序稳定。

### OC-05：实现 permission、MCP 与 session 配置

**允许文件：** `src/acp/permission-adapter.ts`、`src/acp/config-adapter.ts`、runtime adapter、小范围测试。

**步骤：**

1. 将 runtime permission request 转成 ACP permission request。
2. 用稳定 `toolCallId`/request ID 将 allow once/project/global 与 deny 映射为现有 `approval.respond`。
3. 断开、超时、重复或未知 request ID 默认拒绝并产生可诊断结果。
4. 将 ACP session mode 映射到 build/plan/auto/goal；不支持的 mode 不得静默降级。
5. 非空 `mcpServers` 必须逐 session 映射；stdio server 要校验 command 为允许的绝对路径或受控解析结果，并显式传递 args/env。只有能完整连接、隔离和关闭时才接受，否则返回可见错误并保持 capability 声明诚实。
6. 模型选择优先通过 ACP session config option 暴露；不要让 Studio 原生 `agent.default_model` 误控制外部 Agent。

**完成标准：** 允许、拒绝、超时、取消和断线都覆盖；无权限时 mutating tool 未执行。

### OC-06：实现 session/load 与 replay

**允许文件：** `src/acp/session-manager.ts`、session storage/read model 的窄接口、测试。

**步骤：**

1. `session/load` 对请求 cwd 执行 canonicalize，并要求与 session durable metadata 中的 canonical cwd 相等；取得可写 lease 后才能恢复 owner。
2. 打开持久化 runtime，按 durable cursor 重放完整用户、Agent、工具和状态历史。
3. replay 完成后才响应 load 成功；历史损坏时 fail closed，保留原数据。
4. 处理中断 load、重复 load、load 后继续 prompt 和 Studio 重启。
5. 在正确性稳定前不声明 `session/resume`；不要把 load 和 resume 混为一谈。

**完成标准：** Studio 重启后可以打开历史 Orion Code thread，并继续一个新 turn；消息和工具顺序不重复、不丢失。

### OC-07：隔离配置、运行数据与 env

**允许文件：** `src/product/paths.ts`、相关 storage bootstrap、CLI/ACP bootstrap、测试和迁移文档。

**步骤：**

1. 新增 `ORION_CODE_DATA_DIR`，只承载 sessions、cache、logs、receipts 等可变运行数据。
2. 保留 `ORION_CODE_CONFIG_DIR` 作为用户配置根，明确哪些文件仍在该目录。
3. ACP 模式强制禁用 env file 自动加载。
4. Studio 为 sidecar 注入自身 managed data dir；测试使用临时目录。
5. 不自动移动旧数据。若未来需要迁移，另写可重入、非覆盖、可回滚计划。
6. 实现 session lifecycle lease 的原子写入、活跃 owner 检测、stale recovery 和幂等释放；不得仅按 PID/mtime 删除。
7. 增加 CLI 与 sidecar 并行、两个 sidecar session、同 session 双开和异常退出恢复测试。

**完成标准：** CLI 和 Studio 不会争用同一个可变数据库/lock；配置读取兼容现有用户。

### OC-08：完成 npx Preview 包和质量门禁

**允许文件：** package manifest、build/copy scripts、ACP 测试、发布检查；不执行 publish。

**步骤：**

1. 确认 `dist/acp` 与入口被 `files` 包含。
2. `npm pack --dry-run --json` 检查包内容，不允许测试资产、密钥、`.env` 或开发缓存进入包。
3. 从 tarball 安装到临时目录，使用支持的最低/主力/最高 Node 版本运行 ACP transcript。
4. 生成 version、git SHA、Node ABI、协议版本和测试摘要收据。
5. 验证现有 CLI/TUI/Web 的构建与核心测试。

**验证：**

```bash
npm run lint
npm run build
npm test -- --runInBand
npm run test:runtime-matrix
npm pack --dry-run --json
```

**完成标准：** 可从临时安装目录运行完整 ACP golden journey；没有 npm publish 行为。

### OS-01：增加 Orion Code first-party descriptor 与 Registry 展示

**允许文件：** Agent Registry/设置类型、onboarding Featured 列表、测试 fixtures；不修改默认 settings 表示“已安装”。

**步骤：**

1. 定义唯一 ID `orion-code`，避免把显示名当持久化 ID。
2. Dev/Preview 可从官方 Registry 条目读取 npx 描述。
3. Stable 增加随 App 的最小 first-party descriptor，并定义与远程 Registry 的版本优先级。
4. 将 Orion Code 放在 Featured Agent 第一位并标记 Recommended。
5. Registry 无网络、无缓存或无 `orion-code` 条目时，UI 仍能解释不可安装原因并允许 Native fallback。
6. descriptor 只声明可安装来源，不在默认 settings 中伪造“已配置/已安装”，也不覆盖用户已有同 ID 配置；同 ID 冲突必须显示并停止自动安装。

**完成标准：** descriptor 合并无重复 Agent；本地与远程 ID/版本冲突有确定规则。

### OS-02：实现私有安装、健康检查、更新和卸载状态

**允许文件：** `crates/project/src/agent_server_store.rs`、必要的 Orion first-party descriptor/store 文件、paths 与测试。

**步骤：**

1. 复用 `external_agents`，不得写 `/usr/local`、用户 npm prefix 或 shell profile。
2. 安装到 staging/version 目录，完成下载、解压、SHA 和命令存在性校验后再原子切换 active version。
3. 运行轻量 ACP initialize 健康检查，核对 agent ID、版本、protocol 和最低能力。
4. 调整当前 `remove_stale_versioned_archive_cache_dirs` 调用时机：新版本完成 ACP 健康检查并写入 `last-known-good` 收据前，不得删除当前 active 或上一 verified version；晋升后至少保留当前与前一 verified 版本。
5. 更新失败继续指向旧 active，清理只能删除已确认非 active、非 previous、非 staging-in-use 的同 Agent cache。
6. 卸载前先关闭该 Agent connection，只允许解析并删除 `external_agents/registry/orion-code/` 与 `external_agents/registry/npx/orion-code/` 两个 managed install 根；拒绝空路径、根目录、`..` 和 symlink 逃逸。
7. 卸载只清除安装状态，不删除 `ORION_CODE_CONFIG_DIR` 或 Studio managed data dir 中的用户 session；删除这些数据必须是另一个显式确认动作。
8. 记录 source、resolved version、SHA、install time、health result、last-known-good 和 last error，不记录敏感内容。

**完成标准：** 安装、重复安装、升级、降级拒绝、SHA 错误、中断恢复、卸载和重装均有测试。

### OS-03：实现首次引导同意与安装进度

**允许文件：** onboarding Agent UI、新 `crates/agent_ui/src/orion_code_bootstrap.rs` 或等价单一模块、安装状态 view model、必要测试；不得改编辑器核心布局。

**步骤：**

1. 只在真实 `is_new_install` 展示默认选中的 Orion Code 推荐项。
2. 文案说明会下载并运行一个本地 coding agent，以及数据/网络行为。
3. 用户继续后触发安装；显示 downloading、verifying、starting、ready、failed。
4. 用户取消或拒绝后持久化 `declined`，不在下一次启动重新弹出或下载。
5. 安装失败保持 onboarding 可继续，默认 Native，并提供 retry。
6. bootstrap 内部以 `pending -> configured -> ready | failed | disabled_by_user` 表达产品可用性；它是安装状态机的投影，不替代下载/校验细分状态。
7. 用户同意后，使用 `crates/settings/src/settings_file.rs` 的 `update_settings_file_with_completion` 幂等写入 `agent_servers.orion-code = { "type": "registry" }`。缺失时新增、已是同一 Registry 配置时保留；若同 ID 是 Custom 或其他冲突配置则进入可见失败状态并停止，绝不覆盖。只有写盘完成才能进入 `configured`，ACP 健康检查通过后才能进入 `ready`。
8. 按 5.5 的 schema await 写入 scoped KVP；用户点击 Remove 时先写 durable `removed`，再移除 Agent 配置和 managed binary，防止重启竞态重新安装。

**完成标准：** 无网络、慢网络、取消、关闭窗口和重启都不会卡死首次启动。

### OS-04：实现产品默认 Agent 优先级

**允许文件：** `crates/agent_ui/src/agent_panel.rs`、窄 helper/测试；不得改变 `Agent::default()`。

**步骤：**

1. 将“产品首选 Agent”作为现有恢复优先级之后、Native fallback 之前的最后候选。
2. 候选必须同时满足：新安装、用户同意、配置存在、安装 verified、ACP compatible、非 collab。
3. 只有 ready 后才写入全局最近使用 Agent；安装中或失败不得持久化 Custom Agent。
4. 修正/覆盖 `SelectAgent` 在非空白草稿状态下不持久化的产品路径。
5. 用户随后选择其他 Agent 时完全沿用现有行为。
6. 自动回退只允许发生在“全新安装自动默认、草稿尚未发送、用户未主动切换”三项同时成立时；显式选择 Orion Code 的用户遇到失败时显示 Retry 与 Use Native，不得静默迁移已有 thread。

**必须新增测试：**

- clean install + ready -> Orion Code。
- clean install + declined/failed/offline -> Native。
- existing user + no history -> 维持当前产品规则，不强制 Orion Code。
- existing user + workspace/global agent -> 保留原选择。
- removed/uninstalled Orion Code -> Native 或已安装 global agent。
- collab workspace -> Native。
- restored Orion Code thread + agent 暂时缺失 -> 保留 session 绑定但新草稿 fallback。

### OS-05：增加诊断、重试与可恢复错误

**允许文件：** External Agents 设置页、Agent Panel 错误 UI、安装状态展示和测试。

**步骤：**

1. 显示 installed/resolved version、source、health、data/install location 的脱敏信息。
2. 提供 Retry、Reinstall、Remove、Use Native Agent。
3. 区分 registry unreachable、unsupported platform、npm install、download、checksum、extract、missing command、spawn、initialize、configuration 和 runtime exit 错误。
4. stderr 日志提供复制诊断入口，但过滤 token、prompt 和文件内容。
5. 错误必须传播到 UI；不得用 `let _ =` 静默丢弃。

### OS-06：完成 Studio 自动化门禁

**验证命令按由轻到重执行：**

```bash
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p project agent_server_store
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p agent_servers acp
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p agent_ui test_new_workspace
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p agent_ui selected_agent
cargo +stable fmt --all -- --check
git diff --check
./script/clippy
```

要求：

- GPUI timeout/delay 使用 GPUI executor timer，不使用依赖 `run_until_parked()` 的 `smol::Timer`。
- 测试不得访问真实用户 settings、Application Support 或 npm cache。
- Fake Registry/HTTP/FS 必须覆盖无网、SHA mismatch、损坏 archive、旧 cache 和并发安装。
- `./script/clippy` 是最终门禁；不得用 `cargo clippy` 替代。

### INT-01：本地 Custom Agent 联调

**前置：** OC-03 至 OC-07 通过；不需要 npm/Registry 发布。

**步骤：**

1. 在临时 HOME/config 下构建 Orion Code。
2. 在测试设置中以绝对 Node 路径和绝对 `bin/orion acp` 路径注册 Custom Agent。
3. 完成 initialize、新 session、纯文本 prompt、工具允许/拒绝、cancel、load、close。
4. 验证 Studio 关闭 thread 或 app 后没有孤儿进程。
5. 验证目录含空格、中文和 symlink 的项目。

**禁止：** 修改用户真实 `~/.config/orion-studio/settings.json` 或 `~/.orion-code`。

### INT-02：clean-home npx Preview 联调

**步骤：**

1. 使用本地 tarball 或明确授权的 Preview npm 版本，禁止使用未确认的 `latest`。
2. 创建全新临时 Studio data/config 和 npm cache。
3. 从 Registry fixture 安装，记录实际 resolved version。
4. 完成首次引导、安装、健康检查、默认选择和首个 prompt。
5. 重启后加载历史 thread；移除后确认不重装。
6. 注入无网、安装脚本失败、版本过旧和 Node ABI 错误。

**完成标准：** Preview 可以供开发者试用；该结果不等于 Stable 默认启用或公开发布。

### REL-01：构建平台 sidecar archive

**仓库：** Orion Code。

**建议新增：** `scripts/release/build-acp-sidecar.ts`、manifest schema、平台 smoke tests。

**步骤：**

1. 从 clean checkout 和 locked dependencies 构建 production-only sidecar。
2. 仅包含 ACP adapter、runtime 所需代码与生产依赖；排除 TUI/Web 开发资产和测试缓存。
3. 每个平台运行 initialize/prompt/cancel/close smoke test。
4. 生成 manifest、SBOM、license notices、文件 hashes 和 archive SHA-256。
5. macOS/Windows 执行平台签名与验证；无凭据时只产本地 unsigned candidate，并保持 release blocked。
6. 产物必须可解压到任意路径运行，不依赖 shell PATH、全局 Node 或 npm。

### INT-03：archive 安装、更新与回滚联调

覆盖矩阵：

- 首次安装成功。
- 同版本幂等安装。
- 新版本升级成功。
- 新版本 SHA 错误，旧版本继续可用。
- 下载中断和 App 退出，重启后恢复或安全重试。
- archive 缺 command、command 含 traversal、native ABI 不匹配。
- Registry 版本回退或恶意降级被拒绝。
- 卸载不删除用户配置和 session data。

### INT-04：真实 Orion Studio App 验收

至少使用一个全新 macOS 用户数据目录执行：

1. 安装并启动 Orion Studio。
2. 完成 onboarding 并确认 Orion Code 下载提示。
3. Orion Code 安装成功并成为新 thread 默认 Agent。
4. 完成文本、读取、编辑预览、命令权限允许/拒绝、cancel。
5. 退出 App、重新启动、恢复 thread 并继续 prompt。
6. 断网启动仍可使用已缓存 verified sidecar；首次无网则 fallback Native。
7. Remove Orion Code，重启确认不自动重装。
8. 检查无孤儿 Node/sidecar、无 stdout protocol 污染、无密钥日志。

公开 macOS release 还必须单独验证 Developer ID、notarization、Gatekeeper、DMG、干净机器和更新路径。

### REL-02：发布候选与最终门禁

只有以下全部成立才可标记 `GO FOR AUTHORIZED RELEASE`：

- 两仓 clean candidate SHA 已记录，变更可审查。
- ACP contract/golden、Code 全量门禁、Studio 全量门禁通过。
- npx Preview 与 Stable archive journey 均通过。
- 所有 Stable archive 有 SHA、manifest、SBOM、license 和平台签名证明。
- 全新安装、老用户、拒绝、卸载、失败 fallback 和 restart/resume 真实测试通过。
- 用户数据隔离与 CLI/Studio 并发测试通过。
- rollback 演练通过，上一 verified sidecar 可恢复。
- 文档、隐私说明、Release Notes 和已知限制完成。
- npm、GitHub、Registry、签名和合并动作获得人工授权。

任何一项缺失均为 `NO-GO` 或 `GO-WITH-CONDITIONS`，不得写成“已发布”。

## 10. 测试矩阵

### 10.1 Orion Code adapter

| 类别 | 用例 |
| --- | --- |
| Transport | split frame、多个 frame、malformed JSON、EOF、writer failure、stderr noise |
| Initialize | v1、agentInfo、capability truthfulness、不支持版本 |
| Session | new、两个并发 session、未知 ID、load、close、重复 close |
| Prompt | 文本、多 chunk、空输入、不支持 content、busy/follow-up |
| Events | 文本顺序、累计文本转 delta、message ID、稳定 toolCallId、plan、tool lifecycle、diff、large output |
| Permission | allow once/project/global、deny、timeout、disconnect、duplicate response |
| Cancellation | prompt cancel、tool cancel、double cancel、cancel then close |
| Persistence | restart/load/replay、损坏 history、cursor edge、继续下一 turn |
| Paths | 空格、中文、symlink、cwd 越界、Windows path |
| Process | SIGINT、SIGTERM、parent EOF、无孤儿进程 |
| Data | CLI + sidecar 并发、两个 session、异常退出、lock contention |
| Lease | 活跃 owner busy、同 session 双开、PID 复用、stale recovery、close/崩溃释放 |
| Packaging | 独立 `orion-code` bin、`orion acp` alias、CommonJS 根 + ESM ACP island、stdout 纯净 |

### 10.2 Orion Studio

| 类别 | 用例 |
| --- | --- |
| Registry | first-party/remote 合并、缺条目、版本冲突、无缓存 |
| npx | resolved version、失败脚本、managed Node、私有目录 |
| Archive | SHA、格式、traversal、missing command、stale cache、health-before-GC、last-known-good、atomic active version |
| Onboarding | accept、decline、cancel、offline、retry、restart、KVP 写入失败、stale transient 恢复 |
| Default | clean ready、clean failed、existing user、workspace/global、collab |
| Recovery | handshake failure、runtime crash、update failure、Native fallback |
| Removal | 精确 managed 路径、关闭 connection、remove 后不重装、历史 thread 保留绑定、用户 data 保留 |
| Diagnostics | 错误分类、脱敏日志、version/source/path 展示 |

### 10.3 平台

| 平台 | Build | ACP smoke | 安装/升级 | 签名 | 真实 GUI |
| --- | --- | --- | --- | --- | --- |
| macOS arm64 | 必须 | 必须 | 必须 | 公开版必须 | 必须 |
| macOS x64 | 必须 | 必须 | 必须 | 公开版必须 | 可在对应硬件/CI |
| Linux x64 | 必须 | 必须 | 必须 | 按发布策略 | 不适用 |
| Linux arm64 | 必须 | 必须 | 必须 | 按发布策略 | 不适用 |
| Windows x64 | 必须 | 必须 | 必须 | 公开版必须 | 必须 |

## 11. 观测与隐私

允许记录：

- Agent ID、source、resolved version、protocol version。
- 安装阶段、耗时、错误分类和匿名成功率。
- sidecar exit code、signal、健康检查结果。

禁止记录：

- API key、access token、环境变量值。
- 完整 prompt、模型响应、用户文件内容、命令输出正文。
- 用户真实 HOME、项目完整路径或 session 原文。

日志中的路径至少替换 HOME，并允许用户一键清除 sidecar 诊断日志。

## 12. 回滚方案

1. Studio 永远保留 Native Agent；Orion Code 不可用时立即回退。
2. 默认启用逻辑必须可按 release channel 关闭，不删除已安装数据。
3. 更新以 versioned directory 安装，只有健康检查通过才切 active；失败保留旧 active。
4. Registry 条目回退不能删除本地上一 verified version。
5. Orion Code ACP 新版不兼容时，Studio 可固定上一兼容 archive，并展示更新被暂停。
6. 卸载只移除 managed binary 与安装状态；用户配置和 session data 默认保留，删除数据必须单独确认。

## 13. 执行者规则

每次只领取一个任务卡，并按以下格式报告：

```text
Task: OC-03
Status: PASS | PARTIAL | BLOCKED | FAIL
Baseline: branch + full SHA
Files changed: exact paths
Commands run: exact commands
Tests: passed/failed/not run with counts
Evidence: path or artifact hash
Remaining risks: concrete list
Next allowed task: one ID
```

硬性规则：

- 不跨任务顺手重构，不做无关格式化。
- 不修改用户脏工作树，不删除未跟踪文件。
- 不自行选择版本号、发布频道、Registry 权限或签名身份。
- 不把 static review、单测、build、签名、publish、安装和真实运行混为同一门禁。
- 测试未运行必须写 `NOT RUN`，不能写“应当通过”。
- 外部动作未执行必须写 `NOT PUBLISHED/NOT PUSHED/NOT RELEASED`。
- 发现协议、数据或权限语义不清时停止并写 `BLOCKED`，不得猜测。
- 同一台机器不并发执行重型 Rust 构建；macOS 使用 `CARGO_BUILD_JOBS=2`。

## 14. 实施结果与验证收据（2026-08-30）

### 14.1 隔离基线

| 仓库 | 隔离 worktree | 分支 | 基线 SHA |
| --- | --- | --- | --- |
| Orion Studio | `/Users/hope/.codex/worktrees/orion-studio-acp` | `codex/orion-code-managed-agent` | `27de074ad6c40c407aa7550db42c22c4f22c33ff` |
| Orion Code | `/Users/hope/.codex/worktrees/orion-code-acp` | `codex/orion-code-acp` | `81bf78a575112832a6dba405b5c53c6bf8fa758e` |

实施期间没有修改原工作树，没有执行 publish、push、PR、merge、tag、签名、公证或公开发布。

### 14.2 已实现的 Preview 闭环

Orion Code 已完成：

- 独立 `orion-code` ACP bin 和 `orion acp` 兼容入口，根包保持 CommonJS，ACP adapter 使用独立 ESM 编译岛。
- ACP v1 `initialize`、`session/new`、`session/load`、`session/prompt`、`session/cancel`、`session/close`、流式 update、tool update 和 fail-closed permission 映射。
- Text 与 Resource Link 输入；Image、Audio、Embedded Resource 在真实支持前返回显式 unsupported 错误。
- 每 session stdio MCP 生命周期；拒绝当前未支持的 HTTP、SSE 和 ACP 嵌套 transport。
- CLI、Web 和 ACP 共用 renderer-neutral session ownership coordinator；切换采用“先获取目标 lease，再切 runtime”，失败保留原 owner。
- 本机 PID/start-identity/token lease、原子候选发布、并发恢复互斥、异常退出回收，以及 CLI、Web、ACP 跨进程互斥。
- `ORION_CODE_DATA_DIR` 与配置根分离；Studio sidecar 可以使用私有数据目录并禁用环境文件自动加载。

Orion Studio 已完成：

- 内置受信任的 `orion-code` 描述符与远程 Registry 合并；保留 ID 只接受 `@orion-agents/orion-code@<精确 semver>`，拒绝参数、环境注入、binary 冒领和版本不一致。
- 版本化私有 npx 目录、精确版本复用、离线缓存、symlink/path traversal 防护和精确卸载边界。
- durable bootstrap 状态、安装/验证 generation、防陈旧异步写回、严格 ACP identity/protocol/version/load/close 健康检查。
- 30 秒连接超时、启动取消时进程组清理、显式且幂等的 awaitable shutdown，以及跨窗口 Orion Code connection registry。
- 更新失败保留上一 verified 版本并回滚 pin；Retry 清除 pin 后重新拉取，禁止未授权降级。
- Remove 顺序固定为：先持久化 Removed，再等待所有窗口 sidecar 退出，随后删除 Registry 设置和精确 managed version 目录；保留配置与 session data。
- 仅全新安装、非协作 Workspace、无恢复/Workspace/global/thread 选择且 durable Ready 时应用 Orion Code 产品默认；用户显式选择通过进程级 generation 阻止陈旧自动默认覆盖。
- Onboarding 和 External Agents 页面提供 Recommended、安装阶段、Retry Update、Retry、Remove 和 Native fallback 路径；`Agent::default()` 仍保持 Native Agent。

### 14.3 自动化验证收据

Orion Code 在 Node `22.22.3`、npm `10.9.8` 下通过：

| 门禁 | 结果 |
| --- | --- |
| `npm run build:server` | PASS |
| ACP contract/golden/source stdio smoke | PASS |
| 关键 ownership/lease/MCP focused Jest | 9 suites，74 tests PASS |
| 全量 Jest | 313 suites；3755 passed，5 skipped |
| ESLint、Prettier、`git diff --check` | PASS |
| CLI↔ACP、Web↔ACP、ACP↔ACP 真实进程互斥 | PASS |
| SIGKILL 后 lease recovery | PASS |
| npm package lifecycle | 6/6 PASS |

最终本地 tarball 收据：

| 字段 | 值 |
| --- | --- |
| package | `@orion-agents/orion-code@0.3.2` |
| tarball | `orion-agents-orion-code-0.3.2.tgz` |
| SHA-256 | `902514673d89875765f889be612b80700f02fa1b7b4dbec77db27b8d3f6283e6` |
| packed bytes | `2,225,828` |
| entries | `1,441` |
| isolated production install | PASS；约 `123 MiB`（含生产依赖） |
| installed CLI identity | `orion v0.3.2` |
| installed ACP contract smoke | PASS |
| package hygiene scan | 未发现 `.env`、测试夹具、`node_modules`、coverage、target、`.git`、npm cache 或 `.DS_Store` |

Orion Studio 使用 `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0` 通过：

| 门禁 | 结果 |
| --- | --- |
| `project` Orion Code tests | 18/18 PASS |
| `project` agent server store tests | 23/23 PASS |
| `project` agent registry tests | 12 unit + 3 integration PASS |
| `agent_ui` Orion Code tests | 15/15 PASS |
| `agent_ui` connection lifecycle tests | 4/4 PASS |
| `agent_servers` ACP tests | 29/29 PASS |
| ACP startup cancellation/process-group test | PASS |
| new-workspace/default-selection tests | 3 + 4 + 1 PASS |
| `onboarding --lib` compile gate | PASS，当前 0 tests |
| `cargo fmt --all -- --check`、`git diff --check` | PASS |
| `./script/clippy -p project -p agent_servers -p agent_ui -p onboarding` | PASS；release/all-targets/all-features/deny warnings |

### 14.4 任务卡状态与剩余门禁

| 任务 | 状态 | 说明 |
| --- | --- | --- |
| PRE-00、CON-01 | PASS | 隔离基线和协议契约已落实 |
| OC-01..OC-08 | PASS（Preview） | 源码、进程、包生命周期和隔离安装门禁通过；未 publish |
| OS-01..OS-04、OS-06 | PASS（Preview） | 供应链、私有安装、bootstrap、默认选择和相关 crate 门禁通过 |
| OS-05 | PARTIAL | Retry/Remove/fallback 和可见错误已完成；完整脱敏诊断复制面板、Reinstall 与详细 location 展示仍应在真实 App 验收前补齐 |
| INT-01 | PARTIAL | Orion Code 真实 stdio 进程与 Studio ACP transport 分别通过自动化；尚未以真实 Studio App 完成完整 Custom Agent GUI journey |
| INT-02 | PARTIAL | tarball clean install、ACP smoke 与 Studio exact-npx/clean-state 测试通过；尚未完成单一 clean-home Studio GUI 端到端 journey |
| REL-01、INT-03 | GATED | Stable archive、SBOM、license manifest、平台签名及 archive 更新/回滚矩阵未执行 |
| INT-04 | NOT RUN | 需要真实 Orion Studio App、全新数据目录、重启/断网/移除和无孤儿进程验收 |
| REL-02 | NO-GO | 未获得发布授权，且 Stable/真实 App 门禁未齐全 |

### 14.5 已知边界

- 当前 session lease 是本机进程级互斥，适用于 Orion Studio 私有 sidecar、CLI 和本机 Web runtime；不承诺网络文件系统或多主机分布式锁语义。
- Preview 仍使用 Studio 托管 Node 与版本化私有 npx 安装。它不等于不依赖 Node/npm 的 Stable archive。
- 本收据证明源码、协议、进程、包和相关 crate 自动化门禁，不替代签名、公证、干净机器和真实 GUI 证据。
- npm `0.3.2` 只存在于本地候选；未发布，公开 `latest` 不应据此改变。

## 15. Stable 计划完成定义

本计划只有在以下结果同时存在时才算完成：

1. Orion Code 有稳定、测试覆盖的 `orion acp`。
2. Orion Studio 私有安装、校验、健康检查、更新、卸载和 fallback 全部闭环。
3. 全新安装用户在明确同意后默认使用 Orion Code。
4. 老用户、协作 Workspace、拒绝/卸载用户不被强制改变。
5. Studio 重启可以恢复 Orion Code thread；cancel、permission 和 tool updates 可见且正确。
6. Stable sidecar archive 不依赖全局 Node/npm，并通过目标平台签名与安装测试。
7. 失败路径不会阻断 Studio、覆盖数据、留下半安装或自动重装。
8. 两仓测试、真实 App 验收、回滚演练和发布证据齐全。

SDK 抽取可以在上述 ACP 契约稳定后单独立项。届时只把 ACP adapter 下层从现有 runtime 换成 Orion Code SDK；`orion acp`、Agent ID、ACP capability 和 Orion Studio 集成均应保持兼容。
