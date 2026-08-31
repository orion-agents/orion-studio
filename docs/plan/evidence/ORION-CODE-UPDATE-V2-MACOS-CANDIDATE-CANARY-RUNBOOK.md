# Orion Code Update v2 macOS Candidate 与 Canary Runbook

## 1. 文档状态

| 字段 | 值 |
| --- | --- |
| Runbook 状态 | `IMPLEMENTED PATH / AUTHORIZATION REQUIRED / NOT EXECUTED` |
| 适用 target | `darwin-aarch64` |
| 当前发布判定 | `IMPLEMENTED / RELEASE BLOCKED` |
| 外部动作 | 全部等待人工授权 |
| 测试密钥 | 只允许本地 fixture；禁止进入 Stable |

本文是授权前操作手册，不是发布回执。任何命令示例都必须在完成对应 `STOP` 门禁、经过 code review，并由发布负责人明确授权后才能执行。

## 2. 不可越过的安全规则

1. 缺凭据、缺正式实现、输入不唯一、摘要不一致或状态不明确时立即失败；不得自动降级到 ad-hoc 签名、测试密钥、npm `latest`、可变 URL 或未公证 archive。
2. 不在命令行、日志、artifact、issue、PR 或本文中记录证书密码、P12/P8 内容、notary token、release private key 或 installation ID。
3. 不使用 `codesign --deep` 进行签名；所有 Mach-O、`.node`、`.dylib`、helper 和外层 bundle 按内到外顺序显式签名。
4. 不覆盖已经上传的 release asset；每个 URL 必须 immutable。摘要变化意味着新 candidate，而不是原地替换。
5. 上传、签名、公证、发布 index、更新 ACP Registry 和扩大 rollout 都是独立外部动作，每一步都需要独立人工授权。
6. 任何本地 test key 的 `key_id` 必须以 `local-test-` 开头；正式 Studio trust set 永远不能包含该 key。
7. 当前任务运行期间不得删除 current/previous/user data；临时目录只按已核对的绝对路径清理。

Apple 要求 Developer ID 分发使用合适的 Developer ID Application 身份、Hardened Runtime 和 secure timestamp，并禁止启用 `get-task-allow`。Apple 也明确建议用 `notarytool` 与 `stapler`，且 ZIP 本身不能直接 staple，必须 staple 其中的 app 后重新打包：

- [Creating distribution-signed code for macOS](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/)
- [Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)

## 3. 当前必须停止的位置

在下列问题全部解决前，只允许执行本地 unsigned/fixture 步骤：

| Gate | 当前状态 | 必须完成 |
| --- | --- | --- |
| `STOP-LAYOUT` | `IMPLEMENTED / LOCAL` | app-like `OrionCodeSidecar.app/...` 与 Studio command 已一致；正式 signed bytes 仍需重放 |
| `STOP-RECEIPT` | `IMPLEMENTED / NOT EXECUTED` | post-sign/notary/staple final archive 与 releasable external receipt 代码存在；必须用真实 final bytes 执行 |
| `STOP-CI` | `IMPLEMENTED / NOT EXECUTED` | protected workflow 已调用 codesign、notary、staple、验证、远端复下载和条件 publisher；真实 protected run 尚无回执 |
| `STOP-INDEX` | `IMPLEMENTED / NOT EXECUTED` | production Ed25519 signer 与默认 dry-run、exact authorization publisher 已实现；正式 key 与目标 URL 未使用 |
| `STOP-POLICY` | `IMPLEMENTED / CURRENT LOCAL TEST PASS` | same-version rollout/status/rollback transition 已实现；当前 release tooling `36/36` 通过 |
| `STOP-SOURCE` | `BLOCKED` | 当前本地门禁已通过，但两仓仍需形成 reviewed、clean committed exact tag，并在 exact SHA 上重跑完整门禁 |
| `STOP-STUDIO` | `BLOCKED` | 正式 endpoint、host allowlist、公钥、Team ID、bundle ID 尚未冻结并嵌入；启动路径当前正确 fail-closed |
| `STOP-SIGNED-CANDIDATE` | `BLOCKED / NOT AUTHORIZED` | 真实 Developer ID、notary accepted、staple/Gatekeeper、remote SHA 和 release-key signature 均未执行 |
| `STOP-CLEAN-MAC` | `BLOCKED / NOT RUN` | signed Studio/Code candidate 与非构建机真实 ACP journey 未完成 |
| `STOP-CANARY` | `BLOCKED / NOT AUTHORIZED` | 5%～100%、paused、revoked/rollback 的发布与观察回执为空 |

