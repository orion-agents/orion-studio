# Orion Code Update v2 实现证据矩阵

## 1. 结论

| 字段 | 结论 |
| --- | --- |
| 审计日期 | 2026-09-01 |
| 总状态 | `IMPLEMENTED / RELEASE BLOCKED` |
| Studio 代码 | 频道/模式、签名索引、调度、下载/进度、平台验证、真实 ACP session lifecycle、staging、busy activation、rollback、pause/revoke、retention、UI 与启动配置均已接线 |
| Orion Code 代码 | app-like archive、manifest/SBOM/notices、最终 release receipt、same-version policy transition、受保护签名/公证和条件 publisher 路径已实现 |
| Studio 当前定向验证 | 主 agent 已确认：`agent_ui 73/73`、`agent_servers 35/35`、`project 19/19`、core contract `10/10`、settings `5/5`、`auto_update 23/23`、fmt 与 scoped release/all-targets/all-features Clippy `PASS` |
| Orion Code 本地验证 | 当前代码：release tooling `36/36`、lint/Prettier/build/ACP `PASS`、Jest 313 suites（3755 passed，5 skipped）；unsigned candidate/replay 与 local-test-key index dry-run `PASS` |
| Developer ID / 公证 | `NOT RUN`；没有真实证书签名、notary accepted、staple、Gatekeeper 或远端 archive replay 回执 |
| 干净 Mac | `NOT RUN` |
| 外部灰度 | `NOT AUTHORIZED / NOT STARTED` |
| 外部动作 | `NOT PUSHED / NOT UPLOADED / NOT PUBLISHED / NOT SIGNED / NOT NOTARIZED` |

当前可以标记本地开发范围为：

```text
IMPLEMENTED / RELEASE BLOCKED
```

这不等于 `GO FOR AUTHORIZED CANARY`、`Released`、`Stable available` 或 `100% rollout complete`。

## 2. 证据口径

### 2.1 状态定义

| 状态 | 本矩阵中的含义 |
| --- | --- |
| `PASS` | 本地开发任务已实现，且存在主 agent 已确认的对应自动化证据；不自动代表可发布 |
| `PARTIAL` | 代码主体存在，但仍有仓库内实现、接线、测试或文档漂移 |
| `BLOCKED` | 依赖正式配置、凭据、干净环境、外部系统或人工授权，当前不能完成 |

### 2.2 四层证据必须分开

1. **静态实现**：当前隔离 worktree 中存在代码与接线。
2. **本地自动化**：只证明 fixture、fake feed、unsigned candidate 或本机定向测试。
3. **真实签名发布**：要求 Developer ID、notary accepted、staple/Gatekeeper、最终 archive receipt 和远端复下载 SHA。
4. **干净 Mac / canary**：要求非构建机真实 journey、分阶段观察和可重放 pause/revoke/rollback 回执。

下层 `PASS` 不能向上推导更高层 `PASS`。

### 2.3 本轮实现与验证边界

- 本轮在两个隔离 worktree 中修改了 Studio/Orion Code 源码、测试与文档，并运行了下列 Cargo/npm 本地门禁。
- 本轮生成并重放了本地 unsigned archive 与 local-test-key index；它们明确标记为 `NOT_RELEASABLE / NOT_PUBLISHED / NOT_SIGNED_WITH_RELEASE_KEY`。
- 本轮没有使用 Developer ID、notary 或正式 release-key 凭据，也没有 push、PR、签名、公证、上传、发布或灰度动作。
- 两仓仍为 dirty isolated worktree，未形成 reviewed clean committed exact tag，因此本地 `PASS` 不是正式 release evidence。

## 3. 关键闭环静态核对

