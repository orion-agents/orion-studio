# Orion Code 托管 Sidecar 安全自动更新 v2 开发计划

## 0. 计划信息

| 项目 | 内容 |
| --- | --- |
| 计划状态 | `IMPLEMENTED / RELEASE BLOCKED` |
| 编写日期 | 2026-08-31 |
| 目标版本 | `Managed Sidecar Update v2`；Orion Studio 与 Orion Code 的正式 semver 由发布负责人另行确定 |
| 前置计划 | [`orion-code-acp-managed-sidecar-default-agent-plan.md`](orion-code-acp-managed-sidecar-default-agent-plan.md) |
| Orion Studio 基线 | 分支 `codex/orion-code-managed-agent`，commit `844117312f53658c4bffed6b6dd7e7117397734f` |
| Orion Code 基线 | 分支 `codex/orion-code-acp`，commit `de3ea28c66af04e4ab5f9e6829d96925e55a0e28` |
| 主目标 | 为 Studio 私有托管的 Orion Code sidecar 增加频道、定时检查、后台预下载、空闲切换、灰度、兼容约束、紧急撤回和 Stable 签名 archive |
| 首发平台 | macOS Apple Silicon；其他 target 只有在各自构建、签名和真实测试完成后才启用 |
| 协议边界 | Studio 与 Orion Code 仍只通过 ACP v1 通信；本计划不改变 ACP session 契约 |

执行证据按以下文档记录：

- [`evidence/ORION-CODE-UPDATE-V2-00-BASELINE-ADR.md`](evidence/ORION-CODE-UPDATE-V2-00-BASELINE-ADR.md)：隔离基线与架构决策。
- [`evidence/ORION-CODE-UPDATE-V2-IMPLEMENTATION-EVIDENCE-MATRIX.md`](evidence/ORION-CODE-UPDATE-V2-IMPLEMENTATION-EVIDENCE-MATRIX.md)：逐任务实现、测试、签名、公证和外部灰度证据矩阵。
- [`evidence/ORION-CODE-UPDATE-V2-MACOS-CANDIDATE-CANARY-RUNBOOK.md`](evidence/ORION-CODE-UPDATE-V2-MACOS-CANDIDATE-CANARY-RUNBOOK.md)：授权前 macOS candidate、签名、公证、远端复验、canary 与撤回操作手册。

任务完成状态必须以代码、测试输出和 release receipt 为准，而不是仅以本表状态为准。

> [!IMPORTANT]
> 本文授权范围仅为代码、测试、文档、本地 unsigned candidate 和 CI 配置。它不授权 npm publish、GitHub Release、更新索引上线、ACP Registry 提交、Developer ID 签名、公证、push、PR、合并或用户设备灰度；这些外部动作必须单独获得人工授权。

> [!IMPORTANT]
> “Stable / Beta”是发布频道，“Automatic / Manual”是更新模式。产品 UI 可以提供 `Stable`、`Beta`、`Manual` 三个简化选项，但持久化模型必须分开表达，不能把 `manual` 伪装成一个可发布版本的频道。

### 0.1 2026-09-01 实施快照

本计划的本地开发范围已经实现，发布范围仍被外部门禁阻断：

- Studio 已接通 Stable/Beta/Manual、启动 30～90 秒 jitter、6 小时周期、wake/backoff/single-flight、签名索引、后台下载、字节进度、staging、idle activation、rollback、paused/revoked 和 Native fallback。
- `paused` 版本集合持久化在 state-v2 中；已 staged 的版本若在更高 sequence 的签名索引中变为 paused，会被持久化阻断，不能继续激活。
- 当前版本被 revoked 后，新 Orion Code connection/session 立即被拒绝；已有任务先 drain 到 idle，再切换安全 previous。没有安全 previous 时，关闭托管 runtime 并把现有窗口与全局偏好切到 Native Agent。
- 隔离 ACP preflight 实际执行 `initialize -> session/new -> session/load -> session/close`，随后关闭 transport、等待进程退出，并在失败或超时时清理进程组。
- retention 保护 current/previous/staged，要求成功 activation 后至少跨过一次 App restart，最多保留 3 个完整安装；revoked binary 可删除，但 receipt 保留。
- 生产配置入口已在 `agent_ui::init` 调用；正式 endpoint、host allowlist、Ed25519 key、Apple Team ID 和 bundle ID 未冻结时返回 `None`，更新能力保持 fail-closed，且没有环境变量绕过。
- Orion Code 已有 app-like archive、最终 receipt、受保护 signing/notary/publisher 和 same-version rollout/pause/revoke policy transition 的代码路径；这些路径尚未使用真实凭据或生产目标执行。

主 agent 已确认当前 Studio 隔离 worktree 的定向结果：`agent_ui orion_code 73/73`、`agent_servers acp 35/35`、`project orion_code 19/19`、`orion_code_update 10/10`、settings 合计 `5/5`、`auto_update 23/23`；`cargo fmt --all -- --check` 与计划范围的 release/all-targets/all-features scoped Clippy 也已通过。Orion Code 当前代码已通过 `test:release-tooling 36/36`、lint、Prettier、build、`test:acp` 和完整 Jest（313 suites，3755 passed，5 skipped）；本地 unsigned candidate、receipt replay 与 local-test-key index dry-run 也已执行并保持 `NOT_RELEASABLE / NOT_PUBLISHED / NOT_SIGNED_WITH_RELEASE_KEY`。

仍未执行或冻结：clean committed release checkout、最终候选所需的全仓无参数 `./script/clippy`、正式 update endpoint/public key/Team ID/bundle ID、Developer ID signing、Apple notarization、staple/Gatekeeper、生产远端 archive SHA replay、干净 Mac journey、5%～100% canary 和紧急撤回实操。没有 push、PR、上传、发布、签名或公证动作。

## 1. 最终产品体验

### 1.1 Stable 自动更新

已启用 Orion Code 的用户默认使用 Stable 自动更新：

1. Orion Studio 启动后延迟 30～90 秒检查一次更新，避免与窗口恢复、项目扫描和 Agent 首次连接争抢资源。
2. App 持续运行时，每 6 小时最多检查一次；休眠恢复或网络切换不会制造并发检查。
3. 找到兼容且属于当前灰度批次的版本后，在后台下载到 staging，不影响当前 Orion Code 会话。
4. 完成 archive SHA-256、签名、平台、文件清单和隔离 ACP 健康检查后，状态变为 `Ready to Activate`。
5. 当前没有 prompt、tool、permission、load 或 shutdown 操作时自动切换；繁忙时等待，不取消用户任务。
6. 新版成功启动并通过第二次健康检查后才成为 active；旧版至少保留为 previous verified。
7. 新版失败时回滚 previous verified；若没有安全版本，则回退内置 Orion Agent，并显示可恢复错误。

### 1.2 Beta

- Beta 用户可收到 prerelease 与 Stable 版本，候选选择仍使用精确 semver、平台和兼容性约束。
- Beta 不绕过 SHA、签名、公证、ACP 健康检查、busy barrier 或撤回规则。
- 从 Beta 切回 Stable 不立即降级；只有当前 Stable 版本高于现有版本，或签名更新索引明确授权 `rollback_to` 时才切换。

