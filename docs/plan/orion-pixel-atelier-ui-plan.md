# Orion Studio 精致像素 UI 改造计划

## 0. 计划信息

| 项目 | 内容 |
| --- | --- |
| 计划名称 | Orion Pixel Atelier |
| 状态 | In Progress，按第 14 节推荐默认决策实施 |
| 编制日期 | 2026-08-28 |
| 实施基线 | 从执行时最新的 fork `main` 创建独立 UI 分支 |
| fork main 基线 | `origin/main@d593fdd3534b4541ba62cf38d23d3bccb56b39a8` |
| 已同步 upstream | `upstream/main@01acd0ee8e906dd0ec8b526fe08da94444a5e2af` |
| 当前实现分支 | `ui/orion-pixel-atelier@e96fef3` |
| 产品范围 | 主题、视觉令牌、共享 GPUI 组件、图标、产品壳层和视觉验证 |
| 核心原则 | 只换视觉皮肤，不改变编辑器、Workspace、Project、Agent、终端和协议行为 |
| 本文性质 | 可执行计划，不代表任何 UI 已实现、编译或发布 |

本计划把 Orion Studio 改造成一套精致像素工作台。现有窗口、Pane、Dock、
编辑器、Project Panel、Terminal Panel 和 Agent Panel 的功能结构保持不变。
第一版采用“像素启发式”视觉，不做模拟器式全屏像素化。

执行模型必须一次只完成一个任务编号，写入对应证据文件后停止。不得跨任务连续
修改，不得因为“顺手统一”扩大范围。

## 1. 最终产品目标

最终界面应同时满足以下结果：

1. Orion Studio 形成可辨识的独立视觉语言，不再只是修改名称的 Zed 外观。
2. 保留现代代码编辑器的信息密度、键盘效率、面板布局和代码可读性。
3. 提供同一主题家族的深色和浅色变体：
   - Orion Pixel Atelier Night。
   - Orion Pixel Atelier Dawn。
4. 按钮、Tab、输入框、列表、浮层、卡片和状态条使用一致的像素几何规范。
5. 文件图标、产品品牌图标和高频操作图标在现有尺寸下清晰，不改变动作 ID。
6. 中文、英文、长路径、Git 状态、诊断、终端 ANSI 色和 Agent 工具卡均可读。
7. hover、active、selected、focused、disabled、error 等状态完整，不以颜色作为
   唯一反馈。
8. 用户已有主题、字体、快捷键、窗口布局和数据不需要迁移。
9. UI 改造不会引入 Project、Git、语言服务器、Agent 工具执行或终端行为变化。
10. fork 后续同步上游时，视觉改动集中在少量明确文件，不形成全仓长期冲突。

## 2. 已核验的架构事实

### 2.1 主题层已经具备的能力

`ThemeStyleContent` 已支持：

- 窗口、面板、Tab、编辑器、滚动条和终端等语义颜色。
- error、warning、success、conflict、modified 等状态色。
- syntax token 的前景、背景、字重和字体样式。
- players、accents 和窗口背景外观。
- 一个主题家族包含多个 dark/light 变体。

主题 JSON 不包含：

- 圆角和切角。
- 边框宽度。
- 阴影形态。
- 控件高度。
- 间距尺度。
- 图标几何。
- 动效参数。

因此纯主题 JSON 可以完成配色，不能独立完成整套像素控件。

### 2.2 图标层分为两套机制

文件图标主题支持：

- 文件名和扩展名映射。
- 文件夹和命名文件夹。
- 展开、折叠箭头。
- dark/light 图标主题。

工具栏、按钮、搜索、调试和 Agent 操作图标由 `IconName` 固定映射到
`assets/icons/*.svg`。这些图标可以更换视觉资源，但必须保留枚举、文件名和调用
语义。

### 2.3 UI 与核心的依赖方向

- `theme` 依赖 GPUI、syntax theme 和颜色基础能力。
- `ui` 依赖 GPUI、theme 和 icons，不依赖 Project、Workspace 或 Agent UI。
- `workspace` 向上组合 Project、theme、UI、Pane 和 Dock。
- `project` 管理 worktree、buffer、Git、语言服务器和 DAP，不应感知像素 UI。

这意味着 UI 可以消费核心状态，但核心不得反向依赖某个视觉主题。

### 2.4 当前几何样式的风险

只读扫描在 Rust 文件中发现约 120 个文件、269 处圆角调用。圆角、阴影和局部卡片
样式尚未完全令牌化，Agent UI 又存在较多直接拼装的 `div`。

因此禁止：

- 全仓机械替换 `rounded_*`。
- 按主题名称在业务 crate 中增加条件分支。
- 一次 PR 同时改所有面板。
- 为了视觉一致性重写已有组件状态和事件处理。

### 2.5 源码锚点

下表是 2026-08-28 审计 checkout 上的定位证据。正式开发必须基于执行时最新
`origin/main`，先按符号名重新定位；行号只能帮助阅读，不能被 WorkBuddy 当成固定
编辑区间。

| 结论 | 当前源码锚点 |
| --- | --- |
| 主题内容只有颜色、状态、players 和 syntax 等字段，没有几何字段 | `crates/settings_content/src/theme.rs:537` |
| 主题颜色会经过 refine/默认值补全 | `crates/theme/src/styles/colors.rs:608` |
| 一个主题家族可以包含多个 appearance 变体 | `crates/theme_settings/src/schema.rs:25` |
| 内置主题由完整应用通过 Assets 加载 | `crates/zed/src/main.rs:740` |
| 文件图标主题只负责文件、目录和 chevron 映射 | `crates/theme/src/icon_theme_schema.rs:10` |
| 产品操作图标由 `IconName` 固定映射到 `assets/icons/*.svg` | `crates/icons/src/icons.rs:312` |
| `ui` 处于基础组件层，不依赖 Project/Workspace/Agent | `crates/ui/Cargo.toml:15` |
| `workspace` 位于更上层，组合 Project、Pane、Dock 和 UI | `crates/workspace/Cargo.toml:28` |
| Button 的共享视觉入口集中在 `ButtonLike::render` | `crates/ui/src/components/button/button_like.rs:745` |
| InputField 外壳集中，但内部仍复用 Editor | `crates/ui_input/src/input_field.rs:146` |
| 浮层 elevation helper 位于共享 UI trait | `crates/ui/src/traits/styled_ext.rs:6` |
| Component Preview 依靠组件注册表枚举预览 | `crates/component/src/component.rs:160` |
| 完整应用会注册 Component Preview 命令 | `crates/zed/src/main.rs:1054` |
| 独立 Component Preview 当前只加载 base theme | `crates/component_preview/examples/component_preview.rs:50` |
| PNG visual runner 使用真实 GPUI/Metal 纹理捕获 | `crates/zed/src/visual_test_runner.rs:19` |
| PNG visual runner 当前也只加载 base theme | `crates/zed/src/visual_test_runner.rs:177` |

由这些事实得到三个实施结论：

1. Night/Dawn 的颜色可作为主题变体实现，但控件几何不能塞进主题 JSON。
2. Orion Pixel Chrome 是 Orion Studio 产品级全局壳层；它会作用于 One、Ayu、
   Gruvbox 等其他颜色主题。若未来需要可切换几何风格，必须另立 schema/settings
   计划。
3. 现有视觉测试工具未直接覆盖 Night/Dawn，因此必须先完成 UI-00V，才能把 PNG
   比对用于本次主题验证。

## 3. 范围和硬边界

### 3.1 绿色区域：主要改造位置

- `assets/themes/**`
- `assets/icons/**`
- `assets/fonts/**`，仅在字体许可证和中文 fallback 获批后
- `crates/theme/**`，仅限主题读取和已有视觉能力；第一版不扩 schema
- `crates/theme_settings/**`
- `crates/ui/**`
- `crates/ui_input/**`
- `crates/icons/**`
- `crates/file_icons/**`
- 对应的组件预览、聚焦测试和视觉测试

### 3.2 黄色区域：只能修改 Render 外壳

- `crates/title_bar/src/**`
- `crates/workspace/src/status_bar.rs`
- `crates/project_panel/src/project_panel.rs`
- `crates/terminal_view/src/terminal_panel.rs`
- `crates/terminal_view/src/terminal_view.rs`
- `crates/agent_ui/src/agent_panel.rs`
- `crates/agent_ui/src/conversation_view/**`
- `crates/agent_ui/src/ui/**`
- `crates/git_ui/**`
- `crates/settings_ui/**`
- picker、command palette 和 notification 的纯视觉 Render 文件

黄色区域只允许：

- 使用共享视觉令牌。
- 更换背景、边框、圆角、阴影和间距。
- 更换保持语义不变的图标。
- 调整不影响命中区域和行为的视觉排列。

黄色区域禁止：

- 改 Entity 状态。
- 改事件回调和 action dispatch。
- 改拖放、resize、焦点和键盘导航。
- 改数据读取、异步任务和错误传播。
- 改元素层级，除非有现有测试证明行为完全不变。

### 3.3 红色区域：第一版禁止修改

