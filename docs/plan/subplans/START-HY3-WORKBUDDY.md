# HY3 WorkBuddy 启动 Prompt

你现在是 Orion Studio 项目的执行型软件工程 Agent。请在当前工作区直接执行任务，
不要只给方案，也不要自行扩大范围。

## 项目目标

当前仓库是 Zed 源码基线，目标是逐步改造成 Orion Studio：

- 从用户可见品牌迁移到 Orion Studio。
- 迁移运行时路径、环境变量、package、binary、CLI、URL scheme、服务 endpoint、
  平台安装包和 CI/CD 身份。
- 保留必要的许可证、版权、上游贡献和第三方 attribution。
- 旧配置、旧协议和旧 CLI 必须有明确兼容、迁移和回滚策略。
- 每一步都要有可复现的测试证据。

## 现在只执行 S01

本次启动任务只执行：

docs/plan/subplans/01-baseline-inventory.md

不要执行 S02 或任何源码改造。S01 是只读盘点任务，只允许生成：

docs/plan/evidence/S01-baseline-inventory.md

如果 evidence 目录不存在，可以创建它。除这个 evidence 文件外，不得修改任何文件。

## 必须先读取的文件

按顺序读取：

1. AGENTS.md
2. docs/plan/orion-studio-rebrand-migration-plan.md
3. docs/plan/subplans/README.md
4. docs/plan/subplans/00-workbuddy-execution-guide.md
5. docs/plan/subplans/01-baseline-inventory.md

不要修改或清理：

- .workbuddy/
- docs/research/
- docs/plan/ 中已有的计划文件
- 任何用户已有的 tracked、staged 或 untracked 文件

## 第一阶段：Preflight

先在仓库根目录运行：

~~~text
pwd
git branch --show-current
git rev-parse --short HEAD
git status --short --branch
git diff --name-only
git diff --cached --name-only
git ls-files --others --exclude-standard
rustc +stable --version
cargo +stable --version
~~~

必须满足：

- 当前分支是 init。
- 所有 tracked/staged/untracked 状态已经记录。
- .workbuddy/、docs/research/ 和 docs/plan/ 的已有内容不会被覆盖。
- 没有其他 Agent 正在修改同一工作区。

如果当前分支不是 init，或发现无法判断归属的 tracked/staged 修改：
立即停止，返回 BLOCKED，不要修复工作树。

## 第二阶段：执行 S01 基线盘点

严格按照 S01 执行以下检查：

~~~text
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
git grep -In -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://' -- crates
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://' -- assets
git grep -Il -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://' -- script .github legal docs/src
~~~

再执行工作树扫描：

~~~text
rg --hidden --glob '!.git/**' --glob '!target/**' --glob '!.workbuddy/**' \
  -n -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
~~~

分别检查这些区域：

- P0：crates/paths、crates/zed_env_vars、crates/release_channel、
  crates/zed/Cargo.toml、crates/cli、crates/client、crates/remote_server。
- P1：README.md、CONTRIBUTING.md、docs/src、assets、extension_api、测试 fixture。
- P2：.github、ci、Dockerfile、Procfile、mailmap、compliance、legal。
- 服务：crates/collab、cloud_api_client、Dockerfile-collab 和部署清单。

对每个重要命中记录：

- 文件路径和行号。
- 属于展示文本、路径、环境变量、协议、服务 URL、package/API、平台发布、
  法律/上游归属还是历史 fixture。
- 处理结论：MIGRATE、COMPAT、KEEP-ATTRIBUTION、KEEP-HISTORY、
  DELETE-AFTER-APPROVAL 或 OPEN。
- 如果是 COMPAT，记录需要的兼容测试和兼容窗口；不能猜测期限。
- 如果疑似包含 secret，只记录路径和行号，不复制 secret 内容。

## S01 报告必须包含

将结果写入：

docs/plan/evidence/S01-baseline-inventory.md

报告至少包括：

1. 分支、HEAD、工具链和工作树边界。
2. 每个扫描命令、扫描范围和命中数量。
3. P0、P1、P2 和服务端的代表性清单。
4. 当前活动默认的 endpoint、环境变量、路径、App ID、CLI 和 URL scheme。
5. 许可证、商标、上游 attribution 和研究文档中的矛盾。
6. 推荐迁移顺序。
7. 需要人类决定的问题。
8. 明确写出：本次没有修改源码、没有完成构建、没有验证生产服务。

计划文件和研究材料中的 Zed 命中不要混入“源码残留”结论，单独标记为
计划/研究材料。

## 禁止事项

- 禁止全局搜索替换。
- 禁止修改任何 Rust、Cargo、脚本、CI、README、docs/src 或资源文件。
- 禁止 git reset、git checkout --、git clean、rm -rf 或等价破坏命令。
- 禁止提交、推送、创建 PR、发布或操作生产服务。
- 禁止猜测 Orion 域名、Bundle ID、URL scheme、许可证或兼容窗口。
- 禁止把扫描命中数量当作改造完成证据。
- 禁止隐藏失败；命令失败时记录完整命令和错误。

## 完成前检查

运行：

~~~text
git diff --check
git status --short --branch
git diff --name-only
~~~

确认唯一新增/修改文件是：

docs/plan/evidence/S01-baseline-inventory.md

如果有其他文件变化，停止并返回 BLOCKED，不要删除或覆盖它们。

## 返回格式

严格按以下格式返回：

~~~text
子计划：S01 品牌、运行时和发布基线盘点
状态：DONE | BLOCKED | PARTIAL
执行分支：
基线 HEAD：
修改文件：
未修改但检查过的关键文件：
扫描命中数量：
执行命令与结果：
验收项：
- A1：PASS/FAIL/SKIP，证据：
- A2：PASS/FAIL/SKIP，证据：
失败或未决问题：
回滚方式：
建议下一步：
~~~

只有 S01 报告完整且状态为 DONE，才可以等待人工确认后执行 S02。
不要自动进入 S02。

