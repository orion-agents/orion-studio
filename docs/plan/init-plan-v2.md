# Orion Studio 重构续作计划 v2

## 2026-08-15 Finalization Update

> **当前生效状态：** 本节是截至 2026-08-15 的最终本地验收结论，覆盖下方与之冲突的旧结论。下方旧状态、旧命令结果和旧阻塞记录仅作为历史证据保留，不得再将其中的 `PARTIAL`、`PENDING` 或 Metal/WebRTC 阻塞描述当作当前状态。

**Init v1/v2 本地源码收口：`DONE / GO-WITH-CONDITIONS`；macOS Dev 产物：`VERIFIED`；Production release：`NO-GO`。**

- **仓库与交付边界：** 验收基线为 `init@99023dd28964dc1ef729eca2e606b6194d0ace06`；仓库是仅含 2 个 commit 的 shallow checkout，共享工作树仍有大量既有 WorkBuddy/用户改动。本轮没有 stage、commit、push、tag、PR 或 release。
- **品牌门禁：** `./script/check-orion-brand` 最终 **PASS**：扫描 4,217 个文件和 263 个符号链接目标，6,446 个命中全部被逐行 allowlist 解释，`unapproved=0`、`stale=0`、`ambiguous=0`、`errors=0`；allowlist SHA-256 为 `b67e5990a56bdf844acb8779df1ed46204943a55a7911225960d3f320c5dd538`。
- **外部身份契约：** 主 CLI 为 `orion-studio`，短别名为 `orion`；新生成的 deep link、schema URI、provider identity、mention URI、凭据与服务地址均使用 Orion 命名。旧名称只保留在明确的输入兼容、迁移、ABI、上游来源、许可证和测试夹具边界。
- **Workflow fail-closed：** 源文件与生成 YAML 已同步；`release`、`nightly`、`after_release`、`deploy_docs` 分别有 14、12、4、1 个生产副作用 job 绑定 `production` environment，并分别受稳定版、夜间版和 after-release 显式开关约束；相关 xtask 22 个测试 **PASS**。
- **编译与回归：** main `cargo +stable check -p orion-studio --bin orion-studio`、完整 `./script/clippy`、format、workflow generation/check、`git diff --check`、todo/keymap/license 检查、Docker/AppX/XML/entitlements 静态门禁均 **PASS**；`paths` 51 个、`cli` 7 个、`deploy_collab` 7 个测试及约定聚焦测试均 **PASS**。
- **macOS Dev 产物：** `target/aarch64-apple-darwin/release/Orion-Studio-aarch64.dmg` 已生成并通过 `hdiutil verify`；大小 150,297,095 bytes（143 MiB），SHA-256 为 `3fa0c88c88fcc1d1c841adc6a6b338b7aed3815df229164616f33b2a00981f2d`。
- **App 验收：** DMG 内为 `Orion Studio Dev.app`，`CFBundleIdentifier=dev.orion.OrionStudio-Dev`、`CFBundleExecutable=orion-studio`、版本 `1.16.0`，主程序为 arm64；Info.plist lint 与扩展键去重、`codesign --verify --deep --strict`、entitlements 检查均 **PASS**。
- **开发包限制：** 当前仅有 ad-hoc 签名，`TeamIdentifier` 为空，不含 associated-domain entitlement，也未嵌入服务条款或执行 notarization；`spctl` 以状态 3 拒绝该包是预期结果，因此它不是可公开分发的 macOS release。
- **打包稳定性：** `TERM=dumb` 下旧打包工具的彩色输出 panic 已在项目脚本中 fail-safe 规避；重复 plist 扩展会在签名前规范化；许可证内容未变化时保留文件 mtime，最终缓存复跑的 Rust 构建均在约 2 秒内完成。
- **跨平台边界：** Windows canonical CLI、安装器、更新器、scheme 所有权与 workflow 已做源码/交叉检查，但本机不能执行真实 Windows/ISCC/Wine 安装升级卸载 E2E；Windows 更新在 `install/old` 非空的中断恢复仍建议后续增加 transaction journal（P2）。

