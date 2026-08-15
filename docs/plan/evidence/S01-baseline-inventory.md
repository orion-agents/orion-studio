# S01 证据：品牌、运行时和发布基线盘点

> 子计划：S01 品牌、运行时和发布基线盘点
> 状态：**DONE**（仅一处工具缺口，已在正文与验收项标注）
> 执行分支：`init`
> 基线 HEAD：`d2779c3`
> 执行日期：2026-08-09
> 执行人：WorkBuddy（只读扫描，未修改任何源码/脚本/资源）

---

## 0. 摘要（先看这里）

本次 S01 是**只读基线盘点**，唯一写入产物即本文件。任务是回答"哪些 Zed 标识需要迁移 / 兼容 / 保留归属 / 删除"，并把扫描口径固定下来，供后续子计划（`docs/plan/subplans/02..11`）复跑与比对。

主要结论：**必须迁移的运行时身份高度集中在少数 P0 锚点文件**（`crates/paths`、`crates/zed_env_vars`、`crates/release_channel`、`crates/zed/Cargo.toml`、`crates/cli`、`crates/client`、`crates/remote_server`），活动默认服务端点全部硬编码指向 `zed.dev` / `cloud.zed.dev` / `api.zed.dev`，且扩展 API 命名空间为 `zed::`——这是后续兼容性工作的核心风险点。

**一处工具缺口**：执行环境未安装 `rg`（计划工作树扫描的指定工具）。按 S01 失败处理规则，已记录命令与错误、检查工具可用性，并以 `grep -rI`（排除 `.git`/`target`/`.workbuddy`，与 `rg` 默认 ignore 行为一致）作为可审计备用扫描，未做任何宽泛删除。备用扫描口径与 `git grep` 完全吻合（见 §2）。

---

## 1. 基线、工作树边界与工具链

| 项                                                 | 值                                                                                                      |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| 分支                                               | `init`                                                                                                  |
| HEAD                                               | `d2779c3`（`git rev-parse --short HEAD`）                                                               |
| 最近提交                                           | `d2779c3 language: Avoid UTF-16 false positive with embedded ASCII (#61250)`                            |
| 工具链                                             | `rustc 1.95.0 (59807616e 2026-04-14)` / `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`                           |
| 工作树 tracked/staged 修改                         | **无**（`git diff` / `git diff --cached` 均为空）                                                       |
| 未跟踪（按规矩不修改、不清理）                     | `.workbuddy/`、`docs/plan/`（含本计划全部子计划）、`docs/research/open-source-code-editors-research.md` |
| 其他未跟踪（被 `.gitignore` 忽略，不在源码口径内） | `.factory/`（已跟踪 8 文件，内部 prompts/skills，含 `brand-writer`，与产品迁移无关）                    |

环境满足 S01 前置条件：分支为 `init`、HEAD 已记录、tracked/staged 为空、用户文件边界已锁定。

---

## 2. 扫描口径、命令与命中数量

### 2.1 跟踪文件扫描（`git grep`，只看已跟踪文件）

| 命令（简化）                                                   | 范围                          | 命中数        |
| -------------------------------------------------------------- | ----------------------------- | ------------- |
| `git grep -Il -E '<PATTERN>'`                                  | 全仓库（跟踪文件）            | **1009 文件** |
| `git grep -In -E '<PATTERN>'`                                  | 全仓库（跟踪文件，按行）      | **7000 行**   |
| `git grep -Il -E '<PATTERN>' -- crates`                        | crates                        | **596 文件**  |
| `git grep -Il -E '<PATTERN>' -- assets`                        | assets                        | **27 文件**   |
| `git grep -Il -E '<PATTERN>' -- script .github legal docs/src` | script/.github/legal/docs/src | **298 文件**  |

> `<PATTERN>` = `Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://`
> 注意：按区域分桶的"文件数"之和（596+27+298=921）小于全仓库 1009，差值来自未单独分桶的目录（如 `tooling`、`extensions`、顶层文件等）；行数同理可能重叠，故各项**不可相加**，仅作分布参考。

### 2.2 工作树扫描（`rg` 指定工具 → 不可用，启用 `grep` 备用）