如果任一 `STOP-*` 未解除，发布负责人必须记录 `NO-GO`，不得尝试“手工绕过”。

## 4. 已核对的本地入口

以下路径在 2026-09-01 的隔离 worktree 中存在：

```text
scripts/release/build-acp-sidecar.mjs
scripts/release/verify-acp-sidecar.mjs
scripts/release/generate-update-index.mjs
scripts/release/protected-release-preflight.mjs
.github/workflows/acp-sidecar-release-gate.yml
/usr/bin/codesign
/usr/bin/curl
/usr/bin/shasum
/usr/bin/ditto
/usr/sbin/spctl
/Applications/Xcode.app/Contents/Developer/usr/bin/notarytool
/Applications/Xcode.app/Contents/Developer/usr/bin/stapler
```

当前 package scripts：

```text
npm run release:acp-sidecar
npm run release:verify-acp-sidecar
npm run release:update-index
npm run ci:acp-sidecar-unsigned
npm run release:acp-sidecar-protected-preflight
npm run release:acp-sidecar-protected-build
npm run release:acp-sidecar-protected
npm run test:release-tooling
```

重要限制：

- `release:acp-sidecar` 永远输出 `NOT RELEASABLE`，不签名、不公证、不上传。
- `release:update-index` 永远是 local dry-run；只有显式 local test key 才能产生测试签名。
- `release:acp-sidecar-protected` 已静态接通临时 keychain、Developer ID、notary、staple、final receipt、production index signer、默认 dry-run publisher 和 exact authorization publication；本次没有使用真实凭据或生产 URL 执行。
- 当前 `test:release-tooling 36/36`、lint、Prettier、build、完整 Jest 和 `test:acp` 已通过；这些仍只是本地实现证据。

## 5. Phase A：授权前本地 unsigned candidate

### A-01 冻结输入

人工记录并审阅：

- exact Orion Code version、tag、full source SHA。
- `darwin-aarch64` target。
- Studio semver requirement 与 ACP protocol `1`。
- Node exact version、ABI、runtime executable 和 Node license 来源。
- 正式 archive host、release notes host、redirect allowlist。
- 未来的 Team ID、bundle ID、update index key ID。
- previous verified exact version 和紧急 rollback target。
- immutable rollout salt；同一 release 全阶段不得修改。

任何字段未冻结：`NO-GO`。

### A-02 clean source 与本地门禁

下列步骤不使用 Apple 或 release-key 凭据，也不做外部写入：

```bash
cd <orion-code-checkout>
git status --short --branch
git rev-parse HEAD
git diff --check
npm ci
npm run lint
npm run build
npm test -- --runInBand
npm run test:acp
npm run test:release-tooling
npm run release:check -- --skip-tests
```

正式 candidate 必须来自 clean committed exact tag。当前隔离 worktree 是 dirty，因此这里只能验证开发状态，不能生成正式 candidate receipt。`release:check -- --skip-tests` 已按预期仅在 dirty-worktree 门禁返回 `NO-GO`，其余 version/changelog/diff/lint/tsc/package 检查通过。

### A-03 生成 unsigned payload

先创建独立临时目录并记录其绝对路径。不要把输出写进仓库或用户配置目录。

```bash
ORION_CANDIDATE_ROOT="$(mktemp -d -t orion-code-candidate)"
ORION_CODE_VERSION="<exact-semver>"
ORION_CODE_GIT_SHA="<40-hex>"
ORION_NODE_RUNTIME="<absolute-path-to-approved-node>"
ORION_NODE_LICENSE="<absolute-path-to-node-license>"

npm run release:acp-sidecar -- \
  --version "$ORION_CODE_VERSION" \
  --git-sha "$ORION_CODE_GIT_SHA" \
  --target darwin-aarch64 \
  --out "$ORION_CANDIDATE_ROOT/unsigned" \
  --node-runtime "$ORION_NODE_RUNTIME" \
  --node-license "$ORION_NODE_LICENSE" \
  --studio-version-requirement '<frozen-semver-requirement>'
```

