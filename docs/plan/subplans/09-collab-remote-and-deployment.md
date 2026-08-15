# S09：collab、远程 server 和部署配置

## 任务目标

迁移 collab、remote server 和本地部署配置，使服务端规范身份、健康检查、认证、
数据库、对象存储、LiveKit、RPC 和日志契约与 Orion 一致。此子计划不发布生产服务。

## 前置条件

- S02 已批准服务拥有者、许可证、部署环境、数据驻留、认证和回滚策略。
- S08 已提供客户端调用的 endpoint、请求版本和错误契约。
- 已确认当前仓库中的 collab crate 不等于完整商业 SaaS；缺少 billing、SLA、
  监控或灾备时必须明确列为未完成。
- 测试环境有可用的本地依赖或 mock；没有时只能做静态验证。

## 允许修改

- crates/collab/\*\*
- crates/remote_server/\*\*
- Dockerfile-collab
- S01 清单中明确属于 collab/remote 的本地 Docker、Kubernetes、compose 或 Procfile。
- 对应健康检查、配置测试和部署文档。

禁止修改：

- 生产数据库、生产 secret、真实对象存储或 LiveKit。
- 客户端 URL builder、安装器、发布 workflow。
- 未批准的计费、订阅、SLA 或用户账户体系。
- 许可证文件和法律文本。

## 执行步骤

### 1. 盘点服务配置

先读：

```text
crates/collab/Cargo.toml
crates/collab/src/lib.rs
crates/collab/src/main.rs
crates/collab/README.md
Dockerfile-collab
```

搜索：

```text
rg -n -i 'zed|ZED_|environment|url|host|port|database|postgres|s3|livekit|rpc|health' \
  crates/collab crates/remote_server Dockerfile-collab
```

建立配置表：

- 配置键/环境变量。
- 默认值和测试值。
- 是否含 secret。
- 客户端对应。
- 数据类型和数据驻留。
- 健康检查或依赖。
- 迁移/回滚策略。

### 2. 迁移规范服务身份

- 新配置键使用 S02 的规范命名。
- 旧键只按兼容契约读取并告警，不写回。
- zed_environment、cloud URL、内部 API key、checksum seed 等字段按实际用途
  逐一确认，不做机械改名。
- 不把 localhost、staging 和 production 混为一类。
- 日志、healthz、metrics 和错误中使用 Orion 规范名，但不输出 secret。
- 服务启动失败必须返回可定位错误，不能吞掉数据库、对象存储或 LiveKit 错误。

### 3. 保持服务契约兼容

- RPC/WebSocket/API 版本必须与 S08 的客户端契约一致。
- 旧协议需要双读、版本协商或明确迁移；不得只改字符串导致旧客户端崩溃。
- 数据库 schema 变更必须有 forward/rollback 说明和本地迁移测试。
- 不擅自新增 billing、subscription 或 SaaS 运营能力。
- extension route、healthz、静态资源和 CORS/auth 行为保持原有安全约束。

### 4. 做本地部署 smoke

使用仓库已有的 Docker/服务测试方式，不创建生产资源：

- 构建 collab image 或运行最小服务。
- 检查配置校验、启动、healthz、依赖不可用时的错误。
- 如有数据库，使用临时数据库并验证迁移/回滚。
- 如有对象存储/LiveKit，使用 mock 或明确的本地服务；缺少依赖就标记 SKIP。
- 检查容器日志、端口、非 root/权限和 secret 注入方式。

### 5. 部署文件检查

- Dockerfile、Kubernetes、compose、Procfile 的镜像、包名、环境变量和健康检查
  与 S02 一致。
- 不把未批准的旧服务地址留作默认。
- 不提交真实 secret、证书、数据库 URL 或生产域名。
- 部署文件中的上游 attribution/许可证保留。

## 验收命令

```text
git diff --check
cargo +stable fmt --all -- --check
cargo +stable metadata --no-deps --format-version 1
cargo +stable test -p collab
cargo +stable check -p remote_server
```

如果本地服务依赖可用，执行仓库已有的健康检查和 integration test。记录：

- 实际启动命令。
- 依赖版本。
- 健康检查响应。
- 失败日志。
- 是否触碰网络或真实凭据。

静态残留扫描：

```text
rg -n -i 'zed\.dev|collab\.zed|ZED_|zed_environment|zed_cloud' \
  crates/collab crates/remote_server Dockerfile-collab
```

## 验收标准

- 服务配置和客户端契约一致。
- healthz、启动错误、数据库/依赖错误可观察。
- 本地 smoke 通过，或每个 SKIP 有环境原因。
- 没有真实生产资源、secret 或未经批准的 SaaS 功能。
- 许可证实际值与研究文档不一致之处已保留为法务风险，而不是模型自行修正。

## BLOCKED 条件

- 许可证、数据驻留、认证或生产 endpoint 未批准。
- 需要改变数据库 schema 但没有 migration/rollback。
- 服务只能通过真实生产凭据验证。
- 客户端/服务端协议版本不一致且需要跨子计划改动。
- 发现当前 collab 代码不足以支持计划承诺的 SaaS 功能。

## 回滚

本地服务只清理临时容器、临时数据库和临时目录。代码回滚只回滚本子计划 diff。
生产回滚必须由发布责任人按备份和 schema 策略执行，本子计划不得操作。

## 交接

返回：

- 配置矩阵和 endpoint 结果。
- healthz/startup/integration 证据。
- 数据库/对象存储/LiveKit 的实际验证状态。
- 未完成的 SaaS、运维和安全责任。