### 1.3 Manual

- Manual 模式不运行 6 小时定时检查，也不自动下载。
- 用户点击 `Check for Orion Code Updates` 后才获取更新索引。
- 用户必须再次点击 `Download`；下载完成后可选择 `Activate When Idle`。
- Manual 仍受频道、兼容、灰度、撤回和签名规则约束，不能安装任意 URL 或未验证 archive。

### 1.4 更新失败、离线和撤回

- 普通自动检查失败保持安静，设置页显示 `Last check failed`；不得每次打开面板弹窗。
- 离线时继续使用本地 verified 版本；更新索引过期本身不能让已验证版本失效。
- `paused` 停止新的下载和激活，但不强制停止当前 verified 版本。
- `revoked` 禁止再次启动对应版本；优先切换最新的本地非撤回 verified 版本，否则回退 Orion Agent。
- 撤回不得删除用户配置、session、prompt、工具输出或项目数据。

## 2. 当前基线与发布门禁

### 2.1 已有能力

当前 Preview 已具备：

- 受信任的 `orion-code` 保留 ID 与精确 npm package binding。
- `external_agents` 下的版本化私有 npx 安装目录。
- Registry 最多每小时一次的 event-driven refresh。
- ACP identity、protocol、exact version、load 和 close 健康检查。
- `last_verified_version`、version floor、失败 rollback pin 和 `Retry Update`。
- 新旧 sidecar 串行切换、全窗口 connection shutdown barrier 和幂等 close。
- macOS/Linux/Windows binary target、archive URL、相对 command 与可选 SHA-256 的通用 Registry 解析。
- Orion Studio App 自身的 `AutoUpdater` 定时器、wake recovery 和自动/手动检查模式，可作为调度实现参考。

### 2.2 发布前剩余门禁

- 官方 ACP Registry 仍不承担频道、灰度、最低 Studio、暂停、撤回、sequence 或签名控制面；正式自动更新必须继续只信任 Orion first-party 签名索引。
- Studio 的 embedded production configuration 已有启动接线，但正式 index/signature endpoint、feed/redirect/archive host allowlist、Ed25519 key ID/public key、Apple Team ID 和 sidecar bundle ID 尚未冻结，因此生产 feed、installer 和 ACP health check保持 fail-closed。
- Orion Code 受保护流水线代码已覆盖 app-like archive、内到外签名、公证、staple、最终 receipt、远端 SHA replay、release-key 签名和条件 publisher；真实凭据、真实 target URL 与 protected environment 尚未获授权执行。
- 当前两仓仍是 dirty isolated worktree，不是 clean committed exact tag；计划范围 scoped Clippy、最新 protected production tooling suite 和本地 unsigned replay 已通过，但全仓无参数 Clippy、exact clean SHA 上的重放、真实 signed candidate 和 clean-Mac journey仍需执行。
- 5% -> 25% -> 50% -> 100% 灰度、paused、revoked/rollback 和紧急撤回都只有代码与 runbook，没有生产发布、观察窗口或可重放外部回执。

## 3. 架构决策

| ID | 决策 | 原因 |
| --- | --- | --- |
| U-D01 | 增加 Studio 全局 `OrionCodeUpdateCoordinator`，不把调度器塞进 `AgentPanel` | 更新生命周期属于 App 级产品状态，不能依赖某个窗口或面板存在 |
| U-D02 | 官方 ACP Registry 只负责生态发现和 latest Stable 展示；自动更新控制面使用 Orion first-party 签名索引 | 官方 schema 无频道、灰度、兼容、撤回和签名扩展，不能私自添加不兼容字段 |
| U-D03 | `channel = stable | beta` 与 `mode = automatic | manual` 分开持久化 | 频道描述候选集合，模式描述何时检查/下载 |
| U-D04 | 只安装更新索引给出的 exact semver 和 immutable URL | 禁止 `latest`、semver range、可变 URL 或运行时解析漂移 |
| U-D05 | Stable 使用平台 archive；npx 仅保留 Dev/Preview 和回归测试 | Stable 不依赖用户系统 Node/npm，也不执行安装期任意脚本 |
| U-D06 | 下载、验证和激活分为三个事务 | 后台可预下载，但未通过完整门禁的文件不能成为 active |
| U-D07 | 正在运行的 turn、tool、permission、load 或 shutdown 永不被自动更新取消 | 用户任务优先；繁忙时保持 staged，最迟下次 App 启动激活 |
| U-D08 | 激活顺序为“候选预检 -> 等待空闲 -> 旧版退出 -> active pointer 切换 -> 新版复检” | 避免两个 sidecar 同时持有 session lease，也避免半切换 |
| U-D09 | 灰度 cohort 在本地确定性计算，不上传 installation ID | rollout 单调扩容且不增加用户身份数据收集 |
| U-D10 | 更新索引使用单调 sequence、有效期和带 key ID 的 Ed25519 detached-signature envelope | 防止旧索引重放、控制面篡改和无边界缓存 |
| U-D11 | `paused` 与 `revoked` 分开；只有 `revoked` 禁止启动 | 暂停发布不应远程关闭健康的本地版本 |
| U-D12 | 降级默认拒绝；只有签名索引中的精确 `rollback_to` 可以授权 | 防止 Registry 回退、CDN 旧缓存或攻击导致恶意降级 |
| U-D13 | 新安装与已接受 Preview 管理的用户迁移为 Stable Automatic；declined/removed/custom 保持不变 | 不重新打扰已授权 Studio 托管的用户，也不逆转拒绝和自定义配置 |
| U-D14 | macOS Stable archive 的所有嵌套可执行代码逐层签名，不使用 `codesign --deep` 代替正确签名顺序 | 深度签名无法正确表达不同嵌套代码的 entitlement 与位置 |
| U-D15 | ACP adapter、Agent ID、session data root 和用户配置 root 保持不变 | 更新系统不应耦合后续 Orion Code SDK 抽取 |

## 4. 目标架构

```text
Orion Code release CI
  -> production-only platform archive
  -> file manifest + SHA-256 + SBOM + notices
  -> platform signing + macOS notarization
  -> immutable release assets
  -> signed Orion Code Update Index

Official ACP Registry ---------------------> ecosystem discovery / latest Stable display
Signed Orion Update Index ----------------> authoritative channel/rollout/revoke policy
                                               |
                                               v
OrionCodeUpdateCoordinator (global GPUI entity)
  -> scheduler / backoff / wake handling
  -> candidate resolver
  -> download + staging
  -> integrity/platform/signature verifier
  -> isolated ACP preflight
  -> idle activation coordinator
  -> active / previous / staged receipts
                                               |
                                               v
AgentConnectionStore -> ACP -> Orion Code sidecar
```

### 4.1 模块边界