预期结果必须包含 `NOT_RELEASABLE.txt`。若命令输出任何 releasable、signed、notarized 或 published 声明，应视为实现错误并立即停止。

### A-04 replay unsigned receipt

用实际输出文件替换占位符：

```bash
npm run release:verify-acp-sidecar -- \
  --receipt "$ORION_CANDIDATE_ROOT/unsigned/<candidate.receipt.json>" \
  --archive "$ORION_CANDIDATE_ROOT/unsigned/<candidate.zip>"
```

保存：archive bytes、archive SHA、manifest SHA、SBOM SHA、notices SHA、source SHA、Node version/ABI、ACP smoke 与 relocated-path 结果。

此处仍然是本地 unsigned evidence。当前 payload 已使用 app-like layout，但执行完 A-04 仍必须停在 `STOP-SOURCE`、`STOP-STUDIO` 和 `STOP-SIGNED-CANDIDATE`；`NOT RELEASABLE` receipt 不能提交公证、进入 production signer 或写入 Stable index。

2026-09-01 本地重放快照：

- 使用 Node `v24.14.0` / ABI `137` / `darwin-aarch64` 生成 app-like unsigned candidate，builder 明确输出 `NOT_RELEASABLE / NOT_PUSHED / NOT_PUBLISHED / NOT_SIGNED / NOT_NOTARIZED`。
- `release:verify-acp-sidecar` 通过；archive SHA-256 为 `97a8a3743fc29bbdc5ac97a97f109c7bb420bb51a78c28ba763237028bba2e70`，manifest/SBOM/notices SHA-256 分别为 `56e9ce931b2ab8ef62160b4f60069a1e65e7249d103404286e7aa9ec865c48ec`、`68ab52d0648bc9fdc6eb77b848cf99593b26c0b3ccdf0c296d44d22374e13dd3`、`a996a9170e1c52129df33ff3a3d66f7894c0bde214756fa54f55e7cc35d897d3`。
- 使用临时 `local-test-*` key 生成 exact-byte index dry-run 并通过验签；结果明确为 `NOT_RELEASABLE / NOT_PUBLISHED / NOT_SIGNED_WITH_RELEASE_KEY`。本地测试私钥和临时 candidate 不属于发布资产，本轮在记录摘要后已删除。

## 6. Phase B：正式 app-like archive 生成顺序

本阶段既描述目标流程，也对应当前 `production-sidecar-release.mjs`、`run-protected-sidecar-release.mjs` 与 protected workflow 的实现。代码存在不等于执行完成；只有在 `STOP-SOURCE`、`STOP-STUDIO` 解除并获得凭据使用授权后才能运行。

### B-01 目标布局

```text
orion-code-sidecar/
  manifest.json
  SBOM.cdx.json
  LICENSE
  THIRD_PARTY_NOTICES
  OrionCodeSidecar.app/
    Contents/Info.plist
    Contents/MacOS/orion-code-acp
    Contents/Resources/runtime/...
    Contents/Resources/app/...
```

`manifest.json` 位于 `.app` 外、archive root 内，并且不哈希自身。这样可以在内层签名、外层 bundle 签名和 staple 完成后重新生成 manifest，而不破坏 `.app` 的 sealed resources。

### B-02 凭据 fail closed

受保护 GitHub Environment 名称为：

```text
orion-code-sidecar-release
```

当前 protected workflow 需要以下 secret 名称：

```text
ORION_APPLE_DEVELOPER_ID_CERTIFICATE_P12_BASE64
ORION_APPLE_DEVELOPER_ID_CERTIFICATE_PASSWORD
ORION_APPLE_NOTARY_KEY_P8_BASE64
ORION_UPDATE_INDEX_PRIVATE_KEY_PEM_BASE64
ORION_RELEASE_UPLOAD_TOKEN
```

并需要冻结以下 protected variables/inputs：