| 命令                                                                                                | 结果                                                                       |
| --------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `rg --hidden --glob '!.git/**' --glob '!target/**' --glob '!.workbuddy/**' -n -E '<PATTERN>'`       | **FAILED**：`rg: command not found`（PATH 中不存在）。已记录，未改用删除。 |
| 备用：`grep -rI --exclude-dir=.git --exclude-dir=target --exclude-dir=.workbuddy -nE '<PATTERN>' .` | **7110 行**（含未跟踪内容）                                                |

备用扫描的口径拆解（与 §2.1 交叉印证）：

- 其中 **docs/plan + docs/research = 110 行** → 单独标记为**「计划/研究材料」**，不计入源码残留结论（符合 START/S01 要求）。
- 其余 **7000 行** = 跟踪/源码部分，与 `git grep -In` 的 7000 行**完全吻合** → 证明本次工作树扫描未遗漏任何跟踪文件命中，口径可信。

> 复现提示：若后续环境装回 `rg`，建议用原计划 `rg` 命令复跑并比对 7110 这一总数；在 `rg` 缺失环境下，本报告以 `grep` 备用口径为准，数字差异仅来自工具差异，非扫描遗漏。

### 2.3 按顶层目录的命中分布（源码部分，备用扫描）

| 目录                                                                                     | 行数 |
| ---------------------------------------------------------------------------------------- | ---- |
| crates                                                                                   | 2996 |
| docs（docs/src，已跟踪）                                                                 | 2400 |
| .github                                                                                  | 594  |
| script                                                                                   | 320  |
| tooling                                                                                  | 173  |
| assets                                                                                   | 136  |
| legal                                                                                    | 85   |
| .factory（已跟踪，内部工具，非产品）                                                     | 52   |
| extensions                                                                               | 26   |
| nix                                                                                      | 25   |
| 其他（ci/.cloudflare/.agents/.zed/.cargo/lychee.toml/README/Cargo.toml/Procfile.web 等） | 余量 |

---

## 3. P0 清单（运行时身份、数据、可安装产品）— 代表命中

> 分类图例：MIGRATE=必须改为 Orion 规范；COMPAT=旧标识保留为兼容读取层（需窗口与测试，未知写 OPEN）；KEEP-ATTRIBUTION=版权/法律/上游归属保留；KEEP-HISTORY=历史/迁移/测试保留；DELETE-AFTER-APPROVAL=法务批准后替换；OPEN=需人类决策，不猜测。

