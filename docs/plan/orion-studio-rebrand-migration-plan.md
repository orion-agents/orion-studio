# Orion Studio 品牌与底层身份迁移改造计划

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

| 项目     | 内容                                                     |
| -------- | -------------------------------------------------------- |
| 状态     | Draft，待产品、技术、法务与发布责任人确认                |
| 目标分支 | init                                                     |
| 基线     | d2779c3，与当前 main/origin/main 同指向                  |
| 编制日期 | 2026-08-09                                               |
| 计划范围 | 品牌、运行时身份、代码命名、服务端点、打包发布、兼容迁移 |
| 本文性质 | 执行计划，不代表已经完成改造                             |

本计划的目标是把当前 Zed 源码基线改造成名为 **Orion Studio** 的独立产品，
覆盖用户可见品牌以及底层代码和运行时身份。迁移必须是可验证、可回滚的产品迁移，
不能按一次全局字符串替换处理。

当前工作树中已有未跟踪的 .workbuddy/ 与 docs/research/，它们属于用户现有内容；
本计划不修改、不清理、不纳入本次改造提交。

## 2. 基线与证据

当前仓库是一个以 Rust/GPUI 为核心的 Zed fork，workspace 包含 250 个 package，默认
构建目标仍是 crates/zed。只读扫描得到以下基线，扫描结果可能因模式重叠而不能相加：

- git grep 命中 Zed 相关标识：1,563 个文件、14,417 行。
- 边界匹配 zed：1,077 个文件、10,088 次。
- script/ 下相关命中：73 个文件、780 次。
- 关键 P0 位置包括 crates/paths、crates/zed_env_vars、
  crates/release_channel、crates/zed、crates/cli、crates/client、打包资源和
  release workflow。
- collab 服务端、云端客户端、远程服务、扩展 API、账户和 AI 代码都仍包含 Zed
  服务地址或配置命名。
- 当前可执行的稳定基线检查为 metadata、format、TODO/keymap 检查；首次编译尝试在
  拉取 Git 依赖时遇到 HTTP/2 网络错误并中止，不能据此宣称 workspace 已完整编译通过。

研究文档 docs/research/open-source-code-editors-research.md 作为输入材料保留，
但不视为已批准的产品需求。它同时讨论桌面编辑器、云端 IDE 和订阅服务，范围尚未冻结。
另外，研究文档关于 collab 使用 AGPL 的描述与当前 manifest 中的
GPL-3.0-or-later 不一致，必须以实际许可证文件和法务确认结果为准。

## 3. 目标、边界与命名契约

### 3.1 首个交付目标

默认目标建议先定为：**桌面优先的 Orion Studio 原生代码编辑器**，继续复用当前
编辑器、LSP、扩展、远程开发、AI 和协作能力，但逐项完成服务归属和隐私边界确认。

云端 Web IDE、托管协作、账户、计费、SLA 和商业订阅不自动包含在本次品牌迁移中；
它们必须作为独立产品范围，通过服务端 Go/No-Go 后再交付。当前仓库存在 collab
和 cloud 相关代码，不等于已经存在可直接运营的 Orion SaaS 后端。

### 3.2 命名契约（待批准后冻结）

以下是实施建议，不是未经确认的最终值：

| 层级                  | Orion Studio 目标           | 迁移要求                                                       |
| --------------------- | --------------------------- | -------------------------------------------------------------- |
| 展示名称              | Orion Studio                | 窗口、菜单、关于页、安装器、文档、商店元数据统一使用           |
| 仓库/产物 slug        | orion-studio                | 作为产物、下载和自动化中的规范 slug                            |
| Rust package/crate    | orion-studio / orion_studio | 由代码所有者确认是否一次性改名或分阶段改名                     |
| 主二进制与 CLI        | orion-studio                | 旧 zed 命令是否保留兼容别名必须单独决策                        |
| 环境变量              | ORION*STUDIO*\*             | 新代码只产生规范前缀；旧 ZED\_\* 仅进入兼容读取层              |
| 配置、缓存、日志目录  | Orion 专属目录              | 首次启动完成可恢复、幂等的数据迁移                             |
| URL scheme            | Orion 专属 scheme           | 新 scheme 与旧 zed:// 的兼容窗口必须冻结                       |
| Bundle/App/Flatpak ID | Orion 所有的唯一 ID         | 需要域名/组织所有权和安装升级策略确认，禁止猜测                |
| 服务端点              | Orion 所有的域名和凭据      | 不得把 zed.dev、cloud.zed.dev 或 collab.zed.dev 留作活动默认值 |