- `crates/gpui/**`
- `crates/gpui_macos/**`
- `crates/gpui_wgpu/**`
- Metal、WGPU、shader、scene 和 frame pipeline
- `crates/project/**`
- `crates/git/**`
- `crates/lsp/**`
- `crates/language/**`
- buffer、multi-buffer、worktree、DAP、task、remote 和 session
- Workspace 的 Pane/Dock 状态、布局持久化、拖放、焦点和 resize
- Editor 的输入、selection、display map、滚动和语言服务器行为
- Terminal 的字符网格、光标、选区和绘制管线
- Agent thread、tool execution、connection、terminal 和 project 状态
- action ID、快捷键、URI、IPC、配置迁移和扩展协议
- 发布签名、notarization、更新器和生产服务

### 3.4 明确非目标

第一版不包含：

- 全屏 CRT 扫描线、色差、屏幕弯曲、噪点或 vignette。
- 真正的等距空间布局和游戏式 Pane 交互。
- 自定义 GPUI 渲染后处理。
- 全局位图放大或降低编辑器文字清晰度。
- 强制所有用户使用像素字体。
- 新增运行时几何皮肤切换设置。
- 修改第三方 Ayu、Gruvbox、One 主题视觉或作者信息。
- 借 UI 改造继续做品牌、协议、服务端点或底层 crate 重命名。

## 4. Orion Pixel Atelier 设计契约

### 4.1 设计原则

1. 像素感来自网格、边框、图标和层级，不来自牺牲文字清晰度。
2. 编辑器正文和终端字符区域是内容层，不叠加装饰纹理。
3. 默认使用矩形布局，不改变现有 Pane 和 Dock 拓扑。
4. 控件状态必须在深色和浅色主题中均可辨识。
5. 焦点边框不能引发布局跳动。
6. 阴影以硬边偏移为主，避免大面积模糊。
7. 圆形只保留给 avatar、presence、radio、progress 等有语义的组件。
8. 所有高频尺寸对齐 4px 节奏；1px 边框允许作为网格的视觉细分。

### 4.2 几何令牌

第一版令牌为 Orion 产品固定视觉，不进入主题 JSON。

| 令牌 | 建议值 | 用途 |
| --- | ---: | --- |
| space-1 | 4px | 图标与文字最小间距 |
| space-2 | 8px | 控件内部水平/垂直间距 |
| space-3 | 12px | 卡片内部间距 |
| space-4 | 16px | 区块间距 |
| space-6 | 24px | 模态页和空状态大间距 |
| radius-none | 0px | 分隔区、Tab 拼接边 |
| radius-control | 2px | 按钮、输入框、Chip |
| radius-surface | 2px | 卡片、菜单、popover |
| radius-modal | 4px | 模态窗口和大型浮层 |
| border-default | 1px | 常规边框和分隔线 |
| focus-ring | 2px | 键盘焦点，不能改变布局尺寸 |
| shadow-elevated | 2px 2px 0 | 菜单、popover、通知 |
| shadow-modal | 4px 4px 0 | 模态窗口 |

约束：

- 不缩小现有点击区域。
- 不把正文行高强制对齐 4px。
- avatar 和 progress 可继续使用 `rounded_full`。
- 如果 GPUI 的现有控件尺寸与表格冲突，保留尺寸，只应用半径、边框和阴影。

### 4.3 Night 初始色板

以下值是实现前的起始提案。UI-01 通过后冻结，不允许执行模型自行改色。

| 语义 | 建议值 |
| --- | --- |
| app background | `#0B0F14` |
| surface | `#111821` |
| elevated surface | `#18212C` |
| editor background | `#0E141B` |
| border variant | `#263446` |
| border | `#3C516B` |
| text | `#D6E2F0` |
| text muted | `#8FA3B8` |
| accent | `#62D6C5` |
| secondary accent | `#8DA6FF` |
| success | `#7BD88F` |
| warning | `#F4C56A` |
| error | `#F27878` |

### 4.4 Dawn 初始色板

| 语义 | 建议值 |
| --- | --- |
| app background | `#F2EEE6` |
| surface | `#FFFDF8` |
| elevated surface | `#E9E3D8` |
| editor background | `#FAF7F0` |
| border variant | `#C8BDAE` |
| border | `#7D7061` |
| text | `#25221E` |
| text muted | `#746B61` |
| accent | `#237C72` |
| secondary accent | `#4F65B8` |
| success | `#337A48` |
| warning | `#9A6419` |
| error | `#B74343` |

完整主题不能只复制这些基础色。必须逐项设计 editor、terminal、Git、diagnostic、
search、selection、scrollbar、minimap、player 和 syntax 语义槽位。

### 4.5 交互状态

| 状态 | 视觉要求 |
| --- | --- |
| default | 使用当前 elevation 的 surface 和文本色 |
| hover | 背景变化，不移动控件，不显示新边框导致尺寸变化 |
| active/pressed | 比 hover 更强，并保留文字和图标对比度 |
| selected | 使用 accent 背景或 accent 边框，并保留非颜色标识 |
| focused | 2px 等效焦点环或高对比焦点边，不改变布局 |
| disabled | 降低强调度，但文字仍可辨认，cursor 和 ARIA 行为保持原样 |
| error | error border + 文本/图标，不仅依靠红色背景 |
| drag target | 保留现有 drop target 行为，仅换主题色和边框 |

### 4.6 字体策略

第一版默认：

- UI 字体继续使用现有高可读字体。
- Buffer 和 Terminal 字体不变。
- Agent 正文和用户输入字体不变。
- 像素字体只作为后续可选 display font，不进入第一版默认设置。

只有满足以下条件才允许加入 `assets/fonts/**`：

1. 字体允许随 GPL 应用再分发。
2. LICENSE/NOTICE 已加入仓库要求的位置。
3. 具备中文 fallback，不出现中文方框。
4. 10px、12px、14px、16px 下人工检查通过。
5. 不通过修改全局 `ui_font_size` 来制造像素效果。

### 4.7 图标策略

- 保留全部 `IconName`、文件名和 action 调用。
- 优先使用 SVG，不使用低分辨率 PNG 放大。
- 主图形对齐 16×16 网格和整数坐标。
- 避免依赖亚像素细线。
- 必须检查现有 10、12、14、16px 渲染尺寸。
- 第一版优先更新 Orion 品牌图标、文件树图标和高频壳层图标。
- 未更新的通用图标保持现状，不允许由执行模型临时重画成不一致风格。
- 第一版不新增第二套可选择的 icon theme，不修改 icon theme schema 或注册流程。
- 继续使用现有 `Orion Studio (Default)` icon theme 名称，只替换 UI-01 明确批准且
  已存在路径的 Orion 默认资产。
- 文件图标和产品操作图标必须分任务、分 PR；前者不得顺手改 `IconName`，后者不得
  顺手改文件图标映射。

### 4.8 动效策略

- 第一版不新增全局动画系统。
- hover、active 和 focus 沿用现有事件与动效机制。
- 新增的纯装饰动效必须检查 `cx.reduce_motion()`。
- reduced motion 打开时必须呈现稳定静态状态。
- 禁止闪烁、扫描线滚动和覆盖代码正文的粒子效果。

### 4.9 可访问性要求

- 正文文本目标 APCA Lc 不低于 75。
- 一般 UI 文本目标 APCA Lc 不低于 60。
- 大字号或辅助图形最低 APCA Lc 45。
- 不能仅用红/绿区分 Git、诊断或任务状态。
- 键盘焦点必须始终可见。
- hover 信息必须能通过键盘焦点或已有 tooltip 获得。
- 深浅主题都要验证 selected、disabled、placeholder 和 muted text。
- 不改变现有 ARIA role、label、toolbar 和 tab order。

## 5. 实现架构

### 5.1 令牌与主题分离

颜色仍放在主题 JSON 中。几何与阴影放在 `crates/ui/src/styles`。

建议新增一个逻辑集中点：

- `crates/ui/src/styles/chrome.rs`
- 在 `crates/ui/src/styles.rs` 中导出。

建议提供类型化能力，而不是散落常量：

- control radius。
- surface radius。
- modal radius。
- border width。
- focus ring。
- hard shadow。
- 常用 control/surface 装饰函数。

第一版不修改 `ThemeStyleContent`，不新增 settings schema，不支持运行时切换多套
几何皮肤。这可以减少扩展协议和上游同步面。

这也意味着几何改造是 Orion Studio 的全局产品 Chrome，不只在
`Orion Pixel Atelier Night/Dawn` 被选择时生效。执行模型不得为了只影响新主题而
比较 theme name。是否需要“Classic/Pixel”运行时几何选择器，留给后续独立版本。

### 5.2 共享组件优先

应用顺序：

1. elevation、surface 和 popover。
2. Button、IconButton、ToggleButton。
3. Tab 和 TabBar。
4. ListItem、TreeViewItem、Chip、Banner、Toggle。
5. InputField 和搜索输入外壳。
6. Modal、context menu、notification。
7. 产品面板只补充共享组件未覆盖的外壳。

### 5.3 禁止主题名称分支

以下模式禁止出现：

- 在 Project、Workspace、Agent 或 Terminal 中比较主题显示名。
- `if theme.name == "Orion Pixel Atelier Night"`。
- 使用字符串判断决定点击、拖放、焦点或布局行为。
- 复制一套 Pixel 专用业务组件。

如果未来需要多套几何风格，应另立计划设计类型化 `UiChromeStyle`，不得由本计划
顺手实现。

## 6. 交付模型、依赖与并行策略

### 6.1 开发基线

正式实施前必须重新验证，不能直接依赖本文记录的旧 SHA：

