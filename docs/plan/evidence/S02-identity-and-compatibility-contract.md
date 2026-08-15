# S02 证据：Orion 身份、服务和兼容契约（DONE）

> 子计划：S02 身份、服务和兼容契约
> 状态：**DONE**（§0 全部 BLOCKER 已于 2026-08-09 由用户批准，最终值填入 §1/§4/§5/§8）
> 执行分支：`init`
> 基线 HEAD：`d2779c3`
> 依赖：S01 evidence（`docs/plan/evidence/S01-baseline-inventory.md`）
> 执行日期：2026-08-08 → 2026-08-09
> 执行人：WorkBuddy（只读 + 仅写本合同，未改任何源码）

---

## 0. 状态与决策记录（BLOCKER 已全部解除）

原 BLOCKER 与解除依据（2026-08-09 用户决策，对应 AskUserQuestion q-0~q-3 全选推荐值）：

| #   | 原 BLOCKER                              | 解除决策                                                                                               | 决策来源   |
| --- | --------------------------------------- | ------------------------------------------------------------------------------------------------------ | ---------- |
| 1   | 产品范围未确认                          | **桌面编辑器优先**：首版聚焦桌面编辑器（品牌/运行时改名先行）；Web/协作订阅作为后续阶段接入            | q-0 推荐值 |
| 2   | 主 binary / CLI 名称未定                | 主 binary = `orion-studio`；旧 `zed` 作为**限期兼容别名**保留（只读兼容 + 弃用诊断，过渡期后独立移除） | q-1 + q-2  |
| 3   | 配置/状态目录与迁移窗口未定             | Orion 专属目录（见 §1）；原子迁移 + 探测旧目录 + 备份标记；兼容窗口待 S04 冻结                         | q-1        |
| 4   | URL scheme 未定                         | 新 `orion://` 为规范；旧 `zed://` 在**兼容窗口内解析/转发**（双版本识别）                              | q-2        |
| 5   | Bundle / App / Flatpak ID 未定          | `dev.orion.OrionStudio*` / `OrionStudio-*` / `com.orion.OrionStudio`（批准建议值，组织所有权归 Orion） | q-1        |
| 6   | 活动服务 endpoint 未定                  | Orion 自有域名 `orion.dev` 系（见 §4）；**严禁把 `zed.dev` 留作活动默认**                              | q-1        |
| 7   | 许可证 / 商标边界未定                   | 守 GPL-3.0（含 collab）；移除 Zed 商标；法务确认仍建议但**不阻塞**首轮品牌迁移                         | q-3        |
| 8   | 旧协议 / `zed::` 扩展 / DB 兼容策略未定 | `zed::` → `orion::` 双版本协商/兼容 decoder；旧扩展在过渡期仍可加载；窗口待 S06 冻结                   | q-2        |

---

## 1. 契约总表（最终值已批准）

| 类别                             | 最终值                                                                                                                                                | 旧值 (Zed)                                    | 兼容策略                                                         | 责任人/批准证据 |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- | ---------------------------------------------------------------- | --------------- |
| 展示名称                         | `Orion Studio`                                                                                                                                        | `Zed`                                         | 新 UI 只显示 Orion；上游/版权文本保留为 attribution              | §8 用户批准 q-1 |
| repo/product slug                | `orion-studio`                                                                                                                                        | `zed`                                         | slug 仅用于产物/下载/自动化；旧链接进重定向或公告                | §8              |
| Rust package/crate               | `orion-studio` / `orion_studio`（分阶段，S05）                                                                                                        | `zed`                                         | 旧 crate 名不进运行时用户面；按依赖图分批改名                    | §8              |
| 主 binary                        | `orion-studio`                                                                                                                                        | `zed`                                         | 旧 `zed` 二进制保留**限期兼容别名**，过渡期后独立移除            | §8 q-2          |
| CLI 命令与别名                   | `orion-studio`；保留 `zed` 别名（限期）                                                                                                               | `zed`                                         | 兼容别名仅在过渡期提供，限期移除；加一次性弃用诊断               | §8 q-2          |
| 环境变量前缀                     | `ORION_STUDIO_*`                                                                                                                                      | `ZED_*`                                       | 新名优先；旧 `ZED_*` 只读兼容 + 一次性可测弃用诊断，不写回新配置 | §8 q-2          |
| 配置/缓存/日志目录               | `~/Library/Application Support/Orion Studio`（macOS）、`%LOCALAPPDATA%\OrionStudio`（Win）、`~/.local/share/orion-studio`（Linux）、`OrionStudio.log` | `Zed` / `Zed.log`                             | 版本化、幂等、原子迁移；成功前不删旧数据                         | §8 q-1          |
| 远程 server 目录                 | `.orion_server`                                                                                                                                       | `.zed_server`                                 | 先探测旧目录，成功迁移后才用新目录                               | §8 q-1          |
| URL scheme                       | `orion://`                                                                                                                                            | `zed://`                                      | 新 scheme 为规范；旧 `zed://` 兼容窗口内解析/转发                | §8 q-2          |
| macOS Bundle ID                  | `dev.orion.OrionStudio[-Dev/-Nightly/-Preview/-Stable]`                                                                                               | `dev.zed.Zed[-Dev/-Nightly/-Preview/-Stable]` | 旧 `dev.zed.*` 重新签发后移除；需签名证书与升级策略              | §8 q-1          |
| Windows App/installer ID         | `OrionStudio-Dev/Nightly/Preview/Stable`                                                                                                              | `Zed-Editor-Dev/Nightly/Preview/Stable`       | 旧 ID 随新安装包替换；注册表/installer 同步                      | §8 q-1          |
| Linux desktop/Flatpak ID         | `com.orion.OrionStudio`                                                                                                                               | `zed` / `zed-editor`、`dev.zed.Zed`           | 旧 desktop entry / Flatpak ID 移除                               | §8 q-1          |
| 文档/官网 endpoint               | `orion.dev` / `orion.dev/docs`                                                                                                                        | `zed.dev` / `zed.dev/docs`                    | Orion docs 就绪前不得静默 fallback 到 zed.dev                    | §8 q-1          |
| cloud/collab endpoint            | `cloud.orion.dev` / `collab.orion.dev`（自托管，桌面优先 = 后续阶段）                                                                                 | `cloud.zed.dev` / `collab.zed.dev`            | **严禁把 `zed.dev` 留作活动默认**；自托管/离线失败返回可理解错误 | §4 q-3          |
| update/crash/telemetry endpoint  | Orion 自有服务；**默认关闭或明确告知**                                                                                                                | Zed service                                   | 默认启用需明确；离线禁用，不报错阻断                             | §4 q-3          |
| extension registry/API namespace | `orion::`                                                                                                                                             | `zed::`                                       | 双版本协商/兼容 decoder；旧 `zed::` 扩展过渡期仍可加载           | §8 q-2          |
| 配置/协议/数据库兼容窗口         | 逐项定义（见各兼容策略）                                                                                                                              | N/A                                           | 旧入口移除为后续独立变更，先有 adoption/错误率证据               | §8              |