| 模块 | 所属仓库 | 职责 |
| --- | --- | --- |
| `OrionCodeUpdateSettings` | Studio | 用户频道与更新模式；进入 settings schema |
| `OrionCodeUpdateRecordV2` | Studio | 本机检查、下载、verified、staged、rollback 与错误收据；进入 scoped KVP |
| `OrionCodeUpdateIndexV1` | 两仓共享契约 | 频道、版本、平台 target、兼容、灰度、状态和撤回 |
| `OrionCodeUpdateCoordinator` | Studio | App 级调度、状态机和并发串行化 |
| `OrionCodeCandidateResolver` | Studio | 从签名索引选择唯一安全候选 |
| `OrionCodeArchiveInstaller` | Studio/project | bounded download、安全解压、SHA、文件清单与 versioned staging |
| `OrionCodeActivationBarrier` | Studio/agent_ui | 汇总所有窗口/连接是否可安全切换，并等待 idle |
| `build-acp-sidecar` | Orion Code | 构建 production-only 平台 artifact 和本地收据 |
| `generate-update-index` | Orion Code/release | 从已验证 release receipts 生成确定性索引 payload |
| signing/notary workflow | Orion Code CI | 在受保护 runner 中签名、公证、验证并上传 release assets |

## 5. 设置与持久化契约

### 5.1 用户设置

建议增加以下设置：

```json
{
  "agent": {
    "orion_code": {
      "update_channel": "stable",
      "update_mode": "automatic"
    }
  }
}
```

约束：

- `update_channel`: `stable | beta`，默认 `stable`。
- `update_mode`: `automatic | manual`，默认 `automatic`。
- UI 的 `Manual` 简化选项映射为 `update_mode=manual`，并保留用户最近选择的 channel。
- 设置只允许 user scope，不允许 project/worktree 覆盖，避免同一 App 为不同项目并发拉取不同 sidecar。
- Studio Dev 可通过测试注入 fixture URL；正式构建不得接受环境变量覆盖签名 key、release host 或撤回策略。

### 5.2 Durable record v2

保留 namespace `orion-code-bootstrap`，新增 key `state-v2`。最小结构：

```json
{
  "schema_version": 2,
  "choice": "accepted",
  "source": "archive",
  "current_verified": {
    "version": "0.4.0",
    "target": "darwin-aarch64",
    "archive_sha256": "<64 hex>",
    "installed_at": "<RFC3339>",
    "index_sequence": 42
  },
  "previous_verified": {
    "version": "0.3.2",
    "target": "darwin-aarch64",
    "archive_sha256": "<64 hex>"
  },
  "staged": null,
  "last_successful_check_at": "<RFC3339>",
  "next_eligible_check_at": "<RFC3339>",
  "highest_index_sequence": 42,
  "failure_count": 0,
  "last_error_kind": null
}
```

不得写入：

- 完整 HOME、项目路径、prompt、模型响应、工具输出、环境变量值或 token。
- signing private key、Apple 凭据、release API token 或 installation ID 原文。

### 5.3 v1 -> v2 迁移

| v1 状态 | v2 结果 |
| --- | --- |
| accepted + last verified | `stable + automatic`，将版本导入 current verified，source 保留 npx，等待首次 archive 更新 |
| accepted + no verified | `stable + automatic`，保持 configured/failed 语义并重新验证 |
| declined | 保持 disabled，不检查、不下载 |
| removed | 保持 removed，不检查、不重装 |
| custom `orion-code` settings | 不接管，不迁移，由 UI 显示 conflict |
| 损坏或未知 schema | fail closed，保留现有文件与安装，回退 Native 并显示 diagnostics |

迁移必须是幂等事务：先读取 v1、验证、写完整 v2，再标记迁移成功；不得先删除 v1。

## 6. First-party 签名更新索引

### 6.1 为什么不能只使用官方 ACP Registry

当前官方 ACP Registry 的 binary target 只包含 `archive`、可选 `sha256`、`cmd`、`args` 和 `env`；Agent 只有单一 version。它适合发现和 latest Stable 分发，但无法表达本计划需要的频道、灰度、最低 Studio 版本、暂停、撤回、单调 sequence 和签名。

因此：

- 官方 Registry 继续展示 Orion Code，并在 Stable 达到 100% 后指向最新 Stable archive。
- Orion Studio 对保留 ID `orion-code` 的自动更新只信任 first-party update index。
- 官方 Registry 中更高版本不得绕过 first-party rollout、paused、revoked 或 compatibility 决策。
- 如果 future ACP Registry 正式增加这些字段，再通过 ADR 评估是否合并控制面；当前不得私自扩展其 schema。

### 6.2 传输与签名

分发两个 immutable 对象：

```text
orion-code-update-index-v1.json
orion-code-update-index-v1.json.sig
```

- `.sig` 是一个有严格大小和 schema 限制的 envelope，包含 `algorithm=ed25519`、`key_id` 和 base64 signature；它只用于选择 Studio 已内置的公钥并验证 index 原始 bytes。
- index JSON 在验签成功前不得进入候选解析；解析 `.sig` envelope 不会授予新 key 或改变产品状态。
- Studio 内置一个或多个 `key_id -> public_key`，私钥只存在于 release signing 环境。
- index 包含单调 `sequence`。本机存储最高接受 sequence，拒绝更小值，防止有效旧签名重放。
- index 包含 `generated_at` 与 `expires_at`。过期时停止发现新版本，但继续运行非撤回的本地 verified 版本。
- key rotation 必须通过新版 Studio 增加 key，或使用旧 key 对新 key delegation 签名；不得从同一未验证 index 自行信任新 key。

Signature envelope：

```json
{
  "schema_version": 1,
  "algorithm": "ed25519",
  "key_id": "orion-release-2026-01",
  "signature": "<base64 signature over exact index bytes>"
}
```

### 6.3 Index schema

```json
{
  "schema_version": 1,
  "sequence": 42,
  "generated_at": "<RFC3339>",
  "expires_at": "<RFC3339>",
  "releases": [
    {
      "version": "0.4.0",
      "channel": "stable",
      "status": "active",
      "published_at": "<RFC3339>",
      "studio_version_requirement": ">=0.1.0,<0.2.0",
      "acp_protocol": 1,
      "rollout_basis_points": 2500,
      "rollout_salt": "immutable-public-release-salt",
      "rollback_to": null,
      "release_notes_url": "https://example.invalid/orion-code/releases/0.4.0",
      "targets": {
        "darwin-aarch64": {
          "archive_url": "https://example.invalid/orion-code/0.4.0/darwin-aarch64.zip",
          "archive_sha256": "<64 hex>",
          "archive_bytes": 12345678,
          "format": "zip",
          "command": "OrionCodeSidecar.app/Contents/MacOS/orion-code-acp",
          "manifest_sha256": "<64 hex>",
          "sbom_sha256": "<64 hex>",
          "signing_requirement": "developer_id_and_notarized"
        }
      }
    }
  ]
}
```

示例 URL 只用于说明 schema，执行时必须由发布负责人冻结正式域名、redirect allowlist 和 CDN cache policy。

### 6.4 候选选择顺序

候选必须同时满足：