| 文件:行                                         | 命中内容（类别）                                                                                                                        | 处理结论                                                                                                           |
| ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `crates/paths/src/paths.rs:18`                  | `pub const APP_NAME: &str = "Zed";`（展示名/运行时身份常量）                                                                            | **MIGRATE** — 第 17 行注释明示"Forks should change this to avoid colliding with Zed's user data"，是 fork 第一锚点 |
| `crates/paths/src/paths.rs:56/58/65`            | macOS `~/Library/Application Support/Zed`、Win `%LOCALAPPDATA%\Zed`、`%APPDATA%\Zed`（配置/数据/状态目录）                              | **MIGRATE**                                                                                                        |
| `crates/paths/src/paths.rs:239`                 | Zed server directory on SSH host（远程目录）                                                                                            | **MIGRATE**                                                                                                        |
| `crates/paths/src/paths.rs:245/251`             | `Zed.log` / `Zed.log.old`（日志文件名）                                                                                                 | **MIGRATE**（旧日志名进兼容读取）                                                                                  |
| `crates/zed_env_vars/src/zed_env_vars.rs:6`     | `ZED_STATELESS` 环境变量                                                                                                                | **COMPAT/MIGRATE**（新代码用 `ORION_STUDIO_*`，旧名只读兼容）                                                      |
| `crates/release_channel/src/lib.rs:10`          | `ZED_DOCS_URL = "https://zed.dev/docs"`（服务 URL）                                                                                     | **MIGRATE**（在 Orion docs 就绪前是否暂留为 OPEN，见 §6）                                                          |
| `crates/release_channel/src/lib.rs:47-50`       | Windows App ID：`Zed-Editor-Dev/Nightly/Preview/Stable`                                                                                 | **MIGRATE**（平台发布身份）                                                                                        |
| `crates/release_channel/src/lib.rs:15/28/104`   | `ZED_RELEASE_CHANNEL`、`ZED_APP_VERSION`                                                                                                | **COMPAT/MIGRATE**                                                                                                 |
| `crates/zed/Cargo.toml:8`                       | `authors = ["Zed Team <hi@zed.dev>"]`                                                                                                   | **KEEP-ATTRIBUTION**（按法务决定更新）                                                                             |
| `crates/zed/Cargo.toml:285-310`                 | Bundle identifier `dev.zed.Zed-Dev/-Nightly/-Preview`、name `Zed Dev/Nightly/Preview/Stable`                                            | **MIGRATE**（macOS Bundle ID）                                                                                     |
| `crates/cli/src/main.rs:34`                     | `URL_PREFIX` 含 `"zed://"`（URL scheme）                                                                                                | **MIGRATE**（新 scheme 规范，旧 scheme 兼容窗口 OPEN）                                                             |
| `crates/cli/src/main.rs:91/1088/1103`           | `dev.zed.Zed` Flatpak ID                                                                                                                | **MIGRATE**（Flatpak ID）                                                                                          |
| `crates/cli/src/main.rs:577/508/1036/1037/1059` | `ZED_CHANNEL`、`ZED_ASKPASS_SOCKET`、`ZED_FLATPAK_LIB_PATH`、`ZED_FLATPAK_NO_ESCAPE`、`ZED_UPDATE_EXPLANATION`                          | **COMPAT/MIGRATE**                                                                                                 |
| `crates/client/src/client.rs:63-85`             | `ZED_SERVER_URL`/`ZED_RPC_URL`/`ZED_IMPERSONATE`/`ZED_WEB_LOGIN`/`ZED_ADMIN_API_TOKEN`/`ZED_APP_PATH`/`ZED_ALWAYS_ACTIVE`（环境变量簇） | **COMPAT**（新名 `ORION_STUDIO_*` 优先；旧名只读兼容并告警，兼容窗口 OPEN）                                        |
| `crates/client/src/client.rs:1942`              | `pub const ZED_URL_SCHEME: &str = "zed";`                                                                                               | **MIGRATE**（URL scheme 常量）                                                                                     |
| `crates/client/src/client.rs:1946-1993`         | `ZedLink` 枚举、`zed.dev/channel/...` 解析                                                                                              | **MIGRATE/COMPAT**                                                                                                 |
| `crates/client/src/zed_urls.rs:1/5`             | `zed.dev` URL 构造辅助                                                                                                                  | **MIGRATE**（服务 URL）                                                                                            |
| `crates/remote_server/build.rs:4/8/10/29/32`    | 读取 `zed/Cargo.toml`、注入 `ZED_PKG_VERSION`/`ZED_COMMIT_SHA`/`ZED_BUILD_ID`                                                           | **MIGRATE**（改为从 Orion Cargo.toml 派生）                                                                        |
| `crates/remote_server/src/server.rs:691`        | User-Agent `Zed-Server/{}`                                                                                                              | **MIGRATE**                                                                                                        |

> 未发现疑似 secret：上述均为标识符/URL/常量名，未出现任何凭据明文。若后续在私有配置中发现疑似 secret，按 S01 规则只记录路径行号、不复制内容并 BLOCKED。

---

## 4. P1 清单（用户可见内容、协议文案、测试契约）— 代表命中

