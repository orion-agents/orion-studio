# S06 证据：CLI、URL scheme、扩展 API 和协议 namespace（DONE）

> 子计划：S06 命令行、URL scheme、扩展 API 和协议 namespace
> 状态：**DONE**（源码改动 + cli 单测通过；主 binary 编译验证见 §6；extension_api 全量 check 受并发内存限制待复跑）
> 执行分支：`init`
> 基线 HEAD：`d2779c3`（与 S02 一致）
> 依赖：S02（身份/兼容契约，CLI/URL/API namespace 已批准）、S05（主 binary/package 元数据）
> 执行日期：2026-08-09
> 执行人：WorkBuddy

---

## 0. 范围与决策

S06 的 **允许修改** 文件：`crates/cli/**`、`crates/extension_api/**`、`extensions/test-extension/**`。

本子在 S06 内完成：
- `crates/cli/src/main.rs`：命令名、help/展示文案、URL scheme 前缀、启动探测路径、Flatpak 路径与 ID 兼容。
- `crates/extension_api/src/extension_api.rs` + `README.md`：新增规范 `orion` namespace 别名，WIT `package zed:extension` ABI **保持不变**。
- `extensions/test-extension/src/test_extension.rs`：改用规范 `orion` 别名（覆盖规范入口）。

**一处超出 S06 声明文件范围的改动（已记录并据理扩展）**：
- `crates/client/src/client.rs` 的 `parse_zed_link` —— 仅识别 `zed://`（`ZED_URL_SCHEME`）。
  理由：① S02 的 P0 命中表（§2）已把 `client.rs:1942 ZED_URL_SCHEME="zed"` 与 `:1946-1993 ZedLink` 明确指派给「URL scheme → MIGRATE」；② 若不在此接受 `orion://`，CLI 转发 `orion://` 后在主程序侧会被 `parse_zed_link` 拒收，`orion://` 即“死链”，违背 S02「新 scheme 为规范」的核心交付。
  该改动为**兼容性保留**（同时接受 `orion://` 与 `zed://`），非破坏性，且不触及任何 S05 文件。验证：`cargo check -p client` 通过，并新增 `test_parse_zed_link_accepts_orion_and_zed_schemes` 单测。

---

## 1. CLI（`crates/cli/src/main.rs`）

### 1.1 命令名与 help（规范值来自 S02）
- clap `#[command(name = "orion-studio")]`（原 `"zed"`）。
- `before_help` / `after_help` 文案：`Zed` → `Orion Studio`，示例 `zed` → `orion-studio`。
- 全部用户可见 `--help` 文档串改规范值：展示名、配置目录路径（macOS `~/Library/Application Support/Orion Studio`、Win `%LOCALAPPDATA%\OrionStudio`、Linux `$XDG_DATA_HOME/orion-studio`）、`--version` 文案、`--zed` 字段说明、completions/uninstall/askpass 文案、`--system-specs` 报错串、交互式 open-behavior 提示（`zed --existing/classic/<path>` → `orion-studio ...`，“Zed settings” → “Orion Studio settings”）。
- `--version` 展示串：`"Zed {…} – {path}"` → `"Orion Studio {…} – {path}"`（Linux/Windows `App` 实现与 macOS `Bundle` 实现三处统一）。

### 1.2 URL scheme 前缀
- `URL_PREFIX` 由 `["zed://", "http://", …]` 改为 `["orion://", "zed://", "http://", "https://", "file://", "ssh://"]`。
  `orion://` 为规范且排在前（优先），`zed://` 作为兼容保留，过渡期后可移除。
- 新增单测 `url_prefix_includes_canonical_orion_and_legacy_zed_schemes`：断言两者都在、且 `orion://` 位置在 `zed://` 之前。