Production release 必须保持 **NO-GO**：Orion DNS、cloud/collab/OAuth、Cloudflare、Sentry、Apple/Windows 签名与公证凭据、publisher/store/Winget、真实 Windows/Linux packaging E2E、GitHub protected Environments/rulesets/reviewers，以及 trademark/privacy/terms 等法务审批均未完成。任何本地 PASS 或 Dev DMG 都不能替代这些生产、平台、治理与法务门禁。

## 1. 计划信息

| 项目      | 内容                                                                                            |
| --------- | ----------------------------------------------------------------------------------------------- |
| 计划文件  | docs/plan/init-plan-v2.md                                                                       |
| 执行分支  | init                                                                                            |
| 当前 HEAD | d2779c3                                                                                         |
| 复核日期  | 2026-08-11                                                                                      |
| 目标      | 将当前 Zed 派生项目收敛为 Orion Studio，并在保留必要兼容能力的前提下移除面向用户的 Zed 品牌标签 |
| 执行方式  | HY3 腾讯 WorkBuddy 按顺序执行，每次只执行一个子计划                                             |
| 当前状态  | IN_PROGRESS，不能据此宣称已完成改造、可发布或可安装                                             |

本计划是对已有总计划和 S01–S06 子计划的进度复核与续作，不是新的全量重命名指令。已有工作树是用户正在进行的工作，后续执行必须基于当前状态继续。

## 2. 本次复核结论

### 2.1 Git 和工作树基线

当前分支为 init，HEAD 为 d2779c3。工作树不是干净状态，已有 WorkBuddy 改动和计划证据，必须原样保留：

- 已修改的源文件包括 crates/cli/src/main.rs、crates/client/src/client.rs、crates/extension_api/README.md、crates/extension_api/src/extension_api.rs、crates/paths/src/paths.rs、crates/release_channel/src/lib.rs、crates/zed/Cargo.toml、crates/zed/src/main.rs、crates/zed/src/zed.rs、crates/zed_env_vars/src/zed_env_vars.rs、extensions/test-extension/src/test_extension.rs。
- 新增但尚未纳入 Git 的源文件为 crates/paths/src/migration.rs。
- .workbuddy/、docs/research/、docs/plan/ 当前为未跟踪目录，属于本次工作上下文，不能通过 clean、reset、checkout 或删除操作处理。
- 当前没有新的提交；不要把工作树中的改动描述为已经提交、推送、合并或发布。
- 当前初步 diff 约为 11 个已跟踪源文件、424 行新增、168 行删除，实际边界仍以执行时的 git diff 和 git status 为准。

### 2.2 已有子计划状态

| 子计划               | WorkBuddy 报告 | 本次判断                                                                          | 证据                                                                                      |
| -------------------- | -------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| S01 基线盘点         | DONE           | 可接受为盘点完成                                                                  | docs/plan/evidence/S01-baseline-inventory.md                                              |
| S02 身份与兼容契约   | DONE           | 已形成选择，但发布前仍需人工复核                                                  | docs/plan/evidence/S02-identity-and-compatibility-contract.md                             |
| S03 运行时身份与路径 | DONE           | PARTIAL：局部路径和环境变量已改，完整运行时闭环未证明                             | docs/plan/evidence/S03-runtime-identity-and-paths.md、当前源代码                          |
| S04 数据迁移与兼容   | DONE           | PARTIAL/NO-GO：迁移模块有实现和测试，但尚未接入启动流程，且存在错误处理和边界问题 | docs/plan/evidence/S04-data-migration-and-compatibility.md、crates/paths/src/migration.rs |
| S05 核心包和主二进制 | DONE           | PARTIAL：主包和二进制配置已有改动，实际主二进制构建未通过                         | docs/plan/evidence/S05-core-package-and-binary.md、主二进制构建记录                       |
| S06 CLI、API 和协议  | DONE           | PARTIAL：CLI/API 做了第一轮兼容改动，内部 IPC、安装产物和全量协议未闭环           | docs/plan/evidence/S06-cli-api-and-protocol.md、当前源代码                                |
| S07 UI、资源和文档   | 未开始         | NOT STARTED                                                                       | 尚无完整证据                                                                              |
| S08 客户端服务端点   | 未开始         | NOT STARTED                                                                       | 尚无完整证据                                                                              |
| S09 协作、远程和部署 | 未开始         | NOT STARTED                                                                       | 尚无完整证据                                                                              |
| S10 平台打包和 CI    | 未开始         | NOT STARTED                                                                       | 尚无完整证据                                                                              |
| S11 回归和发布门禁   | 未开始         | NOT STARTED                                                                       | 尚无完整证据                                                                              |