```text
ORION_CODE_EMBEDDED_NODE_VERSION
ORION_APPLE_DEVELOPER_ID_APPLICATION
ORION_APPLE_TEAM_ID
ORION_APPLE_NOTARY_KEY_ID
ORION_APPLE_NOTARY_ISSUER_ID
ORION_UPDATE_INDEX_KEY_ID
ORION_CODE_SIDECAR_BUNDLE_ID
ORION_RELEASE_ASSET_UPLOAD_BASE_URL
ORION_RELEASE_PUBLIC_BASE_URL
ORION_RELEASE_PUBLICATION_URL
```

要求：

- 只允许 `workflow_dispatch`。
- 只允许 exact tag `v<package.version>`。
- key ID 不能以 `local-test-` 开头。
- 任何 secret 缺失、格式错误或 protected environment 未批准，job 失败。
- 不允许通过 `set -x`、echo、debug artifact 或错误文本输出 secret value。
- workflow 还会验证 protected tag/ref/SHA/environment、`darwin-aarch64` runner、exact package version、previous index 与显式 publication authorization。任何上下文或凭据缺失都应 fail closed；preflight 通过本身仍不是发布放行。

### B-03 内到外签名

发布脚本必须从冻结 manifest 获得明确的 code component 列表和每项 entitlements，不能用一次递归猜测替代清单。示意命令：

```bash
ORION_SIGNING_IDENTITY_SHA1="<exact-Developer-ID-Application-identity-hash>"
ORION_APP_ROOT="<absolute-path>/orion-code-sidecar/OrionCodeSidecar.app"

security find-identity -p codesigning -v

/usr/bin/codesign --force --sign "$ORION_SIGNING_IDENTITY_SHA1" \
  --timestamp --options runtime \
  "$ORION_APP_ROOT/Contents/Resources/runtime/node"

# 对 manifest 列出的每个 .node、.dylib、framework 和 helper 重复显式签名。

/usr/bin/codesign --force --sign "$ORION_SIGNING_IDENTITY_SHA1" \
  --timestamp --options runtime \
  "$ORION_APP_ROOT"
```

禁止：

```text
codesign --deep ...
sudo codesign ...
codesign --sign - ...
```

每个 component 与外层 app 都必须执行：

```bash
/usr/bin/codesign --verify --strict --verbose=4 "<exact-component-path>"
/usr/bin/codesign --display --verbose=4 "<exact-component-path>"
/usr/bin/codesign --display --entitlements :- "<exact-component-path>"
```

门禁：Team ID、identifier、Hardened Runtime、secure timestamp 必须匹配冻结值；任何 component 含 `com.apple.security.get-task-allow=true` 都失败。

### B-04 notarization

先将已签名 app 单独打包为 notary submission ZIP：

```bash
ORION_NOTARY_ZIP="<absolute-path>/OrionCodeSidecar-notary.zip"
/usr/bin/ditto -c -k --keepParent "$ORION_APP_ROOT" "$ORION_NOTARY_ZIP"

xcrun notarytool submit "$ORION_NOTARY_ZIP" \
  --key "<absolute-path-to-protected-p8>" \
  --key-id "$ORION_APPLE_NOTARY_KEY_ID" \
  --issuer "$ORION_APPLE_NOTARY_ISSUER_ID" \
  --wait --output-format json > "<absolute-path>/notary-result.json"
```

只有 `status=Accepted` 才能继续。无论 accepted 或 rejected，都要下载并审阅 notary log；存在未处理 warning 时保持 `NO-GO`。

```bash
xcrun notarytool log "<submission-id>" \
  --key "<absolute-path-to-protected-p8>" \
  --key-id "$ORION_APPLE_NOTARY_KEY_ID" \
  --issuer "$ORION_APPLE_NOTARY_ISSUER_ID" \
  "<absolute-path>/notary-log.json"
```

### B-05 staple、平台验证和最终 archive

ZIP 不能直接 staple；对 app staple 后再生成最终 archive：

```bash
xcrun stapler staple "$ORION_APP_ROOT"
xcrun stapler validate "$ORION_APP_ROOT"
/usr/bin/codesign --verify --strict --verbose=4 "$ORION_APP_ROOT"
/usr/sbin/spctl --assess --type execute --verbose=4 "$ORION_APP_ROOT"
```

然后按以下顺序完成最终 bytes：