1. index signature、schema、sequence 和时间有效。
2. release channel 符合用户设置；Beta 可消费 Beta 与 Stable，Stable 只能消费 Stable。
3. `status=active`。
4. exact semver 高于 current，或存在有效 `rollback_to`。
5. 当前 Studio version 满足 requirement。
6. ACP protocol 受 Studio 支持。
7. 当前平台 target 存在。
8. Stable target 要求 archive SHA、manifest SHA、SBOM SHA 和平台签名门禁均存在。
9. 本机 rollout cohort 小于 `rollout_basis_points`。
10. 版本不在已接受 signed index 的 revoked 集合。

任何条件失败都只能产生 `No candidate` 或可见的兼容说明，不能退回 npm `latest`。

## 7. 调度、灰度和网络策略

### 7.1 Scheduler

`OrionCodeUpdateCoordinator` 作为 App 全局 Entity 初始化：

- startup check：App ready 后 30～90 秒 deterministic jitter；先读取持久化 `next_eligible_check_at`，未到期时只使用缓存做本地候选检查，不重复访问网络。
- periodic check：以上次成功或完整失败 check 为基准，每 6 小时一次。
- manual check：立即执行，但与自动 check 共享 single-flight lock。
- system wake：若 checking/downloading 的网络 future 可能失效，取消并按 backoff 重建；local verifying/activating 不被 wake 中断。
- automatic 网络失败 backoff：15 分钟 -> 1 小时 -> 6 小时，成功后清零。
- HTTP 使用 ETag / If-None-Match；`304` 更新 last successful check，不重写同一 index。
- settings 改为 Manual 时取消尚未开始的自动下载，但不删除已经 verified 的版本。

### 7.2 灰度 cohort

```text
cohort = first_u64(SHA-256(local_installation_id || release.version || release.rollout_salt)) mod 10000
eligible = cohort < rollout_basis_points
```

- installation ID 只在本地读取，不进入 URL、header、日志或 telemetry。
- 同一 release 的 salt 不可修改，保证 rollout 从 5% 增加到 25% 时单调包含原 cohort。
- `rollout_basis_points` 范围为 `0..=10000`；越界使 release 无效。
- Manual 用户同样遵守 rollout。需要抢先体验时选择 Beta，不提供隐藏绕过开关。
- 自动扩大 rollout 必须由发布流水线或人工审批生成更高 sequence 的新签名 index，不能由客户端猜测。

### 7.3 网络与资源预算

- index 与 signature 各自限制最大 bytes；archive 使用 target 声明 size 加全局硬上限。
- 拒绝无界重定向、非 HTTPS、非 allowlist host、Content-Length 超预算和 decompression bomb。
- 下载写入 `.partial`；校验成功前不得改名为 staged。
- 首版不要求断点续传。若增加 Range resume，必须绑定相同 ETag、长度和 expected SHA，否则重新下载。
- 下载并发固定为 1；不得和 Orion Code npm Preview 安装并行写同一 managed root。

## 8. Archive、签名与公证

### 8.1 通用 archive 内容

```text
orion-code-sidecar/
  manifest.json
  SBOM.cdx.json
  LICENSE
  THIRD_PARTY_NOTICES
  OrionCodeSidecar.app/
    Contents/Info.plist
    Contents/MacOS/orion-code-acp
    Contents/Resources/runtime/...
    Contents/Resources/app/...
```

`manifest.json` 至少包含：

- schema version、Orion Code version、git SHA、目标平台、build timestamp。
- ACP protocol、最低/最高兼容 Studio requirement。
- Node runtime version/ABI 与 native modules 清单。
- archive 内每个文件的 path、mode、bytes 和 SHA-256。
- 入口 command、SBOM path/hash、license notices path/hash。
- 构建工具链版本。最终 release receipt 位于 archive 外部，单向绑定最终 archive、manifest、SBOM 与 notices 摘要；manifest 不反向包含 receipt 摘要，避免形成不可求解的循环摘要。

### 8.2 构建约束

- 从 clean checkout 与 locked dependencies 构建；dirty source 只能生成 local unsigned candidate，不能进入 release job。
- 仅包含 ACP adapter、product runtime 所需模块和 production dependencies；排除 TUI/Web 开发资产、测试、coverage、cache、`.env` 和 source map 中的本机绝对路径。
- 产物可解压到任意只读父目录运行，不依赖 shell PATH、全局 Node/npm 或用户项目依赖。
- archive 生成两次应得到相同文件 manifest；如果压缩 bytes 因 timestamp 不确定，则必须先修复 reproducibility 再允许发布。

### 8.3 macOS Stable

首发建议使用 zip 包含 app-like sidecar bundle：

```text
OrionCodeSidecar.app/
  Contents/Info.plist
  Contents/MacOS/orion-code-acp
  Contents/Resources/runtime/...
  Contents/Resources/app/...
```

门禁：

1. 为 sidecar 使用独立稳定 bundle identifier 与 Developer ID Application identity。
2. 从最内层的 Node、`.node`、`.dylib`、PTY/helper 到外层 bundle 逐层签名。
3. 启用 Hardened Runtime、secure timestamp，不包含 `get-task-allow=true`。
4. 不用 `codesign --deep` 代替逐层签名。
5. 使用 Xcode `notarytool` 或 Notary API 提交；不使用已停止服务的 `altool`。
6. 公证成功后 staple 到可 staple 的 bundle，再生成最终 zip 和最终 SHA-256。
7. 发布前执行 `codesign --verify --strict`、`spctl --assess`、stapler validate、离线 Gatekeeper 和干净 Mac 启动测试。
8. Sidecar 是独立下载产物，不能把 Orion Studio.app 的签名收据当成它的签名证据。

若 app-like bundle spike 证明无法满足当前 launcher、相对资源路径或公证要求，任务必须停在 ADR，不得临时改用未签名 raw Node 目录发布。

### 8.4 其他平台

- Windows：所有 `.exe`、`.dll` 和 helper 使用受信 Authenticode 证书签名，再生成 zip 和 SHA；真实 SmartScreen/干净机测试是发布门禁。
- Linux：至少提供 archive SHA、manifest、SBOM 和 reproducible build receipt；是否增加 Sigstore/minisign 必须单独 ADR。
- 未完成某 target 的真实构建与签名时，index 中不得声明该 target。

## 9. Download、验证和 staging

### 9.1 目录布局

```text
external_agents/registry/orion-code/
  downloads/<version>/<target>.partial
  staging/<version>/<target>/
  versions/<version>/<target>/
  receipts/<version>-<target>.json
```

约束：

- 所有 path 必须从 managed root + validated exact semver + known target 推导。
- 拒绝绝对 archive entry、`..`、symlink/hardlink escape、重复冲突 entry、设备文件和超预算展开。
- staging 完整验证前不能进入 `versions/`。
- active/previous/staged 只存 identity receipt，不依赖不安全的可写 symlink。

### 9.2 验证顺序