“去掉 Zed 标签”定义为：用户可见品牌、规范运行时标识、默认服务归属和发布身份
全部改为 Orion。以下内容不应被盲目删除：第三方许可证、版权归属、上游贡献说明、
迁移代码中的旧标识、兼容测试、Git 历史和法务要求的明确 attribution。

### 3.3 非目标

- 不在本计划内重写编辑器、GPUI 或协作架构。
- 不因为改名顺便做无关重构或功能开发。
- 不把研究文档中的 Theia、Web IDE 或商业服务假设直接变成实现承诺。
- 不删除上游许可证、版权、贡献者和第三方依赖声明。
- 不在没有协议迁移和回滚方案时破坏旧配置、用户数据、扩展 API 或 RPC wire format。

## 4. 改造面与优先级

### P0：运行时身份、数据和可安装产品

这些内容必须先建立兼容策略，再开始改名：

- crates/paths/src/paths.rs：APP_NAME、配置/缓存/状态/日志/临时目录、远程目录、
  .zed_server 和日志文件名。
- crates/zed*env_vars、crates/release_channel：ZED*\* 环境变量、版本、发布通道、
  Windows App ID 和显示名称。
- crates/zed/Cargo.toml、crates/zed/build.rs、crates/zed/src/main.rs：package、
  binary、bundle、启动身份和初始化链。
- crates/cli：命令名、帮助、zed://、数据目录和打开文件协议。
- crates/client/src/client.rs、crates/client/src/zed_urls.rs、
  crates/cloud_api_client：服务 URL、登录、更新、账户和 RPC/WebSocket 地址。
- crates/remote_server、远程启动脚本、凭据和远程目录。
- crates/zed/resources/、script/bundle-\*、script/flatpak/、各平台安装器。
- .github/workflows/run_bundling.yml、.github/workflows/release.yml 及签名、更新、
  artifact 和 release repository 配置。

### P1：用户可见内容、协议文案和测试契约

- README.md、CONTRIBUTING.md、docs/src/、示例和下载链接。
- assets/settings/default.json、keymap、主题、schema URL、默认 provider 和菜单文案。
- crates/extension_api、WIT namespace、扩展文档和 test extension。
- crates/agent、agent_ui、language_models、账户/隐私/遥测/AI 文案。
- visual tests、snapshot、E2E、eval fixture、CLI 测试和 zed:// 测试数据。

### P2：组织归属、上游链接和法律文本

- .github/ 的 owner、仓库、secret、机器人和 CI 条件。
- Cargo.toml 中的上游 fork URL、ci/、Dockerfile、Procfile.web 和部署脚本。
- CODEOWNERS、mailmap、compliance 检查和开发工具中的组织名。
- legal/terms.md、legal/privacy-policy.md、script/terms/：单独法务审批，不能
  用普通搜索替换处理。

## 5. 分阶段执行计划

### Phase 0：冻结范围并建立可重复基线

**任务**

- [ ] 确认产品形态：桌面、远程、自托管协作、云端 Web IDE 是否分别纳入。
- [ ] 确认底座策略：继续深度 fork、保持可 rebase 的品牌层，还是只复用 GPUI；Theia
      只作为候选，不在未完成 PoC 前替换底座。
- [ ] 建立 brand-inventory：按用户可见、规范运行时、兼容、法律/归属、历史五类
      标记所有命中，不用全局替换。
- [ ] 固化当前分支、commit、可用工具链、依赖缓存、构建矩阵和未跟踪文件边界。
- [ ] 记录当前安装、启动、登录、打开文件、扩展、LSP、远程、协作和 AI 的基线证据。

**产出与验收**