| 文件:行                                                                                        | 命中内容（类别）                                                                   | 处理结论                                                                                                                        |
| ---------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `README.md:1/3/6/12`                                                                           | `# Zed`、badge `zed-industries/zed`、`zed.dev` 下载/文档链接                       | **MIGRATE**（展示文本）；badge/repo 链接 **KEEP-ATTRIBUTION** 或按决策 MIGRATE                                                  |
| `assets/settings/default.json:2`                                                               | `"$schema": "zed://schemas/settings"`（URL scheme 用于配置 schema）                | **MIGRATE/COMPAT**（scheme 改名需同步 schema 服务与客户端）                                                                     |
| `assets/settings/default.json:14/27/31/57/59`                                                  | `icon_theme: "Zed (Default)"`、`base_keymap: "Zed"`、`.ZedMono`、`.ZedSans` 字体   | **MIGRATE**（用户可见品牌/默认值）                                                                                              |
| `crates/extension_api/src/extension_api.rs:21-52`、`README.md:43/47`                           | `zed::` WIT / Rust 扩展命名空间（`zed::extension::*`、`zed::register_extension!`） | **COMPAT（关键）**：第三方扩展依赖此命名空间；改名需双版本协商或兼容 decoder，兼容窗口 **OPEN**（不得无迁移破坏 wire/扩展格式） |
| `docs/src/`（约 2400 行）                                                                      | 文档正文、截图、链接中的 `Zed` / `zed.dev`                                         | **MIGRATE**（展示文本）+ **KEEP-ATTRIBUTION**（上游链接/归属）                                                                  |
| 多处 UI 帮助链接（`agent_ui`、`debugger_ui`、`editor`、`extensions_ui`、`edit_prediction` 等） | `https://zed.dev/docs/...` 打开文档                                                | **MIGRATE**（指向 Orion docs；在 Orion docs 就绪前是否暂留 OPEN）                                                               |
| 测试 fixture / visual test / E2E / eval（分布在各 crate）                                      | `zed://` 测试数据、`Zed` 期望文案、snapshot                                        | **KEEP-HISTORY**（迁移代码与测试中保留旧标识作为兼容/历史参考）                                                                 |

---

## 5. P2 清单（组织归属、上游链接、法律文本）— 代表命中

| 文件:行                                                                                                                         | 命中内容（类别）                                                                                         | 处理结论                                                                                                                    |
| ------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `.github/workflows/*.yml`（如 `after_release.yml:29/42/108/110`、`add_commented_closed_issue_to_project.yml:14/42/55/85`）      | `github.repository == 'zed-industries/zed'`、`owner: zed-industries`、`ZedIndustries.Zed[.Preview]` 包名 | **MIGRATE**（CI/发布身份）；⚠️ **风险**：若 fork 仍带 `zed-industries` 条件，workflow 会静默跳过——必须在 Orion 仓库实跑验证 |
| `legal/terms.md`、`legal/privacy-policy.md`（多处）                                                                             | "Zed Industries, Inc."、`zed.dev`、`legal@zed.dev`、仲裁地址                                             | **KEEP-ATTRIBUTION / DELETE-AFTER-APPROVAL**：属法律文本，须**法务单独审批**后重写，禁止用普通搜索替换处理                  |
| `Dockerfile`、`Dockerfile-collab`、`Procfile`/`Procfile.web`、`ci/`、`mailmap`、`compliance/`、`.claude`/`CODEOWNERS`（如存在） | 组织名、上游 fork URL、部署脚本                                                                          | **MIGRATE**（组织引用）/ **KEEP-ATTRIBUTION**（版权/第三方）                                                                |
| `Cargo.toml`（顶层）                                                                                                            | 上游 fork URL 等                                                                                         | **MIGRATE/复核**（是否继续依赖上游 fork URL）                                                                               |

---

## 6. 服务端清单（collab / cloud / 部署）— 活动默认 endpoint

| 文件:行                                                      | 当前活动默认值（服务 URL / 标识）                                                | 处理结论                                                                  |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `crates/collab/src/lib.rs:153-154`                           | staging→`https://staging.zed.dev`，默认→`https://zed.dev`                        | **MIGRATE**（不得把 `zed.dev` 留作活动默认；自托管/离线失败需可解释）     |
| `crates/collab/src/lib.rs:162`                               | 默认 RPC/cloud→`https://cloud.zed.dev`                                           | **MIGRATE**                                                               |
| `crates/cloud_api_client/src/cloud_api_client.rs:338/357`    | `https://cloud.zed.dev/client/users/me`                                          | **MIGRATE**                                                               |
| `crates/context_server/src/oauth.rs:37/1773/1785`            | `https://zed.dev/oauth/client-metadata.json`                                     | **MIGRATE**                                                               |
| `crates/http_client/src/http_client.rs:269`                  | `https://zed.dev` ⇒ `https://api.zed.dev`（API host 映射）                       | **MIGRATE**                                                               |
| `crates/release_channel/src/lib.rs:10`                       | `ZED_DOCS_URL = "https://zed.dev/docs"`                                          | **MIGRATE**（Orion docs 就绪前暂留属 OPEN，不得静默 fallback 到 zed.dev） |
| `crates/remote_server/src/server.rs`（UA/版本派生）          | 从 `ZED_*` env 派生版本；UA `Zed-Server/{}`                                      | **MIGRATE**                                                               |
| `Dockerfile-collab`、`compose.yml`、`livekit.yaml`、部署清单 | collab 服务端容器/编排（本次扫描未在这些文件命中 `zed` 字面，但属 S09 迁移范围） | **MIGRATE/复核**（S09 处理，不在 S01 改写）                               |