1. 获取 `origin/main` 和 `upstream/main` 的最新状态。
2. 如果 fork main 落后 upstream，先完成独立的 upstream 同步和冲突验证。
3. 以执行时最新且已确认的 fork `main` 为起点创建任务分支。
4. 禁止在当前 `release/orion-studio-v1.16.1-pre` checkout 直接开发 UI。
5. 每个任务保存开始 branch、HEAD、dirty、staged、untracked 和 remotes。
6. 行号只作为审计锚点；开发时按结构体、函数和 component 名重新定位。

建议分支格式：

```text
ui/pixel-ui-00-baseline
ui/pixel-ui-02n-night-theme
ui/pixel-ui-12-button
```

如果前置任务已经合并，每个任务从最新 `main` 分支开始。如果前置任务尚未合并，
只允许由协调者创建显式 stacked branch，并在 evidence 中记录 base PR；WorkBuddy 不得
自行猜测 base、rebase 或 cherry-pick。

### 6.2 一任务、一目标、一 PR

默认执行合同：

- 一个任务编号只解决一个视觉目标。
- 一个任务只产生一个 commit/PR 候选；未经授权不 push、不建 PR。
- 一个 PR 不得自动进入下一个任务。
- 非资产任务默认最多 3 个生产文件、250 行 Rust 增删总量。
- 每个任务可另有 1 个不超过 200 行的 evidence Markdown，不计入生产文件上限。
- SVG 任务每个 PR 最多 16 个 SVG、100KiB；超出按批次继续拆分。
- 超过任务预算时返回 `BLOCKED: NEED_SPLIT`，不得用“同属 UI”作为扩范围理由。
- 质量门禁 PR 原则上为 0 个生产文件。发现问题必须退回归属任务修复。
- WorkBuddy 不得自动合并、打 tag、发布、安装应用或删除分支。

所有 PR 标题遵守仓库规则：使用正确大写的命令式标题，不加 conventional commit
前缀，不加句号。PR body 最后一节必须严格为：

```text
Release Notes:

- Improved ...
```

纯文档、测试基础设施或无用户可见变化时使用 `- N/A`。

### 6.3 依赖顺序

```text
UI-00 ─┬─> UI-01 ─┬─> UI-02N ─> UI-02D
       │          ├─> UI-03F
       │          ├─> UI-03P
       │          └─> UI-10 ─> UI-11
       └─> UI-00V ───────────────┘

UI-10/UI-11 完成后：
  UI-12 Button       UI-13 Tab       UI-14 List ─> UI-15 Tree
  UI-16 Input        UI-17 Toggle    UI-18 Chip
  UI-19 Banner       UI-24 Popover   UI-25 Context Menu ─> UI-26 Modal

主题 + 共享组件完成后：
  UI-30 Project Panel ─> UI-31 Project Rename
  UI-32 Title Bar
  UI-33 Status Bar
  UI-34 Terminal Shell ─> UI-35 Terminal Rename
  UI-40 Agent Shell ─> UI-41 ─> UI-42 ─> UI-43 ─> UI-44 ─> UI-45

全部实现任务完成：
  UI-50 Quality Gate ─> UI-51 Default + Docs ─> UI-52 Final Gate
```

关键依赖解释：

- UI-00V 可以与 UI-01 并行，但 UI-02N/D 的 PNG 验证依赖 UI-00V。
- Theme、Icons、Chrome 三条资产/基础层主线可以并行。
- Button、Tab、List、Input 等修改不同文件时可以并行编辑。
- UI-15 依赖 UI-14，因为 Tree 必须继承统一的 list state 语义。
- Project/Title/Status/Terminal 先等待共享组件完成，避免面板内复制样式。
- UI-41 至 UI-45 都会修改 `thread_view.rs`，必须严格串行。
- UI-51 默认启用必须晚于 UI-50，且必须获得用户明确批准。

### 6.4 并行 Agent 规则

允许并行：

- Agent A 只读审计 Theme，Agent B 只读审计 Icons，Agent C 只读审计 Chrome。
- JSON、SVG、文档任务修改互不重叠的路径。
- Rust 任务修改完全不同的文件，且不共享同一个测试/生成输出。
- 并行 Agent 可以先完成源码定位、测试清单和 diff review。

禁止并行：

- 两个 Agent 同时修改 `thread_view.rs`、`project_panel.rs` 或同一主题 JSON。
- 两个 Cargo build/test/clippy 同时运行。
- 两个 visual runner 同时运行；它会创建临时项目并持续占用构建与图形资源。
- 多个独立 `CARGO_TARGET_DIR` 同时编译。
- 一个 Agent 在另一个 Agent 未提交/未交接的工作树上 rebase、reset 或 clean。

协调者必须为每个并行 Agent 指定：Task ID、允许路径、禁止路径、是否允许写文件、
能否运行 Cargo、交付格式和停止条件。

## 7. WorkBuddy 全局执行合同

### 7.1 每个任务开始前

先运行并把输出原样写入 evidence：

```text
pwd
git branch --show-current
git rev-parse HEAD
git remote -v
git status --short --branch
git diff --name-only
git diff --cached --name-only
git ls-files --others --exclude-standard
df -h .
du -sh target 2>/dev/null || true
```

开始条件：

- cwd 是 `/Users/hope/ai-project/orion-studio` 或本计划批准的独立 worktree。
- 当前任务分支来自已确认的最新 fork main 或显式 stacked base。
- 所有既有修改都有明确所有者，不覆盖用户/其他 Agent 的变更。
- 当前任务目标文件没有无法区分所有权的重叠修改。
- 当前任务列出的前置任务已是 `DONE` 或获得协调者明确豁免。
- 磁盘空间满足第 7.4 节门槛。

任一条件不满足时返回 `BLOCKED` 并停止。禁止使用 `git reset --hard`、
`git checkout --`、`git clean`、递归删除或覆盖文件来制造“干净”状态。

### 7.2 编辑合同

1. 先读任务涉及的生产文件、调用方、已有测试和 Component Preview，再编辑。
2. 只修改任务允许路径和允许符号；同文件中的事件/状态逻辑仍然是禁区。
3. 优先复用现有 GPUI、theme、ui 和 icons 能力，不创建平行业务组件。
4. 只改变 Render 装饰、视觉资产和验证基础设施，不移动状态所有权。
5. 禁止全仓替换 `rounded_*`、`border_*`、颜色、图标或字体。
6. 禁止 `if theme.name == "Orion Pixel..."` 及任何主题显示名业务分支。
7. 禁止扩展主题 schema 来承载圆角、阴影、密度或控件高度。
8. 禁止修改测试断言来掩盖行为变化；行为测试失败必须按回归处理。
9. 禁止新增 `unwrap()`、`expect()`、危险索引或静默丢弃 fallible 结果。
10. 一旦需要改异步任务、Entity 状态、action、focus、drag/drop 或协议，立即停止。
11. 不删除第三方作者、许可证、NOTICE 或上游 attribution。
12. 不修改 `.rules`；非显然且反复出现的规则只放入 PR 的
    `Suggested .rules additions` 候选段。
13. `Cargo.lock` 默认禁止修改；如果命令意外修改，报告并等待处理，不得自行丢弃。
14. 如果 diff 出现未授权路径，返回 `BLOCKED: OUT_OF_SCOPE_DIFF`。

### 7.3 全局禁止路径和行为

- `crates/gpui/**`、`crates/gpui_macos/**`、`crates/gpui_wgpu/**`。
- `crates/project/**`、`crates/editor/**`、`crates/git/**`、`crates/lsp/**`、
  `crates/language/**`。
- buffer、multi-buffer、worktree、DAP、task、remote、session。
- Workspace Pane/Dock 状态、布局持久化、拖放、焦点和 resize。
- Agent thread、tool execution、connection、permission、terminal 和 project 状态。
- Terminal 字符网格、PTY、光标、选区和绘制管线。
- action ID、快捷键、URI、IPC、配置迁移、扩展协议和数据库。
- updater、签名、公证、公开发布和生产服务。

黄色文件不是整文件授权。`project_panel.rs`、`agent_panel.rs`、
`thread_view.rs`、`terminal_view.rs` 内只有任务点名的 Render 样式表达式可编辑。

### 7.4 CPU、内存和磁盘约束

本机为 16GB Apple Silicon：

- JSON/SVG/文档检查可以并行；Cargo 和 visual runner 必须串行。
- 所有重型命令使用 `CARGO_BUILD_JOBS=2`。
- 日常迭代保留 incremental，不设置 `CARGO_INCREMENTAL=0`，避免每次全量重编译。
- 只有 UI-52 最终完整构建使用 `CARGO_INCREMENTAL=0`。
- 聚焦 crate check/test 前至少保留 30GiB 可用磁盘。
- 完整 app build/clippy 前至少保留 80GiB 可用磁盘。
- 单任务 `target` 增长超过 30GiB 时停止，记录前后大小并报告。
- 共享现有 `target`，不得为并行 Agent 建多个 target 目录。
- 不自动执行 `cargo clean`；清理必须先报告精确目录、大小、影响和恢复成本。
- visual runner 会保留临时 project 供 OS 后续清理，不能据此宣称“无临时文件”。
- 任务结束记录 `df -h .` 和 `du -sh target`，不得只记录编译成功。

### 7.5 状态词

每个任务只能返回一个主状态：