- 命名/端点/数据目录/协议/发布物的 inventory 可由脚本重复生成。
- 每个 Zed 命中都有迁移、兼容、保留归属或删除结论。
- 未跟踪用户文件没有被改写，基线构建失败原因与环境条件有记录。

**Gate 0**：没有产品形态、开源边界和现有服务归属结论时，No-Go，不开始大规模
package 或 crate 改名。

### Phase 1：批准身份、法律与兼容契约

**任务**

- [ ] 冻结展示名、slug、Rust package/crate 名、二进制、CLI、URL scheme、Bundle/App/
      Flatpak ID、配置目录和环境变量前缀。
- [ ] 确认 Orion 域名、账户/OAuth、更新、崩溃、遥测、扩展 registry、云端和 RPC
      endpoint 的所有权、凭据和隐私责任。
- [ ] 逐 crate 和第三方依赖生成 SPDX/许可证清单；确认当前实际的
      GPL-3.0-or-later、Apache 组件、商标和 hosted service 边界。
- [ ] 冻结兼容窗口：旧 CLI、旧 scheme、旧配置目录、旧环境变量、旧扩展/协议版本
      各保留多久，何时只读，何时移除。

**产出与验收**

- 命名矩阵、服务矩阵、许可证矩阵、数据迁移矩阵、兼容矩阵均有责任人和批准记录。
- 任何继续使用旧标识的地方都能引用兼容或归属理由。

**Gate 1**：身份矩阵或许可证/商标边界未批准时，No-Go；不能生成 Orion 安装包。

### Phase 2：运行时身份与数据迁移底座

**任务**

- [ ] 先改造 crates/paths 和身份常量，使路径、日志、临时目录、远程目录和应用名
      从单一规范来源派生。
- [ ] 新用户只创建 Orion 目录；升级用户检测旧目录，执行版本化、幂等、可恢复迁移。
- [ ] 迁移采用临时目录 + 原子 rename/copy、权限保留、备份标记和失败回退；成功前不
      删除旧数据，失败后保留诊断信息。
- [ ] 规范环境变量优先；旧 ZED\_\* 只读兼容并发出一次可测试的弃用诊断，不把旧名
      写回新配置。
- [ ] 对配置 schema、数据库、扩展目录、密钥、缓存和日志分别定义迁移，不把缓存当作
      可恢复的用户数据。

**产出与验收**

- 新装、旧装升级、迁移中断后重试、权限不足、磁盘不足和回滚均有测试。
- 用户数据内容、权限和关键凭据不丢失；迁移重复执行不会重复覆盖或破坏数据。
- crates/paths、env var、release channel、启动初始化和日志中不再存在未解释的
  规范 Zed 身份。

**Gate 2**：数据迁移没有恢复证据、原子性或回滚演练时，No-Go；不允许切换默认生产
路径。

### Phase 3：核心代码、package 和 API namespace 迁移

**任务**

- [ ] 按依赖图分批改 crates/zed、CLI、workspace、extension API 和测试 package，
      每批保持可编译、可回滚，不进行一次性不可审查的全仓替换。
- [ ] 更新 Rust package、crate/module、action namespace、WIT namespace、feature 名、
      binary 查找和测试 fixture。
- [ ] 保留必要的兼容模块/别名，但让新代码只能依赖 Orion 规范入口。
- [ ] 重新生成锁文件、扩展 SDK/示例和文档链接，并审查上游 fork URL 是否应继续依赖。

**产出与验收**

- 分批提交可以独立编译和回滚；编译错误、链接错误、运行时 dynamic lookup 和扩展
  加载错误都有记录。
- 新 package/binary/namespace 在干净 checkout 中可构建，旧别名只出现在兼容层或
  测试中。

**Gate 3**：核心 package 改名导致扩展、CLI、远程或协议无法运行，No-Go；先补兼容
层和契约测试。

### Phase 4：UI、资源、文档和用户可见品牌

**任务**

- [ ] 更新窗口标题、菜单、命令面板、关于页、欢迎页、错误/诊断、通知、主题、图标、
      schema、默认 provider 和安装文案。
