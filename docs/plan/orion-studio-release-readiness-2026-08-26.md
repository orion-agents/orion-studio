# Orion Studio 准出发布计划与审计

## 2026-08-27 Local Init Runtime Closure

本地 Init 已在
`release/orion-studio-v1.16.1-pre@5bf21d2707eb95d35ce16c10ba864838a3963c9a`
重新完成验收：1.16.1 arm64 Dev App 构建、ad-hoc 签名、DMG 校验和安装通过；
`/Applications/Orion Studio Dev.app` 正常出现 CoreGraphics 可见窗口，能打开当前项目和
`README.md`，通过应用菜单退出后可重启恢复。System Events 的 0 窗口结果是
inaccessible GPUI 应用的探测假阴性，没有触发启动核心源码修改。

迁移测试 64/64、品牌门禁 6,066/6,066 和品牌脚本 3/3 均通过；旧数据保留、marker
权限为 600、无迁移临时目录，Orion LMDB 在首次启动和重启后均打开。构建缓存已用
`cargo clean` 回收 24.5 GiB，安装 App 和用户数据保留。详细证据见
`evidence/INIT-V2-V09-REGRESSION-GATE.md`。

这只把本地 Dev 交付维持为 **VERIFIED / LOCAL-ONLY**。Developer ID、Apple 公证、
受信任 runner、签名 tag、干净机器验收和公开 GitHub Release 仍未完成，因此公开
Preview 继续保持 **NO-GO / EXTERNAL-BLOCKED**。

## 1. 当前结论

本计划以 `main@bb9ec1fbd72ddf7765a6c7b61630304ae0e2788d` 为改造基线，首个公开
交付范围限定为 **macOS Apple Silicon Preview**。

截至 2026-08-26，结论必须分三层理解：

- **源码准出：GO。** Orion canonical identity、fail-closed 数据迁移、卸载安全、
  Preview/Dev 托管能力边界、品牌门禁、发布门禁和全量 Clippy 均已通过。
- **本地 Dev 交付：VERIFIED。** `Orion Studio Dev.app` 1.16.1 已完成 arm64 Release
  构建、ad-hoc 签名结构校验、安装和启动冒烟，仅供当前 Mac 本地使用。
- **公开 Preview：NO-GO / EXTERNAL-BLOCKED。** 未配置 Apple Developer ID、
  notarization 凭据、受信任专用 runner、签名 release tag 和签名产物的干净机器验收。
  本地 ad-hoc DMG 不得上传到 GitHub Release。

## 2. 准出目标

| ID | 目标 | 准出条件 |
| --- | --- | --- |
| T1 | 品牌身份 | 品牌扫描无未批准、过期或歧义命中；Zed 只存在于上游归因、许可证、兼容输入、ABI、内部兼容标识和测试夹具 |
| T2 | 运行时身份 | 新路径、CLI、URL scheme、bundle ID、凭据和服务标识以 Orion Studio 为 canonical identity |
| T3 | 数据迁移 | 首装、升级、幂等、失败保留和回滚测试通过；迁移在持久化初始化前完成；旧 Zed 数据不被删除 |
| T4 | 源码门禁 | format、品牌、密钥、entitlements、卸载、Preview 脚本、聚焦测试和全 workspace Clippy 可复现通过 |
| T5 | 构建资源 | Release 构建仅在受信任 arm64 runner 上运行；签出前至少 100 GiB、签出后至少 90 GiB 可用；结束总是清理 `target` |
| T6 | 签名公证 | Developer ID、hardened runtime、notarization、stapling、Gatekeeper 和 Team ID 校验全部通过 |
| T7 | 不可变发布 | tag、版本、channel、main ancestry 和 checkout SHA 一致；Draft 仅接收 DMG 与校验和 |
| T8 | GitHub 治理 | Preview environment 需要人工 reviewer；runner、variables、secrets、branch/ruleset 按发布范围配置 |
| T9 | 用户验收 | 在无开发工具和旧缓存的干净 Apple Silicon Mac 上完成下载、校验、安装、首次启动、迁移和回滚 |

## 3. 已完成的 P0/P1 改造

