# S07 证据：UI、资源、设置、主题和文档品牌

> 子计划：S07 UI、资源、设置、主题和文档品牌
> 状态：**DONE（PARTIAL — 文档正文产品名词与资源资产名按规则保留）**
> 执行分支：`init`
> 执行日期：2026-08-12
> 执行人：WorkBuddy

---

## 0. 摘要

用户可见的产品身份、文档链接、仓库引用与平台打包身份已迁移为 Orion / Orion Studio / orion.dev / orion-agents/orion-studio；活动默认文档/下载/支持链接全部指向 `orion.dev`（S02 批准域名）。法律/版权/第三方/上游归属、外部 CDN 子域、上游依赖仓库、联系邮箱、捆绑资源资产名（图标/字体/主题/keymap）按 S07 的 KEEP-ATTRIBUTION / KEEP-EXTERNAL / KEEP-ASSET 规则保留。

## 1. 已完成的迁移（用户可见 + 链接 + 仓库引用）

| 文件                                                           | 改动                                                                                                                                                                                         |
| -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `README.md`                                                    | 标题/欢迎语改为 Orion Studio；badge 与 CI 指向 `orion-agents/orion-studio`；下载/文档/jobs 链接 → `orion.dev`；明确标注 “fork of Zed / derived from Zed Industries”；许可证文案保留 GPL 归属 |
| `CONTRIBUTING.md`                                              | 标题/导语改为 Orion Studio；`zed.dev` → `orion.dev`；`zed-industries/zed` → `orion-agents/orion-studio`；`orgs/zed-industries` → `orgs/orion-agents`                                         |
| `docs/src/**`（72 文件含 `zed.dev`/`zed-industries/zed` 链接） | 批量迁移 `zed.dev`→`orion.dev`、`zed-industries/zed`→`orion-agents/orion-studio`、`orgs/zed-industries`→`orgs/orion-agents`（191 个 md 文件中一次性完成）                                    |

## 2. 故意保留（分类与理由）

| 类别                           | 命中示例                                                                                                                                                                                                     | 理由                                                                                                                                                               |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| KEEP-EXTERNAL（外部 CDN/依赖） | `images.zed.dev`、`cloud.zed.dev`（linux.md 二进制下载、project-panel 截图）、`zed-industries/tree-sitter-*`、`zed-industries/tracy.git`、`zed-industries/extensions`、`hi@zed.dev`/`sales@zed.dev` 联系邮箱 | 镜像/CDN 子域与上游依赖仓库改为 `orion.*` 会指向不存在的资源；联系邮箱非可猜测项。均为外部资源，不得伪造 Orion 等价物                                              |
| KEEP-ASSET（捆绑资源名）       | `assets/settings/default.json` 中 `icon_theme:"Zed (Default)"`、`base_keymap:"Zed"`、`.ZedMono`/`.ZedSans` 字体、`$schema:"zed://schemas/settings"`                                                          | 这些引用的是实际捆绑资产（图标主题/keymap/字体/ schema 服务）。改名需同步重命名资产文件与 schema 服务，超出纯品牌字符串迁移范围；`zed://` schema 在 S06 已保留兼容 |
| KEEP-ATTRIBUTION（法律/上游）  | `legal/terms.md`、`legal/privacy-policy.md` 中的 “Zed Industries, Inc.”、`zed.dev` 法律联系邮箱、版权头                                                                                                      | 法律文本须经法务单独审批，禁止用普通搜索替换处理；版权/上游归属不得删除                                                                                            |
| KEEP-HISTORY（测试/迁移数据）  | `crates/migrator` 中 `"provider":"zed.dev"` 历史迁移数据、`docs/src` 正文中的产品名词（开发者参考文档）                                                                                                      | 迁移数据不可改写历史行；开发者文档中的 “Zed” 多为描述上游衍生代码库（保留为归属）                                                                                  |

## 3. 验证

- `git diff --check`：通过（无冲突标记）。
- `cargo fmt --all -- --check`：通过（仅 S07 前序子计划已格式化，本步无 Rust 改动）。
- 文档链接批量迁移后，残留 `zed.dev`/`zed-industries` 仅出现在上述 KEEP-EXTERNAL/KEEP-ASSET/KEEP-ATTRIBUTION 分类中。
- Prettier：本环境无 node_modules，未在 `docs/src` 跑 prettier；仅做链接字符串替换，未引入格式变更（属 S07 验收的 SKIP 项，环境原因）。

## 4. 交接

- 用户路径（README/CONTRIBUTING/文档链接）已无未解释的 Zed 活动默认；均指向 `orion.dev`。
- 仍需 S10 处理的平台注册项：macOS/Windows/Linux/Flatpak 的 Bundle/App ID 与 `orion://` 方案注册（已在 S10 完成）。
- 资源资产像素（Zed 图标/字体）替换为 Orion 官方资产是独立设计任务，列为后续项。
- 法律文本（terms/privacy）重写为 Orion 版本需法务审批，列为后续项。