1. URL、host、size、target 和 semver 预校验。
2. 下载到 partial，同时计算 SHA-256。
3. archive SHA 与 update index 精确匹配。
4. 安全解压到新 staging。
5. 读取内嵌 manifest，验证 schema、版本、target、入口和所有文件 hashes。
6. 验证 SBOM 与 notices hash。
7. 执行平台签名/公证验证。
8. 使用临时 config/data root 和 `ORION_CODE_DISABLE_ENV_FILES=1` 运行 ACP preflight。
9. 预检完成 `initialize` identity/protocol 校验，再实际执行 `session/new`、`session/load`、`session/close`，关闭 transport 并确认进程退出且无残留进程组。
10. 原子晋升为 staged candidate 并持久化 receipt。

任何失败都清理本次 partial/staging；不得删除 active、previous 或用户 data。

## 10. 空闲切换与回滚

### 10.1 Busy 定义

任一条件成立即为 busy：

- 任一窗口存在 Orion Code prompt/turn 进行中。
- 任一 tool call 或 permission request 未终结。
- session load/replay、connection startup、shutdown 或版本切换进行中。
- active connection 的请求计数不为 0。
- durable session 正处于需要旧 runtime 完成写入的事务。

普通已连接但无 active work 的 session 可在 graceful close 后由 durable session/load 恢复，不算 busy。

### 10.2 Activation

```text
staged
  -> waiting_for_idle
  -> shutdown old connections across all windows
  -> verify old process exited and leases released
  -> persist activating receipt
  -> select candidate command
  -> start + ACP health verify
  -> persist current verified and previous verified atomically
  -> ready
```

- 等待空闲不设强制取消超时；App 退出时保留 staged，下次启动在创建 Orion Code connection 前激活。
- 激活采用全局 single-flight lock；多个窗口只能观察同一任务。
- active pointer 持久化失败时不得启动 candidate。
- candidate 启动失败时，先完整关闭 candidate，再恢复 previous。
- previous 恢复失败时 fallback Native，并把 Orion Code 标记为可见 Failed；不得循环重启。

### 10.3 Retention

- 永远保留 current verified、previous verified 和正在使用的 staged。
- activation 成功并经过一个完整 App 重启后，才能清理更旧版本。
- 默认最多保留 3 个完整 archive install；清理必须再次验证路径和 receipt ownership。
- revoked 版本可以删除 binary，但其最小 receipt 应保留用于诊断与防重装。
- 实现只扫描已知 target 下的 managed `versions/`，执行前重放 inventory 与 receipt ownership；symlink、非法 semver、未知 target、路径逃逸或 inventory 漂移都会使清理 fail closed。

## 11. Pause、Revoke 与紧急回滚

### 11.1 Release status

| 状态 | 客户端行为 |
| --- | --- |
| `active` | 按频道、兼容和 rollout 正常选择 |
| `paused` | 不新增下载/激活；已运行 verified 版本继续使用 |
| `revoked` | 不下载、不激活、不再启动；切换本地安全版本或 Native |

在线发现当前运行版本被 revoked 时，立即拒绝创建新的 Orion Code prompt/session，并进入 `Draining Revoked Version`；已有任务遵守“不被自动更新取消”的边界，完成后立即 shutdown 并切换安全版本。若安全事件必须立刻终止正在执行的进程，应由独立 Studio 安全 hotfix 或明确用户操作完成，不在普通 update index 中加入隐式远程 kill 能力。

每次验签成功后，客户端用该 index 的完整 paused 集合替换本地 paused 状态，并将 revoked 版本单调并入 durable revoked 集合。这样 staged candidate 在下载后被暂停也不能继续激活，而已经获知的撤回离线后仍保持有效。

### 11.2 Signed rollback

`rollback_to` 必须满足：

- 来自更高 sequence 的有效签名 index。
- source release 被 paused/revoked，或 release record 明确说明 rollback。
- target exact version 存在且未 revoked。
- target 满足平台、Studio 和 ACP compatibility。
- 本地 verified target 可直接恢复；否则先按正常 archive 门禁下载验证。
- 当前 version floor 只能被这条已验证的 signed rollback transaction 临时覆盖；事务结束后将 floor 设置为 rollback target，并保留原撤回版本 receipt 防止重装。

客户端不得把任意较低 semver、官方 Registry 旧版本或 CDN 缓存解释成 rollback 授权。

### 11.3 离线策略

- 已获取并持久化的 revoked 集合离线时继续生效。
- index 过期但没有已知 revoke 时，允许当前 verified 继续运行。
- 不能在线确认的新 revoke 不推断、不猜测；UI 显示 last checked time。
- 安全事件需要立即阻止所有版本时，使用签名 index 明确 revoke，并让 Studio fallback Native，而不是发布损坏 archive。

## 12. UI 与诊断

External Agents / Orion Code 卡片增加：

- Installed version、Active source、Channel、Update mode。
- Last checked、Next automatic check、Index sequence。
- `Checking`、`Downloading`、`Verifying`、`Ready to Activate`、`Waiting for current task`、`Activating`、`Rolled back`、`Paused`、`Revoked`。
- candidate version、download `received / total` 字节与百分比、release notes。
- `Check Now`、`Download`、`Activate When Idle`、`Retry`、`Use Previous Version`、`Remove`、`Use Orion Agent`。

诊断导出允许包含：

- 脱敏版本、target、channel/mode、index sequence、SHA 前缀、状态时间、错误分类、exit code。

禁止包含：

- prompt、response、文件内容、命令输出正文、token、环境变量值、完整用户路径、installation ID、Apple/CI 凭据。

## 13. 发布和灰度操作流程

### 13.1 发布候选

1. 从 clean Orion Code commit 构建各 target production archive。
2. 运行 ACP、package、platform 与 clean-install 测试。
3. 生成 manifest、SBOM、notices 和 release receipt。
4. 在受保护 runner 完成平台签名、公证和验证。
5. 上传 immutable assets；读取远端 bytes 重新计算 SHA，必须与 receipt 一致。
6. 生成新 sequence 的 update index，签名，但尚不发布。
7. 在 fixture Studio 中重放 signed index -> download -> stage -> activate -> rollback journey。
8. 获得人工授权后原子发布 index 与 signature。

灰度期间 release assets 必须位于不会被官方 ACP Registry 自动提升为 latest Stable 的预发布来源；只有 100% rollout 与观察窗口完成后，才更新官方 Registry 可发现的 Stable release source。

### 13.2 建议灰度节奏

| 阶段 | Stable rollout | 最短观察窗口 | 前进条件 |
| --- | ---: | ---: | --- |
| Canary | 5% | 24 小时 | 无 P0/P1；下载、验证、启动和 rollback 指标健康 |
| Early | 25% | 24 小时 | 无新增阻断缺陷；支持渠道无集中失败 |
| Broad | 50% | 24 小时 | 平台分布与升级成功率稳定 |
| General | 100% | 48 小时 | 回滚演练通过；Release Notes 与支持文档完成 |

- 每次扩大 rollout 都生成更高 sequence 的签名 index。
- 指标只能在用户现有 telemetry 同意范围内使用；没有遥测时由人工 canary、错误报告和 CI 证据决定。
- 官方 ACP Registry 只在 Stable 达到 100% 并通过观察窗口后指向该版本，防止绕过 first-party rollout。