- [ ] 更新 README.md、CONTRIBUTING.md、docs/src/、下载链接、徽章和示例。
- [ ] 明确所有 Zed 版权、上游贡献和第三方主题作者信息的保留方式；法律文本走单独
      审批，不把上游 attribution 当成品牌残留误删。
- [ ] 更新 visual test、snapshot、keymap action、E2E 和 eval fixture，确保新品牌可见
      结果与内部标识一致。

**产出与验收**

- 新用户从启动、编辑、报错、登录、扩展、更新到退出的可见文字均为 Orion Studio
  或明确的第三方/兼容说明。
- macOS、Windows、Linux 的截图/快照和人工 smoke review 均通过。

**Gate 4**：用户可见界面仍显示 Zed，或文档仍把用户导向未经批准的 Zed 服务，No-Go。

### Phase 5：客户端服务、collab、远程和部署迁移

**任务**

- [ ] 替换 crates/client、zed_urls、cloud_api_client、collab、remote server
      的规范配置和 endpoint；删除活动默认值中的旧服务地址。
- [ ] 为认证、账户、更新、遥测、扩展 registry、RPC/WebSocket、对象存储、LiveKit、
      数据库和密钥建立 Orion 服务契约。
- [ ] 检查 Dockerfile-collab、其他 Dockerfile、Kubernetes/部署清单、Procfile.web
      和健康检查；不要把现有 collab crate 直接当成完整商业 SaaS。
- [ ] 对 RPC、扩展协议、数据库 schema 和消息格式采取版本协商或双读/双写；禁止无
      迁移地改 wire format。

**产出与验收**

- 本地最小部署可启动、健康检查可用、认证和 WebSocket/RPC 可完成一次真实链路。
- 客户端离线/自托管模式不依赖 Zed hosted service；服务异常会向 UI 返回可理解的错误。
- 数据库备份、schema migration、密钥轮换、监控和回滚演练有证据。

**Gate 5**：任何活动默认连接旧服务、使用未批准密钥、缺少数据归属/隐私说明或无法
回滚时，No-Go。

### Phase 6：跨平台打包、更新与 CI/CD

**任务**

- [ ] 更新 macOS bundle、签名、notarization、URL scheme、Windows installer/registry、
      Linux desktop/Flatpak、产物名和 executable。
- [ ] 更新 .github/workflows/ 的 owner 条件、仓库链接、release、artifact、缓存、
      secret、签名和发布服务；核对 fork 仍使用 zed-industries 条件导致 workflow
      静默跳过的问题。
- [ ] 更新更新通道、版本显示、崩溃/遥测 DSN、发布说明、安装脚本和卸载脚本。
- [ ] 在干净环境验证新安装、旧版本升级、保留配置、卸载、再次安装和回滚。

**产出与验收**

- 至少覆盖 macOS、Windows、Linux 的可安装 artifact；每个 artifact 有 hash、签名/
  签名状态、启动截图或日志和对应 commit。
- CI 在 Orion 仓库实际触发并报告结果，不依赖旧组织条件的偶然匹配。
- 安装后进程、应用 ID、路径、协议注册和更新源均为 Orion 规范。

**Gate 6**：只有源码构建、没有干净环境安装/升级/回滚证据时，No-Go，不得发布。

### Phase 7：收口、灰度与旧标识退场

**任务**

- [ ] 用统一扫描脚本复扫源码、资源、文档、脚本、artifact 和运行时日志。
- [ ] 对剩余 Zed 命中生成 allowlist：法律/版权、上游归属、兼容、迁移、测试和历史
      记录分别标注，禁止把整个文件夹加入宽泛白名单。
- [ ] 灰度发布 Orion；观察启动成功率、迁移失败率、登录、更新、扩展、远程和服务错误。
- [ ] 在兼容窗口到期且数据/协议 adoption 证据充分后，单独提交旧 CLI/env/scheme/path
      的移除计划，不与首轮品牌迁移混在一起。

**Gate 7（最终 Go/No-Go）**

只有以下条件全部满足才可称为 Orion Studio 首个可发布版本：

