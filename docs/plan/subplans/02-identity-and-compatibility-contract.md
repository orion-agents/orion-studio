# S02：Orion 身份、服务和兼容契约

## 任务目标

把 S01 的发现转化为一个可供后续子计划读取的“唯一命名合同”。
本子计划不修改源码；没有人类批准时，后续实现子计划必须停止。

## 前置条件

- S01 的 evidence 报告存在。
- 用户或产品负责人已经确认 Orion Studio 的首个产品范围。
- 法务/责任人至少确认许可证和上游 attribution 的处理原则。
- 不要求模型自行决定域名、App ID、scheme、CLI 兼容或商业服务边界。

## 允许写入

只允许新增或更新：

- docs/plan/evidence/S02-identity-and-compatibility-contract.md

禁止修改源码、Cargo manifest、CI、README、docs/research 或主计划。

## 必须填写的契约字段

报告中必须有一张最终值表，不能只写“以后决定”：

| 类别 | 最终值 | 旧值 | 兼容策略 | 责任人/批准证据 |
| --- | --- | --- | --- | --- |
| 展示名称 |  | Zed |  |  |
| repo/product slug |  | zed |  |  |
| Rust package/crate |  | zed |  |  |
| 主 binary |  | zed |  |  |
| CLI 命令与别名 |  | zed |  |  |
| 环境变量前缀 |  | ZED_ |  |  |
| 配置/缓存/日志目录 |  | Zed/zed |  |  |
| 远程 server 目录 |  | .zed_server |  |  |
| URL scheme |  | zed:// |  |  |
| macOS Bundle ID |  | dev.zed.* |  |  |
| Windows App/installer ID |  | Zed |  |  |
| Linux desktop/Flatpak ID |  | zed/zed-editor |  |  |
| 文档/官网 endpoint |  | zed.dev |  |  |
| cloud/collab endpoint |  | cloud.zed.dev/collab.zed.dev |  |  |
| update/crash/telemetry endpoint |  | Zed service |  |  |
| extension registry/API namespace |  | Zed |  |  |
| 配置、协议、数据库兼容窗口 |  | N/A |  |  |

## 执行步骤

1. 阅读 S01 evidence，并把每个 P0 命中映射到契约行。
2. 把总计划中的建议值和真正批准值分开；建议值不得当作最终值。
3. 对每个旧值写清楚：
   - 旧数据是否读取。
   - 旧数据是否复制或迁移。
   - 旧入口是否只读。
   - 旧入口何时告警、何时移除。
   - 失败时用户看到什么。
4. 对服务写清楚所有权、认证方式、数据类型、数据驻留、默认是否启用和离线行为。
5. 对许可证写清楚实际 SPDX 标识、适用 crate、第三方依赖、商标和 attribution。
6. 在文档底部添加人类审批区：

~~~text
产品范围批准：
身份矩阵批准：
服务端点批准：
许可证/商标批准：
兼容窗口批准：
批准人：
日期：
决策记录链接：
~~~

## 阻塞规则

下面任意一项没有最终值，报告必须标记 BLOCKED：

- 主 binary 或 CLI 名称。
- 配置/状态目录和迁移窗口。
- URL scheme。
- Bundle/App/Flatpak ID。
- 活动服务 endpoint。
- 许可证/商标边界。
- 旧协议、旧扩展 API 或旧数据库的兼容策略。

模型不得使用“先随便用一个域名”“先用 com.orion”“先删除旧入口”
来绕过阻塞。

## 验收标准

- 契约表每一行都有最终值、旧值、兼容策略和批准责任。
- S03 至 S10 可以只读本文件而不需要猜测命名。
- 合同明确区分规范值、兼容值、法律/上游保留值。
- 没有服务 secret、token、个人数据或未批准生产信息进入文件。
- 若没有批准，状态为 BLOCKED，而不是 DONE。

## 交接

返回：

- 契约文件路径。
- 哪些字段已批准，哪些字段 BLOCKED。
- 后续子计划可以开始到哪一项。
- 未决问题和需要谁批准。

