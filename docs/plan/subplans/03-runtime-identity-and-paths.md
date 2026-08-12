# S03：运行时身份、路径、环境变量和版本显示

## 任务目标

把 Orion 的规范运行时身份集中到现有 paths、environment variables 和 release
channel 代码中。此子计划不处理用户数据迁移、不改平台安装器、不改服务端点。

## 前置条件

- S01 DONE。
- S02 的身份契约已批准；没有最终目录、环境变量前缀和版本显示名时 BLOCKED。
- 工作树中没有其他 agent 正在修改本子计划的文件。

## 允许修改

主要范围：

- crates/paths/**
- crates/zed_env_vars/**
- crates/release_channel/**

可以修改这些目录中的测试和必要的 fixture。只允许修改下列范围外的文件，
如果编译错误明确需要它，必须先停止并报告：

- Cargo.toml/Cargo.lock
- crates/zed/Cargo.toml
- crates/cli/**
- crates/client/**
- crates/zed/resources/**

## 执行步骤

### 1. 建立现状映射

先读：

~~~text
crates/paths/src/paths.rs
crates/zed_env_vars/src/zed_env_vars.rs
crates/release_channel/src/lib.rs
~~~

搜索：

~~~text
rg -n 'APP_NAME|ZED_|Zed|zed|release|channel|log|config|cache|state' \
  crates/paths crates/zed_env_vars crates/release_channel
~~~

把每个命中分成规范值、兼容值、第三方归属或待决，不直接替换。

### 2. 迁移 paths 的规范来源

按身份契约更新现有 APP_NAME 及其派生逻辑：

- 新用户的 config、data、cache、state、log、temp 和 extension 目录使用 Orion。
- 目录派生只从现有统一常量或函数获得，不在调用方复制字符串。
- 日志文件名、远程相关路径和诊断输出的规范值使用 Orion。
- 旧目录名称保留为 S04 的迁移输入；不要在 S03 删除旧目录，也不要在这里实现
  复制、rename 或用户确认流程。
- 保留现有平台差异、权限和大小写规则。

如果现有 API 同时承担“当前路径”和“旧路径探测”，先增加清晰的语义边界；
不要把旧路径直接改成新路径导致 S04 无法发现旧数据。

### 3. 迁移环境变量规范

按照 S02 的最终前缀：

- 新代码和新文档使用 Orion 前缀。
- 旧 ZED_* 名称只作为兼容读取入口，不能被新配置写回。
- 每个兼容读取入口必须有注释或测试说明其移除条件；不要让旧名散落在业务代码。
- 处理环境变量时保持现有优先级：命令行/显式配置、规范环境变量、兼容环境变量、
  默认值的优先顺序不得无意改变。
- 不记录环境变量的 secret 值。

### 4. 迁移 release channel 的运行时显示

只处理 release_channel 中的规范运行时身份：

- 应用显示名称、版本显示和文档基础 URL 使用 S02 的最终值。
- 保留发布通道语义，不能为了换名改变 stable/preview/nightly/dev 的行为。
- 平台 bundle metadata、安装器和 URL scheme 留给 S10。
- 如果同一个常量同时被 S10 使用，先只改其运行时来源并在交接中标出冲突。

### 5. 增加小范围测试

优先沿用已有测试风格，至少覆盖：

- 新路径在 macOS/Linux/Windows 条件下的规范结果。
- 旧目录常量仍能被 S04 探测。
- 规范环境变量优先于旧环境变量。
- 旧环境变量只触发兼容读取，不被写回。
- 各 release channel 的显示名称和版本格式没有改变。

## 验收命令

~~~text
git diff --check
cargo +stable metadata --no-deps --format-version 1
cargo +stable fmt --all -- --check
cargo +stable test -p paths
cargo +stable test -p zed_env_vars
cargo +stable test -p release_channel
~~~

如果某个 package 没有测试 target，运行对应的 cargo check，并在交接中写明原因。
最后确认：

~~~text
git diff --name-only
git status --short --branch
~~~

## 验收标准

- 规范路径、环境变量和运行时显示名来自统一来源。
- 新路径不再默认为 Zed；旧路径仍可被后续迁移逻辑探测。
- 没有改变发布通道、权限、平台路径规则或错误传播行为。
- 旧环境变量没有被静默删除，也没有写回新配置。
- 修改文件只在本子计划范围。
- 所有实际运行的测试和失败命令都有记录。

## 禁止事项

- 不实现 S04 的数据复制、删除、恢复或用户确认。
- 不改 crates/cli、服务 URL、协议 scheme、安装器或 UI 文案。
- 不重命名整个 workspace。
- 不写入真实 endpoint、secret 或个人路径。
- 不用全局替换删除 Zed 字符串。

## 回滚

只回滚本子计划新增的 diff。回滚后必须确认：

- 旧路径函数仍可编译。
- 旧环境变量仍可读取。
- 未删除任何用户目录。
- S01 evidence 和 docs/plan 内容未被改变。

## 交接

说明：

- 哪些规范常量已改。
- 哪些旧常量作为兼容输入保留。
- 哪些测试通过/跳过。
- S04 是否可以开始。