1. 重新扫描 stapled `.app` 和 archive root 的所有文件。
2. 重新生成 archive-root `manifest.json`；包含最终 signed/stapled 文件摘要，但不包含 manifest 自身。
3. 校验 SBOM、LICENSE 和 notices 完整。
4. 使用 deterministic ZIP 生成最终 archive。
5. 对最终 archive、manifest exact bytes、SBOM 和 notices 计算 SHA-256。
6. 最后生成 archive 外部 release receipt，单向绑定四个摘要。
7. replay receipt；任何差异都删除本次 candidate 并重新开始，不得局部修补 receipt。

当前仓库已实现 B-05 的 post-staple finalization、production receipt 生成和 replay verifier，但尚未用真实 Developer ID/notary bytes 执行。没有 `notary accepted`、final receipt 和平台验证回执时，`STOP-SIGNED-CANDIDATE` 仍然有效。

## 7. Phase C：上传与远端重新下载 SHA

本阶段是外部写入。只有用户明确授权“上传这个 exact candidate 到这个 exact immutable location”后执行。

### C-01 上传前记录

记录：

- source tag/full SHA。
- final local archive absolute path、bytes、SHA-256。
- manifest/SBOM/notices SHA。
- Developer ID identity SHA-1、Team ID、bundle ID。
- notary submission ID、accepted result 和 log digest。
- stapler、codesign、spctl 结果。
- release receipt SHA。
- exact destination URL；禁止 `latest`。

### C-02 上传

`PENDING HUMAN AUTHORIZATION`。`production-publisher.mjs` 已实现 immutable HTTPS PUT、禁止 redirect、exact protected tag/SHA authorization、archive 远端复下载 SHA replay，并在最后写 publication commit；发布平台、tag、URL、token 和 overwrite policy 未冻结或未授权时默认只返回 `DRY_RUN`。

### C-03 从远端重新下载

上传成功不等于 bytes 正确。使用新的临时文件重新下载 immutable URL：

```bash
ORION_REMOTE_URL="https://<frozen-allowlisted-host>/<immutable-versioned-path>/<archive.zip>"
ORION_REMOTE_COPY="$(mktemp -t orion-code-remote)"

/usr/bin/curl --fail --silent --show-error --location \
  --proto '=https' --proto-redir '=https' \
  --output "$ORION_REMOTE_COPY" \
  --write-out '%{url_effective}\n%{size_download}\n' \
  "$ORION_REMOTE_URL"

/usr/bin/shasum -a 256 "$ORION_REMOTE_COPY"
```

门禁：

- effective URL host 仍在冻结 allowlist。
- remote bytes 与 local final bytes 相等。
- remote SHA 与 final receipt 的 archive SHA 完全一致。
- 重新解压 remote copy 后，receipt replay、codesign、stapler、spctl 与 ACP preflight 全部再次通过。

任一不一致：`NO-GO`，冻结 asset 与 index 发布，保留取证；不得覆盖同一 URL。

## 8. Phase D：正式签名 index

### D-01 当前能力边界

当前能力分成两个明确入口：

- `release:update-index` 仍只用于 local dry-run 和 `local-test-*` fixture key。
- protected release path 可从 production receipt replay 生成 deterministic exact bytes，使用正式 Ed25519 private key 签名，并支持更高 sequence 的 same-version rollout/status/rollback policy transition。
- publisher 默认 `DRY_RUN`；只有 `publish:vVERSION:EXACT_SHA` 与 protected tag/SHA 完全一致时才允许外部写入。

这些 production 路径尚未使用真实 release key、previous production index 或远端目标执行，因此下面步骤仍受 `STOP-SOURCE`、`STOP-STUDIO`、`STOP-SIGNED-CANDIDATE` 和人工授权约束。

### D-02 正式 generator 必须满足

1. 输入只接受远端复下载验证通过的 final release receipt。
2. `sequence = previous.sequence + 1`，不允许手工回退或复用。
3. archive URL、bytes 和所有摘要与 receipt 完全相同。
4. exact index bytes 先落盘，再用受保护 Ed25519 release key 签名；签名后不得重新序列化。
5. detached envelope 只含冻结的 `algorithm=ed25519`、正式 `key_id` 和 signature。
6. 用 Studio trust set 对 exact bytes 独立验签；一字节 bit flip 必须失败。
7. generator 不拥有默认 publish 权限；输出先进入待审批 artifact。
8. 同版本 policy replacement 只能改变 `rollout_basis_points`、`status` 和受约束的 `rollback_to`；version、target URL、bytes、digests、salt、channel 和 compatibility 不得漂移。