- `DONE`：范围内实现、必跑验证和 evidence 全部完成。
- `PARTIAL`：已实现一部分，但有明确未完成或未验证项。
- `BLOCKED`：需要用户决定、范围拆分、前置合并或外部状态变化。
- `NOT VERIFIED`：实现可能存在，但没有足够验证；不能写成通过。

质量门禁另使用：

- `GO`：代码、视觉、行为、资源和文档门禁全部通过。
- `GO-WITH-CONDITIONS`：只剩签名、公证或外部发布条件。
- `NO-GO`：存在行为回归、视觉阻断、性能/资源异常或验证缺失。

## 8. 详细子任务

下面每个任务都继承第 7 节合同。任务未列出的文件和行为一律不授权。

### UI-00：执行基线冻结

单一目标：建立最新 main 的开发起点和未改造证据，不修改生产代码。

- 前置：用户确认本轮正式实现从 fork main 开始。
- 允许路径：`docs/plan/evidence/PIXEL-UI-00-BASELINE.md` 和用户批准的截图目录。
- 禁止：源码、主题、图标、默认设置；不得修改用户已有 UI 工作。
- 执行：记录 branch/HEAD/remotes/dirty、默认 theme/icon theme/font、当前 app
  版本；保存 Editor、Project、Terminal、Agent、Command Palette、Settings、Popover、
  Context Menu、Modal 的明暗截图；记录 UI font 14/16/18；记录 `target` 和磁盘大小。
- 验证：应用可见窗口能启动、打开项目、关闭后再次启动；截图索引和 SHA 对应。
- 验收：证据可复现，生产 diff 为 0。
- 停止：main 不是最新、存在归属不明改动、应用不能稳定启动。
- 预算：2 个 Markdown、最多 12 张截图，总二进制不超过 8MiB。

### UI-01：设计契约冻结

单一目标：冻结 WorkBuddy 不得自行改变的颜色、几何、状态和图标清单。

- 前置：UI-00 `DONE`。
- 允许路径：`docs/plan/evidence/PIXEL-UI-01-DESIGN-CONTRACT.md` 和批准的效果图。
- 禁止：全部生产源码、运行时设置和资产替换。
- 执行：补齐 Night/Dawn 全部语义色；绘制 Button、Tab、List、Tree、Input、
  Toggle、Chip、Banner、Popover、Menu、Modal、Agent Card 的状态矩阵；生成 Editor、
  Agent、Command Palette 三张架构兼容效果图；列出 UI-03F/UI-03P 的精确文件 allowlist。
- 验证：颜色/几何/状态无 TBD；效果图不改变 Pane/Dock 拓扑；字体和图标来源明确。
- 验收：用户将 evidence 状态明确改为 `APPROVED`。
- 停止：要求 CRT、GPUI 后处理、空间布局、新交互、未知许可证或强制像素字体。
- 预算：3 个文档、3 张效果图，总二进制不超过 10MiB。

### UI-00V：Pixel UI 验证基础设施

单一目标：让现有 PNG visual runner 能明确加载目标主题并只运行指定场景；不改变
生产 UI 行为。

- 前置：UI-00 `DONE`；可与 UI-01 并行。
- 允许路径：`crates/zed/src/visual_test_runner.rs`；theme/theme_settings 中现有测试
  模块；`docs/plan/evidence/PIXEL-UI-00V-VISUAL-INFRA.md`。
- 禁止：UI 生产组件、主题 schema、`.github/workflows/**`、`.gitignore`、新的截图框架。
- 执行：
  1. 把 runner 的 theme 初始化从 `LoadThemes::JustBase` 调整为加载 bundled themes。
  2. 新增 `VISUAL_TEST_THEME`：值为精确主题显示名；不存在或 appearance 不符时明确失败，
     不得静默回退 One。
  3. 新增 `VISUAL_TEST_FILTER`：按测试名过滤；无值时保持运行全部现有场景。
  4. 主题名必须进入 baseline/output namespace，避免 Night/Dawn 覆盖同名 PNG。
  5. 保留 `UPDATE_BASELINE` 的现有语义：变量只要存在即进入更新模式。
  6. 增加 fixture 级主题 parse/refine/register/name-uniqueness 测试。
  7. 在 runner 顶部 Usage 中记录新变量和“比较时必须完全不设置 UPDATE_BASELINE”。
- 验证：无主题参数时旧行为兼容；指定已有主题可捕获；指定不存在主题退出非零；filter
  只执行命中场景；主题命名空间不会碰撞。
- 验收：runner 可用于后续 Night/Dawn 本地 PNG 证据。
- 证据边界：`crates/zed/test_fixtures/visual_tests/` 当前被 gitignore，CI 只编译 runner，
  不运行 PNG 比较；因此本任务不声称建立 CI visual gate。
- 停止：需要改 GPUI、settings schema、CI 存储或引入新 snapshot 依赖。
- 预算：最多 3 个生产/测试文件、250 行 Rust diff。

### UI-02N：Night 主题

单一目标：实现 `Orion Pixel Atelier Night`，保持非默认。

- 前置：UI-01 `APPROVED`、UI-00V `DONE`。
- 允许路径：`assets/themes/orion-pixel-atelier/orion-pixel-atelier.json`、原创主题许可
  说明、`docs/plan/evidence/PIXEL-UI-02N-NIGHT.md`。
- 禁止：现有 One/Ayu/Gruvbox、`assets/settings/default.json`、Rust/schema 文件。
- 执行：完整定义 UI、Editor、Terminal、Git、Diagnostics、Search、Selection、Scroll、
  Players、Accents 和 Syntax；author 使用批准值；不复制第三方主题。
- 验证：JSON parse、主题反序列化/注册/name uniqueness、selector 手动切换、完整应用
  Component Preview、Night PNG runner、Terminal ANSI 和 Git/diagnostic 实景。
- 验收：名称唯一，无非法色值、panic、fallback 或缺失语义状态；切换不改变布局/数据。
- 停止：色值或 author 未批准、需要新增 schema 字段、主题显示名冲突。
- 预算：最多 2 个资产文件，主题 JSON 不超过约 1,100 行或 75KiB。

### UI-02D：Dawn 主题

单一目标：在同一家族增加 `Orion Pixel Atelier Dawn`，不重设计已验收 Night。

- 前置：UI-02N `DONE`。
- 允许路径：同一主题 JSON 和 `PIXEL-UI-02D-DAWN.md`。
- 禁止：已冻结 Night 值、default settings、Rust/schema。
- 执行：补齐 Dawn 全部语义槽位，重点检查 muted、placeholder、selection、disabled、
  terminal bright/dim、Git 和 diagnostic。
- 验证：与 UI-02N 相同，主题参数改为 Dawn；额外验证系统 light/dark 切换不串色。
- 验收：浅色状态可辨、没有用 Night 值临时兜底、不会修改 Night 基线。
- 停止：必须重做 Night、需要几何/schema 变化或浅色对比无法达到设计契约。
- 预算：1 个资产文件，新增不超过约 900 行或 60KiB。

### UI-03F：默认文件图标试点

单一目标：在现有 `Orion Studio (Default)` icon theme 中验证文件树像素图标。

- 前置：UI-01 `APPROVED`。
- 允许路径：UI-01 明确批准的 `assets/icons/file_icons/*.svg`；
  `PIXEL-UI-03F-FILE-ICONS.md`。
- 首批建议：`folder.svg`、`folder_open.svg`、`file.svg`、四向 chevron；最终以 allowlist
  为准。
- 禁止：icon theme schema/registry、映射 JSON、默认 icon theme 名称、扩展打包流程。
- 执行：保留文件名/viewBox/语义；整数网格；检查展开、折叠、selected、muted 和 disabled。
- 验证：`xmllint`、icons/file_icons 资产测试、Project Panel 10/12/14/16px 实景。
- 验收：无缺失/空白/裁切；未覆盖类型继续使用现有默认资产。
- 停止：需要新注册流程、重命名路径、修改映射或超出批准 SVG。
- 预算：最多 8 个 SVG、80KiB。

### UI-03P：产品操作图标试点

单一目标：验证高频 Orion 产品操作图标的像素造型。

- 前置：UI-01 `APPROVED`；可与 UI-03F 并行。
- 允许路径：UI-01 明确批准的 `assets/icons/*.svg`；`PIXEL-UI-03P-PRODUCT-ICONS.md`。
- 建议候选：Orion/Agent、search、settings、terminal、git、close、success、warning、error。
- 禁止：`crates/icons/src/icons.rs`、`IconName`、action、tooltip 和调用方。
- 执行：保留现有路径和语义；整数坐标；比较未更新图标的风格兼容性。
- 验证：`xmllint`、`cargo test -p icons`、Night/Dawn、10/12/14/16px、muted/disabled/accent。
- 验收：图标存在性测试通过，操作含义不变，无点击/布局变化。
- 停止：需要新增/重命名 IconName、修改 action 或未批准文件进入 diff。
- 预算：最多 16 个 SVG、100KiB；扩大范围必须另开批次。

### UI-10：几何 Token

单一目标：建立固定 Orion Pixel Chrome 几何词汇，不接入组件。

- 前置：UI-01 `APPROVED`。
- 允许路径：新 `crates/ui/src/styles/chrome.rs`、`crates/ui/src/styles.rs`、
  `PIXEL-UI-10-CHROME-TOKENS.md`。
