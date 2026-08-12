# S06：CLI、URL scheme、扩展 API 和协议 namespace

## 任务目标

迁移用户通过命令行、URL scheme 和扩展 API 进入 Orion 的入口，同时保持旧入口在
兼容窗口内可解释、可测试。此子计划不改服务 endpoint、不改数据库和安装器。

## 前置条件

- S02 已批准 CLI、URL scheme、扩展 API namespace 和兼容窗口。
- S05 DONE，主 binary/package 可由 metadata 解析。
- S04 已明确旧配置和项目路径如何传给 CLI。
- 没有真实外部服务或真实扩展 registry 依赖。

## 允许修改

- crates/cli/**
- crates/extension_api/**
- extensions/test-extension/**
- 与上述入口直接相关的测试和 fixture。

如编译错误涉及主 binary，只能记录并交回 S05；不要修改 S05 的文件。

## 执行步骤

### 1. 盘点现有入口

先读：

~~~text
crates/cli/src/main.rs
crates/cli/Cargo.toml
crates/extension_api/src/extension_api.rs
extensions/test-extension/
~~~

搜索并分类：

~~~text
rg -n 'zed://|zed::|Zed|zed|binary|command|WIT|namespace|extension' \
  crates/cli crates/extension_api extensions/test-extension
~~~

记录参数、退出码、打开文件/目录、行列号、stdin/stdout、错误输出和协议版本。

### 2. 迁移 CLI

实现规范 CLI：

- 新命令名和 help 使用 S02 的最终值。
- 打开文件、目录、行列号、等待、复用已有窗口等语义不变。
- 显式参数优先级、退出码和错误输出保持兼容。
- 旧命令是否作为 shim 只能按合同实现；shim 不复制核心逻辑。
- 新 CLI 不自动连接未批准服务，不读取未批准的 secret。

### 3. 迁移 URL scheme

实现：

- Orion scheme 的解析、注册输入和参数映射。
- 旧 zed:// 在兼容窗口内按合同解析、转换或给出可操作提示。
- 不信任 URL 中的路径、参数和外部命令；沿用现有安全校验。
- 保持文件、行列、workspace、callback 等现有语义。
- 不在本子计划修改 macOS/Windows/Linux 注册文件，这些交给 S10。

### 4. 迁移扩展 API namespace

按照 S02 的 API 版本和 namespace 策略：

- 新扩展文档、示例和测试使用 Orion 规范 namespace。
- 旧 namespace 只保留明确的兼容 decoder/alias。
- 不破坏旧扩展加载所需的 ABI/WIT 数据类型。
- test-extension 同时覆盖规范入口和兼容入口（如果合同要求）。
- 不把“文本中的 Zed”与实际 namespace 混为一谈；第三方 attribution 交 S07。

### 5. 增加契约测试

至少覆盖：

- 新 CLI 启动帮助和版本。
- 打开文件/目录以及行列号。
- 非法参数、非法 URL 和权限错误。
- 新 scheme 和旧 scheme 的兼容行为。
- 规范 namespace 的扩展加载。
- 旧 namespace/旧 CLI alias 的期限和弃用诊断。
- 失败时错误到达 UI/调用层，不静默成功。

## 验收命令

~~~text
git diff --check
cargo +stable fmt --all -- --check
cargo +stable metadata --no-deps --format-version 1
cargo +stable test -p cli
cargo +stable test -p extension_api
cargo +stable test -p test-extension
~~~

如果 package 名不同，先从 cargo metadata 获取实际 package 名；不要猜测。
在可用环境运行主 binary 的 CLI smoke：

~~~text
cargo +stable run -p <最终主 package> --bin <最终 binary> -- --help
cargo +stable run -p <最终主 package> --bin <最终 binary> -- --version
~~~

## 验收标准

- 规范 CLI、scheme 和 API namespace 全部来自 S02。
- 旧入口的行为、兼容期限和错误提示有测试。
- 不改平台注册文件、不改服务地址、不改 wire format。
- 旧 API 不会无提示地被当作新 API。
- 所有失败命令和环境限制有真实记录。

## BLOCKED 条件

- URL scheme、CLI alias 或 API namespace 未批准。
- 需要改平台注册文件、服务端协议或数据库 schema。
- 需要破坏现有扩展 ABI/WIT。
- 测试只能连接真实 registry 或使用真实 token。

## 回滚

回滚只涉及 CLI/API 目录和测试。回滚后验证旧 CLI、旧 scheme 和旧 namespace
仍可由原始基线测试解析；不删除用户注册表、应用目录或扩展目录。

## 交接

返回：

- 新入口与旧入口的行为差异。
- 兼容测试结果和期限。
- 主 binary smoke 结果。
- S07/S10 仍需处理的文案、资源和平台注册项。

