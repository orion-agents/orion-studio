# S10：跨平台打包、安装器、更新和 CI/CD

## 任务目标

把 macOS、Windows、Linux、Flatpak、artifact、安装器、更新源和 CI/CD 的发布身份
迁移为 Orion，并验证新装、升级、卸载、再安装和回滚。此子计划不修改业务逻辑。

## 前置条件

- S05、S06、S07、S08、S09 DONE。
- S02 已批准 Bundle/App/Flatpak ID、URL scheme、artifact slug、更新源、签名主体、
  发布仓库和兼容升级策略。
- 没有签名证书、发布 token 或真实 CI secret 时只做静态和本地验证。
- 当前平台工具链可用性已记录。

## 允许修改

- crates/zed/Cargo.toml 中的平台 bundle metadata 部分。
- crates/zed/resources/**
- script/bundle-*
- script/flatpak/**
- Windows installer、Linux desktop/Flatpak 元数据和 macOS bundle 配置。
- .github/workflows/run_bundling.yml
- .github/workflows/release.yml
- 与发布 identity 直接相关的 artifact/config 测试。

禁止修改：

- 主 package 逻辑、client/collab 业务逻辑。
- 发布 secret、证书、token、生产 release。
- 无关 workflow、依赖版本或 CI 重构。
- 删除旧版本 artifact 或破坏既有用户升级路径。

## 执行步骤

### 1. 建立平台矩阵

针对每个平台记录：

| 平台 | 展示名 | executable | App/Bundle ID | URL scheme | artifact | 更新源 |
| --- | --- | --- | --- | --- | --- | --- |
| macOS |  |  |  |  |  |  |
| Windows |  |  |  |  |  |  |
| Linux desktop |  |  |  |  |  |  |
| Flatpak |  |  |  |  |  |  |

每一格必须来自 S02；空值为 BLOCKED，不得自行补值。

### 2. 迁移 macOS

检查和更新：

- bundle display name、identifier、executable、URL scheme。
- dev/nightly/preview/stable 通道区别。
- entitlements、签名输入和 notarization 配置。
- artifact 名和更新 metadata。

保持现有权限、沙箱、签名和错误处理，不改业务功能。

### 3. 迁移 Windows

检查和更新：

- installer publisher、product name、executable。
- registry App ID、URL protocol、卸载项、安装目录。
- dev/nightly/preview/stable artifact。
- 更新/回滚文件名和签名输入。

不要手工删除用户注册表；安装器行为在临时环境验证。

### 4. 迁移 Linux/Flatpak

检查和更新：

- desktop Name、Exec、Icon、Keywords、MimeType。
- URL scheme handler。
- Flatpak app ID、command、module、install path、metainfo、截图和帮助链接。
- artifact/包名和通道。

验证 JSON/XML/desktop 文件语法，保留许可证和作者字段。

### 5. 迁移 CI/CD

逐个检查：

- workflow owner/repository 条件。
- artifact、release repository、cache、bucket、secret 名。
- ZED_* 构建环境变量。
- 签名、notarization、更新 manifest 和发布说明。
- 旧组织条件是否导致 Orion workflow 静默跳过。

不提交真实 secret；如果 secret 名称需要新建，写出清单并 BLOCKED。

### 6. 本地安装矩阵

在可用平台或 CI 产物上验证：

1. 全新安装。
2. 启动和显示身份。
3. URL scheme 注册。
4. 打开文件/目录。
5. 读取旧版本配置。
6. 升级后保留数据。
7. 卸载后行为。
8. 再安装。
9. 回滚到上一版本。

## 验收命令

~~~text
git diff --check
cargo +stable fmt --all -- --check
cargo +stable metadata --no-deps --format-version 1
./script/check-keymaps
~~~

按平台执行仓库已有的 bundle/installer 命令；不能用不存在的命令代替。
对所有产物记录：

- 生成命令和 commit。
- 文件名、hash、签名状态。
- 安装/启动日志或截图。
- 平台、OS、工具链版本。

静态扫描：

~~~text
rg -n -i 'Zed|zed\.dev|dev\.zed|zed://|ZED_' \
  crates/zed/Cargo.toml crates/zed/resources script .github/workflows
~~~

剩余命中必须是兼容、历史、法律或上游归属，并有理由。

## 验收标准

- 每个平台的身份矩阵完整，或明确标记尚未支持。
- artifact 可安装、启动、注册协议并读取升级数据。
- CI 在 Orion repository 条件下真实触发，未静默跳过。
- 没有真实 secret 泄漏或生产发布动作。
- 安装、升级、卸载、再安装和回滚有证据。

## BLOCKED 条件

- App ID、签名主体、更新源或 release repository 未批准。
- 平台工具链不可用且没有 CI artifact。
- 需要生产证书/secret 才能继续。
- 升级会覆盖用户数据或无法恢复旧版本。
- workflow 改动会超出发布 identity 范围。

## 回滚

代码回滚只针对平台 metadata、资源、脚本和 workflow。发布回滚使用上一版已验证
artifact，不删除用户配置和注册信息。任何 schema forward-only 迁移由 S04/S09
责任人处理。

## 交接

返回：

- 平台矩阵。
- 每个平台的静态/安装/升级结果。
- artifact hash/signature 结果。
- CI 是否真实触发。
- 未完成平台和外部依赖。