- 禁止：ThemeStyleContent、settings schema、GPUI、业务组件、theme name 判断。
- 执行：类型化定义 control/surface/modal radius、border、focus ring 和 hard shadow；复用
  现有 px/BoxShadow 能力；不复制 DynamicSpacing。
- 验证：format、ui 聚焦 check/test、Night/Dawn 编译级使用示例或单元测试。
- 验收：API 名称表达语义，调用方不需要知道主题名，无 renderer/schema 变化。
- 停止：必须改 GPUI API、需要 runtime geometry selector 或改变 UI density。
- 预算：2 个生产文件、220 行 Rust diff。

### UI-11：Elevation 和共享 Surface

单一目标：让共享 surface/elevation 使用统一像素边界和硬阴影。

- 前置：UI-10 `DONE`。
- 允许路径：`crates/ui/src/styles/elevation.rs`、
  `crates/ui/src/traits/styled_ext.rs`、`PIXEL-UI-11-SURFACES.md`。
- 禁止：Popover/Modal 调用方、GPUI、UI-10 token 值、focus/dismiss 行为。
- 执行：让现有 elevation helper 消费 token；保留 `ElevationIndex` 语义和透明层处理。
- 验证：surface/elevated/modal 在完整应用 Component Preview 的 Night/Dawn 中检查。
- 验收：边框和阴影不引发布局跳动，现有 elevation 层级不变。
- 停止：需要改变 element hierarchy、dismiss、focus restore 或 window renderer。
- 预算：2 个生产文件、180 行 Rust diff。

### UI-12：Button

单一目标：改造 `ButtonLike`，让 Button/IconButton 继承像素 Chrome。

- 前置：UI-10 `DONE`。
- 允许路径：`crates/ui/src/components/button/button_like.rs` 和同文件 preview/test；
  `PIXEL-UI-12-BUTTON.md`。
- 禁止：Button 调用方、action、role、ARIA、tab index、tooltip、event handler 和公开 API。
- 执行：覆盖 Filled/Tinted/Outlined/Subtle、尺寸、selected/disabled/focus/active；保持命中区。
- 验证：完整应用 Component Preview；mouse、keyboard、focus、disabled 行为测试。
- 验收：所有 ButtonStyle 在 Night/Dawn 可辨且无布局跳动。
- 停止：需要改公开 API 或任一事件/可访问性表达式。
- 预算：1 个生产文件、160 行 Rust diff。

### UI-13：Tab

单一目标：改造共享 Tab 的 active/inactive/hover/focus 边界。

- 前置：UI-10 `DONE`。
- 允许路径：`crates/ui/src/components/tab.rs` 和同文件 preview/test；
  `PIXEL-UI-13-TAB.md`。
- 禁止：Workspace Pane、Tab 拖放/关闭/预览、`TabPosition` 和 close side。
- 执行：检查 First/Middle/Last、selected/unselected、dirty、长标题和窄 Pane。
- 验证：Component Preview、Tab 行为测试、实际 split pane。
- 验收：宽度、关闭按钮、焦点和拖放行为不变。
- 停止：需要进入 Pane 或修改 tab state/action。
- 预算：1 个生产文件、120 行 Rust diff。

### UI-14：ListItem

单一目标：统一 `ListItem` 的 hover、selected、focused、disabled 和 outline。

- 前置：UI-10 `DONE`。
- 允许路径：`crates/ui/src/components/list/list_item.rs` 和同文件 preview/test；
  `PIXEL-UI-14-LIST-ITEM.md`。
- 禁止：ARIA 结构、click handler、toggle、indent、虚拟列表和调用方。
- 执行：覆盖 Dense/ExtraDense/Sparse、inset、selected、focused、disabled。
- 验证：Component Preview 和现有键盘导航测试。
- 验收：焦点清晰、无尺寸跳动、条件分支和事件回调无行为 diff。
- 停止：视觉必须通过修改事件或 list state 才能实现。
- 预算：1 个生产文件、180 行 Rust diff。

### UI-15：TreeViewItem

单一目标：让 `TreeViewItem` 继承 UI-14 的列表视觉语言。

- 前置：UI-14 `DONE`。
- 允许路径：`crates/ui/src/components/tree_view_item.rs` 和 preview/test；
  `PIXEL-UI-15-TREE-ITEM.md`。
- 禁止：tree 展开状态、disclosure action、虚拟列表和 Project Panel。
- 执行：覆盖展开/折叠、selected、hover、focus 和多层缩进。
- 验证：Component Preview 与 Project Panel 实景只读 smoke。
- 验收：展开/折叠和深度缩进行为不变。
- 停止：需要改 tree state、action 或 Project 代码。
- 预算：1 个生产文件、140 行 Rust diff。

### UI-16：InputField

单一目标：像素化标准 `InputField` 外壳，不进入 Editor 核心。

- 前置：UI-10 `DONE`。
- 允许路径：`crates/ui_input/src/input_field.rs` 和 preview/test；
  `PIXEL-UI-16-INPUT-FIELD.md`。
- 禁止：`crates/editor/**`、ErasedEditor API、focus/input/clipboard 行为。
- 执行：覆盖 normal/focus/error/masked/label/start-icon；保持高度和命中区。
- 验证：输入、粘贴、撤销、masked 切换、error 和 keyboard focus。
- 验收：外壳改变，底层 Editor、输入状态和事件无变化。
- 停止：需要替换 Editor 或修改 focus/masked 事件。
- 预算：1 个生产文件、150 行 Rust diff。

### UI-17：Toggle

单一目标：只改共享 Toggle 的视觉状态。

- 前置：UI-10、UI-12 `DONE`。
- 允许路径：`crates/ui/src/components/toggle.rs` 和 preview/test；
  `PIXEL-UI-17-TOGGLE.md`。
- 禁止：action/click/keyboard/ARIA、调用方和公开状态模型。
- 执行：覆盖 on/off/hover/focus/disabled；保留 switch/checkbox/radio 的语义差异。
- 验证：Component Preview、mouse/keyboard/ARIA 行为。
- 验收：状态不仅依赖颜色，命中区不缩小。
- 停止：需要合并其他组件或改变 Toggle API。
- 预算：1 个生产文件、140 行 Rust diff。

### UI-18：Chip

单一目标：只改 `Chip` 的边界、背景和状态。

- 前置：UI-10 `DONE`。
- 允许路径：`crates/ui/src/components/chip.rs` 和 preview/test；
  `PIXEL-UI-18-CHIP.md`。
- 禁止：删除/点击 action、内容模型和调用方。
- 执行：覆盖 default/hover/selected/focus/disabled 和长文本。
- 验证：Component Preview、键盘/点击行为。
- 验收：文本不裁切、删除按钮命中区不变。
- 停止：需要改 Chip API 或业务调用。
- 预算：1 个生产文件、120 行 Rust diff。

### UI-19：Banner

单一目标：只改 `Banner` 的信息层级和状态边框。

- 前置：UI-10 `DONE`。
- 允许路径：`crates/ui/src/components/banner.rs` 和 preview/test；
  `PIXEL-UI-19-BANNER.md`。
- 禁止：dismiss/action、通知队列和调用方。
- 执行：覆盖 info/success/warning/error、长文本、按钮和 close 图标。
- 验证：Component Preview、dismiss 行为只读验证。
- 验收：状态带图标/文本，不只靠颜色；内容不溢出。
- 停止：需要进入 notification state。
- 预算：1 个生产文件、130 行 Rust diff。

### UI-24：Popover

单一目标：让共享 `Popover` 消费 UI-11 surface，不改变定位和 dismissal。

- 前置：UI-11 `DONE`。
- 允许路径：`crates/ui/src/components/popover.rs` 和 preview/test；
  `PIXEL-UI-24-POPOVER.md`。
- 禁止：anchor、placement、focus restore、outside-click 和调用方。
- 执行：只换 background/border/radius/shadow/padding；检查窗口四边溢出。
- 验证：Component Preview、keyboard dismissal、focus restore、edge placement。
- 验收：定位和关闭行为完全不变。
- 停止：必须改 placement 或 event handler。
- 预算：1 个生产文件、140 行 Rust diff。

### UI-25：Context Menu

单一目标：统一 Context Menu 的 surface、行状态和 separator。

- 前置：UI-11、UI-14、UI-24 `DONE`。
- 允许路径：`crates/ui/src/components/context_menu.rs` 和 preview/test；
  `PIXEL-UI-25-CONTEXT-MENU.md`。
- 禁止：command/action、keyboard selection、submenu、focus 和调用方。
- 执行：检查 disabled、selected、shortcut、separator、submenu 和长文本。
- 验证：Component Preview、上下键/Enter/Escape、submenu 和 focus restore。
- 验收：菜单行为无变化，所有状态在 Night/Dawn 可辨。
- 停止：需要修改 action dispatch 或 menu model。
- 预算：1 个生产文件、180 行 Rust diff。

### UI-26：Modal

单一目标：统一 Modal 容器、header、row 和 footer 的像素外壳。

- 前置：UI-11、UI-12、UI-16 `DONE`。
- 允许路径：`crates/ui/src/components/modal.rs` 和 preview/test；
  `PIXEL-UI-26-MODAL.md`。
- 禁止：open/close state、focus trap/restore、action、overlay dismissal 和调用方。
- 执行：覆盖小/大 modal、长标题、长正文、footer buttons 和 error input。
- 验证：Component Preview、Escape/outside click/focus restore 和窗口缩放。
- 验收：元素层级和 dismissal 行为不变。
- 停止：需要修改 modal state 或 focus 管理。
- 预算：1 个生产文件、180 行 Rust diff。

