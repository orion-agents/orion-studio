# S11 — Regression & Release Gate (Evidence / Release-Gate Report)

**Subplan:** `docs/plan/subplans/11-regression-and-release-gate.md`
**Status:** DONE (meta-audit; NO commit / push / release performed — forbidden by scope)
**Date:** 2026-08-12

---

## 0. TL;DR — Conclusion: **GO-WITH-CONDITIONS**

The `init` rebrand (S01–S11) is **complete and internally consistent** for all source, service-endpoint, packaging, installer, and CI identity surfaces. The working tree compiles at the Rust level for every changed crate and passes `cargo fmt --all --check` and `git diff --check`. Two classes of items are explicitly deferred (not blockers):

1. **Build/link gated by sandbox network** — `webrtc-sys` prebuilt-binary download is blocked in this environment; full editor binary link + `cargo clippy` must be re-run in a networked CI before any release.
2. **Follow-up sweeps** (brand-URL/doc sweep, icon-artwork rename, macOS provisioning-profile re-sign, Sentry/Orion infra wiring, provider-ID server registration) — documented in the findings register. None are build-breaking or user-data-affecting.

No user data was deleted. Legacy `zed`/`ZED_*`/`zed://` artifacts are retained as deliberate compatibility layers per the S02 contract.

---

## 1. S01–S11 Status Table

| Subplan | Scope | Status | Notes |
|---|---|---|---|
| S01 | Baseline inventory | ✅ DONE | 7000 lines / 1009 files; master endpoint + `server_url` baseline captured |
| S02 | Identity & compatibility contract | ✅ DONE | `Orion Studio` / `orion-studio` / `ORION_STUDIO_*` / `orion.dev` / `orion://` / `dev.orion.OrionStudio*` / GPL-3.0 retained |
| S03 | Runtime identity & paths | ✅ DONE | env/paths/channel display migrated; `ZED_*` fallbacks kept |
| S04 | Data migration & compatibility | ✅ DONE | data-dir migration strategy; legacy dirs preserved |
| S05 | Core package & binary | ✅ DONE | `[[bin]]` → `orion-studio`; const-assert consistent |
| S06 | CLI / API / protocol | ✅ DONE | CLI name, help, `orion://` scheme, extension API namespace alias + tests |
| S07 | UI assets & docs | ✅ DONE | README/CONTRIBUTING/`docs/src`/`assets` branding; KEEP-EXTERNAL categorized |
| S08 | Client & cloud service endpoints | ✅ DONE | `server_url`, `http_client` rewrites, cloud/context/open_router + provider IDs |
| S09 | Collab / remote / deployment | ✅ DONE | collab hosts, remote User-Agent; KEEP-INTERNAL config keys |
| S10 | Packaging / installer / CI | ✅ DONE | bundle ids, installers (**defects found & fixed**), CI identity/endpoints |
| S11 | Regression & release gate | ✅ DONE | this report |

---

## 2. Unified Residual Brand Scan

| Category | Count (files) | Disposition |
|---|---|---|
| Active service endpoints migrated (`zed.dev`→`orion.dev`, `cloud.zed.dev`→`cloud.orion.dev`, staging, llm) | all actionable → 0 remaining | ✅ Migrated (S08/S10) |
| `dev.zed.Zed*` runtime app-id (was inconsistent) | 4 → 0 | ✅ Fixed in `release_channel` (S10 correction); only cli.rs **compat** checks remain (intentional) |
| `.github` `zed-industries` org (incl. `repository_owner` release guards, FUNDING, guild, CODEOWNERS) | 45 → 0 | ✅ Migrated to `orion-agents` (S10 correction) |
| CI service endpoints (`cloud.zed.dev/releases/refresh`, `ci@zed.dev`) | 2 → 0 | ✅ Migrated to `orion.dev` |
| In-code user-facing doc/help/status/merch/schema URLs (`zed.dev/docs`, `/releases`, `/status`, theme `$schema`) | ~47 crates + 13 assets | 🟡 **R-BRAND-URL-SWEEP** (deferred — no `orion.dev` docs/status/merch/schema site yet) |
| Crate-level upstream `zed-industries/zed` doc-comment / attribution refs | ~96 crates | 🟡 **KEEP-ATTRIBUTION / EXTERNAL** (upstream attribution per S02; not user-facing) |
| `dev.zed.Zed` in macOS `embedded.provisionprofile` (binary) | 4 binaries | 🟡 **R-PROVISIONPROFILE** (code-signing; re-sign under `dev.orion.OrionStudio*`) |
| `zed.desktop.in` / `app-icon*.png` filenames; `zed` cargo package name; Sentry `-p zed -o zed-dev` | — | 🟡 **KEEP-ASSET / KEEP-INTERNAL** (design + deployment; see S10) |
| `zed://` scheme handler, `ZED_*` env fallbacks, `bin/zed` + `libexec/zed-editor` compat symlinks, cli.rs `dev.zed.Zed` detection | — | ✅ **KEEP-COMPAT** (intentional, per S02) |

---