---

## 7. 当前活动默认 endpoint / 环境变量 / 路径 / App ID / CLI / URL scheme 汇总

| 类别                    | 当前默认值（Zed）                                                                     | 证据位置                                                             | 计划目标（待 S02 冻结）                                      |
| ----------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------ |
| 展示名 / APP_NAME       | `Zed`                                                                                 | `paths.rs:18`                                                        | Orion Studio（MIGRATE）                                      |
| 配置/数据/状态/日志目录 | `~/Library/Application Support/Zed`、`%LOCALAPPDATA%\Zed`、`%APPDATA%\Zed`、`Zed.log` | `paths.rs:56/58/65/245/251`                                          | Orion 专属目录（MIGRATE，旧目录兼容迁移）                    |
| 远程目录                | `.zed_server`（SSH host）                                                             | `paths.rs:239`                                                       | Orion 远程目录（MIGRATE，先探测旧目录）                      |
| macOS Bundle ID         | `dev.zed.Zed[-Dev/-Nightly/-Preview]`                                                 | `zed/Cargo.toml:285-310`                                             | Orion 唯一 ID（MIGRATE，**禁止猜测**，需域名/组织所有权）    |
| Windows App ID          | `Zed-Editor-Dev/Nightly/Preview/Stable`                                               | `release_channel:47-50`                                              | Orion App ID（MIGRATE）                                      |
| Flatpak ID              | `dev.zed.Zed`                                                                         | `cli/main.rs:91/1088/1103`                                           | Orion Flatpak ID（MIGRATE）                                  |
| 主二进制 / CLI 命令     | `zed`                                                                                 | `cli`、各启动脚本                                                    | `orion-studio`（旧 `zed` 是否保留兼容别名 **OPEN**）         |
| URL scheme              | `zed://`（`ZED_URL_SCHEME="zed"`）                                                    | `client.rs:1942`、`cli/main.rs:34`                                   | Orion scheme（旧 `zed://` 兼容窗口 **OPEN**）                |
| 服务 endpoint           | `zed.dev` / `staging.zed.dev` / `cloud.zed.dev` / `api.zed.dev`                       | `collab/lib.rs`、`cloud_api_client`、`http_client`、`context_server` | Orion 自有的域名与凭据（**禁止留 zed.dev 作活动默认**）      |
| 环境变量前缀            | `ZED_*`（`ZED_SERVER_URL`/`ZED_RPC_URL`/`ZED_CHANNEL`/`ZED_STATELESS`/…）             | `client.rs`、`release_channel`、`cli`、`zed_env_vars`                | `ORION_STUDIO_*`（旧 `ZED_*` 仅进兼容读取层，窗口 **OPEN**） |
| 扩展命名空间            | `zed::`                                                                               | `extension_api`                                                      | Orion 命名空间（扩展兼容策略 **OPEN**，关键风险）            |

---

## 8. 许可证、商标、上游归属与研究文档矛盾

- **许可证（以实际文件为准，非研究文档）**：仓库含 `LICENSE-APACHE` 与 `LICENSE-GPL`。GPUI 框架为 Apache-2.0；编辑器/多数 crate 为 GPL-3.0-or-later；`collab` 服务端随主许可为 GPL-3.0-or-later（**注意**：`docs/research/` 调研文档称 collab 用 AGPL，与当前 manifest 不一致——以实际许可证文件 + 法务确认结果为准，不采信研究文档的二手描述）。
- **商标**：`Zed` 名称、Logo、`dev.zed.Zed*` Bundle ID、`ZedIndustries.Zed` 包名均为 Zed Industries 商标。产品面向用户的品牌标识必须改为 Orion；仅在法务要求的 attribution 中可保留"基于 Zed"等说明。
- **上游归属**：版权头、`CONTRIBUTORS`、第三方依赖声明（`Cargo.lock`、各 crate LICENSE）不得删除（KEEP-ATTRIBUTION）。
- **研究文档矛盾**：`docs/research/open-source-code-editors-research.md` 同时讨论 Theia / Web IDE / 商业订阅，并称 collab 为 AGPL；该文档是**输入材料**，不代表已批准的产品需求，且其许可描述与真实 manifest 冲突。按 S01 要求，该文档（及全部 `docs/plan/`）的 Zed 命中**单独标记为「计划/研究材料」**，不混入源码残留结论。