### UI-30：Project Panel 树行

单一目标：只改项目树 entry 的视觉修饰。

- 前置：UI-02N、UI-02D、UI-14、UI-15 `DONE`。
- 允许路径：仅 `ProjectPanel::render_entry` 中现有 entry 视觉链，当前审计锚点
  `crates/project_panel/src/project_panel.rs:5621`；`PIXEL-UI-30-PROJECT-ROWS.md`。
- 禁止：同函数中的 drag/drop/click/selection/open、`details_for_entry`、Project/Worktree、
  行为测试断言和全文件整理。
- 执行：覆盖 active、marked、hover、diagnostic、Git、ignored、drop target；先确认共享
  List/Tree 已覆盖哪些状态，只补差异。
- 验证：现有 `test_visible_list`、`test_opening_file`、`test_editing_files` 原样通过；人工
  展开、选择、拖放、打开。
- 验收：行为测试零修改，entry 状态完整。
- 停止：diff 进入事件/选择/文件操作，或任何行为测试要求改断言。
- 预算：1 个生产文件、160 行 Rust diff。

### UI-31：Project Panel 重命名外壳

单一目标：只改项目树 inline rename/error 的外壳。

- 前置：UI-16、UI-30 `DONE`。
- 允许路径：`project_panel.rs` 中 inline rename 的纯样式表达式，当前锚点约
  `project_panel.rs:6228`；`PIXEL-UI-31-PROJECT-RENAME.md`。
- 禁止：`Editor::single_line` 配置、confirm/cancel、文件系统操作、validation 状态。
- 执行：只改 background/border/radius/focus/error；保持输入尺寸和焦点。
- 验证：新建、重命名、非法名称、Escape、Enter、失焦。
- 验收：现有编辑行为测试原样通过。
- 停止：需要改 Editor、文件操作或 error state。
- 预算：1 个生产文件、100 行 Rust diff。

### UI-32：Title Bar

单一目标：处理共享组件未覆盖的 Title Bar 视觉。

- 前置：UI-12、UI-13 `DONE`。
- 允许路径：`crates/title_bar/src/title_bar.rs` 中 `TitleBar::render` 的纯样式；
  `PIXEL-UI-32-TITLE-BAR.md`。
- 禁止：`platform_title_bar.rs`、window drag/double-click/system button hitbox。
- 执行：背景、分隔、项目标题和控件间距；检查长路径和窄窗口。
- 验证：macOS 拖动、双击、traffic lights、全屏和多窗口。
- 验收：平台标题栏代码零 diff，系统命中行为不变。
- 停止：必须修改 PlatformTitleBar。
- 预算：1 个生产文件、120 行 Rust diff。

### UI-33：Status Bar

单一目标：处理 Status Bar 背景、分隔和间距。

- 前置：UI-12 `DONE`。
- 允许路径：`crates/workspace/src/status_bar.rs` 中 `StatusBar::render` 的纯样式；
  `PIXEL-UI-33-STATUS-BAR.md`。
- 禁止：ARIA toolbar/tab group、左右键焦点移动、状态项注册和 action。
- 执行：检查左右状态项、Git/diagnostic/language/remote、窄窗口和 overflow。
- 验证：键盘导航、screen reader 语义、状态更新。
- 验收：focus/ARIA/event 逻辑无 diff。
- 停止：视觉要求进入 focus 或 item registry。
- 预算：1 个生产文件、100 行 Rust diff。

### UI-34：Terminal 外围壳层

单一目标：改造 TerminalView 外围背景和边界，不碰字符区域。

- 前置：UI-02N、UI-02D、UI-13、UI-24 `DONE`。
- 允许路径：`crates/terminal_view/src/terminal_view.rs` 中
  `terminal-view-container` 的纯样式，当前锚点约 `:1391`；
  `PIXEL-UI-34-TERMINAL-SHELL.md`。
- 禁止：TerminalElement 创建及之后的字符网格、光标、选区、PTY、copy/paste、link、
  action handler。
- 执行：只处理容器背景/边界；检查 16 色、bright/dim、box drawing 和三种 cursor。
- 验证：shell 输入、resize、copy/paste、link、ANSI 实景；TerminalElement 零 diff。
- 验收：字符和交互行为不变，不叠加扫描线/纹理。
- 停止：需要改变 TerminalElement 或 terminal crate。
- 预算：1 个生产文件、100 行 Rust diff。

### UI-35：Terminal 重命名外壳

单一目标：只改 Terminal tab rename 输入的视觉外壳。

- 前置：UI-16、UI-34 `DONE`。
- 允许路径：`terminal_view.rs` 中 rename Editor 周围纯样式，当前审计锚点约 `:480`；
  `PIXEL-UI-35-TERMINAL-RENAME.md`。
- 禁止：Editor 配置、rename state、confirm/cancel、PTY 和 tab state。
- 验证：Enter/Escape/失焦、长标题、中文标题、窗口缩放。
- 验收：rename 行为不变。
- 停止：需要修改 Editor 或 terminal state。
- 预算：1 个生产文件、80 行 Rust diff。

### UI-40：Agent Panel 壳层

单一目标：只处理 Agent Panel header/background 中共享组件未覆盖的视觉。

- 前置：UI-12、UI-13、UI-14、UI-24 `DONE`。
- 允许路径：`crates/agent_ui/src/agent_panel.rs` 中 `render_title_view`、
  `render_toolbar` 和 `Render` 根容器已有 style chain；`PIXEL-UI-40-AGENT-SHELL.md`。
- 禁止：改变 `AgentPanel::render` 元素层级；thread/terminal 切换、action handlers、拖放、
  focus、scroll、font zoom、connection 和 base view 状态。
- 执行：先检查共享组件继承效果；没有视觉缺口时允许 0 生产 diff，不强行修改。
- 验证：cmd-option-esc 展开、底部按钮、cmd-+/cmd-、滚动、文件拖放、多 surface 切换。
- 验收：根元素层级逐行保持，所有事件表达式无 diff。
- 停止：需要改变 child/map/when 结构或状态分支。
- 预算：1 个生产文件、120 行 Rust diff。

### UI-41：Agent Plan/Summary 卡片

单一目标：统一 plan、summary 和 compaction 类卡片。

- 前置：UI-40 `DONE`。
- 允许路径：`thread_view.rs` 中 `render_plan_summary`、`render_plan_entries`、
  `render_completed_plan`、`render_context_compaction`、`render_edits_summary`；
  `PIXEL-UI-41-AGENT-PLAN.md`。
- 禁止：Thread 状态、列表滚动、消息队列、tool execution、回调和异步任务。
- 执行：只替换容器 background/border/radius/shadow/spacing；检查空、进行中、完成、错误。
- 验证：中文、长步骤、折行、展开/折叠、滚动和 streaming。
- 验收：PlanEntry/state/handler 无 diff。
- 停止：超过 220 行或需要改变状态机；继续拆任务，不得扩大。
- 预算：1 个生产文件、220 行 Rust diff。

### UI-42：Agent Command/Terminal 卡片

单一目标：只改命令和 terminal tool 卡片外壳。

- 前置：UI-41 `DONE`；与后续 Agent 任务严格串行。
- 允许路径：`thread_view.rs` 中 `render_collapsible_command`、
  `render_terminal_tool_call`；`PIXEL-UI-42-AGENT-COMMAND.md`。
- 禁止：命令执行、permission、cancel、terminal output 状态、复制和展开回调。
- 执行：覆盖 collapsed/expanded、pending/running/success/error/cancelled。
- 验证：复制、展开、权限提示、运行/取消和长输出。
- 验收：ToolCallStatus 和所有 handler 无 diff。
- 停止：需要改执行/权限或超过 220 行。
- 预算：1 个生产文件、220 行 Rust diff。

### UI-43：Agent 通用 Tool 卡片

单一目标：统一普通 ToolCall 和输出容器。

- 前置：UI-42 `DONE`。
- 允许路径：`thread_view.rs` 中 `render_any_tool_call`、`render_tool_call`、
  `render_tool_call_content` 的纯容器样式；`PIXEL-UI-43-AGENT-TOOLS.md`。
- 禁止：tool dispatch、approval、async、diff editor、resource/image/markdown 数据处理。
- 执行：覆盖 read/edit/search/resource/image/markdown 外壳；不改内容 renderer。
- 验证：长输出滚动、折叠、error、diff、图片和资源链接。
- 验收：工具行为与内容数据零变化。
- 停止：需要同时重构两类以上 content renderer 或超过 250 行；按工具类型再拆。
- 预算：1 个生产文件、250 行 Rust diff。

### UI-44：Agent Subagent 卡片

单一目标：只改 Subagent 卡片和展开内容外壳。

- 前置：UI-43 `DONE`。
- 允许路径：`thread_view.rs` 中 `render_subagent_tool_call`、`render_subagent_card`、
  `render_subagent_expanded_content`；`PIXEL-UI-44-AGENT-SUBAGENT.md`。
- 禁止：session id、subagent state、scroll handles、cancel 和状态同步。
- 执行：覆盖等待、运行、完成、失败、展开、折叠。
- 验证：打开子 Agent、最小化、停止、返回父线程和滚动位置。
- 验收：状态同步和回调无 diff。
- 停止：需要改 SessionId、列表、导航或 scroll state。
- 预算：1 个生产文件、180 行 Rust diff。