### P0：首启迁移 fail-closed

迁移已移动到关键持久化初始化之前。canonical `.orion` 文件优先，旧 `.zed` 仅做
只读回退；空 canonical 文件会遮蔽 legacy，损坏或冲突不会悄悄回退。复制失败不写
完成标记、保留 legacy、可重试，并有 52 个迁移测试与项目设置/任务/调试历史专项
回归覆盖。

### P0：卸载只处理 Orion 所有内容

卸载脚本不再删除 `.config/zed`、`.local/share/zed` 或无法证明归属的旧 CLI。
隔离 HOME 的 5 个 sentinel 测试证明旧 Zed 数据完整保留；只移除 Orion Studio
应用、数据和由 Orion 创建的兼容 launcher。

### P0：品牌门禁从机械替换升级为精确边界

用户可见品牌、Action 显示名、图标、设置和错误文案已切换为 Orion。扫描枚举
4,282 个候选、实际扫描 4,230 个文件；6,060 个保留命中均由 6,060 条精确规则
解释，`unapproved=0`、`stale=0`、`errors=0`。保留内容限于上游归因、许可证、
依赖坐标、兼容输入/ABI、内部 crate 名和测试夹具。

### P0：Preview/Dev 托管能力 fail-closed

Preview/Dev 不连接继承的账户、协作、云模型、编辑预测、遥测、扩展市场、自动更新
或远程服务器下载端点。第三方 OAuth DCR 和用户自行配置的 provider 保留；Orion CIMD
在无 Orion 服务时拒绝。扩展仅支持本地/开发安装，SSH 只允许精确匹配的缓存或手工
服务器。

### P0：发布链 fail-closed

workflow 校验 SSH 签名 tag、版本、Preview channel、main ancestry、不可变 checkout、
Developer ID 输入、notarization、Team ID、entitlements、Gatekeeper、stapler、架构、
最低系统版本、DMG 与 SHA-256。provisioning profile 当前可选；如提供则严格校验。
签出前磁盘门槛提高到 100 GiB、签出后 90 GiB，结束阶段无论成功失败均执行
`cargo clean`。

## 4. 2026-08-26 验证证据

| 门禁 | 结果 |
| --- | --- |
| `./script/test-orion-brand` | PASS：3/3 |
| `./script/check-orion-brand --max-findings 1000` | PASS：6,060/6,060 approved；0 unapproved、0 stale、0 errors |
| `./script/test-uninstall` | PASS：5/5 |
| `./script/test-preview-release` | PASS：20/20，包含签出前 100 GiB、签出后 90 GiB 阈值回归 |
| `./script/check-macos-entitlements` | PASS：3 个已批准 entitlement |
| `./script/check-secrets` | PASS：扫描 13 个 commit、约 74 MB，无泄漏 |
| `cargo fmt --all -- --check` / `git diff --check` | PASS |
| `./script/clippy` | PASS：workspace、release、all targets、all features、deny warnings |
| 迁移 / CLI / open listener / OAuth / hosted boundary / 扩展 / 项目设置测试 | PASS：详见 V2-09 evidence 文档 |
| workflow YAML、plist、XML | PASS：解析成功；生成 workflow 与源同步 |

## 5. 本地 arm64 Dev 产物实测