| 能力 | 静态实现证据 | 当前判定 |
| --- | --- | --- |
| Durable paused block | `orion_code_update.rs` 持久化、校验 `known_paused_versions`；`orion_code_update_coordinator.rs` 从每个已验签 index 替换 paused 集合；activation 在 stage 与 activate 两处拒绝 paused version | `IMPLEMENTED` |
| Revoked drain / new-session guard | `agent_connection_store.rs` 在创建或恢复 Orion Code connection 前检查 durable current revoke；`orion_code_update_activation.rs` 观察 update record，等待全局 activity idle 后才 shutdown/switch | `IMPLEMENTED` |
| Safe previous / Native fallback | 安全 previous 走同一 durable activation barrier；无安全 previous 时清除 managed runtime，`agent_panel.rs` 把已打开窗口与全局偏好切到 `NativeAgent`，不删除用户数据 | `IMPLEMENTED` |
| 真实 ACP preflight lifecycle | `agent_servers/src/acp.rs` 执行 `initialize -> session/new -> session/load -> session/close`，关闭 transport，限时等待退出，失败/超时清理进程组 | `IMPLEMENTED` |
| Retention | `orion_code_update_retention.rs` 保护 current/previous/staged，使用 App start sequence 作为 restart grace，最多保留 3 个完整安装，revoked binary 可删而 receipt 保留；coordinator 启动时接线 | `IMPLEMENTED` |
| Download byte progress | installer bounded streamed progress；pipeline 使用 latest-value channel 保证单调；External Agents UI 显示 `received / total` 与百分比，固定诊断 `archive_download_in_progress` | `IMPLEMENTED` |
| Embedded production startup | `agent_ui::init` 调用 `configure_embedded_orion_code_managed_update`；production hosts/test keys 有校验；正式常量未冻结时 `None` 并保持 feed/installer/health check disabled | `IMPLEMENTED / FAIL-CLOSED` |
| GPUI scheduler | coordinator 使用 GPUI executor timer；测试覆盖 30～90 秒 jitter、精确 6 小时、15 分钟 -> 1 小时 -> 6 小时 backoff、wake overdue 和 manual single-flight | `IMPLEMENTED` |
| Signed feed -> install E2E | macOS Apple Silicon fixture test覆盖 exact-byte Ed25519、candidate resolve、受控 streamed download、progress、manifest/SBOM、platform/preflight seam、stage、busy barrier 与 atomic activation commit | `IMPLEMENTED / FIXTURE ONLY` |
| Protected release pipeline | Orion Code workflow与脚本静态覆盖 app-like build、临时 keychain、内到外 codesign、notarytool、stapler、final receipt replay、production Ed25519 signing、same-version policy transition、immutable upload和远端 SHA replay | `IMPLEMENTED / REAL RUN NOT EXECUTED` |

## 4. 逐任务证据矩阵