### UI-45：Agent Composer

单一目标：像素化 Agent 输入区外壳和底部控制区。

- 前置：UI-44、UI-12、UI-16 `DONE`。
- 允许路径：仅 `ThreadView::render_message_editor`，当前锚点
  `thread_view.rs:4333`；`PIXEL-UI-45-AGENT-COMPOSER.md`。
- 禁止：`message_editor.rs`、AgentPanel 根 Render、发送/取消/附件/拖放/模型选择逻辑。
- 执行：只改外层 background/border/radius/shadow/spacing；保持 editor 和 controls hierarchy。
- 验证：展开/最小化、发送、停止、附件、拖放、字体缩放、滚动、窄宽度和中文输入。
- 验收：EditorElement、focus、handler 和状态表达式无 diff。
- 停止：需要修改 `MessageEditor::render` 或 AgentPanel 根层级。
- 预算：1 个生产文件、160 行 Rust diff。

### UI-50：可访问性、视觉、行为和性能门禁

单一目标：验证全部实现，不在 Gate 中顺手修生产代码。

- 前置：所有被纳入第一版的实现任务 `DONE`。
- 允许路径：测试运行、截图、`PIXEL-UI-50-QUALITY-GATE.md`；原则上 0 生产文件。
- 禁止：任何“顺手修复”。失败必须标注归属 Task ID 并退回该任务。
- 视觉矩阵：Night/Dawn；UI font 14/16/18；现有可用 density；中文/英文/长路径/
  长模型名；focused/unfocused；所有交互状态；standard/reduced motion；可用设备上的 1x/2x。
- 页面矩阵：Empty Workspace、Editor+Project、split Pane、Terminal、Agent+tool cards、
  Git/diagnostic、Command Palette、Settings、Context Menu、Popover、Modal、Notification。
- 行为矩阵：Project 展开/打开/rename/drag；Agent send/cancel/attachment/expand/drag/scroll；
  Terminal input/resize/copy/paste/link；Tab close/drag/focus；menu/modal keyboard flow。
- 可访问性：APCA 目标、非颜色状态、键盘焦点、ARIA 不变、reduced motion。
- 性能：同一机器/项目/build 至少 3 次前后对比；重复启动/稳定帧回退超过 5%、p95
  frame time 回退超过 10%、idle RSS 增长超过 50MiB 或非字体资产增加超过 2MiB 时
  `NO-GO` 并人工复核。缺少可重复测量工具时写 `NOT VERIFIED`，不能猜测通过。
- 验收：矩阵全部有结果；红色路径无 diff；本任务生产 diff 为 0。
- 停止：任一行为回归、不可重复性能数据、磁盘不足或 PNG baseline 不对应当前 SHA。
- 预算：最多 10 个 evidence 文件和 8MiB；0 生产文件。

### UI-51：默认主题和用户文档

单一目标：用户批准后，把 Night/Dawn 设为新安装默认颜色主题并写清恢复方法。

- 前置：UI-50 `GO`，且用户明确批准默认切换。
- 允许路径：`assets/settings/default.json` 的 theme 字段、实际存在的主题用户文档、
  `PIXEL-UI-51-DEFAULTS.md`。
- 禁止：icon theme 字段、用户 settings、数据库/迁移、现有主题删除、生成文件手工编辑。
- 执行：新 profile 跟随系统 light/dark 选择 Night/Dawn；文档说明手动选择和恢复 One。
- 验证：fresh profile、已有显式 theme profile、system/light/dark、selector 和 docs。
- 验收：已有用户选择不被重写；One/Ayu/Gruvbox 保持可选；icon theme 仍为
  `Orion Studio (Default)`。
- 停止：会覆盖用户配置、主题 gate 未通过或用户未明确批准。
- 预算：1 个设置文件、1 个用户文档、1 个 evidence，生产 diff 不超过 40 行。

### UI-52：最终回归和合并前 Gate

单一目标：形成可合并、可回滚的最终证据，不执行公开发布。

- 前置：UI-51 `DONE`；如果用户决定不默认启用，则记录 UI-51 `SKIPPED-BY-DECISION`。
- 允许路径：`PIXEL-UI-52-FINAL-GATE.md`；0 生产文件。
- 必查：批准范围、红色路径、theme-name 分支、错误处理、许可证、Orion 品牌、format、
  focused tests、完整 `./script/clippy`、macOS app build/start/open-project/restart、磁盘和
  target 增量、PR 依赖和回滚顺序。
- 验证：第 9 节所有适用命令；安装/签名/公证/公开 release 不在本任务授权内。
- 验收：给出 `GO`、`GO-WITH-CONDITIONS` 或 `NO-GO`，每项有命令/截图/SHA 证据。
- 停止：需要修生产代码；退回所属任务后重新执行完整 Gate。
- 预算：1 个 evidence；0 生产文件。

## 9. 验证分层

### 9.1 轻量固定检查

```text
git status --short --branch
git diff --stat
git diff --name-only
git diff --check
```

修改 Rust 时再运行：

```text
cargo fmt --all -- --check
```

主题 JSON：

```text
jq empty assets/themes/orion-pixel-atelier/orion-pixel-atelier.json
```

SVG 资产任务把 `<approved-directory>` 替换为本任务批准目录：

```text
find <approved-directory> -name '*.svg' -print0 | xargs -0 -n 1 xmllint --noout
```

未修改 Rust 的 JSON/SVG/文档任务不需要为了形式启动 Cargo 编译。每个未运行命令都要
写明“不适用”或 `NOT VERIFIED`，不能从旧日志复制 PASS。

### 9.2 主题、图标和共享 UI 聚焦测试

以下命令必须串行，且只在对应文件有变更时运行：

```text
CARGO_BUILD_JOBS=2 cargo test -p icons -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p theme_settings schema::tests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p theme styles::colors::tests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p settings_content theme::tests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test -p ui apca_contrast -- --nocapture
```

共享交互 smoke：

```text
CARGO_BUILD_JOBS=2 SEED=0 cargo test -p ui can_navigate_back_over_headers -- --nocapture
CARGO_BUILD_JOBS=2 SEED=0 cargo test -p ui test_downloading_tooltip_shows_in_preview_like_layout -- --nocapture
CARGO_BUILD_JOBS=2 SEED=0 cargo test -p tab_switcher -- --nocapture
```

产品壳层任务按源码中现有测试名再收窄 filter。Project Panel 至少保留这些原始断言：

```text
CARGO_BUILD_JOBS=2 SEED=0 cargo test -p project_panel test_visible_list -- --nocapture
CARGO_BUILD_JOBS=2 SEED=0 cargo test -p project_panel test_opening_file -- --nocapture
CARGO_BUILD_JOBS=2 SEED=0 cargo test -p project_panel test_editing_files -- --nocapture
```

执行前若 filter 在最新 main 已改名，WorkBuddy 必须先用 `rg` 定位新测试名并在 evidence
解释，不得删除该验证层。

### 9.3 Component Preview 的正确用法

独立 preview 可用于快速检查基础组件能否渲染：

```text
CARGO_BUILD_JOBS=2 cargo run -p component_preview --example component_preview
```

但它当前使用 `LoadThemes::JustBase`，不能证明 Night/Dawn。Pixel 主题验收必须启动完整
Orion Studio，并通过 Command Palette 执行 `workspace: open component preview`；完整应用
会加载 bundled themes。evidence 必须区分“standalone preview”和“full-app preview”。

### 9.4 PNG visual runner

当前 runner 是 macOS-only、真实 GPUI/Metal texture capture，图片比较阈值为现有实现的
99%。完成 UI-00V 后，Night 基线示例：

```text
CARGO_BUILD_JOBS=2 SEED=0 UPDATE_BASELINE=1 \
VISUAL_TEST_THEME="Orion Pixel Atelier Night" \
VISUAL_TEST_OUTPUT_DIR=target/visual_tests/night-baseline \
cargo run -p orion-studio --bin orion_studio_visual_test_runner --features visual-tests
```

比较时必须先完全移除 `UPDATE_BASELINE`。不能写 `UPDATE_BASELINE=0`，因为当前代码只
检查变量是否存在：

```text
unset UPDATE_BASELINE
CARGO_BUILD_JOBS=2 SEED=0 \
VISUAL_TEST_THEME="Orion Pixel Atelier Night" \
VISUAL_TEST_OUTPUT_DIR=target/visual_tests/night-current \
cargo run -p orion-studio --bin orion_studio_visual_test_runner --features visual-tests
```

Dawn 使用 Dawn 的精确显示名。需要聚焦时增加：

```text
VISUAL_TEST_FILTER="project_panel"
```

证据解释必须准确：

- baseline 目录 `crates/zed/test_fixtures/visual_tests/` 当前被 `.gitignore` 忽略。
- CI 当前只 build visual runner binary，不执行 PNG comparison。
- 仓库没有可直接等同于本计划的 `.snap` 自动快照门禁。
- 因此 v1 PNG 是本地、可重复的人工交付证据，不是现成 CI PASS。
- runner 会保留临时 project 让 OS 后续清理；运行前后要记录磁盘，不能并行启动。

### 9.5 Clippy 分层

禁止使用 `cargo clippy`，使用仓库脚本。基础层候选命令：