结论：不能把 S01–S06 的统一 DONE 标签当成产品完成度。当前真正可继续推进的工作是先修正 P0 数据迁移链路，再处理运行时、构建产物和 IPC 契约。

### 2.3 本次实际验证结果

以下结果是本次在当前工作树上实际执行的结果：

| 检查                                          | 结果         | 说明                                                                                                          |
| --------------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------- |
| cargo +stable test -p paths                   | PASS         | 13 个测试通过                                                                                                 |
| cargo +stable test -p cli                     | PASS         | 6 个测试通过                                                                                                  |
| cargo +stable check -p client                 | PASS         | 通过                                                                                                          |
| cargo +stable check -p zed_extension_api      | PASS         | 通过                                                                                                          |
| cargo +stable fmt --all -- --check            | PASS         | 通过                                                                                                          |
| ./script/check-todos                          | PASS         | 通过                                                                                                          |
| ./script/check-keymaps                        | PASS         | 通过                                                                                                          |
| git diff --check                              | PASS         | 通过                                                                                                          |
| cargo +stable check -p zed --bin orion-studio | NOT VERIFIED | gpui_macos 报缺少 Metal Toolchain；随后重量级 webrtc-sys 构建等待过久，测试进程被停止，不能当作代码通过或失败 |
| ./script/clippy                               | NOT RUN      | 必须在后续门禁中执行，不能用 cargo clippy 替代                                                                |

主二进制当前没有可接受的成功构建证据。缺少 Metal Toolchain 是环境门禁问题；WorkBuddy 必须把它记录为 BLOCKED 或环境前置条件，不得把本次结果改写成代码错误，也不得跳过该门禁。

## 3. 已确认的目标契约

S02 已记录的目标值先作为当前工作契约使用，涉及发布、法律和兼容的内容仍需人工最终确认：

- 产品显示名：Orion Studio。
- 小写 slug：orion-studio。
- 新环境变量前缀：ORION*STUDIO*\*。
- 新文档域名族：orion.dev。
- 新产品 URL scheme：orion://。
- 新 macOS bundle 标识族：dev.orion.OrionStudio\*。
- 新 Windows app 标识族：OrionStudio-\*。
- 新通用包标识：com.orion.OrionStudio。
- 许可证选择：GPL-3.0-or-later，必须保留并核对上游许可证和归属说明。
- 兼容策略：对已有 Zed 用户数据和旧入口提供有边界的兼容与迁移；兼容层不是继续向用户展示 Zed 品牌的理由。

以下内容在 S02 中没有足够明确的最终值时，不得由执行模型自行猜测：

- 内部 CLI URL scheme，例如 orion-cli:// 是否正式取代 zed-cli://。
- socket 文件命名是否从 zed-_.sock 改为 orion-_.sock，以及旧 socket 的过渡周期。
- 生产服务域名、OAuth 回调域名、云端 API 域名。
- 远程服务器 User-Agent 的精确格式。
- 包管理器、签名、notarization、发布频道的最终标识。
- 是否彻底移除某个旧兼容分支。

遇到这些未定契约时，先写出发现和影响，状态为 BLOCKED，等待人工确认；不能以“看起来合理”为理由修改协议。