| Task | 状态 | 本地实现与测试证据 | 发布限制 |
| --- | --- | --- | --- |
| `UP-PRE-00` | `PARTIAL` | 隔离 worktree、branch/baseline 和 ADR 已记录 | ADR 尚需人工审阅；隔离 diff 未形成可复现 commit |
| `UP-CON-01` | `PASS` | 两仓 contract/schema/fixtures 与 exact-byte Ed25519 已实现；core + fixture 合计 `10/10`；Orion Code contract README 已与当前 receipt schema 对齐 | 正式 production contract 仍需随 clean release SHA 审阅冻结 |
| `UP-ST-01` | `PASS` | channel/mode、state-v2、v1 migration、app-start/activation receipts；settings 合计 `5/5`，相关测试包含在 `agent_ui 73/73` | 无生产发布含义 |
| `UP-ST-02` | `PASS` | bounded signed feed、ETag、sequence/expiry、candidate resolver、durable paused/revoked；core `10/10` 与 `agent_ui 73/73` | 正式 endpoint、hosts 和 Ed25519 trust set 未冻结 |
| `UP-ST-03` | `PASS` | 全局 scheduler、startup jitter、6 小时、backoff、wake、manual/single-flight；真实 GPUI timer tests 包含在 `agent_ui 73/73` | production feed fail-closed |
| `UP-OC-01` | `PASS` | app-like `OrionCodeSidecar.app/Contents/MacOS/orion-code-acp`、embedded Node、deterministic ZIP、ACP smoke 已由当前 release tooling `36/36`、build 与 `test:acp PASS` 验证 | local builder 明确 `NOT RELEASABLE`；真实 protected build 未执行 |
| `UP-OC-02` | `PASS` | manifest、CycloneDX SBOM、LICENSE/notices、external receipt 和 replay verifier 已实现 | 正式 receipt 必须来自真实 signed/stapled final bytes |
| `UP-CI-01` | `PASS` | protected workflow 与 signing/notary/finalization/publisher 代码已实现并 fail closed；当前 release tooling `36/36`、lint 与 Prettier 通过 | 真实凭据运行、公证、生产远端 replay 均 `NOT RUN` |
| `UP-ST-04` | `PASS` | bounded download、streamed SHA/bytes、host/redirect/size/timeout、安全解压、manifest/SBOM、atomic staging；包含在 `agent_ui 73/73` | 真实生产 archive 未下载 |
| `UP-ST-05` | `PASS` | macOS Team/bundle/Hardened Runtime/get-task-allow/codesign/spctl/stapler verifier；平台命令异步执行；真实 ACP session lifecycle；`agent_servers acp 35/35`、`agent_ui 73/73` | 真实 signed/notarized candidate仍 `NOT RUN` |
| `UP-ST-06` | `PASS` | turn/tool/permission/load/startup/shutdown/version-switch activity 与 all-store barrier；包含在 `agent_ui 73/73` | 干净 Mac 多窗口 journey 未执行 |
| `UP-ST-07` | `PASS` | durable activation、busy->idle、health recheck、rollback、paused block、revoked drain、新连接 guard、previous/Native fallback、restart-safe retention；包含在 `agent_ui 73/73` | 生产 revoke/rollback index 未发布 |
| `UP-ST-08` | `PASS` | Stable/Beta/Manual、Check/Download/Activate/Retry/Previous/Native、status/bytes/signing/notary diagnostics、窄布局和 UI helper tests；包含在 `agent_ui 73/73` | 真实 signed candidate UI journey 未执行 |
| `UP-REL-01` | `PASS` | deterministic exact-byte index、production Ed25519 signer、same-version rollout/pause/revoke/rollback policy和默认 dry-run publisher 已实现；当前 tooling `36/36` 与 local-test-key index dry-run 通过 | 正式 key和远端 publication未使用 |
| `UP-INT-01` | `PASS` | signed fixture全链路、failed health restore、interrupted activation recovery、revoked staged block、revoked current drain/previous/Native path；包含在 `agent_ui 73/73` | fixture platform/preflight 不替代 Apple/clean-Mac evidence |
| `UP-INT-02` | `BLOCKED` | runbook 与验收清单存在 | signed Studio/Code candidate、notary、Gatekeeper、非构建机与真实 ACP journey均 `NOT RUN` |
| `UP-REL-02` | `BLOCKED` | fail-closed authorization与阶段回执格式已写入 runbook/publisher | 未获上传、index publish或任何 canary授权 |

`UP-CI-01`、`UP-REL-01` 已达到本地实现与自动化 `PASS`，但这里只证明 fail-closed 代码、fixture 与 unsigned dry-run；真实 Developer ID/notary/release-key/远端 publication 未执行，所以总状态仍为 `IMPLEMENTED / RELEASE BLOCKED`。

## 5. 已确认测试结果

### 5.1 当前 Studio 隔离 worktree

| 范围 | 主 agent 已确认结果 | 证据边界 |
| --- | ---: | --- |
| `cargo test -p agent_ui orion_code` | `73/73` | 本轮最终源码重跑；包含设置/状态、scheduler、installer/progress、异步平台验证、activation、retention、UI 与 fixture E2E；不含真实 Apple签名 |
| `cargo test -p agent_servers acp` | `35/35` | 包含实际 preflight session lifecycle fake-process tests |
| `cargo test -p project orion_code` | `19/19` | managed runtime/path/registry 边界 |
| `cargo test -p orion_code_update` | `10/10` | core unit + cross-repo contract fixtures |
| settings 定向测试 | `5/5` | `settings_content 3/3` + `agent_settings 2/2` |
| `cargo test -p auto_update` | `23/23` | Studio updater 回归边界 |
| `cargo fmt --all -- --check` | `PASS` | 当前 Studio diff 格式通过 |
| scoped `./script/clippy -p project -p agent_servers -p agent_ui -p auto_update -p settings_content` | `PASS` | release、all-targets、all-features、warnings denied；全仓无参数 Clippy 留给 clean release SHA |

`agent_ui 73/73`、fmt 与 scoped Clippy 是最终源码后的本轮重跑；其余定向结果来自同一实施目标中、相关代码切片完成后的主 agent 运行。

### 5.2 当前 Orion Code 隔离 worktree