命令：

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 ./script/bundle-mac aarch64-apple-darwin
```

结果：

- 主 App Release 构建 58m52s；远程服务器构建 21m32s。
- `Orion Studio Dev.app`：版本 1.16.1、400 MiB、单一 arm64、
  `dev.orion.OrionStudio-Dev`；`codesign --verify --deep --strict` 通过。
- 本地 DMG：145 MiB，`hdiutil verify` 通过，SHA-256
  `ca6d492d977eeb8b81d8842dd452e42d6d681c421cab03aa42cd78dc899cc953`。
- 本地包仅为 ad-hoc 签名；`spctl` 拒绝符合预期，不能用于公开分发。
- 已安装到 `/Applications/Orion Studio Dev.app`；启动后检测到 1 个窗口，稳定态
  RSS 约 98 MiB，无新 crash report 或 panic/fatal 系统日志。
- 退出请求被用户取消后保留正在运行的应用，没有强制结束或触碰未保存状态。

## 6. 构建资源实测与清理策略

本轮在同一工作树先后执行测试、全量 Release Clippy 和 arm64 bundle，`target` 峰值
89 GiB：`target/debug` 41 GiB、宿主 `target/release` 8.3 GiB、
`target/aarch64-apple-darwin` 40 GiB。主 `rustc` Thin LTO 采样峰值约 3.0 GiB RSS；
16 GiB Apple Silicon 主机使用 `CARGO_BUILD_JOBS=2` 可稳定完成。

安装和冒烟后执行 `cargo clean`，删除 253,029 个可再生文件、回收 88.5 GiB；磁盘
可用空间从约 98 GiB 增至 186 GiB。源码、Git 历史、用户配置和已安装 App 均未删除。

发布/日常规则：

1. 重型构建统一 `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0`。
2. 同一时间只运行一个 Cargo build/test/clippy 链。
3. 不在 release runner 长期保留 workspace `target`；成功或失败都清理。
4. 公发 runner 签出前至少 100 GiB、签出后至少 90 GiB 可用。

## 7. Completion Audit

| 目标 | 状态 | 阻塞或证据 |
| --- | --- | --- |
| T1 品牌身份 | PASS | 6,060/6,060 精确批准，0 未批准/过期/错误 |
| T2 运行时身份 | PASS | Orion canonical 路径、CLI、URL、socket、bundle、文案与图标；legacy 仅只读兼容 |
| T3 数据迁移 | PASS | pre-init fail-closed、legacy 保留、幂等/冲突/失败专项回归通过 |
| T4 源码门禁 | PASS | 静态门禁、专项测试和完整 `./script/clippy` 通过 |
| T5 构建资源 | CODE-READY | 本地实测完成；workflow 阈值/2 jobs/always-clean 已落实；专用 runner 尚未配置 |
| T6 签名公证 | EXTERNAL-BLOCKED | 等待 Apple Developer ID 与 notarization 凭据 |
| T7 不可变发布 | CODE-READY | 脚本 fail-closed；尚未创建 1.16.1 Preview signed tag，当前 channel 仍为 Dev |
| T8 GitHub 治理 | PARTIAL | Preview reviewer 已有；专用 runner、Apple secrets/variables、ruleset 待配置 |
| T9 用户验收 | PARTIAL | 本地 Dev 安装/启动通过；签名 Draft 的干净 Mac 验收待 T6 |

源码和本地 Dev 交付已达到准出要求。公开 Preview 只有在 T5 外部配置、T6、T8、T9
全部 PASS 后才能从 Draft 发布。

## 8. 后续公开发布顺序

1. Apple Developer Program 审核完成后生成 Developer ID Application 证书和
   App Store Connect API key。
2. 配置受信任 arm64 runner、`preview` environment secrets/variables、reviewer 和
   release tag SSH allowed-signers。
3. 合并源码 PR；单独提交 `crates/zed/RELEASE_CHANNEL=preview`，并在 main 上创建
   与 1.16.1 匹配的 SSH 签名 annotated tag `v1.16.1-pre`。不得移动旧 tag。
4. 手动触发 `Release macOS Apple Silicon Preview (Bootstrap)`；只接受 workflow
   生成并验证的 Draft DMG 与 `SHA256SUMS`。
5. 在干净 Mac 完成 checksum、Developer ID、Gatekeeper、stapler、安装、首次启动、
   迁移、CLI、`orion://` 和卸载保留验证。
6. 验收记录齐全后人工公开 Pre-release；失败则保留 Draft，用新 patch 版本修复。

## 9. 依据

- [Preview 发布操作手册](../releases/preview-release-operations.md)
- [Preview 用户范围与限制](../releases/preview-macos-arm64.md)
- [V2-09 回归证据](evidence/INIT-V2-V09-REGRESSION-GATE.md)
- [Apple Developer ID](https://developer.apple.com/support/developer-id/)
- [GitHub self-hosted runner 标签](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/use-in-a-workflow)