### D-03 5% index 准备

5% 等于 `500` basis points。生成前冻结：

```text
channel=stable
status=active
rollout_basis_points=500
rollout_salt=<immutable-for-this-release>
studio_version_requirement=<frozen-range>
sequence=<previous+1>
```

生成并签名后仍保持 `NOT PUBLISHED`。发布 index 与 signature 需要独立人工授权。

## 9. Phase E：Canary 5% → 25% → 50% → 100%

### E-01 灰度表

| 阶段 | Basis points | 最短观察 | 外部动作 | 前进门禁 |
| --- | ---: | ---: | --- | --- |
| Prepared | `0` | 不适用 | assets 可已授权上传；index 未发布 | signed 5% index、rollback index、证据包均已审阅 |
| Canary | `500` | 24 小时 | `PENDING AUTHORIZATION` | 无 P0/P1；下载、验签、平台验证、ACP preflight、idle activation 和 rollback 健康 |
| Early | `2500` | 24 小时 | `PENDING AUTHORIZATION` | 无新增阻断；各 macOS 版本与硬件分布无集中失败 |
| Broad | `5000` | 24 小时 | `PENDING AUTHORIZATION` | 升级成功率、启动、资源占用和支持反馈稳定 |
| General | `10000` | 48 小时 | `PENDING AUTHORIZATION` | 回滚演练、Release Notes、支持手册和 clean-Mac 复验完成 |

每次扩大 rollout：

1. 保持同一 version、archive URL/digests 和 rollout salt。
2. 生成更高 sequence 的完整 index。
3. 用正式 release key 对 exact bytes 重新签名。
4. 独立验证 signature、sequence diff 和 only-allowed-policy diff。
5. 发布负责人查看上一阶段观察回执。
6. 用户明确授权该 exact sequence 与 rollout 后才原子发布 index/signature。
7. 发布后从生产 URL重新下载 index/signature并验签，确认 CDN 没有返回旧 sequence。

### E-02 每阶段观察项

只在现有 telemetry consent 范围内使用聚合数据：

- index fetch、signature、sequence、expiry、compatibility 和 cohort eligibility。
- archive download success、bytes、SHA mismatch、redirect/size/timeout。
- manifest/SBOM/notices、codesign、notary/staple/Gatekeeper、ACP preflight。
- staged、waiting-for-idle、activation、current/previous、rollback、Native fallback。
- crash、hang、孤儿 sidecar、重复下载、磁盘增长和启动时间。
- 用户支持反馈与 P0/P1/P2 缺陷。

禁止收集 prompt、response、工具参数/正文、项目路径、token、环境变量值、installation ID 或 cohort 原值。

### E-03 停止条件

任一条件触发立即停止扩大 rollout：

- 任一 P0/P1。
- signature、SHA、platform 或 ACP preflight 出现系统性失败。
- 进行中任务被更新终止。
- candidate failed 且 previous/Native fallback 不可靠。
- 数据丢失、session 无法恢复、孤儿进程或无限重启。
- 指标来源不可信、观察窗口不足或 index/asset receipt 不可重放。

停止并不自动授权 pause/revoke 发布；先生成待审 signed emergency index，再请求人工授权。

## 10. Phase F：暂停、撤回与回滚

### F-01 Paused

适用于需要停止新增下载/激活、但没有证据证明当前已验证版本必须禁用的情况：

1. 从当前生产 index 生成更高 sequence。
2. 将 exact release 的 `status` 改为 `paused`。
3. 保持 version、target URL/digests、channel、salt 和 compatibility 不变。
4. 签名、独立验证 only-allowed-policy diff。
5. `PENDING HUMAN AUTHORIZATION` 后发布。

客户端应让已经运行的非撤回 current 继续工作，只停止新增下载和激活。

### F-02 Revoked + signed rollback

适用于确认 candidate 不应再启动：