## 14. 开发任务与依赖

| 任务 | 仓库 | 依赖 | 主要产物 |
| --- | --- | --- | --- |
| UP-PRE-00 | 两仓 | 无 | 隔离基线、ADR 与文件所有权清单 |
| UP-CON-01 | 两仓 | UP-PRE-00 | UpdateIndexV1、artifact manifest、receipt schema 与 golden fixtures |
| UP-ST-01 | Studio | UP-CON-01 | settings + KVP v2 + v1 migration |
| UP-ST-02 | Studio | UP-CON-01 | signature/index fetch + candidate resolver |
| UP-ST-03 | Studio | UP-ST-01、UP-ST-02 | 6 小时 scheduler、backoff、wake handling |
| UP-OC-01 | Orion Code | UP-CON-01 | production-only sidecar archive builder |
| UP-OC-02 | Orion Code | UP-OC-01 | manifest/SBOM/notices/release receipt |
| UP-CI-01 | Orion Code | UP-OC-02 | platform build/sign/notary workflow |
| UP-ST-04 | Studio | UP-CON-01 | bounded download、安全解压、staging verifier |
| UP-ST-05 | Studio | UP-ST-04、UP-CI-01 | 平台签名/公证验证和 ACP preflight |
| UP-ST-06 | Studio | UP-ST-01 | global busy tracker 与 activation barrier |
| UP-ST-07 | Studio | UP-ST-02、UP-ST-05、UP-ST-06 | 原子激活、rollback、pause/revoke |
| UP-ST-08 | Studio | UP-ST-03、UP-ST-07 | 设置、进度、诊断和用户操作 UI |
| UP-REL-01 | 两仓 | UP-CI-01、UP-ST-07 | index generator、签名、fixture publisher |
| UP-INT-01 | 两仓 | UP-ST-08、UP-REL-01 | fake-feed 全链路与 clean-home E2E |
| UP-INT-02 | 两仓 | UP-INT-01 | signed macOS candidate + clean Mac Gatekeeper 验收 |
| UP-REL-02 | 两仓 | UP-INT-02 | 授权发布门禁与灰度 runbook |

### 14.1 可并行边界

- UP-CON-01 冻结后，UP-OC-01/UP-OC-02 与 UP-ST-01/UP-ST-02/UP-ST-06 可并行。
- UP-ST-03 只依赖 settings/index fetch，不等待 archive builder。
- UP-ST-04 可先用 deterministic fixture archive 开发；UP-ST-05 再接真实签名 candidate。
- UP-ST-08 在状态枚举冻结后可与 UP-ST-07 后半段并行，但不能自己发明新状态。
- 同一文件只允许一个 Agent 修改；同一台 16 GB Mac 只运行一个重型 Rust lane，使用 `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0`。

### 14.2 当前任务状态

| 范围 | 开发状态 | 发布状态 |
| --- | --- | --- |
| `UP-CON-01`、`UP-ST-01`～`UP-ST-08` | `IMPLEMENTED`；当前 Studio 定向测试证据见 implementation matrix | 正式 trust constants 未冻结 |
| `UP-OC-01`、`UP-OC-02` | `IMPLEMENTED`；app-like local candidate、manifest、SBOM、notices、receipt/replay 代码存在 | 当前 local builder 仍明确 `NOT RELEASABLE` |
| `UP-CI-01`、`UP-REL-01` | `IMPLEMENTED`；protected signing/notary/finalization、production index signer、same-version policy transition 与 publisher 代码存在；当前 release tooling `36/36` 通过 | 真实凭据、protected run、生产远端 replay 均 `NOT RUN` |
| `UP-INT-01` | `IMPLEMENTED`；exact-byte signed fixture 覆盖 feed -> download byte progress -> stage -> busy barrier -> activation commit，并含失败恢复/撤回路径 | 不替代 signed clean-Mac journey |
| `UP-INT-02`、`UP-REL-02` | 实施说明与 runbook 已完成 | `BLOCKED / NOT AUTHORIZED / NOT STARTED` |

## 15. 详细任务卡

### UP-PRE-00：冻结基线与 ADR

**步骤：**

1. 记录两仓 branch、full SHA、remote、ahead/behind、status 和用户未提交文件所有权。
2. 从已确认基线创建隔离 worktree；不得在用户脏工作树实施。
3. 写 ADR：first-party signed index、Stable archive、channel/mode 分离、idle activation、rollback/revoke。
4. 列出目标文件与每个并行 Agent 的独占所有权。

**完成标准：** 基线可复现、ADR 已审阅、没有覆盖用户改动。

### UP-CON-01：冻结更新契约

**建议文件：** Studio `crates/project` 或独立窄 crate 的 schema；Orion Code `docs/architecture` 和 JSON schema/fixtures。

**步骤：**

1. 定义 UpdateIndexV1、TargetV1、ArtifactManifestV1、InstallReceiptV2。
2. 固定 exact bytes detached signature、sequence、expires、key ID 和 key rotation 规则。
3. 固定 semver/channel/compatibility/rollout/revoke/rollback 选择算法。
4. 生成 golden valid/invalid fixtures；两个仓库必须读取同一份事实源或由 CI 校验 digest 一致。
5. 冻结错误分类，不把任意网络错误解释为签名错误。

**必须测试：** schema round-trip、未知字段策略、越界 rollout、过期、sequence replay、错误签名、未知 key、无 target、恶意 URL、非法 command、rollback abuse。

### UP-ST-01：设置、状态机与迁移

**允许文件：** `settings_content` Agent 设置、`orion_code_bootstrap.rs` 的窄迁移或新 update state 模块、测试。

**步骤：**

1. 增加 channel/mode settings 与 user-scope schema。
2. 新增 KVP state-v2，不无限扩张 bootstrap v1 record。
3. 实现 v1 -> v2 幂等迁移与损坏记录 fallback。
4. 设置变更通过 generation 防止旧异步任务写回。
5. Manual/Removed/Declined/Custom 时取消 future work，但不删除 verified install。

**完成标准：** 所有迁移矩阵和并发设置变更测试通过。

### UP-ST-02：可信索引与候选解析

**步骤：**

1. 实现 bounded index/signature fetch、ETag cache 和 single-flight。
2. 先验签 exact bytes，再解析 JSON。
3. 保存最高 sequence；拒绝 replay、未知 key 和越界时间。
4. 纯函数实现 candidate resolver，输入包含 setting、Studio version、platform、cohort、current/previous、revoked。
5. Orion Code 保留 ID 不再以官方 Registry 高版本作为自动更新 authority。

**完成标准：** candidate 决策无网络/FS side effect，表驱动测试覆盖所有过滤顺序。

### UP-ST-03：6 小时更新调度器

**步骤：**

1. 参考 App `AutoUpdater` 的 GPUI timer/wake pattern，创建独立全局 coordinator。
2. startup jitter、6 小时周期、manual check、backoff 和 wake restart 都走单一 task。
3. App quit 时取消网络 future，保留已完成 staging。
4. 自动失败静默、手动失败可见；更新 last checked 与 next check。
5. 测试使用 GPUI executor timer，不使用依赖 `run_until_parked()` 的 `smol::Timer`。