## 4. 当前最高风险和开发顺序

### P0：数据迁移模块尚未接入启动流程

crates/paths/src/migration.rs 已有迁移状态、临时目录、原目录备份、标记文件和测试，但在 crates 内搜索时，除模块导出和自身测试外，没有发现应用启动调用。也就是说，代码存在不等于用户升级时会执行迁移。

必须先完成：

1. 明确配置目录、数据目录、服务器目录和扩展目录的读取顺序。
2. 找到第一次读取这些目录、数据库或工作区状态的位置。
3. 在此之前调用一次性、可重入的迁移入口。
4. 把迁移失败传递到用户可见的启动错误或明确日志，而不是静默继续使用新空目录。
5. 验证重复启动、部分迁移、中断恢复、旧目录保留和新目录已存在的行为。

### P0：迁移模块不符合当前 Rust 规则，且有失败安全问题

当前 migration.rs 中存在对 remove*dir_all 结果使用 let * = 的处理，这违反 AGENTS.md 的错误处理规则。迁移标记解析对格式异常的处理也可能把异常状态当成没有迁移状态，进而继续合并或覆盖目录。测试中也有新引入的 unwrap/expect，后续修改不得继续增加这类脆弱路径。

至少要解决：

- 所有清理、复制、重命名、写标记和同步操作都要明确处理错误。
- 标记文件损坏、版本未知、来源与目标不一致时必须 fail closed。
- 临时目录清理失败不能被静默丢弃；应返回带上下文的错误，或在确认主操作成功后按项目约定显式记录。
- 保证目标目录已有内容时不会无条件覆盖用户数据。
- 明确符号链接、特殊文件、权限、跨设备 rename 和中断恢复策略。
- 补齐 Flatpak 旧路径和新路径的对称映射。
- 明确 .zed_wsl_server 是否需要迁移到 .orion_wsl_server；不能只处理普通 server 目录。
- 新增或修改的生产代码不要使用 unwrap、expect 或可能越界 panic 的索引。

### P1：构建期身份仍是 Zed

运行时 paths 和部分环境变量已经有 Orion 逻辑，但 release_channel/build.rs 仍只监听和设置 ZED_RELEASE_CHANNEL。remote_server/build.rs 仍使用 ZED_MANIFEST、ZED_PKG_VERSION、ZED_COMMIT_SHA、ZED_BUILD_ID，remote_server 的 User-Agent 也仍是 Zed-Server。Windows app identifier 和 app_id 仍包含 Zed。

这会造成“界面看起来已改名，但构建、诊断、系统注册和服务器仍叫 Zed”的混合状态，必须集中收敛并保留明确的旧环境变量回退。

### P1：CLI 显示名和实际安装产物可能不一致

crates/cli/src/main.rs 的 clap 名称已是 orion-studio，但 crates/cli/Cargo.toml 的 package 和 bin 名称仍是 cli。必须确认：

- Cargo package 名称、bin target 名称、生成的可执行文件名和安装脚本实际使用的文件名。
- 现有调用方、workspace 依赖和发布脚本是否依赖 cli 这个包名。
- 是否需要只改 bin 名称，还是需要一次有兼容别名的 package 迁移。
- 不能仅凭 clap --help 的显示结果宣称 CLI 产物已完成。

### P1：内部 IPC 和 OS 注册协议仍大量使用 Zed

当前仍能发现：

- zed-cli:// 内部 URL。
- zed-\*.sock socket 命名。
- ZED_CHANNEL、ZED_ASKPASS_SOCKET 等运行时协议变量。
- install_cli 中的 register_zed_scheme。
- Windows 单实例和 open_listener 中的旧 scheme。
- CLI 中同时接受 orion:// 和 zed:// 的过渡逻辑。

这些字符串有些是兼容入口，有些是私有协议。必须先建立协议矩阵，区分 canonical、legacy-compatible、test-only 和禁止继续出现四类，不能使用全局替换破坏升级链路。

### P1：服务端点仍未处理