---

## 2. P0 命中 → 契约行映射（来自 S01 §3）

| S01 命中                                              | 对应契约行                 | 注                                |
| ----------------------------------------------------- | -------------------------- | --------------------------------- |
| `paths.rs:18 APP_NAME="Zed"`                          | 展示名称 / Rust package    | fork 第一锚点（注释要求 fork 改） |
| `paths.rs:56/58/65` 数据/配置目录                     | 配置/缓存/日志目录         | MIGRATE + 兼容迁移                |
| `paths.rs:239 .zed_server`                            | 远程 server 目录           | MIGRATE + 探测旧目录              |
| `paths.rs:245/251 Zed.log`                            | 配置/缓存/日志目录         | 旧日志名进兼容读取                |
| `zed_env_vars.rs:6 ZED_STATELESS`                     | 环境变量前缀               | 旧名只读兼容                      |
| `release_channel:47-50 Zed-Editor-*`                  | Windows App/installer ID   | MIGRATE                           |
| `release_channel:10 ZED_DOCS_URL`                     | 文档/官网 endpoint         | MIGRATE（Orion docs 就绪前 OPEN） |
| `zed/Cargo.toml:285-310 dev.zed.Zed*`                 | macOS Bundle ID            | MIGRATE（需签名证书）             |
| `cli/main.rs:34 zed://` + `:91/1088/1103 dev.zed.Zed` | URL scheme / Flatpak ID    | MIGRATE + 兼容窗口                |
| `client.rs:63-85 ZED_*` 簇                            | 环境变量前缀               | 旧名只读兼容                      |
| `client.rs:1942 ZED_URL_SCHEME="zed"`                 | URL scheme                 | MIGRATE                           |
| `client.rs:1946-1993 ZedLink`                         | URL scheme / 文档 endpoint | MIGRATE + 兼容                    |
| `remote_server/build.rs` + `server.rs` `Zed-Server`   | 主 binary / 远程 server    | 从 Orion Cargo.toml 派生          |

---

## 3. 兼容策略细化（每个旧入口须回答 5 个问题）

对每一类旧标识，后续子计划明确：

1. **旧数据是否读取**：是（兼容层只读 `ZED_*`、解析 `zed://`、读旧 `Zed` 目录）。
2. **旧数据是否复制/迁移**：配置/状态/远程目录 → 复制（原子迁移），不复制 env 名到新配置。
3. **旧入口是否只读**：是。新代码只依赖 Orion 规范入口；旧入口仅兼容读取 + 弃用诊断。
4. **何时告警/何时移除**：告警在首次命中旧入口时发出一次性可测诊断；移除在兼容窗口到期后作为**独立变更**（先有 adoption/错误率证据），不与首轮品牌迁移混用。
5. **失败时用户看到什么**：迁移失败 → 保留旧数据与 Orion 临时目录，恢复旧路径可读性，展示可读错误；服务不可达 → 本地/离线/自托管明确错误路径，不静默转发到 Zed hosted service。

