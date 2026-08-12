# S01：品牌、运行时和发布基线盘点

## 任务目标

生成一个可重复的清单，回答“哪些 Zed 标识需要迁移、兼容、保留归属或删除”。
本子计划只读扫描，不修改源码。

## 前置条件

- 已阅读 S00。
- 当前分支为 init。
- 工作树状态已记录。
- 不要求当前 workspace 完整编译通过；若编译失败，记录原因，不要修依赖。

## 允许写入

只允许新增或更新以下证据文件：

- docs/plan/evidence/S01-baseline-inventory.md

如果 evidence 目录不存在，可以创建目录。禁止修改 README、总计划、docs/research、
源码、脚本、Cargo.lock 或工作树中其他文件。

## 执行步骤

### 1. 固定基线

记录：

~~~text
git branch --show-current
git rev-parse HEAD
git status --short --branch
git log -1 --oneline
rustc +stable --version
cargo +stable --version
~~~

### 2. 运行跟踪文件扫描

执行并保存命中数量：

~~~text
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
git grep -In -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
~~~

再按下面区域分别统计：

~~~text
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://' -- crates
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://' -- assets
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://' -- script .github legal docs/src
~~~

### 3. 运行工作树扫描

扫描时排除 .git、target、.workbuddy 和构建输出：

~~~text
rg --hidden --glob '!.git/**' --glob '!target/**' --glob '!.workbuddy/**' \
  -n -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
~~~

将 untracked 的 docs/research 和 docs/plan 单独列为“计划/研究材料”，
不能把它们的命中混入源码残留结论。

### 4. 按优先级分类

至少检查以下位置：

- P0：crates/paths、crates/zed_env_vars、crates/release_channel、
  crates/zed/Cargo.toml、crates/cli、crates/client、crates/remote_server。
- P1：README.md、CONTRIBUTING.md、docs/src、assets、extension_api、测试 fixture。
- P2：.github、ci、Dockerfile、Procfile、mailmap、compliance、legal。
- 服务：crates/collab、cloud_api_client、Dockerfile-collab 和部署清单。

每个命中至少写出：

- 文件路径和行号。
- 命中内容属于哪一种身份：展示文本、路径、环境变量、协议、服务 URL、
  package/API、平台发布、法律/上游归属、历史 fixture。
- 处理结论：MIGRATE、COMPAT、KEEP-ATTRIBUTION、KEEP-HISTORY 或 DELETE-AFTER-APPROVAL。
- 如果是 COMPAT，写出兼容窗口和测试要求；未知就写 OPEN，不要猜。

### 5. 生成报告

报告必须包含：

1. 基线 commit、分支、工作树边界和工具链。
2. 各扫描命令、命中数量和扫描范围。
3. P0/P1/P2 清单。
4. 活动默认 endpoint、环境变量、路径、App ID、CLI 和 URL scheme。
5. 许可证、商标、上游归属和研究文档矛盾。
6. 推荐的迁移顺序和需要人类批准的决策。
7. “未完成项”：此子计划没有改源码、没有完成构建、没有完成服务验证。

## 验收标准

- S01 报告存在且可由命令重新生成。
- 至少包含一个 P0、P1、P2 和服务端清单。
- 所有命中都有处理分类或 OPEN 标记。
- 没有源码、脚本、Cargo.lock 或用户文件改动。
- 工作树 diff 只能显示允许的 evidence 文件。

## 失败处理

- rg/git grep 命令失败：记录命令和错误，检查工具可用性；不要改用宽泛删除。
- 命中数量与历史基线不同：记录扫描口径，不要手工“修正”数字。
- 发现疑似 secret：不要复制 secret 到报告，记录文件和行号并 BLOCKED。
- 发现服务默认值但没有合同：标记 OPEN，交给 S02。

## 交接

状态必须为 DONE 或 BLOCKED。返回：

- 扫描口径和命中数量。
- 最重要的 10 个 P0 命中。
- 未决的命名/服务/许可证问题。
- evidence 文件路径。