crates/client/src/zed_urls.rs、cloud API、context OAuth、HTTP host、collab/remote 相关代码仍有 zed.dev、cloud.zed.dev 和旧 User-Agent/endpoint 逻辑。客户端局部解析已支持 orion://，不代表请求端点、认证、WebSocket、远程会话和 OAuth 回调已经迁移。

必须在确认域名后逐个检查生产端点、测试端点、回退端点、错误信息和日志，任何未确定的域名都要停在 BLOCKED。

## 5. v2 执行规则

1. 每次 WorkBuddy 只执行一个 V2 子计划，完成后停止并回报，不要连续跨越多个阶段。
2. 任何阶段开始前先运行 git status --short、git diff --stat，并检查当前分支和 HEAD。
3. 保留当前 dirty worktree，不执行 git reset、git checkout、git clean、删除 .workbuddy 或删除 docs/plan、docs/research。
4. 不覆盖别人的未提交源代码。若目标文件在开始前已有未提交改动，先记录边界，再在同一文件内最小修改。
5. 只修改子计划中列明的允许路径。需要扩大范围时先写 BLOCKED，列出新增路径和原因。
6. 不进行全仓库机械式 Zed 到 Orion 替换。每个命中都要判断它是用户可见身份、内部 canonical 名称、旧兼容入口、上游归属、测试 fixture 还是法律说明。
7. 不删除上游归属、LICENSE、版权、NOTICE、协议名称或必须保留的历史兼容内容。
8. 不猜测未确认的服务域名、协议名称、包标识、签名或发布策略。
9. 所有新增测试必须在当前 Rust/GPUI 规则下实现；不要新增 unwrap、expect 或静默丢弃错误。
10. 使用 ./script/clippy；不要用 cargo clippy 代替项目规定的检查。
11. 不提交、推送、建 PR、打 tag、发布包或修改生产数据。用户另行授权后再做这些动作。
12. “通过”只表示指定命令在指定工作树和指定 HEAD 上通过；未执行、被环境阻塞或中途停止都必须如实标记。

## 6. v2 子计划

### V2-00：进度审计和工作树边界确认

目标：把 S01–S06 的报告状态与当前源码、diff、测试结果对齐，生成唯一的续作基线。

允许修改：

- docs/plan/evidence/INIT-V2-PROGRESS-AUDIT.md

执行内容：

1. 记录分支、HEAD、是否 shallow/grafted、dirty 文件清单和已有未跟踪目录。
2. 阅读 S01–S06 evidence，逐项抽查其声称修改的源码。
3. 搜索当前用户可见和运行时关键残留，包括 Zed、zed.dev、cloud.zed.dev、ZED\_、zed-cli://、zed-\*.sock、dev.zed、Zed-Server。
4. 记录本计划中已经实际验证的命令，不要把 WorkBuddy 报告的命令当成本次验证。
5. 标注每个结论为 VERIFIED、PARTIAL、NOT VERIFIED、BLOCKED 或 NOT STARTED。
6. 不修改源代码，不修复问题，不重跑重量级构建。

验收命令：

- git status --short
- git branch --show-current
- git rev-parse HEAD
- git diff --check
- 对关键命中使用 grep 或仓库可用的 rg，并保存命令与摘要结果

完成标准：

- 证据文件存在。
- 清楚区分 HEAD 内容、工作树改动和未跟踪文件。
- 明确 V2-01 的允许修改边界。
- 若发现当前状态与本计划冲突，状态为 BLOCKED 并停止。

### V2-01：数据迁移模块硬化

目标：让现有迁移实现达到可以安全接入启动流程的质量，先不接入启动。

允许修改：

- crates/paths/src/migration.rs
- crates/paths/src/paths.rs
- crates/paths/src/lib.rs
- crates/paths 中与迁移直接相关的现有测试文件
- docs/plan/evidence/INIT-V2-V01-MIGRATION-HARDENING.md

必须完成：