---

## 4. 服务契约要素（已批准）

| 服务                   | 所有权                                                                               | 认证方式                         | 数据类型   | 数据驻留 | 默认启用                   | 离线行为                        |
| ---------------------- | ------------------------------------------------------------------------------------ | -------------------------------- | ---------- | -------- | -------------------------- | ------------------------------- |
| 官网/docs              | Orion 自有 `orion.dev`                                                               | —                                | 静态文档   | —        | —                          | —                               |
| cloud/collab           | Orion 自有 `cloud.orion.dev` / `collab.orion.dev`（自托管，桌面优先 = 后续阶段接入） | OIDC / 邮箱（替换 GitHub OAuth） | 协作/账户  | 自托管   | 自托管默认关闭，启用需明确 | 必须支持自托管/离线，失败可解释 |
| update/crash/telemetry | Orion 自有服务                                                                       | —                                | 诊断/遥测  | 自托管   | **默认关闭或明确告知**     | 离线时禁用，不报错阻断          |
| extension registry     | Orion 自有或复用 Open VSX                                                            | —                                | 扩展元数据 | —        | —                          | —                               |

> 硬性要求（来自总计划 §3.2 / §6 / §8）：**不得把 `zed.dev`、`cloud.zed.dev`、`collab.zed.dev` 留作活动默认 endpoint**；未经批准不得把用户请求静默转发到 Zed hosted service。

---

## 5. 许可证 / 商标 / 上游归属（已批准边界）

| 项                                                          | 当前实际（以仓库文件为准）           | 处理原则                                                                    | 状态                     |
| ----------------------------------------------------------- | ------------------------------------ | --------------------------------------------------------------------------- | ------------------------ |
| 编辑器/多数 crate 许可                                      | GPL-3.0-or-later（`LICENSE-GPL`）    | 强 copyleft；开源项目/内部使用无影响，闭源分发需遵守                        | **已批准**（q-3 守 GPL） |
| GPUI 框架许可                                               | Apache-2.0（`LICENSE-APACHE`）       | 可单独用于闭源产品，不继承编辑器 GPL                                        | 沿用                     |
| `collab` 服务端许可                                         | GPL-3.0-or-later（以 manifest 为准） | 守 GPL-3.0 自托管；与研究文档称 AGPL 的矛盾以 manifest 为准，已按 GPL 决策  | **已批准**（q-3）        |
| 商标（`Zed` / Logo / `dev.zed.Zed*` / `ZedIndustries.Zed`） | Zed Industries 所有                  | 产品面向用户品牌必须改 Orion；仅在法务要求 attribution 中保留"基于 Zed"说明 | **已批准去商标**（q-3）  |
| 上游版权/贡献/第三方依赖                                    | 保留（KEEP-ATTRIBUTION）             | 版权头、`CONTRIBUTORS`、`Cargo.lock` 第三方声明不得删除                     | 沿用                     |

> 建议仍由法务对 collab 实际 SPDX 与商标 attribution 文案做最终确认，但该确认**不作为首轮品牌迁移（S03–S10）的阻塞项**。

---

## 6. 区分三类值（合同要求）

- **规范值（Orion）**：上表"最终值"列，新代码只依赖此类。
- **兼容值（旧 Zed）**：仅存在于兼容读取层/迁移代码/测试中，限期移除。
- **法律/上游保留值**：版权、商标 attribution、第三方许可声明，长期保留，不视为品牌残留。

---

## 7. 后续子计划可读到哪一步

本合同 **DONE**。S03（运行时身份与路径）可立即启动。依赖链：

```
01 -> 02 -> 03 -> 04 -> 05 -> 06 -> 07 -> 08 -> 09 -> 10 -> 11
```

每阶段保留 checkpoint；若发现 package/API 依赖无法按计划拆分，先暂停新增更小子计划，不得合并成一次全仓替换。

---

## 8. 人类审批区（已填写）

```
产品范围批准：桌面编辑器优先（Web/协作订阅为后续阶段）  —— q-0 推荐值
身份矩阵批准：§1 各"最终值"列（按建议批准值填充：Orion Studio / orion-studio / orion-studio binary / ORION_STUDIO_* / dev.orion.OrionStudio* / OrionStudio-* / com.orion.OrionStudio / orion:// / orion.dev 系）  —— q-1
服务端点批准：§4（orion.dev 系自有域名；collab 自托管、桌面优先后续接入；telemetry 默认关闭）  —— q-1 + q-3
许可证/商标批准：§5（守 GPL-3.0 含 collab；移除 Zed 商标；KEEP-ATTRIBUTION）  —— q-3
兼容窗口批准：§1/§3（zed:// 与 zed:: 双版本识别过渡；旧 CLI/env 限期别名 + 弃用诊断；具体窗口由 S04/S06 冻结）  —— q-2
批准人：Hope（用户）
日期：2026-08-09
决策记录链接：AskUserQuestion q-0~q-3 全选推荐值
```

**状态：DONE。S03–S10 已解锁。**