---

## 9. 推荐迁移顺序（依据总计划 §5 阶段）

1. **S01（本步）**：基线盘点 + 可重复 inventory（完成）。
2. **S02 身份契约**：冻结展示名 / slug / crate 名 / 二进制 / CLI / URL scheme / Bundle ID / 配置目录 / 环境变量前缀 / 兼容窗口；确认 Orion 域名与凭据所有权、许可证与商标边界。
3. **S03 运行时身份与路径**：先改 `crates/paths` + 身份常量，使路径/日志/临时/远程目录/应用名从单一规范来源派生。
4. **S04 数据迁移**：新装只建 Orion 目录；升级检测旧目录，版本化、幂等、可恢复迁移；旧 `ZED_*` 只读兼容并告警。
5. **S05 核心 package / binary**：按依赖图分批改名，保持可编译、可回滚。
6. **S06 CLI / API / 协议**：迁移 CLI、`zed://` scheme、`zed::` 扩展命名空间（双版本协商）。
7. **S07 UI / 资源 / 文档**：窗口标题、菜单、关于页、README、docs/src、图标、主题。
8. **S08 客户端服务**：替换 `client`/`zed_urls`/`cloud_api_client` 的 endpoint，删除活动默认中的旧服务地址。
9. **S09 collab / 远程 / 部署**：迁移 `collab`、remote server、Dockerfile-collab、compose、livekit、健康检查。
10. **S10 打包 / CI**：更新各平台安装包、`.github/workflows` owner/secret/签名/`zed-industries` 条件。
11. **S11 回归 / 发布门禁**：统一脚本复扫 + allowlist + 灰度 + 旧标识退场。

---

## 10. 需要人类决定的问题（OPEN，模型不猜测）

1. Orion 首个版本形态：桌面编辑器，还是同时承诺 Web IDE / 托管协作？（影响服务迁移范围）
2. 深度维护 Zed fork，还是只复用 GPUI / 部分组件？上游同步目标是什么？
3. 主二进制、CLI（`zed`）、URL scheme（`zed://`）、Bundle ID、Flatpak ID 的最终值；旧 `zed` 是否提供一个版本兼容 shim？
4. Orion 的官网、登录、更新、崩溃、遥测、扩展 registry、cloud、collab、RPC 域名由谁持有运营？（禁止猜测域名）
5. 客户端与 collab 的许可证、第三方依赖、商标、hosted service 可分发边界（尤其 collab 实际为 GPL-3.0-or-later 还是 AGPL，需法务确认）。
6. BYOK / 本地模型 / 离线 / 自托管支持；数据驻留与保留策略。
7. 旧配置 / 密钥 / 扩展 / 协议 / 数据库的兼容窗口长度与用户通知方式。
8. 签名证书、发布仓库、CI secret、更新服务、监控、安全响应责任人。
9. （工具）执行环境是否安装 `rg`？建议后续在 CI/执行机装回 `rg` 以复现原计划口径。

---

## 11. 未完成项（明确声明）

- ❌ 未修改任何 Rust / Cargo / 脚本 / CI / README / docs/src / 资源文件（S01 只读）。
- ❌ 未执行、未完成任何构建（`cargo build` 等）；不宣称 workspace 已编译通过。
- ❌ 未对生产服务做任何验证（无登录、无健康检查、无 RPC 链路）。
- ⚠️ `rg` 不可用，工作树扫描以 `grep` 备用口径完成；总数 7110 与 `git grep` 口径吻合，但建议在有 `rg` 的环境复跑比对。
- ⚠️ 所有兼容窗口、Orion 具体值、许可证细节均标记 OPEN，未猜测。
- ⚠️ 命中数量**不代表**改造完成度，仅为基础清单（符合 S01 "禁止把扫描命中数量当作改造完成证据"）。

