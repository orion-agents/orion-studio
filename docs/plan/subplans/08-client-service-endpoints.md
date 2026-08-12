# S08：客户端服务、URL、登录、更新和 RPC 配置

## 任务目标

把客户端侧的服务 URL、账户、登录、更新、遥测、云端和 RPC/WebSocket 配置迁移为
Orion 契约。不得让客户端在活动默认路径上继续依赖未经批准的 Zed hosted service。

## 前置条件

- S02 已批准所有服务 endpoint、认证、隐私、数据驻留、离线和自托管策略。
- S03 DONE，规范环境变量和运行时身份可读取。
- S06 DONE，CLI/协议入口已经明确。
- 不需要真实 token、生产账号或外部服务才能运行测试。

## 允许修改

- crates/client/**
- crates/cloud_api_client/**
- 与客户端服务配置直接相关的测试和 mock。
- 如 S01 明确列出其他 URL builder 文件，只能在报告中申请后修改。

禁止修改：

- crates/collab/**、crates/remote_server/**
- crates/zed/resources/**、安装器、workflow。
- 认证数据库、生产 secret、法律条款和计费实现。
- RPC wire format 和数据库 schema。

## 执行步骤

### 1. 盘点 URL 来源

先读：

~~~text
crates/client/src/client.rs
crates/client/src/zed_urls.rs
crates/cloud_api_client/
~~~

搜索：

~~~text
rg -n -i 'ZED_|zed\.dev|cloud\.zed|collab\.zed|rpc|websocket|login|update|telemetry|crash' \
  crates/client crates/cloud_api_client
~~~

按来源分类：

- 命令行/环境变量覆盖。
- 本地配置默认值。
- URL builder。
- 认证/登录。
- 更新/崩溃/遥测。
- cloud/collab/RPC。
- 测试 mock 或历史 fixture。

### 2. 建立单一服务配置来源

按 S02：

- 规范 endpoint 由统一配置/URL builder 产生。
- 环境变量覆盖优先级保持不变。
- 旧 ZED_* 只读兼容并产生可测试的弃用诊断。
- 生产 URL、测试 URL、localhost 和 staging 必须明确区分。
- 默认配置不含 secret；日志不能打印 token、cookie、授权 header 或完整用户 URL。
- 离线/自托管模式不应该静默回退到 Zed 服务。

不要凭空创建域名。如果合同没有给出某个 URL，标记 BLOCKED。

### 3. 迁移 URL builder 和客户端请求

逐个更新：

- 文档和帮助 URL。
- 登录/OAuth redirect 和账户 URL。
- 更新 manifest、artifact 和 release channel URL。
- crash/telemetry DSN 或开关。
- cloud API、collab API、RPC/WebSocket URL。
- websocket scheme、redirect、认证 header 和错误处理。

保持：

- 超时、重试、取消和状态机语义。
- HTTPS/WSS 安全要求。
- 认证失败、网络错误、服务错误到 UI 的传播。
- 测试环境不调用真实 endpoint。

### 4. 添加 mock/契约测试

至少覆盖：

- 默认 endpoint 来源是 Orion。
- 显式配置覆盖默认值。
- 规范环境变量优先于旧环境变量。
- 没有凭据时不会发送授权请求。
- HTTP 到 WSS 的转换只在允许的 URL 上发生。
- 旧 endpoint 不会成为隐式 fallback。
- 认证失败、重试耗尽、超时和服务 5xx 返回可理解错误。
- URL 中不泄露 secret、个人路径或 token。

### 5. 做网络隔离验证

如果测试框架允许，使用本地 mock server 或请求拦截器：

- 记录请求 host、scheme、path 和是否带凭据。
- 禁止测试访问真实 zed.dev、cloud.zed.dev、collab.zed.dev。
- 发现任何真实网络访问时立即 BLOCKED，不要放宽限制。

## 验收命令

~~~text
git diff --check
cargo +stable fmt --all -- --check
cargo +stable metadata --no-deps --format-version 1
cargo +stable test -p client
cargo +stable test -p cloud_api_client
~~~

按实际 package 名补充 URL、登录或 RPC 专用测试。最终检查：

~~~text
rg -n -i 'zed\.dev|cloud\.zed|collab\.zed|ZED_' \
  crates/client crates/cloud_api_client
~~~

剩余命中必须属于兼容、测试、法律或上游归属，并在交接中列出。

## 验收标准

- 活动默认 endpoint 全部来自 S02 的 Orion 契约。
- 旧环境变量只作为兼容读取，不改变规范配置优先级。
- 没有真实 secret、真实账户或未批准服务请求。
- 网络错误、认证错误和服务错误都能到达调用层/UI。
- RPC/WebSocket 的 wire format 没有无计划改变。
- mock/契约测试实际运行并记录结果。

## BLOCKED 条件

- 某个服务 endpoint、OAuth redirect、更新源或遥测归属未批准。
- 需要修改 collab/remote server 才能完成客户端编译。
- 测试依赖生产凭据或无法阻断真实网络。
- 发现旧 endpoint 是协议兼容必需项，但合同没有允许它。

## 回滚

回滚客户端 URL/config diff，保留测试 mock 和 evidence。回滚后确认旧客户端行为
仍可在本地测试中运行；不恢复到未经批准的生产服务作为临时方案。

## 交接

返回：

- URL/环境变量/认证来源前后对照。
- mock 网络请求记录。
- 所有剩余旧 endpoint 命中及理由。
- S09 需要的服务端契约和字段。

