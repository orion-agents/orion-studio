# S05：主 package、binary 和默认构建入口

## 任务目标

把当前主产品 package 和默认 binary 迁移到 Orion 规范，同时保持 workspace
可解析、默认构建入口明确、改动可回滚。本子计划不迁移所有 crate，不改平台安装资源。

## 前置条件

- S02、S03、S04 DONE。
- 已批准 Rust package 名、binary 名、legacy CLI/binary alias 和 workspace 迁移策略。
- 已确认是否允许保留 package 名 zed 作为过渡；没有批准时 BLOCKED。
- 当前工作树没有其他 manifest 改动。

## 允许修改

- Cargo.toml：只修改 workspace default-members 或明确的主 package 引用。
- crates/zed/Cargo.toml：只修改 package、default-run、binary 和直接相关 metadata。
- crates/zed/build.rs：只修改与 package/binary identity 直接相关的逻辑。
- crates/zed/src/main.rs：只修改 binary identity 直接相关的启动入口。
- crates/zed/\*\* 中与上述变更直接相关的测试。

禁止在本子计划：

- 批量重命名 crates/\*。
- 修改 crates/cli、client、collab、resources、release workflow。
- 重新设计 workspace 依赖。
- 无理由更新 Cargo.lock 或升级依赖。
- 直接删除 zed 兼容入口。

## 执行步骤

### 1. 建立 manifest 依赖图

运行：

```text
cargo +stable metadata --no-deps --format-version 1
cargo +stable metadata --no-deps --format-version 1 | \
  rg '"name"|"default-members"|"targets"|"kind"'
```

记录：

- 当前主 package 名。
- default-run 和 binary target。
- workspace default-members。
- 哪些 package、测试和脚本硬编码主 binary。
- 哪些兼容 alias 是 S06 或 S10 的责任。

### 2. 修改 package identity

按照 S02 的最终值，最小范围更新：

- package name/description/author 中的规范值。
- default-run。
- 主 binary target。
- 直接用于启动诊断的 product name。
- root workspace default-members 和引用主 package 的最小配置。

保持：

- edition、license、依赖版本、feature 语义和 build profile 不变。
- 现有 binary 参数和退出码不变。
- build script 的生成文件路径、资源输入和错误处理不变。

如果 Cargo package rename 导致超过本子计划范围的引用错误，停止并列出
错误文件；不要继续对整个仓库执行替换。

### 3. 处理兼容 alias

只有 S02 明确要求时才增加 legacy binary/command alias。alias 必须：

- 调用同一实现，不复制启动逻辑。
- 在帮助和日志中明确显示规范名称。
- 有测试证明参数、退出码和路径行为一致。
- 标记移除版本或兼容截止条件。

如果平台 alias 属于安装器或 PATH wrapper，移交 S10，不要在这里实现。

### 4. 验证启动链

读取并确认启动链仍然有效：

- main 初始化 GPUI。
- 全局状态、workspace、project 和窗口初始化顺序不变。
- 失败仍通过现有错误路径报告。
- 默认 workspace、恢复 session 和 headless/测试入口没有被误改。

只修复与身份改动直接相关的编译错误。

## 验收命令

```text
git diff --check
cargo +stable metadata --no-deps --format-version 1
cargo +stable fmt --all -- --check
cargo +stable check -p <最终主 package> --bin <最终 binary>
```

将尖括号替换为 S02 的最终值。如果首次拉取 Git 依赖失败，记录完整错误和
环境，不修改依赖源。能运行时再执行：

```text
cargo +stable test -p <最终主 package>
```

## 验收标准

- workspace metadata 可以解析。
- 默认 package/binary 与 S02 一致。
- 主启动链的行为和错误路径没有非目标变化。
- legacy alias 只有在合同批准后存在。
- diff 不包含全仓 package 或无关依赖改动。
- 构建失败和网络限制被诚实记录。

## BLOCKED 条件

- package/binary 最终值未批准。
- 修改 package 名会要求同时改写大量外部 crate、扩展或发布脚本。
- 需要改变 public API、协议或数据目录。
- 无法区分规范入口和兼容 alias。
- 构建失败疑似由本子计划引入，但无法定位。

## 回滚

保留 package rename 前后两个 metadata 结果。回滚只针对本子计划 diff，
确认 default-members、default-run 和 binary target 恢复；不删除 Cargo.lock、
构建缓存或用户文件。

## 交接

返回：

- 修改的 manifest 和启动文件。
- package/binary 前后值。
- metadata、check、test 的真实结果。
- 仍需 S06/S10 处理的引用。