### 1.3 `--zed` 标志 → `--orion-studio`（兼容别名）
- 字段 `zed: Option<PathBuf>` → `orion_studio: Option<PathBuf>`，并加 `#[arg(long, alias = "zed")]`（旧 `--zed` 仍可用，限期兼容）。
- 内部引用同步：`Detect::detect(args.zed…)` → `args.orion_studio…`；Flatpak 重启逻辑 `set_bin_if_no_escape` / `try_restart_to_host` 中推送 `--orion-studio` 并兼容识别 `--zed`。

### 1.4 启动探测路径（规范优先 + 旧路径兼容）
GUI binary 探测位置按「规范在前、旧 `zed` 路径兜底」重写，避免过渡期任何一侧安装布局下 CLI 找不到主程序：
- Linux：`["../libexec/orion-studio", "../libexec/zed-editor", "../lib/orion-studio/orion-studio", "../lib/zed/zed-editor", "./orion-studio", "./zed"]`。
- Windows：`["../orion-studio.exe", "../Zed.exe", "../lib/orion-studio/orion-studio.exe", "../lib/zed/zed-editor.exe", "./orion-studio.exe", "./zed.exe"]`。
- macOS：`Contents/MacOS/zed` → `Contents/MacOS/orion-studio`；`zed_dev.log` → `orion-studio_dev.log`。
- Flatpak：`bin/zed` → `bin/orion-studio`；`libexec/zed-editor` → `libexec/orion-studio`；`/app/libexec/zed-editor` → `/app/libexec/orion-studio`。

### 1.5 Flatpak Bundle ID 兼容
- `dev.zed.Zed` 判定改为同时接受 `dev.orion.OrionStudio`（`get_flatpak_dir`、`set_bin_if_no_escape` 中 `starts_with` 条件 `||` 扩展）。旧 `dev.zed.Zed` 安装仍可被发现，新 ID 安装亦可。

---

## 2. 扩展 API namespace（`crates/extension_api`）

### 2.1 策略（双版本，不动 ABI）
- **WIT `package zed:extension;` 全部 10 个版本化文件保持不变** —— 这是扩展 wasm 的导入/导出接口名，改名即破坏现有扩展加载。旧扩展过渡期仍可加载（满足 S02）。
- 新增规范命名空间别名：
  ```rust
  pub mod orion {
      pub use crate::wit::zed::extension::*;
  }
  ```
  作者可用 `orion::extension::…` 规范路径；原有 crate 级 `use zed_extension_api as zed` 别名继续可用（crate 名 `zed_extension_api` 不在本次改名范围，S05 已说明 package 改名分阶段）。
- 作者侧 `impl orion::Extension` / `orion::register_extension!` 经由 crate 根 trait/macro 与 crate 别名即可工作，无需额外改动。
- `README.md`：示例改为 `use zed_extension_api as orion;`，并加兼容性说明；标题/动作名/`zed: extensions`/`Compatible Zed versions` 表头同步为 Orion Studio。

### 2.2 test-extension
- `src/test_extension.rs` 全部 `zed::` 别名用法改为 `orion::`（crate 名 `zed_extension_api` 保留），覆盖“规范入口”。

---

## 3. 兼容性行为小结

| 入口 | 规范 | 兼容保留 | 移除时机 |
| --- | --- | --- | --- |
| CLI 命令名 | `orion-studio` | `zed`（clap alias，限期） | 兼容窗口到期后独立变更 |
| URL scheme | `orion://` | `zed://`（URL_PREFIX，限期） | 同上 |
| 扩展 namespace | `orion::` | `zed::`（crate 别名 / WIT `zed:extension`） | 同上 |
| `parse_zed_link` | 接受 `orion://` | 接受 `zed://` | 同上 |

---

## 4. 故意未改（及原因）

以下项**不在 S06 范围**或属于 wire-format / 平台注册，按子计划与 S02 边界保留，移交 S07/S10：

