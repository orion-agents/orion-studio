# S07：UI、资源、设置、主题和文档品牌

## 任务目标

把用户看到的产品名称、菜单、设置、主题、schema、文档和示例迁移为 Orion，
同时保留法律、版权、第三方主题作者和上游 attribution。本子计划不改 package、
服务端点、平台注册和协议解析。

## 前置条件

- S06 DONE。
- S02 已批准展示名称、官网、schema URL、provider 文案和 attribution 策略。
- S01 已把命中分类为用户可见或法律/上游保留。
- 如果需要修改 UI 行为代码而不只是身份文本，停止并另开子计划。

## 允许修改

- README.md
- CONTRIBUTING.md
- docs/src/\*\*
- assets/\*\*
- S01 明确列出的用户可见 Rust/UI 文件中的字符串、标题或链接。
- 这些文件对应的 snapshot、JSON fixture 和文档测试。

禁止修改：

- docs/research/\*\*
- docs/plan/\*\*
- crates/client/**、crates/collab/**、crates/cli/\*\*
- crates/zed/resources/\*\*、安装器和 workflow。
- 任何业务逻辑、布局、性能或交互重构。

## 执行步骤

### 1. 先建立用户路径清单

从启动到退出逐项搜索：

- 应用标题、窗口标题、菜单、命令面板、关于页和欢迎页。
- 登录、更新、错误、诊断、空状态和通知。
- settings schema、默认 provider、keymap action、theme author。
- README、贡献指南、docs/src、下载和支持链接。
- extension 文档和 AI/远程/协作说明。

使用：

```text
rg -n -i 'Zed|zed\.dev|zed://|ZED_|zed-industries' \
  README.md CONTRIBUTING.md docs/src assets
```

把结果分成 REPLACE、COMPAT、KEEP-ATTRIBUTION、KEEP-EXTERNAL 或 OPEN。
不要因为匹配包含 Zed 就全部替换。

### 2. 迁移展示文本和链接

- 用户看到的主品牌统一使用 S02 的最终展示名。
- 官网、文档、下载、支持、隐私和账户链接使用批准的 Orion endpoint。
- 旧链接只有在兼容页面、迁移说明或上游归属中保留。
- 帮助文本必须告诉用户如何处理旧配置/旧协议，不能只改名不解释升级。
- 旧 keymap/action 如果是用户配置格式的一部分，保持兼容解析；只改显示名或增加
  规范 alias，不擅自改快捷键语义。

### 3. 迁移 settings、themes、schemas 和 badge

- schema URL 使用批准值，且 JSON 仍然有效。
- 默认 provider、登录、更新和 AI 文案不指向未批准的 Zed 服务。
- 主题作者、许可证、版权、来源链接按 attribution 策略保留。
- SVG、JSON、TOML/XML/YAML 修改后分别做语法验证。
- 不修改第三方主题的视觉设计或许可证字段，除非有法务批准。

### 4. 迁移文档

docs/src 变更遵守 docs/AGENTS.md：

- 不把计划文件加入 mdBook SUMMARY，除非用户另行要求。
- 保持标题层级、相对链接、action/keybinding 预处理语法和代码块有效。
- 不做整个 docs/src 的无关格式化。
- 仅对改动文件运行 Prettier，发现已有大范围格式差异时停止并报告。
- 文档明确区分当前已实现能力和未来 Orion 服务规划。

根 README/CONTRIBUTING 不得写成已提供尚未验证的 cloud、billing 或 Web IDE 能力。

### 5. 更新视觉和文案测试

只更新因品牌变化而必然变化的 snapshot/fixture：

- 启动窗口、菜单、关于和错误文案。
- settings/keymap/theme schema。
- 文档链接和 CLI 示例。
- 不调整布局、颜色、快捷键或交互来“顺便优化”。

## 验收命令

```text
git diff --check
cargo +stable fmt --all -- --check
./script/check-keymaps
```

对 JSON/YAML/TOML/XML 使用仓库已有校验方式；文档使用：

```text
cd docs && npx prettier --check src/<实际修改的文件>
```

如果修改了可运行 UI，运行对应的 GPUI/visual test；没有可用图形环境时标记
SKIP 并记录环境，不得伪造截图。

最终扫描：

```text
rg -n -i 'Zed|zed\.dev|zed://|ZED_|zed-industries' \
  README.md CONTRIBUTING.md docs/src assets
```

每一个剩余命中都必须在交接中列出理由。

## 验收标准

- 普通用户路径不再显示未解释的 Zed。
- 文档和设置不再把用户默认导向未经批准的 Zed 服务。
- 法律/版权/第三方/上游归属没有被误删。
- schema、keymap、theme 和文档链接仍可解析。
- 没有改变 UI 行为、布局或功能范围。
- Prettier、keymap、语法和可用的 visual checks 有真实结果。

## BLOCKED 条件

- 展示名称、文档域名或 schema URL 未批准。
- 命中内容属于法律文本、第三方许可证或上游 attribution，但没有决定。
- 需要修改 UI 逻辑、布局、交互或运行时配置加载。
- Prettier 会产生大范围无关 diff。
- 需要访问外部官网、账户或 registry 才能验证。

## 回滚

只回滚当前子计划的文案、资源和测试 diff。不要删除 docs/research 或用户生成的
settings。回滚后重新运行 JSON/keymap/Prettier 检查。

## 交接

返回：

- 用户路径中已迁移的区域。
- 保留的法律/上游/兼容命中及理由。
- 文档、资源和视觉检查结果。
- 仍需 S08/S10 处理的 endpoint、资源和平台注册项。
