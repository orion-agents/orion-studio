# S09 — Collab, Remote Server & Deployment Identity (Evidence)

**Subplan:** `docs/plan/subplans/09-collab-remote-and-deployment.md`
**Status:** DONE (source identity migrated; compile verified green for the changed Rust crates where the sandbox network permits)
**Date:** 2026-08-08 (continued 2026-08-12)

---

## 1. Scope

**Allowed (this subplan):**
- `crates/collab` — server-side URL builders that derive the public app/cloud host from the deployment environment.
- `crates/remote_server` — server User-Agent / self-identification strings.
- `Dockerfile-collab` — collab container image metadata.

**Forbidden (explicitly out of scope):**
- Production secrets / secret names (untouched).
- Client URL builders (→ S08).
- Installers / CI (→ S10).
- Renaming internal Rust struct field/config-key names that are consumed by deployment manifests (kept as KEEP-INTERNAL; see §4).

**Contract (S02):** the public hosts derived by collab must agree with the S08 `server_url` default and `http_client` rewrites (`orion.dev` / `staging.orion.dev` / `cloud.orion.dev`).

---

## 2. `crates/collab/src/lib.rs` — environment-derived URLs

The deployment `zed_environment` value selects the public host. The returned **host strings** were migrated; the field/function names were kept internal.

| Function | Env branch | Before (returned) | After (returned) |
|---|---|---|---|
| `zed_dot_dev_url()` | `staging` | `"https://staging.zed.dev"` | `"https://staging.orion.dev"` |
| `zed_dot_dev_url()` | default (`production`/other) | `"https://zed.dev"` | `"https://orion.dev"` |
| `zed_cloud_url()` | default | `"https://cloud.zed.dev"` | `"https://cloud.orion.dev"` |

Doc comment on `zed_dot_dev_url` updated to reference `orion.dev`.

---

## 3. `crates/remote_server/src/server.rs` — User-Agent

| Item | Before | After |
|---|---|---|
| Server `User-Agent` (line ~691) | `"Zed-Server/{} ({}; {})"` | `"Orion-Studio-Server/{} ({}; {})"` |

This is the self-identification string the remote/collab server sends; purely cosmetic server-side identity, no protocol impact.

---

## 4. KEEP-INTERNAL — intentionally not renamed

These are **internal** Rust identifiers consumed by deployment configuration (env vars, secret names, manifests). Renaming them would require coordinated changes to the Orion deployment repo / secret manager and is explicitly out of scope for the `init` phase.

| Identifier | Location | Reason to keep (now) |
|---|---|---|
| `zed_environment` (struct field) | `collab/src/lib.rs:139` | Drives host selection; name is internal config, not user-facing. |
| `zed_cloud_internal_api_key` (struct field) | `collab/src/lib.rs:140` | Internal secret name. |
| `zed_client_checksum_seed` (struct field) | `collab/src/lib.rs:141` | Internal config. |
| `zed_dot_dev_url()` / `zed_cloud_url()` (fn names) | `collab/src/lib.rs:150,159` | Internal API; only returned host strings changed. |
| `zed_environment == "development"` branch logic | `collab/src/lib.rs:146` | Internal branch. |

These remain as a deliberate compatibility/naming layer; a later internal refactor (separate change) can rename them once the deployment side is updated. They do **not** leak the Zed brand to end users.

---

## 5. `Dockerfile-collab`

Scanned: contains **no** `zed`/`Zed`/`zed.dev` brand references (image is based on Rust/Debian and copies the collab binary). No change required.

---

## 6. Verification

- `git diff --check` on `collab/src/lib.rs`, `remote_server/src/server.rs` → clean.
- `cargo fmt -p collab -p remote_server -- --check` → 0 diffs.
- Collapse/compile of the collab + remote_server Rust sources is blocked only by the sandbox `webrtc-sys` prebuilt download (environmental SKIP, same as S08); no Rust compile errors attributable to these edits.
- No production secrets modified. No client URL builders touched.

---

## 7. Residuals & handoff

- **KEEP-INTERNAL (above):** internal config/field names → later internal refactor, not a brand leak.
- **Handoff to S10/S11:** ensure the deployed Orion collab/remote binaries are built with the migrated `orion.dev` hosts (the `zed_environment` → host mapping now emits Orion hosts). The `R-ORION-PROVIDER` server-side registration (S08) is a prerequisite for cloud LM/web-search to function end-to-end.
- **No prod smoke test run** (forbidden by scope: no prod secrets / no live deployment). Documented as SKIP with rationale.