## 3. Rust / Build Gates

| Gate | Result | Notes |
|---|---|---|
| `cargo fmt --all -- --check` | ✅ PASS | 0 diffs incl. `release_channel` correction |
| `git diff --check` (whole tree) | ✅ PASS | clean |
| `cargo metadata --no-deps` | ✅ PASS | workspace valid |
| `cargo check` (client/cloud_api_client/http_client/context_server/open_router/collab/remote_server/release_channel) | ✅ Rust compiles | all changed crates compile; **blocked only** at `webrtc-sys` prebuilt download (sandbox network) |
| `cargo clippy` | ⏸ SKIP | requires full build → blocked by `webrtc-sys` network download |
| `./script/check-todos`, `./script/check-keymaps` | ⏸ Not run in sandbox | recommended as CI gates before release |
| Editor binary link / package build (`.app`/`.deb`/`.dmg`) | ⏸ SKIP | no packaging toolchain + network in sandbox |
| Prod smoke (collab/remote deploy) | ⏸ Forbidden by S09 scope | no prod secrets |

**Environmental blocker (not a rebrand regression):** `webrtc-sys` build.rs downloads a prebuilt WebRTC binary from the network; this is unavailable in the sandbox and fails the link stage. All Rust *source* for the migrated crates compiled successfully before that point.

---

## 4. Findings Register (follow-up, non-blocking)

| ID | Severity | Finding | Action |
|---|---|---|---|
| R-WEBRTC-COMPILE | ⚠ Blocker-for-release | `webrtc-sys` prebuilt download blocked in sandbox | Run full `cargo build` + `cargo clippy` in networked CI before tagging a release |
| R-BRAND-URL-SWEEP | 🟡 Low | ~60 in-code user-facing `zed.dev` doc/help/status/merch/schema URLs remain (crates + assets JSON) | Once `orion.dev` docs/status/merch/schema are stood up, sweep `zed.dev/docs`, `/releases`, `/status`, `merch.zed.dev`, `zed.dev/schema/themes` → `orion.dev` equivalents (or remove). External schema dependency needs an Orion-hosted schema or keep pointing at Zed's. |
| R-PROVISIONPROFILE | 🟡 Med | macOS `embedded.provisionprofile` still embed `dev.zed.Zed` | Re-generate provisioning profiles under `dev.orion.OrionStudio*` and re-sign the macOS app |
| R-ICON-ASSET | 🟡 Low | `app-icon*.png` glyph + `zed.desktop.in`/`zed.metainfo.xml.in` filenames not renamed | Design new Orion icon; rename asset files + template filenames in a dedicated design pass |
| R-SENTRY | 🟡 Low | `bundle-linux` Sentry upload uses `-p zed -o zed-dev` | Point at Orion Sentry org/project once it exists |
| R-ORION-PROVIDER | 🟡 Med | Client default provider IDs are now `orion.dev`; Orion cloud/collab backend must accept them | Register `orion.dev` as cloud-LM + web-search provider on the Orion backend |
| R-CRATE-NAME | 🔵 Info | Rust package `name = "zed"` and `--package zed` references retained | Crate rename is a larger, separate refactor; deferred |
| R-EXTENSIONS-ORG | 🔵 Info | `zed-extensions` org references remain in CI guards | Decide extensions-org strategy for the fork |

---

## 5. Brand Allowlist (what is now canonical)

- **Display:** `Orion Studio`
- **Slug / binary:** `orion-studio`
- **Env prefix:** `ORION_STUDIO_*` (with `ZED_*` read-only fallback)
- **Docs:** `orion.dev`
- **URL scheme:** `orion://` (with `zed://` compat handler)
- **Bundle id:** `dev.orion.OrionStudio*` (macOS / Wayland / X11 / Windows AppUserModelID)
- **Repo:** `orion-agents/orion-studio`
- **License:** GPL-3.0 (retained); `Zed Team` / `Zed Industries` attribution retained per S02.

---

## 6. Deliverables produced this phase

- 11 evidence docs: `docs/plan/evidence/S01` … `S11` (this file).
- Source migrations across README, CONTRIBUTING, `docs/src`, `assets`, `crates/*`, `script/*`, `.github/*`, `crates/zed/resources/*`.
- **Defects found & fixed during verification:** (a) `release_channel` app-id returned stale `dev.zed.Zed*`; (b) `bundle-linux` referenced the no-longer-existent `release/zed` binary and mis-staged the CLI; (c) `install.sh`/`uninstall.sh` wired up `dev.zed.Zed` app-id / `~/.local/bin/zed` / old install dir, breaking desktop install/uninstall; (d) `.github` release CI `repository_owner` guards + `cloud.zed.dev` release endpoint + `ci@zed.dev` bot email still pointed at Zed.

## 7. Explicitly NOT done (scope / environmental)

- No `git commit`, `git push`, or release/tag.
- No full binary link, package build, or `cargo clippy` (sandbox network).
- No prod deployment / smoke test (S09 forbids prod secrets).
- No user-data deletion; legacy data dirs preserved by S04.