1. 清点 migration.rs 的所有 fallible operation；移除 let \_ = 对错误的静默丢弃。
2. 为复制、重命名、临时目录、标记文件写入和清理建立一致的错误语义，错误中包含来源、目标和阶段。
3. 对损坏、空文件、未知版本、重复字段、来源目录变化的 marker 采取 fail closed 行为。
4. 证明目标目录已有内容时的合并策略不会无条件覆盖用户文件，并为冲突补测试。
5. 证明迁移可以重复执行且不会重复创建备份或损坏已完成状态。
6. 重新检查普通 config/data、Flatpak、server、WSL server、symlink 和特殊文件路径；补齐旧路径与新路径的对称映射。
7. 迁移 secrets 时保持权限和旧目录保留策略；不打印 secret 内容。
8. 新增或修改的生产代码不要使用 unwrap、expect 或可能越界 panic 的索引。
9. 只做路径和迁移本身的修复，不调用迁移、不改启动顺序、不做品牌全局替换。

验证门禁：

- cargo +stable fmt --all -- --check
- cargo +stable test -p paths
- ./script/clippy
- git diff --check

交付证据必须包含：

- 修改文件列表。
- marker 状态机和异常状态行为。
- 各类路径的迁移矩阵。
- 测试命令、结果和未运行项目。
- 若 clippy 或测试被环境阻塞，标为 BLOCKED/PARTIAL，不得写 DONE。

### V2-02：启动流程接入迁移

前置条件：V2-01 已有证据，且 paths 测试和 clippy 通过。

允许修改：

- crates/zed/src/main.rs
- 现有 paths/migration 公共接口文件
- 必须直接参与启动前目录初始化的现有文件
- docs/plan/evidence/INIT-V2-V02-STARTUP-WIRING.md

必须完成：

1. 找到应用首次读取配置、数据、扩展、server 或数据库的真实调用点。
2. 在第一次读取前调用迁移；同一次启动中不能因为多个消费者重复执行不同迁移。
3. 明确迁移失败时的行为：向 UI 或启动层返回带上下文的错误，并阻止以空的新目录继续运行。
4. 保证无旧目录时启动行为不变，有旧目录时只执行一次，重复启动为幂等。
5. 为启动前迁移增加最小可执行测试或可验证的集成测试；测试必须隔离临时目录并清理失败可见。
6. 不在本阶段修改 UI 文案、服务域名、发布配置或内部 IPC。

验证门禁：

- cargo +stable fmt --all -- --check
- 受影响 crate 的 focused test/check
- ./script/clippy
- git diff --check

### V2-03：构建期、运行期和诊断身份闭环

前置条件：V2-00 完成；若 V2-02 未完成，必须说明为什么不会影响本阶段验证。

允许修改：

- crates/release_channel/src/lib.rs
- crates/release_channel/build.rs
- crates/zed_env_vars/src/zed_env_vars.rs
- crates/zed/build.rs
- crates/cli/build.rs
- crates/remote_server/build.rs
- crates/remote_server/src/server.rs
- 与这些值直接耦合的已有测试和文档证据文件

必须完成：

1. 建立 canonical ORION*STUDIO*_ 与 legacy ZED\__ 的来源、优先级、回退和 deprecation 矩阵。
2. build.rs 必须同时处理契约中确认的 Orion 构建变量和旧变量回退，输出 cfg、rerun-if-env-changed 以及版本/提交信息行为。
3. 统一 app_id、Windows app identifier、bundle 相关值、诊断标签、User-Agent 和 release channel 派生值。
4. 对有意保留的旧兼容值增加说明和测试，确保不会意外生成面向用户的 Zed 新身份。
5. 检查 release_channel 的 env/version 处理是否违反当前错误处理规则；不得新增 panic。
6. 不改生产服务域名；服务端点属于 V2-06。

验收：

- 受影响 crate 的 focused check/test。
- cargo +stable fmt --all -- --check。
- ./script/clippy。
- 对构建脚本分别使用 canonical 和 legacy 环境变量做最小验证。
- 输出身份矩阵及仍保留的 legacy 命中清单。

