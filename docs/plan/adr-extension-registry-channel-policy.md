# ADR: Extension Registry 的 Release Channel 策略

- 状态：Accepted（维持现状）
- 日期：2026-09-08
- 关联：V2-06（服务端点迁移，BLOCKED）、PR #5（fail-closed 端点原则）、PR #7

## 背景

Orion Studio fork 自 Zed。upstream Zed 的 extension registry（zed.dev 生态）对**所有** release channel 开放——upstream 的 `release_channel` 没有任何 registry 概念。

fork 在 rebrand 过程中为 extension registry 引入了 channel 开关（`release_channel::extension_registry_available`，upstream 无此方法）：

```rust
pub fn extension_registry_available(&self) -> bool {
    matches!(self, ReleaseChannel::Nightly | ReleaseChannel::Stable)
}
```

其连锁行为（均有测试锁定，`extension_store_test.rs` 的 fail-closed 断言系列）：
- Dev / Preview 构建：菜单显示 "Local Extensions"；registry 的 search / version / update / download 网络请求全部拒绝；Extensions 页显示内置提示文案。
- 本地扩展（`extensions/` 目录的 dev extension、手动安装）**不受影响**，任何 channel 均可用。

## 决策

**维持现状**：Dev 与 Preview 构建不连接 extension registry（fail-closed）；Stable 与 Nightly 开放。

## 理由

1. **端点归属未定**：fork 自有的 extension registry 端点尚未建设（依赖 orion.dev 基础设施，V2-06 范畴）。若现在放开 Dev/Preview，唯一可连的是上游 zed.dev 生产 API——让开发版依赖并打生产服务，与 PR #5 确立的 "production endpoints remain fail closed until frozen and reviewed" 原则冲突。
2. **开发版用途优先级**：Dev 版定位是本地开发验证；三方插件验证可由 Stable/Nightly 构建（`./script/bundle-mac`，release profile）或本地 dev extension 覆盖。
3. **上游归因与解耦**：fork 长期目标是自有 registry 生态；现在放开只会加深对上游端点的隐性依赖，将来切换成本更高。

## 后果

- Dev/Preview 用户无法浏览/安装在线三方插件（UI 已有明确提示文案）。
- 解封条件（满足其一即可重新评估）：
  1. orion 自有 registry 端点上线并冻结契约；
  2. 产品决策明确采用上游 zed.dev registry 生态作为过渡。
- 解封所需改动：`release_channel::extension_registry_available` 判定加入目标 channel + 同步更新 `extension_store_test.rs` 中 4+ 处 fail-closed 断言（`dev_extension_registry_network_boundaries_fail_closed` 等）+ Extensions 页文案。
- 本扩展开发工作流不变：`extensions/` 下 dev extension 的加载、热重载在所有 channel 可用。
