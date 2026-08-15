# S00：WorkBuddy 执行手册

## 任务目标

让执行模型在每个 Orion Studio 子计划中采用固定、安全、可复核的流程。
本文件本身不要求修改源码。

## 每次任务开始前

在仓库根目录执行：

```text
pwd
git branch --show-current
git rev-parse --short HEAD
git status --short --branch
git diff --name-only
git diff --cached --name-only
git ls-files --others --exclude-standard
```

必须满足：

- 当前分支为 init。
- HEAD、tracked 修改、staged 修改和 untracked 文件已记录。
- .workbuddy/、docs/research/、docs/plan/ 下的用户文件不能删除或覆盖。
- 只执行被调用子计划列出的文件范围。

如果检查失败，返回 BLOCKED，不要尝试修复工作树。

## 执行规则

1. 先读总计划、当前子计划以及它声明的前置子计划结果。
2. 先搜索再编辑。每个要修改的文件先读相关上下文和测试。
3. 一次只处理一个明确的行为边界，禁止全仓字符串替换。
4. 优先复用已有常量、路径、配置和测试模式，不创建平行架构。
5. 旧标识只可放入明确的兼容、迁移、法律或上游归属分支。
6. 不猜测 Orion 域名、App ID、许可证、数据库字段或外部服务行为。
7. 不使用 git reset --hard、git checkout --、git clean、rm -rf 或等价破坏命令。
8. 不提交、不推送、不创建 PR、不发布、不操作生产服务。
9. 遇到 Rust 编译错误先最小修复当前范围；如果需要跨子计划，停止并交接。
10. 所有错误都要显示记录；不得用静默忽略错误的方式“让测试通过”。

## 修改后固定检查

根据子计划选择最小范围运行：

```text
git diff --check
cargo +stable fmt --all -- --check
cargo +stable metadata --no-deps --format-version 1
```

之后运行该子计划列出的包测试、集成测试或脚本。不要把“命令未运行”
写成通过。

## 完成前固定检查

```text
git status --short --branch
git diff --stat
git diff --check
git diff --name-only
```

逐个确认 diff 文件都属于当前子计划。发现越界文件时，恢复动作必须可逆，
不要直接删除用户内容；先报告。

## WorkBuddy 返回内容

必须使用 README 中的交接模板，并额外回答：

- 改动是否只落在本子计划范围？
- 是否运行了所有列出的命令？
- 是否有旧 Zed 标识仍出现在规范路径、默认 endpoint 或用户可见界面？
- 是否需要人类决定，而不是模型继续猜测？