---

## 12. 交接（按 README 模板）

```
子计划：S01 品牌、运行时和发布基线盘点
状态：DONE
执行分支：init
基线 HEAD：d2779c3
修改文件：docs/plan/evidence/S01-baseline-inventory.md（唯一新增）
未修改但检查过的关键文件：
  - crates/paths/src/paths.rs, crates/zed_env_vars/src/zed_env_vars.rs,
    crates/release_channel/src/lib.rs, crates/zed/Cargo.toml,
    crates/cli/src/{cli.rs,main.rs}, crates/client/src/{client.rs,zed_urls.rs},
    crates/remote_server/{build.rs,src/server.rs},
    assets/settings/default.json, README.md, docs/src/*,
    crates/extension_api/*, .github/workflows/*, legal/*,
    crates/collab/src/lib.rs, crates/cloud_api_client/src/cloud_api_client.rs,
    crates/context_server/src/oauth.rs, crates/http_client/src/http_client.rs
扫描命中数量：
  - git grep（跟踪文件）：1009 文件 / 7000 行
    （crates 596 文件、assets 27 文件、script/.github/legal/docs/src 298 文件）
  - 工作树扫描：rg 不可用(FAILED) → grep 备用 7110 行
    （其中 docs/plan+research 计划/研究材料 110 行；源码/跟踪 7000 行）
执行命令与结果：
  - 固定基线(pwd/branch/HEAD/status/diff/ls-files/rustc/cargo)：全部成功，分支=init，无 tracked/staged 修改
  - git grep -Il / -In / 分区域：成功，见 §2.1
  - rg 工作树扫描：FAILED(rg not found)，已记录并使用 grep 备用，见 §2.2
  - P0/P1/P2/服务端采样与活动默认提取：成功，见 §3-§7
验收项：
  - A1：PASS，证据=本报告存在且可由命令重新生成（§2 列出完整命令）
  - A2：PASS，证据=至少包含 P0(§3)/P1(§4)/P2(§5)/服务端(§6) 四类清单
  - A3：PASS，证据=所有代表命中均标注分类或 OPEN（§3-§6）；未发现 secret（仅标识符/URL）
  - A4：PASS，证据=除 docs/plan/evidence/S01-baseline-inventory.md 外，git diff 为空（见 §13 校验）
  - A5：PARTIAL，证据=rg 不可用，工作树扫描以 grep 备用完成；口径与 git grep 吻合，但工具缺口需在后续环境补齐（§2.2/§10-Q9）
失败或未决问题：
  - rg 未安装（工具缺口，已用 grep 备用，不影响结论可信度）
  - 全部兼容窗口/Orion 具体值/collab 实际许可 标记 OPEN，待 S02 人类决策
  - docs/research 文档称 collab 为 AGPL，与 manifest(GPL-3.0-or-later)不一致，待法务确认
回滚方式：本子计划只读，无源码改动，无需回滚；唯一新增文件可整体删除（不影响工作树其他内容）
建议下一步：执行 S02（身份契约），在 S02 中冻结命名/服务/许可证/兼容窗口，
           并决策 §10 列出的 OPEN 项；S03 起才允许首次源码改动
```

---

## 13. 完成前校验（工作树边界）

执行后再次确认仅 evidence 文件变化：

```
git diff --check            # 无冲突标记
git status --short --branch # 仅 ?? docs/plan/evidence/S01-baseline-inventory.md 为新增（其余 docs/plan 等早已未跟踪）
git diff --name-only        # 空（无 tracked 修改）
```

> 校验结果：tracked 文件零修改；新增文件仅 `docs/plan/evidence/S01-baseline-inventory.md`，符合 S01 "工作树 diff 只能显示允许的 evidence 文件" 要求。未触碰 `.workbuddy/`、`docs/research/`、`docs/plan/` 中既有计划文件，也未执行 git reset/clean/rm -rf、未提交/推送。

**状态：DONE。等待人工确认后，方可进入 S02；不自动进入后续子计划。**
