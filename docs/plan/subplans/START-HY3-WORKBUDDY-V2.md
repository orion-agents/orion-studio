# HY3 WorkBuddy 续作引导：Orion Studio init-plan-v2

你现在位于 Orion Studio 仓库根目录。请作为执行代理，继续当前 init 分支上的 Orion Studio 重构。

## 先读这些文件

按顺序读取：

1. AGENTS.md
2. docs/plan/init-plan-v2.md
3. docs/plan/subplans/README.md
4. docs/plan/subplans/00-workbuddy-execution-guide.md
5. docs/plan/evidence/S01-baseline-inventory.md
6. docs/plan/evidence/S02-identity-and-compatibility-contract.md
7. docs/plan/evidence/S03-runtime-identity-and-paths.md
8. docs/plan/evidence/S04-data-migration-and-compatibility.md
9. docs/plan/evidence/S05-core-package-and-binary.md
10. docs/plan/evidence/S06-cli-api-and-protocol.md
11. 当前 git diff 和 git status

不要把旧的 START-HY3-WORKBUDDY.md 当成当前任务入口；它是第一轮 prompt。当前入口是本文件和 init-plan-v2.md。

## 当前基线

- 分支：init
- HEAD：d2779c3
- 工作树是 dirty，已有 WorkBuddy 改动，必须保留。
- 不得执行 git reset、git checkout、git clean、删除 .workbuddy、删除 docs/plan 或删除 docs/research。
- 不得提交、推送、建 PR、打 tag、发布包或修改生产数据。
- 不要假设 S01–S06 的 DONE 标签等于完整完成；以源码、diff、测试命令和 evidence 为准。
- 当前已验证：paths 测试 13 个通过，cli 测试 6 个通过，client 和 zed_extension_api check 通过，fmt/check-todos/check-keymaps/git diff --check 通过。
- 主二进制 cargo +stable check -p zed --bin orion-studio 尚未通过验证，已遇到缺少 Metal Toolchain；不要改写为代码通过。
- ./script/clippy 尚未在本轮验证，必须按 AGENTS.md 使用它。

## 本轮硬约束

本轮最多执行一个子计划；先执行 V2-00，只有 V2-00 没有阻塞时才执行 V2-01。V2-01 完成后立即停止，等下一轮指令，不要自动进入 V2-02。

每个命令都要记录：

- 执行目录
- 使用的分支和 HEAD
- 命令
- 退出码
- 关键输出
- 是否因环境或超时停止

如果发现工作树在开始后与基线不一致，先记录，不要覆盖已有改动。

## V2-00：只读进度审计

只允许写：

- docs/plan/evidence/INIT-V2-PROGRESS-AUDIT.md

不得修改任何源代码。

审计必须包含：

1. git status --short、git branch --show-current、git rev-parse HEAD、git diff --stat。
2. 当前已有修改和未跟踪目录的边界。
3. S01–S06 evidence 中的结论与源码抽查结果。
4. 搜索并分类这些命中：
   - Zed
   - zed.dev
   - cloud.zed.dev
   - ZED\_
   - zed-cli://
   - zed-\*.sock
   - dev.zed
   - Zed-Server
5. 检查 migration.rs 是否只有定义和测试，是否真正从启动流程调用。
6. 记录本轮已通过的 focused tests，以及主二进制 Metal Toolchain 阻塞。
7. 为 V2-01 列出精确允许文件和待修问题。
8. 结论必须区分 VERIFIED、PARTIAL、NOT VERIFIED、BLOCKED、NOT STARTED。

V2-00 验收：

- 不修改源代码。
- 生成 INIT-V2-PROGRESS-AUDIT.md。
- git diff --check 通过。
- 文档中没有把未运行命令写成通过。
- 若任何基线信息无法确认，写 BLOCKED 并停止。

## V2-01：迁移模块硬化

只有 V2-00 无阻塞时执行。允许修改：

- crates/paths/src/migration.rs
- crates/paths/src/paths.rs
- crates/paths/src/lib.rs
- crates/paths 中与迁移直接相关的现有测试文件
- docs/plan/evidence/INIT-V2-V01-MIGRATION-HARDENING.md

必须完成：

1. 处理 migration.rs 中所有 fallible operation；禁止 let \_ = 静默丢弃错误。
2. marker 损坏、空文件、未知版本、重复字段、来源变化必须 fail closed。
3. 迁移失败、临时目录清理失败、复制/rename/写 marker 失败都要有明确错误语义和上下文。
4. 目标已有文件时不得无条件覆盖；补冲突和重复执行测试。
5. 检查并补齐 config/data、Flatpak、server、WSL server、symlink、特殊文件映射。
6. 确认 .zed_wsl_server 到 .orion_wsl_server 的兼容策略；不能无依据跳过。
7. secrets 迁移不得打印内容，旧目录保留策略必须有测试或证据。
8. 新增或修改的生产代码不要使用 unwrap、expect 或可能越界 panic 的索引。
9. 不接入启动流程，不修改服务域名、UI、IPC 或发布配置。

V2-01 验收命令：

- cargo +stable fmt --all -- --check
- cargo +stable test -p paths
- ./script/clippy
- git diff --check

如果 ./script/clippy 因环境或已有问题失败，必须保留完整命令和退出码，状态写 PARTIAL 或 BLOCKED，不得写 DONE。

## 停止和交付

V2-00 或 V2-01 完成后立即停止。最终只回报本轮执行的一个阶段，使用：

- 阶段 ID
- 状态：DONE / PARTIAL / BLOCKED
- 分支和 HEAD
- 开始前和结束后的 dirty 边界
- 修改文件
- 关键实现或审计摘要
- 测试命令、退出码和结果
- 未运行/环境阻塞
- 遗留风险
- 下一步建议
- 回滚方式

不要因为计划文件存在就宣布项目完成。不要删除 Zed 兼容入口，除非契约和迁移证据明确允许。