### V2-04：主二进制、CLI 产物和内部 IPC 契约

前置条件：V2-03 完成，且 V2-00 已明确协议命中分类。

允许修改：

- crates/zed/Cargo.toml
- crates/zed/src/main.rs
- crates/zed/src/zed.rs
- crates/cli/Cargo.toml
- crates/cli/src/main.rs
- crates/install_cli/src/register_zed_scheme.rs
- crates/install_cli/src/zed.rs
- crates/zed/src/zed/open_listener.rs
- crates/zed/src/zed/windows_only_instance.rs
- 相关 build/test/docs 证据文件

必须完成：

1. 证明 Cargo package、bin target、default-run、实际生成文件名和安装脚本一致。
2. 明确 CLI 是否需要 package 兼容别名；若无法由现有契约决定，先 BLOCKED，不要自行改 package 名称。
3. 为产品 scheme、旧 scheme、内部 CLI scheme、socket 名称建立协议矩阵。
4. 对 zed-cli://、zed-\*.sock、ZED_CHANNEL、ZED_ASKPASS_SOCKET 等命中逐一判定 canonical 或 legacy。
5. 若改内部协议，发送方、接收方、单实例、安装注册、Windows/macOS/Linux 路径必须同一阶段成套验证。
6. 保持旧入口的兼容窗口和错误提示，不能直接删除升级所需的旧入口。
7. 不以 clap --help 代替实际二进制构建和安装产物验证。

验收：

- cargo +stable check -p zed --bin orion-studio，环境缺少 Metal Toolchain 时记录为 BLOCKED。
- CLI package/bin focused check/test。
- install_cli 相关 focused check/test。
- ./script/clippy。
- 在不安装到用户系统的临时目录验证产物和注册内容。

### V2-05：扩展 API、WIT/ABI、UI 资产和文档

前置条件：V2-04 的可见命名和 ABI 边界已确认。

允许修改：

- crates/extension_api/\*\*
- extensions/test-extension/\*\*
- crates/zed/src/\*\*
- assets/\*\*
- docs/\*\*
- 对应 Cargo/build/test 文件

必须完成：

1. 区分 Rust crate 名、WIT namespace、导出 ABI、宏生成路径和用户看到的产品名。
2. 保留 ABI 兼容的必要旧 namespace；新增 Orion 别名必须有测试和迁移说明。
3. 使用 wasm32-wasip2 做真实编译检查；若工具链缺失，记录环境阻塞。
4. 更新 README、帮助文本、命令面板、错误文案、图标和资源文件中的面向用户标签。
5. 保留上游归属和许可证文本；对历史兼容说明不要机械删除。
6. UI 只在行为和资源边界明确后修改，不以截图漂亮作为功能完成证据。

验收：

- extension_api focused check/test。
- test-extension wasm32-wasip2 build/check。
- UI 受影响 crate focused test/check。
- ./script/clippy。
- 产品可见扫描结果和保留旧标签的分类清单。

### V2-06：客户端服务端点、认证和网络协议

前置条件：生产域名、OAuth 回调、API/WS/远程端点由人工确认并写入契约。

允许修改：

- crates/client/\*\*
- crates/context_server/\*\*
- crates/collab/\*\*
- crates/remote_server/\*\*
- 相关配置、测试、文档

必须完成：

1. 先列出每个 endpoint 的用途、环境、来源、fallback、认证方式和 owner。
2. 一次性覆盖 HTTP、WebSocket、OAuth、远程连接、错误信息、User-Agent、日志和测试 fixture。
3. 本地 mock/loopback E2E 与真实租户、真实云服务验证分开记录。
4. 不在没有真实凭据和用户授权时声称生产 endpoint 已验证。
5. 对密钥、token、cookie 和账号信息只做 secretless 验证，不写入计划证据。

验收：

- focused unit/integration tests。
- secret scanner。
- mock gateway/loopback E2E。
- 若需要真实租户或部署，明确写 BLOCKED 并等待授权。