1. P0 用户数据、运行时身份、安装器和 endpoint 已完成迁移并可回滚。
2. 规范代码、package、binary、namespace 和服务契约已经由干净 checkout 验证。
3. 用户可见品牌、平台安装包、更新、文档和支持入口全部指向 Orion。
4. 旧标识只存在于有理由的兼容、迁移、法律、版权或上游归属位置。
5. 许可证、商标、隐私、数据驻留、凭据和服务责任均已批准。
6. 构建、测试、安装、升级、服务部署、监控和回滚证据已归档。

## 6. 兼容与迁移矩阵

| 旧身份                           | Orion 规范           | 迁移策略                                         | 验收证据                          |
| -------------------------------- | -------------------- | ------------------------------------------------ | --------------------------------- |
| Zed 显示文本                     | Orion Studio         | 新界面只显示 Orion；上游/版权文本单独保留        | UI smoke、snapshot、文案扫描      |
| zed package/binary               | Orion package/binary | 分批 rename；旧命令是否为 shim 由 Phase 1 决定   | clean checkout build、CLI E2E     |
| ZED\_\* env                      | ORION*STUDIO*\*      | 新名优先；旧名只读兼容并告警                     | env precedence、弃用日志测试      |
| Zed 配置/缓存/状态目录           | Orion 专属目录       | 版本化、幂等、原子迁移，保留备份                 | 新装/升级/中断恢复                |
| .zed_server 等远程目录           | Orion 远程目录       | 先探测旧目录，成功迁移后才使用新目录             | remote server smoke               |
| zed://                           | Orion scheme         | 新 scheme 为规范；旧 scheme 兼容窗口内解析或转发 | OS protocol registration、CLI E2E |
| zed.dev/旧 cloud/collab endpoint | Orion endpoint       | 不能静默 fallback；自托管/离线失败需可解释       | URL inventory、网络隔离测试       |
| 旧 RPC/扩展协议                  | Orion 版本化协议     | 双版本协商或兼容 decoder；禁止无迁移破坏         | protocol contract tests           |
| 旧更新 artifact                  | Orion artifact       | 保留已发布版本可回滚，新的更新源只发 Orion       | install/upgrade/rollback          |

旧标识的具体保留期限、是否提供 CLI shim、是否支持旧 scheme，必须由 Phase 1 的
兼容决策记录批准；本表不替代决策。

## 7. 验证与证据清单

### 7.1 静态与构建验证

在改造过程中每个阶段都保留命令、commit、环境和结果：

```text
git status --short --branch
git rev-parse --short HEAD
git diff --check
cargo +stable metadata --no-deps --format-version 1
cargo +stable fmt --all -- --check
./script/check-todos
./script/check-keymaps
./script/clippy
```

品牌扫描必须同时覆盖跟踪文件和工作树文件，建议使用固定脚本而不是人工 rg：

```text
git grep -n -I -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
rg --hidden --glob '!.git/**' --glob '!.workbuddy/**' \
  -n -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
```

扫描输出必须包含路径、命中类别、处理结论和 allowlist 原因。计划文件和研究材料
本身可保留上游名称，但不能成为源码/发布扫描的宽泛豁免。

### 7.2 功能与发布验证

- 新装：启动、创建项目、打开文件、保存、终端、LSP、扩展、AI、退出和再次启动。
- 升级：旧版本数据迁移、权限、密钥、扩展、远程连接、失败重试和回滚。
- 协议：CLI、文件/行列打开、URL scheme、远程 server、RPC/WebSocket。
- 服务：认证、健康检查、数据库迁移、对象存储/LiveKit（若纳入范围）、服务异常反馈。
- 平台：macOS、Windows、Linux 的应用名、图标、签名、注册表/desktop entry、安装器。
- 视觉：窗口、菜单、关于、登录、更新、错误、空状态和商店元数据人工审查。
- 供应链：许可证、依赖、secret 泄漏、artifact hash、签名、更新源和回滚包。

每个验收项都要产出“命令或操作、环境、commit、结果、日志/截图/链接”，而不是只
写“已测试”。

## 8. 回滚设计

1. 每个 Phase 使用独立可审查提交；在 P0、P2、P5、P6 结束时打内部 checkpoint。
2. 发布前保留上一版可安装 Orion artifact、配置备份和数据库备份；不依赖重新构建来
   进行紧急回滚。
