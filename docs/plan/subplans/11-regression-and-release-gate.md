# S11：品牌残留扫描、回归测试和最终发布门禁

## 任务目标

对整个 Orion 改造做最后的证据审计，确认旧 Zed 标识只留在批准的兼容、迁移、
法律或上游归属位置，并验证构建、测试、安装、服务和回滚。此子计划默认不发布。

## 前置条件

- S01 至 S10 的交接结果都存在。
- S02 的身份契约已批准。
- 所有 BLOCKED/PARTIAL 项都有责任人和处理计划。
- 当前工作树状态已经冻结；不得在审计过程中混入新功能。

## 允许修改

默认只允许：

- docs/plan/evidence/S11-release-gate.md
- tests/snapshots 或测试 fixture 中因前面改动必然需要的最小文件。

如缺少品牌扫描脚本，可以提出新增 script/check-orion-brand，但必须先报告；
不要在最终门禁阶段自行扩大源码范围。

## 执行步骤

### 1. 审计工作树和变更范围

运行：

```text
git status --short --branch
git diff --stat
git diff --name-only
git diff --check
git log --oneline --decorate -10
```

检查每个文件是否能映射到某个 S01-S10。发现无主文件、未解释的用户改动或
计划外文件时 BLOCKED。

### 2. 运行统一残留扫描

扫描跟踪文件和工作树文件，排除构建输出与计划/研究材料：

```text
git grep -n -I -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
rg --hidden --glob '!.git/**' --glob '!target/**' --glob '!.workbuddy/**' \
  --glob '!docs/plan/**' --glob '!docs/research/**' \
  -n -E 'Zed|zed\.dev|zed-industries|ZED_|dev\.zed|zed://'
```

逐个命中填写：

- 路径和行号。
- 归类：规范残留、兼容、迁移、法律/版权、上游、测试、历史。
- 允许保留的批准来源。
- 如果应删除，关联哪个后续变更。

不能用“整个目录 allowlist”隐藏命中。

### 3. 运行静态和 Rust 门禁

至少运行：

```text
cargo +stable metadata --no-deps --format-version 1
cargo +stable fmt --all -- --check
./script/check-todos
./script/check-keymaps
./script/clippy
```

按改动范围运行主 package、paths、client、collab、CLI、extension API 和新增迁移
测试。记录实际 package 名和完整命令。

如果完整编译因首次 Git 依赖下载、网络或工具链失败，记录环境和错误；不能把
“命令启动过”写成 PASS。

### 4. 运行功能回归

使用本地临时目录/临时服务，不能使用用户生产数据：

- 新装启动和应用身份。
- 旧目录升级迁移。
- 迁移中断后恢复。
- CLI、打开文件/目录、行列号、URL scheme。
- workspace、LSP、扩展、AI 和远程基础流程。
- 登录/服务错误和离线行为。
- collab healthz、RPC/WebSocket 或 mock。
- macOS/Windows/Linux 可用的安装和升级 smoke。
- 上一版本 artifact 回滚。

每项写 PASS、FAIL 或 SKIP 和证据；SKIP 必须有环境理由。

### 5. 生成最终 Gate 报告

报告必须有：

- 基线和工作树边界。
- S01-S10 状态表。
- 目标验收项 A1-A9 的证据。
- 品牌残留 allowlist。
- 构建/测试/安装/服务/回滚结果。
- 许可证、商标、隐私、secret、数据和发布风险。
- 明确结论：GO、GO WITH CONDITIONS 或 NO-GO。
- 条件、责任人和下一步。

建议状态表：

| Gate | 条件                     | 证据    | 状态 |
| ---- | ------------------------ | ------- | ---- |
| G0   | 产品和身份契约批准       | S02     |      |
| G1   | 运行时路径和迁移安全     | S03/S04 |      |
| G2   | package/CLI/API 可用     | S05/S06 |      |
| G3   | UI/文档/资源无未解释残留 | S07     |      |
| G4   | 客户端/服务端契约一致    | S08/S09 |      |
| G5   | 平台安装和 CI 可发布     | S10     |      |
| G6   | 回归、扫描、回滚证据完整 | 本报告  |      |

### 6. 发布边界

本子计划不得执行：

- git commit、push、tag、release、publish、PR。
- 真实签名、生产部署、迁移生产数据库。
- 删除旧兼容入口。
- 修改计划外文件来“清零”扫描。

只有用户另行授权、S11 为 GO、测试和 CR 通过后，才可以进入发布流程。

## 验收标准

- 每个 S01-S10 都有真实交接证据。
- 所有必需测试有 PASS；SKIP 和 FAIL 有明确原因。
- 活动规范路径、默认 endpoint、安装身份和用户可见 UI 无未解释 Zed 残留。
- allowlist 逐文件、逐原因记录。
- 回滚至少完成一次本地演练或有同等 artifact/日志证据。
- 最终结论不是由模型主观判断，而是由证据表得出。

## NO-GO 条件

- 用户数据迁移未验证。
- 有默认旧服务 endpoint 或未批准 secret。
- 许可证/商标/隐私责任未批准。
- 任一平台安装会覆盖或丢失数据。
- CI/release 静默跳过或无法重现。
- 关键测试失败且没有隔离原因。
- 只完成字符串替换，没有运行时、协议和安装证据。

## 回滚

S11 本身默认不改源码。若只生成报告，回滚就是删除本次新生成的 evidence
报告，但不要删除已有 evidence 或用户文件。若获准更新测试 fixture，回滚只针对
该 fixture 并重新运行对应测试。

## 交接

返回：

- 最终 evidence 文件路径。
- S01-S10 的状态和证据。
- GO/GO WITH CONDITIONS/NO-GO。
- 所有阻塞项、责任人和下一步。
- 是否满足“可以交给发布负责人”的条件。