**完成标准：** fake clock 下不重复检查、不忙等、不因休眠产生并发任务。

### UP-OC-01：production-only sidecar builder

**步骤：**

1. 增加 `build-acp-sidecar`，输入 exact version、git SHA、target 与输出目录。
2. 构建 ACP ESM island、所需 runtime、embedded Node 和 production dependencies。
3. 排除 Web/TUI 开发资产、tests、cache、`.env` 和 release secrets。
4. 生成可移动目录并从含空格/中文路径运行 ACP smoke。
5. 二次构建比较 manifest，消除 timestamp、路径和文件顺序漂移。

**完成标准：** archive 不依赖全局 Node/npm，ACP initialize/prompt/cancel/load/close 通过。

### UP-OC-02：manifest、SBOM 与 license

**步骤：**

1. 对 archive 内所有文件生成排序 manifest 与 SHA。
2. 生成 CycloneDX SBOM 或经 ADR 选择的等价标准格式。
3. 汇总 LICENSE 与 THIRD_PARTY_NOTICES，缺失 license 阻断 release。
4. 输出机器可读 release receipt，绑定 source SHA、artifact SHA、manifest/SBOM/notices digest 和测试结果。
5. 为 receipt 写重放校验器。

**完成标准：** receipt 从最终 bytes 重算全部 digest；不信任构建过程中的临时值。

### UP-CI-01：平台签名与公证流水线

**步骤：**

1. 无凭据 PR job 只构建 unsigned candidate，并明确 `NOT RELEASABLE`。
2. 受保护 release job 才读取 signing/notary secrets，日志不得输出凭据。
3. macOS 内到外签名、notarytool submit、日志检查、staple、codesign/spctl/stapler 验证。
4. 上传最终 archive 后重新下载并校验 SHA。
5. 只有 final signed/notarized bytes 进入 update index generator。

**完成标准：** unsigned、签名失败、公证 rejected、staple 失败或远端 SHA 不一致均 fail closed。

### UP-ST-04：下载、安全解压与 staging

**步骤：**

1. 实现 size/timeout/redirect/host 限制和 streamed SHA。
2. `.partial` 与 staging 使用唯一 operation ID，崩溃后按 receipt 恢复或清理。
3. 安全解压拒绝 traversal、link escape、duplicate、device 和 bomb。
4. 校验内嵌 manifest 与所有文件，原子晋升 staged。
5. 清理只作用于已验证 managed path；不得删除 current/previous/user data。

**完成标准：** fault-injection 覆盖每个 I/O 边界和 App 中断点。

### UP-ST-05：平台验证与 ACP preflight

**步骤：**

1. 抽象 platform verifier；macOS 实现 Developer ID、Hardened Runtime、notary/Gatekeeper 验证。
2. 校验 expected Team ID、bundle ID/identifier 与 archive manifest。
3. 从 staging 用临时 config/data 启动 candidate，不读取用户 `.env`。
4. 验证 exact identity/version/protocol/load/close 后显式 shutdown 并确认无孤儿进程。
5. 预检不获取真实 session lease、不执行用户工具、不访问项目内容。

**完成标准：** 篡改任一 nested binary、manifest 或 signature 均阻断 staging。

### UP-ST-06：全局 busy tracker 与 activation barrier

**步骤：**

1. 为 Orion Code connection/session 暴露 active turn/tool/permission/load/shutdown 计数与 idle event。
2. 汇总所有窗口/AgentConnectionStore；任一 busy 即保持 staged。
3. busy -> idle 只触发一次 activation wakeup。
4. App 退出保留 staged；下次启动在第一个 sidecar connection 前激活。
5. 不改变其他 ACP Agent 的连接语义。

**完成标准：** 两窗口并发、长工具、pending permission、load、连接中和退出竞态测试通过。

### UP-ST-07：激活、回滚、暂停和撤回

**步骤：**

1. 实现全局 activation mutex 与 generation。
2. await 所有旧连接 shutdown，再切 candidate。
3. candidate 复检成功后原子写 current/previous；失败恢复 previous。
4. 实现 paused/revoked 与 signed rollback_to。
5. 无安全版本时显式 fallback Native，不重启循环。
6. retention 只删除不被 receipt 引用的旧 managed versions。

**完成标准：** 每个持久化、spawn、shutdown 和 rollback 失败点都有 deterministic 测试。

### UP-ST-08：设置与诊断 UI

**步骤：**

1. 增加 Stable/Beta/Manual 简化选择，并正确映射内部 channel/mode。
2. 展示 update status、progress、candidate、last/next check 和 waiting-for-idle。
3. 提供 Check、Download、Activate When Idle、Retry、Use Previous、Use Native、Remove。
4. paused/revoked 使用明确安全文案，不能只显示普通下载失败。
5. 诊断复制执行敏感字段过滤测试。

**完成标准：** 键盘可达、屏幕阅读标签和窄窗口布局通过；状态重启后不跳变。

### UP-REL-01：索引生成、签名与 fixture 发布

**步骤：**

1. generator 只接受通过 receipt replay 的 immutable artifacts。
2. 自动计算下一个 sequence，拒绝重复 version/target、SHA 冲突和越界 rollout。
3. 输出 deterministic index bytes 和 detached signature。
4. 提供本地 fixture server 与 test key；test key 不能进入 release key set。
5. 正式 publish 命令默认 dry-run，必须显式 release authorization 才能写远端。

**完成标准：** 相同输入生成相同 bytes；一字节篡改导致客户端验签失败。

### UP-INT-01：全链路自动化

覆盖：

- Stable automatic、Beta prerelease、Manual。
- startup jitter、6 小时、backoff、wake、ETag 304。
- 5% -> 25% -> 100% cohort 单调性。
- min Studio 不满足、平台缺失、协议不兼容。
- 下载中断、SHA 错、archive bomb、签名错、公证错、ACP preflight 错。
- active task 不被终止、idle 后切换、App restart 前切换。
- candidate failure -> previous；previous failure -> Native。
- paused、revoked、signed rollback、index replay、expired offline。
- v1 accepted/declined/removed/custom 到 v2 迁移。

### UP-INT-02：真实 macOS candidate 验收

至少在一台非构建机或干净 macOS 环境完成：

1. 安装已签名 Orion Studio candidate。
2. 从签名 fixture/预发布 index 更新真实 notarized Orion Code sidecar。
3. Gatekeeper、codesign、spctl、stapler 全部通过。
4. 运行真实 ACP prompt/tool/permission/cancel/load/restart。
5. prompt 运行中下载完成但不切换；任务结束后切换。
6. 断网运行 current；恢复网络后检查更新。
7. 注入撤回并回滚；检查无孤儿进程和数据丢失。

### UP-REL-02：授权发布门禁

只有以下全部成立才可标记 `GO FOR AUTHORIZED CANARY`：