1. 冻结已验证且未 revoked 的 exact rollback target。
2. 生成更高 sequence，将问题版本改为 `revoked`，并设置 `rollback_to=<exact-safe-version>`。
3. 确认 rollback target 在完整 index 中有可验证 target/receipt，且满足 Studio/ACP/platform 兼容。
4. 使用正式 key 签名并在 fixture Studio 重放 revoke → drain → rollback → restart。
5. 验证已有任务不被普通 update index 强制终止；新 session 不再启动 revoked 版本。
6. `PENDING HUMAN AUTHORIZATION` 后原子发布 index/signature。
7. 从生产 URL复下载并验签，确认 sequence 已生效。

不要删除或覆盖问题 archive。immutable asset 与 receipt 是诊断和防重装证据。

### F-03 当前执行门禁

generator 已支持受约束的 same-version policy transition，并拒绝 sequence 回退、rollout 缩小、revocation 反转、rollback target 漂移和 artifact immutable field 漂移。当前阻断来自真实 production key、previous production index、远端目标、signed candidate 与人工授权均缺失；不要通过复用 sequence、改 version/salt、手写 JSON 或测试 key 绕过。

### F-04 更新器自身故障

若问题位于 Studio updater，而不是 Orion Code candidate：

- 停止发布新的 first-party index。
- 使用经审阅的 Studio hotfix 默认禁用 coordinator。
- 保留 current/previous/staged receipts 和用户数据。
- 不把官方 ACP Registry 偷换成自动更新 authority。
- hotfix 的签名、公证、发布仍需独立授权。

## 11. Clean Mac 验收清单

只有在非构建机或干净 macOS 环境完成以下项目，`UP-INT-02` 才可能 PASS：

- [ ] 从真实 immutable URL 下载 signed/notarized Orion Code archive。
- [ ] 远端 SHA 与 final release receipt 一致。
- [ ] `codesign --verify --strict`、`stapler validate`、`spctl --assess` 全部通过。
- [ ] 离线状态下 Gatekeeper 仍接受 stapled app。
- [ ] Studio 从 signed fixture index 下载、stage，但 prompt 进行中不切换。
- [ ] turn/tool/permission/load 结束后只切换一次。
- [ ] ACP initialize identity/version/protocol、prompt、tool、permission、cancel、load、close、restart 通过。
- [ ] 断网继续使用 current；恢复网络后按 scheduler 检查。
- [ ] paused 停止新增动作但不杀死 current。
- [ ] revoked 拒绝新 session，并回滚 previous 或 fallback Native。
- [ ] 无孤儿进程、无限重启、session/config/data 丢失。
- [ ] 记录 Mac 型号、macOS version、Studio SHA、Code SHA、archive/index SHA 和结果，但不记录用户隐私数据。

## 12. 授权点清单

以下每一项都必须由用户或发布负责人单独明确授权：

- [ ] 使用 Developer ID certificate 和 notary credentials。
- [ ] 使用正式 update index private key。
- [ ] 上传 exact release asset 到 exact immutable URL。
- [ ] 发布 5% index/signature。
- [ ] 扩大到 25%。
- [ ] 扩大到 50%。
- [ ] 扩大到 100%。
- [ ] 更新官方 ACP Registry Stable source。
- [ ] 发布 paused index。
- [ ] 发布 revoked/rollback index。
- [ ] 发布 Studio emergency hotfix。

未勾选不等于默许；默认行为始终是停止。

## 13. 每次阶段回执

```text
Stage: <candidate|notary|upload|index-5|index-25|index-50|index-100|pause|revoke>
Status: PASS | PARTIAL | BLOCKED | FAIL
Authorization: exact approver + scope + time, or NOT AUTHORIZED
Studio source: branch + full SHA
Orion Code source: tag + full SHA
Artifact: filename + bytes + SHA-256
Manifest/SBOM/notices: SHA-256
Signing: identity SHA-1 + Team ID + bundle ID, no secret values
Notary: submission ID + Accepted/Rejected + log digest
Remote replay: effective URL + bytes + SHA-256
Index: sequence + payload SHA-256 + key ID + signature verification
Rollout: basis points + immutable salt identifier
Tests: exact commands + passed/failed/not-run counts
Observation window: start/end + aggregate outcome
External actions: exact list
Remaining risks: concrete list
Next authorized action: one action only
```

没有以上可重放回执时，不得将阶段标记为 PASS。