| 范围 | 已确认结果 | 证据边界 |
| --- | --- | --- |
| `npm run test:release-tooling` | `36/36` | 当前 protected release、publisher、CLI lifecycle 与 early-child-exit 修复均包含在内 |
| `npm run lint` | `PASS` | 当前源码 |
| `npx prettier --check 'scripts/release/*.mjs' 'scripts/release/__tests__/*.mjs'` | `PASS` | release scripts 与 tests |
| `npm run build` | `PASS` | 当前源码 |
| `npm test -- --runInBand` | `PASS` | 313 suites；3755 passed，5 skipped，3760 total |
| `npm run test:acp` | `PASS` | package ACP smoke，不是 Studio clean-Mac E2E |
| `npm run release:check -- --skip-tests` | `EXPECTED NO-GO` | 仅 dirty worktree 门禁失败；其余 version/changelog/diff/lint/tsc/package checks 通过 |
| local unsigned candidate + receipt replay | `PASS / NOT_RELEASABLE` | archive、manifest、SBOM、notices digest 重放通过；未签名、未公证、未上传 |
| local-test-key update index | `PASS / DRY_RUN` | exact-byte Ed25519 fixture；`NOT_PUBLISHED / NOT_SIGNED_WITH_RELEASE_KEY` |

以上均为本地实现证据。真实 signing、notary、protected publisher、生产远端 replay 和任何外部写入仍未运行。

### 5.3 本次文档门禁

| 检查 | 本次结果 |
| --- | --- |
| `check_md_links.py`：计划、implementation matrix、runbook | `PASS`：全部链接存活、无残留 token |
| `check_code_blocks.py`：计划、implementation matrix、runbook | `PASS`：全部代码块已标注且配平 |
| Studio worktree `git diff --check` | `PASS` |
| Orion Code worktree `git diff --check` 与 contract README 文档门禁 | `PASS` |

## 6. 当前硬门禁

### G-01：冻结并嵌入 Studio production trust

以下值仍未冻结，`embedded_orion_code_managed_update_configuration()` 因而保持 `None`：

- production index URL 与 detached signature URL。
- feed/redirect/archive host allowlist。
- Ed25519 release `key_id` 与 public key bytes。
- Apple Developer Team ID。
- Orion Code sidecar bundle identifier。

不得使用环境变量或测试 key 绕过该门禁。

### G-02：形成 clean reviewed release source

两仓均为 dirty isolated worktree。必须先完成人工 review、形成 exact clean commits/tags，并在这些 exact SHA 上重放仓库级完整门禁，才能产生正式 candidate。本轮通过的 scoped Clippy 与 Node 全量门禁不能替代 clean release SHA 证据。

### G-03：执行真实 protected pipeline

代码路径存在，但没有以下回执：

- Developer ID identity 与临时 keychain signing。
- Hardened Runtime / secure timestamp / entitlement 检查。
- `notarytool Accepted`、notary log、staple、`codesign`、`spctl`、`stapler validate`。
- signed/stapled final archive 与 production receipt replay。
- 正式 Ed25519 index signature。
- immutable upload、远端重新下载与 SHA-256 replay。

### G-04：signed clean-Mac 验收

没有签名 Studio candidate、非构建机、离线 Gatekeeper 或真实 Orion Code ACP prompt/tool/permission/cancel/load/restart journey。

### G-05：外部 canary 与撤回

5%、25%、50%、100%、paused、revoked/rollback 均未发布、未观察、未授权。任何阶段都需要 exact sequence、signature、receipt、观察窗口和单独人工授权。

## 7. 下一步门禁顺序

1. 人工 review 两仓隔离 diff，并形成 clean commits/tags。
2. 在 exact clean SHA 上重跑两仓完整门禁，包括 Studio 全仓无参数 Clippy。
3. 冻结 production endpoint/hosts、Ed25519 key、Team ID 与 bundle ID，并嵌入 Studio；重新编译验证 fail-open 不可能发生。
4. 单独授权使用 Developer ID/notary/release-key 凭据，在 protected environment 生成 signed candidate，但先保持 publisher dry-run。
5. 重放最终 receipt、远端 SHA、codesign/stapler/spctl，并在 clean Mac 完成真实 ACP journey。
6. 生成并审阅 5% signed index；只有获得 exact 发布授权后才开始 canary。
7. 每一阶段独立观察，再决定 25% -> 50% -> 100% 或 paused/revoked rollback。