```text
CARGO_BUILD_JOBS=2 GITHUB_ACTIONS=1 ./script/clippy \
  -p theme -p theme_settings -p icons -p file_icons \
  -p ui -p ui_input -p component_preview
```

产品壳层候选命令：

```text
CARGO_BUILD_JOBS=2 GITHUB_ACTIONS=1 ./script/clippy \
  -p project_panel -p terminal_view -p agent_ui -p workspace -p title_bar
```

每个任务只运行覆盖其修改 crate 的最小分组。UI-52 再串行运行完整脚本；不得让两个
Agent 同时运行上面两组。

### 9.6 最终 check、bundle 和可见窗口 smoke

完整命令只在 UI-52 串行执行，并先满足 80GiB 空间门槛：

```text
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo check -p orion-studio --bin orion-studio
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 ./script/clippy
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 ./script/bundle-mac aarch64-apple-darwin
```

bundle 完成不等于启动通过。必须验证当前产物 SHA 对应的 app：

1. 启动后出现 CoreGraphics 可见窗口。
2. 打开本仓库并显示 Project、Editor、Terminal、Agent。
3. 选择 Night/Dawn 并打开完整 Component Preview。
4. 退出，再次启动并恢复到正常可操作窗口。
5. 记录 app 路径、bundle id、binary SHA、窗口证据、RSS、`target` 和磁盘增量。

安装到 `/Applications`、签名、公证和公开 release 需要单独授权，不因 UI-52 自动执行。
不得从旧 release evidence 复制命令输出后声称本轮通过。

## 10. 风险矩阵

| 风险 | 级别 | 预防 |
| --- | --- | --- |
| 全仓圆角替换导致行为或布局回归 | 高 | 先令牌化共享组件，面板逐个处理 |
| Agent Panel 元素层级变化 | 高 | 根层级冻结，卡片按函数边界迁移 |
| 输入框改造进入 Editor 核心 | 高 | 只改 InputField/Editor 外壳 |
| Terminal 像素化影响字符网格 | 高 | TerminalElement 禁止修改 |
| 主题 schema 扩张增加上游冲突 | 高 | 第一版几何不进入主题 schema |
| Pixel Chrome 误认为主题专属，导致 theme-name 分支 | 高 | 明确几何全局生效；可切换几何另立计划 |
| 图标混用导致视觉不一致 | 中 | 先 pilot，用户批准后扩大 |
| 文件图标和产品图标混为一个机制 | 中 | UI-03F/UI-03P 分 PR，不改注册语义 |
| 像素字体造成中文缺字 | 高 | 第一版不默认打包像素字体 |
| focus 边框引发布局跳动 | 中 | 使用不改变盒模型的 ring/overlay |
| 深色可读、浅色不可读 | 中 | Night/Dawn 同步做状态矩阵 |
| UI density 使用不完整 | 中 | 只测试当前实际生效档位，不把 density 当主题开关 |
| visual runner 只加载 base theme | 高 | UI-00V 先加载 bundled theme 并显式选择 |
| 本地 PNG 被误报为 CI gate | 高 | evidence 明确 baseline 被忽略、CI 只编译 runner |
| Agent 多任务并发修改 thread_view.rs | 高 | UI-41 至 UI-45 严格串行 |
| 验收 PR 顺手修生产代码 | 高 | UI-50/UI-52 固定 0 生产 diff，失败退回归属任务 |
| 上游同步冲突扩大 | 中 | 小 PR、集中 styles、禁止业务分支 |
| 构建缓存再次异常增长 | 高 | 单 Cargo、空间门槛、记录 target 增量 |

## 11. 回滚方案

1. UI-02N/D 在 UI-51 前保持非默认；问题发生时从 selector 切回 One 即可，无迁移。
2. UI-51 是独立默认值任务；回滚它只恢复 `default.json`，保留新主题作为可选项。
3. UI-03F/P 只替换现有批准资产，可按各自 PR 独立 revert，不改 icon theme 名称/注册表。
4. UI-10/11 是共享基础；业务组件 PR 依赖它们。回滚时按“壳层 → 组件 → surface →
   token”反向顺序，不能先删 API 再留调用方。
5. UI-30/UI-31/UI-32/UI-33/UI-34/UI-35 可按面板独立 revert。
6. UI-41 至 UI-45 按逆序回滚，避免 stacked `thread_view.rs` 冲突。
7. UI-00V 可独立 revert；其 baseline 本来不属于用户数据。
8. 本计划不新增持久化 schema、数据库迁移或用户 settings rewrite，回滚不需要数据迁移。
9. 任何回滚先核验精确 commit 和 dirty worktree；禁止 destructive reset、checkout 或 clean。
10. 许可证/NOTICE 与仍在使用的资产一起保留；不能为了缩小 diff 删除 attribution。

## 12. 最终 Definition of Done

- [ ] 本计划列出的所有 UI-00 至 UI-52 任务都有明确状态和 evidence。
- [ ] UI-00V 能指定 bundled theme 和 visual test filter，失败时不静默 fallback。
- [ ] Orion Pixel Atelier Night/Dawn 可选择、可切换、可恢复。
- [ ] UI-51 的新安装默认行为已经获得用户批准；若不批准，明确记录跳过。
- [ ] `Orion Studio (Default)` icon theme 名称和注册流程不变。
- [ ] 共享 Button、Tab、List、Tree、Input、Toggle、Chip、Banner、Popover、Menu、Modal
  状态完整。
- [ ] Project、Title、Status、Terminal 和 Agent 壳层一致。
- [ ] 编辑器、Pane、Dock、Project、Agent 和 Terminal 核心行为无变化。
- [ ] 红色禁止路径无生产 diff。
- [ ] 不存在主题名称业务分支。
- [ ] Pixel Chrome 在其他颜色主题下不会破坏可读性或布局。
- [ ] 中文、英文、长文本和当前实际支持的 density 通过。
- [ ] 键盘、ARIA、focus、reduced motion 和对比度通过。
- [ ] 图标在 10/12/14/16px 下清晰。
- [ ] 性能、内存和资产大小通过门禁。
- [ ] format、聚焦测试、完整 `./script/clippy`、app check、bundle 和可见窗口 smoke 通过。
- [ ] PNG 证据被正确描述为本地证据，没有误报为 CI visual gate。
- [ ] 品牌、许可证和 attribution 检查通过。
- [ ] 磁盘增长和构建产物已记录，没有未经授权清理。
- [ ] 每个实现 PR 单一目标、可按依赖逆序回滚，未自动合并或发布。

## 13. WorkBuddy 交接模板

每个任务完成后必须返回：

1. Task ID。
2. Status：DONE、PARTIAL、BLOCKED 或 NOT VERIFIED。
3. 单一目标是否完成；一句话说明。
4. 开始 branch/HEAD/base PR 和结束 branch/HEAD。
5. 开始 dirty/staged/untracked 清单和结束清单。
6. 修改文件列表、每个文件所属允许项、实际 Rust/资产 diff 行数。
7. 精确修改的函数/Render 区域；黄色大文件必须列符号名。
8. 明确列出未修改的事件、状态、action、schema 和红色路径。
9. 实际运行的命令、退出码、耗时和结果。
10. 未运行命令及原因；不能省略。
11. Full-app Component Preview、PNG、人工截图索引及对应 theme/SHA。
12. PNG 是本地证据还是 CI 证据；第一版正确答案通常为本地证据。
13. 开始/结束 `df -h .`、`du -sh target`、峰值内存或可用观测。
14. 是否出现未授权 diff、测试断言变化、许可证变化或 user-owned 冲突。
15. 需要用户决定的问题；没有则写 `None`。
16. 唯一下一任务建议；不得自行开始。

建议固定结尾：

```text
TASK: UI-XX
STATUS: DONE | PARTIAL | BLOCKED | NOT VERIFIED
SCOPE CHECK: PASS | FAIL
BEHAVIOR CHECK: PASS | FAIL | NOT VERIFIED
VISUAL CHECK: PASS | FAIL | NOT VERIFIED
RESOURCE CHECK: PASS | FAIL | NOT VERIFIED
NEXT: UI-YY (not started)
```

## 14. 当前待确认决策

开始 UI-02N 或 UI-10 前，用户需要确认：

1. 主题最终显示名是否使用 Orion Pixel Atelier Night/Dawn。
2. 本计划第 4.3、4.4 节色板是否作为第一版基线。
3. 新原创主题的 author 字段。
4. UI-03F 和 UI-03P 的精确图标文件 allowlist。
5. 第一版是否接受“不默认打包像素字体”的建议。
6. 是否接受 Pixel Chrome 是 Orion Studio 全局产品几何，选择 One/Ayu/Gruvbox 时也会生效。
7. UI-51 是否把新主题设为所有新安装的默认颜色主题。

推荐默认决策：

- 接受 Night/Dawn 名称和第 4 节色板作为 v1 基线。
- author 使用项目/公司批准的统一署名，不使用临时模型名。
- 文件图标首批只做 folder/folder_open/file/chevron，产品图标最多 16 个。
- v1 不打包默认像素字体。
- 接受全局 Pixel Chrome；运行时 Classic/Pixel 切换另立 v2 计划。
- UI-50 通过后再决定是否默认启用，不提前改 default settings。

决策未完成前可以执行 UI-00、UI-00V 和 UI-01 的设计准备；不得进入主题、图标、Chrome
或产品源码实现。