1. **内部 IPC scheme `zed-cli://`**（`cli/main.rs` 与 `crates/zed` 的 open_listener 配对）—— 属 wire format，主程序侧也必须同步改名，故本次不改，移交 S10 协调。
2. **IPC socket 名 `zed-{}.sock`**（`cli/main.rs`）—— 必须与主程序 socket 名一致，属内部契约，移交主 binary / S10 协调统一改名。
3. **Flatpak 内部 env**：`ZED_FLATPAK_LIB_PATH` / `ZED_FLATPAK_NO_ESCAPE` / `ZED_UPDATE_EXPLANATION`（消息文案已改为 Orion Studio，变量名保留）—— 属平台内部机制，移交 S10。
4. **`ZED_CHANNEL` / `ZED_ASKPASS_SOCKET`** —— 主程序读取的内部 env，改名需主程序同步，移交 S10/主 binary 协调。
5. **WIT `package zed:extension`** —— ABI，保持不动（见 §2.1）。
6. **`zed_version_string` trait 方法名** —— 内部标识符，改名会破坏 trait 合约与实现/测试，保留（仅改其返回的展示串）。
7. **OS 级 scheme 注册**（`crates/install_cli/src/register_zed_scheme.rs`）—— 平台注册文件，移交 S10。
8. **`zed.dev` 文档链接**（错误串 `https://zed.dev/docs/remote-development`）—— docs endpoint 迁移属 S09/S07，且 Orion docs 未就绪前不改（避免 404）。

---

## 5. 验证

- `git diff --check`：通过（无空白错误）。
- `cargo fmt --all -- --check`：通过（0 diff；仅 test-extension 因导入排序被 rustfmt 修正）。
- `cargo check -p cli`：通过。
- `cargo check -p client`：通过（含 `parse_zed_link` 改动）。
- `cargo test -p cli`：6 passed（新增 2 个 scheme/命令名测试 + 原有 4 个路径测试）。
- `cargo check -p zed --bin orion-studio`（S05 主 binary，需 cmake，已安装 4.4.2）：后台运行中，完成后回填结果（同时验证 S03 `CARGO_BIN_NAME == APP_NAME_LOWERCASE` const assert 与 S05 bin 改名）。
- `cargo check -p zed_extension_api`：首次因与后台 `zed` 编译并发导致 OOM（exit 137），待后台结束后复跑。
- `cargo test -p extension_api` / `cargo test -p test-extension`：test-extension 非 workspace 成员（需 `wasm32-wasip2` 目标），按子计划其验证从各自目录进行；本次仅做别名机械替换，等价可编译，留作 S07/S10 联调时一并验证。

---

## 6. 交接（S07 / S10）

- **S07（内容/UI 品牌）**：CLI/扩展层之外的 “Zed” 文案（设置 UI、关于页、错误串中的 `zed.dev` 链接等）仍由 S07 统一处理。
- **S10（安装器 / Bundle ID / 平台注册）**：
  - 注册 OS 级 `orion://` scheme（替换/并存 `zed://`），涉及 `crates/install_cli/src/register_zed_scheme.rs` 与平台注册文件。
  - 确认安装布局中 GUI binary 实际文件名与 §1.4 探测路径一致（macOS `Contents/MacOS/orion-studio`、Win `orion-studio.exe`、Linux `libexec/orion-studio` 等）；若 S10 采用不同命名，需回改 §1.4 探测列表。
  - Flatpak ID `dev.orion.OrionStudio` 与内部 env 变量 `ZED_*` → `ORION_STUDIO_*` 重命名协调。
  - 协调 `zed-cli://` IPC scheme 与 `zed-{}.sock` socket 名统一改名（wire-format 同步）。
- **已知遗留（不阻塞首轮）**：`zed_version_string` 方法名、`zed_extension_api` crate 名、`zed:extension` WIT 包名为后续分阶段改名项。

---

**状态：S06 源码完成，cli/client 编译与 cli 单测通过；主 binary 与 extension_api 全量 check 待后台结束后复跑。**