- 两仓 candidate SHA、review 和 release receipts 齐全。
- UpdateIndex schema/golden、Studio tests、Code tests、archive tests 和真实 App tests 全部通过。
- 每个已声明 target 有最终 artifact SHA、SBOM、notices、签名与平台验证证据。
- macOS target 有 Developer ID、notary accepted、staple/Gatekeeper 和干净机证据。
- rollback、paused、revoked 和 index replay 演练通过。
- 5% canary index 已生成并签名但尚未发布。
- 用户明确授权发布 index、release assets 和灰度范围。

## 16. 测试门禁

### 16.1 Studio

```bash
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p project orion_code
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p agent_servers acp
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p agent_ui orion_code
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo +stable test -p auto_update
cargo +stable fmt --all -- --check
git diff --check
./script/clippy -p project -p agent_servers -p agent_ui -p auto_update -p settings_content
```

本轮计划范围的 scoped Clippy 已通过。最终候选还需在 clean exact SHA 上执行仓库要求的全仓无参数 `./script/clippy`。若磁盘不足，不得并发重建多个 target；先记录现有 target 大小和剩余空间。

### 16.2 Orion Code

```bash
npm run lint
npm run build
npm test -- --runInBand
npm run test:acp
npm run release:check -- --skip-tests
npm run release:artifact -- --out-dir <temp-dir> --receipt <temp-dir>/artifact.json
```

新增 archive/index scripts 后还必须运行：

```bash
npm run release:acp-sidecar -- --target <target> --out <temp-dir>
npm run release:update-index -- \
  --receipts <receipt-dir> \
  --out <empty-temp-dir> \
  --generated-at <RFC3339> \
  --expires-at <later-RFC3339> \
  --status active \
  --rollout-basis-points 10000 \
  --channel stable \
  --studio-version-requirement '<frozen-semver-requirement>' \
  --rollout-salt <immutable-local-salt> \
  --archive-base-url <https-immutable-base-url> \
  --release-notes-url <https-release-notes-url> \
  --allow-unsigned-fixture \
  --test-private-key <local-test-key.pem> \
  --test-key-id local-test-<id>
```

local generator 始终是 dry-run，不需要也不能借此获得发布权限。命令名称是目标接口；实现任务若需要调整，必须先更新本计划与 package scripts 一致性测试。

### 16.3 安全测试

- signature bit flip、wrong key、unknown key、sequence replay、expired index。
- URL redirect escape、oversize、truncated、SHA mismatch。
- zip-slip、symlink/hardlink escape、duplicate entry、device file、decompression bomb。
- nested binary tamper、wrong Team ID、wrong bundle identifier、missing notarization。
- malicious rollback、revoked target、official Registry bypass attempt。
- diagnostic secret sentinel 和绝对路径脱敏。

### 16.4 文档门禁

```bash
DOCS_WRITER_SCRIPTS="<path-to-docs-writer-skill>/scripts"
python3 "$DOCS_WRITER_SCRIPTS/check_md_links.py" \
  docs/plan/orion-code-managed-sidecar-update-v2-plan.md
python3 "$DOCS_WRITER_SCRIPTS/check_code_blocks.py" \
  docs/plan/orion-code-managed-sidecar-update-v2-plan.md
```

## 17. 观测与隐私

允许的聚合事件：

- channel/mode、release version/target、update phase、duration、错误分类、rollback/Native fallback。
- rollout basis points 与本机 `eligible=true/false`；不发送 cohort 数值或 installation ID。
- 签名、公证、ACP health 的布尔结果；不发送用户数据。

禁止：

- prompt/response、工具参数/输出、文件内容、环境变量、token、完整路径。
- signing key、Apple credentials、CI secret、index signing payload 的私钥材料。
- 将更新同意与模型/项目内容遥测绑定。

## 18. 回滚与功能开关

1. Studio 始终保留 Native Agent fallback。
2. coordinator 有 compile-time/default-off emergency switch，可停止自动检查而不删除安装。
3. 设置改为 Manual 可立即停止未来自动任务。
4. current/previous/staged receipts 使本地 rollback 不依赖网络。
5. first-party index 不可用时，官方 Registry 不得偷偷成为自动更新 authority。
6. 更新系统代码出现严重问题时，可发布 Studio hotfix 禁用 coordinator；不得远程删除用户 session data。

## 19. 执行者回执格式

每个任务只提交其允许范围，并报告：

```text
Task: UP-ST-04
Status: PASS | PARTIAL | BLOCKED | FAIL
Baseline: repository + branch + full SHA
Files changed: exact paths
Contract version: schema/index/receipt versions used
Commands run: exact commands
Tests: exact passed/failed/not-run counts
Artifacts: filenames + bytes + SHA-256
External actions: NOT PUSHED / NOT PUBLISHED / NOT SIGNED unless explicitly authorized
Remaining risks: concrete list
Next allowed task: one task ID
```

规则：

- 不将 source build、unsigned artifact、Developer ID signing、notarization、upload、index publish 和 rollout 混为一个 PASS。
- 测试没跑必须写 `NOT RUN`；不能写“应该通过”。
- 没有签名凭据不是代码任务失败，但 release 必须保持 `BLOCKED`。
- 不使用 `git clean`、`git reset --hard` 或宽路径删除。
- 临时目录和 target 清理由明确绝对路径完成；先验证 realpath 和所有权。

## 20. 版本完成定义

本版本只有在以下全部存在时才算开发完成：

1. Stable/Beta/Manual UI 与内部 channel/mode 契约一致。
2. 启动检查、6 小时调度、backoff、wake 和 Manual 均有 deterministic 测试。
3. signed index、sequence replay 防护、灰度、compatibility、paused、revoked 和 rollback 完整闭环。
4. archive 后台下载与验证不阻断当前 Orion Code 使用。
5. 活跃任务不会被自动更新中断，idle 或下次启动能完成切换。
6. current/previous/staged 状态原子、崩溃可恢复、失败可回滚。
7. Orion Code Stable archive 不依赖全局 Node/npm，并包含 manifest、SBOM 和 notices。
8. macOS Apple Silicon candidate 完成真实 Developer ID 签名、公证、Gatekeeper 和干净机测试。
9. 两仓自动化、跨仓 E2E、安全测试、诊断脱敏和文档门禁通过。
10. 所有外部发布动作有独立人工授权和可重放 release receipt。

达到 1～7 但缺少签名凭据或真实发布证据时，只能标记：

```text
IMPLEMENTED / RELEASE BLOCKED
```

截至 2026-09-01，当前两个隔离 worktree 使用这一状态。它只说明本地开发范围已实现，不表示 production configuration 已启用，也不表示 signed candidate、notarization、clean Mac 或 canary 已完成。

不得写成 `Released`、`Stable available` 或 `100% rollout complete`。

## 21. 官方参考

- [ACP Registry Format](https://github.com/agentclientprotocol/registry/blob/main/FORMAT.md)
- [ACP Agent Registry schema](https://github.com/agentclientprotocol/registry/blob/main/agent.schema.json)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Apple: Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
- [Apple: Creating distribution-signed code for macOS](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/)
- [Apple: Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates)