### V2-07：协作、远程和自托管最小闭环

前置条件：V2-06 的服务契约稳定。

必须覆盖：

- 协作邀请、登录和 token 传递。
- 远程项目启动、连接、断开和重连。
- 自托管/本地 server 的数据目录和身份。
- Orion 客户端与旧兼容 server 的边界。
- 端到端日志和错误回传。

必须分别报告：

- mock/loopback proof。
- 本地真实进程 proof。
- 真实云端或真实租户 proof。
- 未验证的发布/部署条件。

### V2-08：平台打包、安装、升级和 CI

前置条件：V2-03 至 V2-07 已有 focused 证据。

必须覆盖：

- macOS bundle、URL scheme、bundle identifier、应用显示名。
- Windows app identifier、单实例、安装注册和升级路径。
- Linux/Flatpak/AppImage/deb/rpm 等仓库实际支持的目标。
- CLI 安装、卸载和 PATH 行为。
- CI workflow、构建脚本、缓存键、产物名、签名和 release channel。
- 旧版本升级到 Orion Studio，以及失败后回滚到旧目录的行为。

任何需要签名、notarization、发布平台账号或生产环境的步骤都必须单独标记授权状态；不以本地构建替代发布证据。

### V2-09：全量回归与发布门禁

这是最后一个阶段，不能提前标 DONE。

必须完成：

1. 全仓库品牌扫描，并逐项分类为用户可见、canonical 内部、legacy 兼容、上游归属、测试 fixture、第三方依赖或误报。
2. 在干净 checkout 或明确隔离的临时 worktree 复现核心构建；当前 dirty tree 的 focused pass 不能代替 clean proof。
3. 执行仓库规定的格式、测试、lint、keymap、todo、secret 和打包检查，包含 ./script/clippy。
4. 验证首装、旧版本升级、重复启动、迁移失败、回滚和旧入口兼容。
5. 验证仓库声明支持的 macOS/Windows/Linux 目标；缺少 Metal Toolchain、wasm 工具链或签名环境时清楚列为 BLOCKED。
6. 核对 LICENSE、NOTICE、版权、归属、版本号和 release notes。
7. 形成 Go/No-Go 表；只要主二进制、迁移接入、安装产物、关键服务端点或协议兼容没有证据，结论就是 NO-GO。

禁止在本阶段自动发布、推送或打 tag。发布是独立授权动作。

## 7. WorkBuddy 每阶段交付格式

WorkBuddy 每次只输出一个阶段的结果，使用以下字段：

- 阶段 ID：
- 状态：DONE / PARTIAL / BLOCKED
- 当前分支和 HEAD：
- 开始前 dirty 边界：
- 修改文件：
- 未修改但检查过的关键文件：
- 实现或审计摘要：
- 测试和命令结果：
- 未运行或环境阻塞：
- 遗留风险：
- 下一阶段建议：
- 回滚方式：

DONE 的最低条件是：允许路径内的改动完成、指定门禁通过、证据文件写出、没有未披露的阻塞。若任何一项不满足，只能写 PARTIAL 或 BLOCKED。

回滚只允许使用可恢复的方式。不要用 git reset --hard、git checkout --、git clean 或删除用户目录来恢复。

## 8. 立即执行建议

下一轮不要从 UI 或大范围品牌替换开始，严格按以下顺序：

1. V2-00：生成当前进度审计证据。
2. V2-01：修复迁移模块的错误处理、marker fail-closed 和路径边界。
3. V2-02：把迁移接到真实启动前路径读取点。
4. 再进入 V2-03 和 V2-04，收敛构建身份、主二进制、CLI 产物与 IPC。
5. 最后处理服务端点、UI、打包和全量回归。

首要判断标准是“已有用户升级时数据不会丢、迁移失败不会静默启动到空目录、CLI/主二进制真实产物与 Orion Studio 契约一致”。在这三个条件有证据之前，不进入发布或大规模清理旧兼容标签。