3. 数据迁移失败时保留旧数据和 Orion 临时目录，恢复旧路径/旧版本可读性；不能自动
   删除或覆盖用户数据。
4. 服务 schema 必须支持回滚或明确 forward-only 迁移；forward-only 时先确认旧客户端
   的兼容读取，再允许上线。
5. 服务不可用时提供本地/离线/自托管的明确错误路径；未经批准不能把用户请求静默转
   发到 Zed hosted service。
6. 旧兼容层的移除必须是后续独立变更，且先有 adoption、错误率和数据恢复证据。

## 9. 主要风险

| 风险                                      | 影响 | 缓解措施                                                |
| ----------------------------------------- | ---- | ------------------------------------------------------- |
| 路径迁移覆盖或丢失用户数据                | 高   | 版本化、备份、原子操作、故障注入和回滚演练              |
| 全局改名破坏 Rust package、扩展或动态查找 | 高   | 依赖图分批、兼容入口、每批编译与契约测试                |
| 旧 scheme/CLI/协议无法打开项目            | 高   | 双版本解析、shim/迁移矩阵、真实 OS E2E                  |
| 仍连接旧服务或使用旧凭据                  | 高   | endpoint allowlist、网络隔离、secret 扫描和服务责任审批 |
| GitHub owner 条件导致 CI/发布静默跳过     | 高   | 在 Orion 仓库真实触发 workflow，审查 secrets 和权限     |
| GPL、第三方依赖或商标边界错误             | 高   | crate 级 SPDX 清单与法务 Gate，保留 attribution         |
| 上游持续变更导致长期无法同步              | 中   | 规范身份集中管理、最小差异、定期 rebase checkpoint      |
| 云端/商业范围蔓延                         | 中   | Web、托管、计费、SLA 单独立项和 Go/No-Go                |

## 10. 待决问题

以下问题在 Phase 1 前必须有明确答案；若没有答案，执行范围只能停留在桌面客户端
品牌 PoC：

1. Orion 首个版本是桌面编辑器，还是同时承诺 Web IDE/托管协作？
2. 是深度维护 Zed fork，还是只保留 GPUI/部分组件？上游同步目标是什么？
3. 主二进制、CLI、URL scheme、Bundle ID、Flatpak ID 的最终值是什么？旧 zed 是否
   提供一个版本的兼容 shim？
4. Orion 的官网、登录、更新、崩溃、遥测、扩展 registry、cloud、collab 和 RPC 域名
   由谁持有和运营？
5. 客户端与 collab 的许可证、第三方依赖、商标和 hosted service 的可分发边界是什么？
6. 是否支持 BYOK、本地模型、离线模式和自托管；源代码、提示词、协作数据和遥测的
   数据驻留与保留策略是什么？
7. 旧配置、密钥、扩展、协议和数据库的兼容窗口是多少？如何通知用户？
8. Orion 的签名证书、发布仓库、CI secret、更新服务、监控和安全响应责任人是谁？

## 11. 完成定义

本计划对应的改造只有在最终 Gate 通过后才能标记完成：

- Orion Studio 是唯一规范的产品展示和发布身份。
- 运行时路径、配置、日志、环境变量、协议、package、binary、服务端点和安装包均有
  规范来源。
- 旧 Zed 标识只存在于有明确理由的兼容、迁移、法律、版权、上游归属或历史记录中。
- 新装、升级、失败恢复、服务异常、跨平台安装、更新和回滚均有可复现证据。
- 许可证、商标、隐私、数据和服务运营边界已获得批准。
- 计划中的未决项全部关闭，或被记录为明确的后续独立项目；没有把“源码改名完成”
  当作“产品发布完成”。

## 12. 第一轮实施建议

计划获批后，第一轮只做三件事：

1. 生成机器可读品牌 inventory/allowlist 和当前基线证据。
2. 冻结身份、服务、许可证和兼容矩阵。
3. 在 crates/paths 上实现一个最小数据迁移 PoC，并先通过新装、升级、中断恢复和
   回滚测试。

只有这三项通过 Gate 0/1/2，才进入全仓 package、UI、服务和发布链迁移。
