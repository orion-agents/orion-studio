# v1.17.0 AI Native v3 — 真实验收证据

本目录存放 [v3 计划](../../v1.17.0-ai-native-v3.md) §6 / §9 要求的**真实窗口**验收证据。

## 硬性要求

1. **每份证据必须记录构建身份**，否则验收无效：
   - `CFBundleShortVersionString`
   - `CFBundleVersion`
   - 构建 commit（`git rev-parse HEAD`）
   - 安装时间
   读取方式：
   ```sh
   defaults read "/Applications/Orion Studio Dev.app/Contents/Info.plist" CFBundleShortVersionString
   defaults read "/Applications/Orion Studio Dev.app/Contents/Info.plist" CFBundleVersion
   ```
   版本与当前冻结 commit 不一致 ⇒ 该证据作废。
2. 截图/录屏注明窗口尺寸与主题（1440×900 / 1180×800 / 1100×760；dark / light / high-contrast）。
3. **不得删除或覆盖用户配置**来获得干净截图；配置解析 toast（`Invalid global tasks file` 等）单独记录。

## 场景清单（对应 v3 §6）

| # | 场景 | 需要的证据 |
| --- | --- | --- |
| 1 | 启动与版本 | Dev App 启动首屏截图 + 版本/commit 信息 |
| 2 | 切到 AI Native | 中央无 editor tab row；右侧无 `Changes / Files / Review`；左侧任务轨 |
| 3 | New conversation → chooser | Native、external ACP、Terminal 各一次；只建草稿，首发才起 session |
| 4 | 已有消息 thread 的 Agent badge | 禁用 + 新建会话说明 |
| 5 | 环境七行 | Changes / Branch / Commit or push / Plan / Subagents / Sources 各一屏；点文件只出右侧预览 |
| 6 | 代码工作区往返 | 确认层（取消无变化）→ 确认进入 → 可见 `Return to task` → 返回同一草稿/滚动 |
| 7 | 边界场景 | 无 Git、无 sources、无 subagent、权限/elicitation、AI disabled、窄窗口收起顺序 |
| 8 | 可访问性 | 浅色/深色/高对比 + 键盘 Tab + VoiceOver 的 pure-icon 控件核对 |

## 命名约定

```
<NN>-<scene>-<window-size>-<theme>.png        例：02-ainative-1440x900-dark.png
<NN>-<scene>.mov                              录屏
README.md                                     本文件
```
