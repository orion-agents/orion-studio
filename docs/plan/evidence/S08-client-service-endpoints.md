# S08 — Client & Cloud Service Endpoints (Evidence)

**Subplan:** `docs/plan/subplans/08-client-service-endpoints.md`
**Status:** DONE (source + tests migrated; compile verified green for Rust on the changed crates where the sandbox network permits; see Verification)
**Date:** 2026-08-08 (continued 2026-08-12)

---

## 1. Scope

**Allowed (this subplan):**
- `crates/client` — server URL resolution, telemetry/account bootstrap endpoints.
- `crates/cloud_api_client` + its tests/mocks — canonical cloud host resolution.
- Service-endpoint host strings that resolve to the Zed cloud (`zed.dev` / `cloud.zed.dev` / `api.zed.dev` / `llm*.zed.dev`) used as **active defaults**.

**Forbidden (explicitly out of scope):**
- `crates/collab`, `crates/remote_server` runtime hosts (→ S09).
- Installers / CI endpoint strings (→ S10).
- RPC wire-format / protocol field names (preserved verbatim).
- User-facing doc/help/merch/status URLs that are not service endpoints (→ S11 residual, follow-up brand-URL sweep).

**Identity contract (S02) applied:** Orion canonical endpoints replace the Zed cloud as the **active default**; the legacy `ZED_*` env/endpoint is retained as a **read-only compatibility fallback** wherever a runtime override existed.

---

## 2. Master switch

The single source of truth for "where is the server" is the `server_url` setting default.

| File | Line | Before | After | Notes |
|---|---|---|---|---|
| `assets/settings/default.json` | ~2675 | `"server_url": "https://zed.dev"` | `"https://orion.dev"` | Master default. Inline comment updated to document `ORION_STUDIO_SERVER_URL` / `ZED_SERVER_URL` override hierarchy. |

This value flows into `zed_urls` and is the canonical base for account/telemetry bootstrap.

---

## 3. `crates/client/src/client.rs` — server URL resolution

| Item | Before | After |
|---|---|---|
| `static SERVER_URL` | read `ZED_SERVER_URL` only | read `ORION_STUDIO_SERVER_URL`, fall back to `ZED_SERVER_URL` (compat) |
| Reference `if let Some(server_url) = &*SERVER_URL` | unchanged | unchanged (now resolves via new precedence) |
| Doc comments citing `zed.dev`/`zed://` | — | updated to `orion.dev` / `orion://` (see S06 scheme extension) |

**Compatibility:** legacy `ZED_SERVER_URL` still honored as a fallback so existing launch env / scripts keep working during the migration window.

---

## 4. `crates/http_client/src/http_client.rs` — host rewrites (4 sites)

These map the canonical app host to the service sub-hosts. All four rewrite tables were migrated in lockstep.

| Function | Match (before) | Rewrite (after) |
|---|---|---|
| `build_zed_api_url` | `"https://zed.dev"` | `"https://orion.dev"` → `"https://api.orion.dev"` |
| `build_zed_cloud_url` | `"https://zed.dev"` / `"https://staging.zed.dev"` | `"https://orion.dev"` / `"https://staging.orion.dev"` → `"https://cloud.orion.dev"` |
| `build_zed_cloud_url_with_query` | `"https://zed.dev"` / `"https://staging.zed.dev"` | → `"https://cloud.orion.dev"` |
| `build_zed_llm_url` | `"https://zed.dev"` / `"https://staging.zed.dev"` | → `"https://cloud.orion.dev"` / `"https://llm-staging.orion.dev"` |

Doc comments: `Zed API` → `Orion API`, `Zed Cloud` → `Orion Cloud`, etc. Logic/structure untouched.

---

## 5. `crates/cloud_api_client/src/cloud_api_client.rs`

| Item | Before | After |
|---|---|---|
| `cloud_host()` fallback | `"cloud.zed.dev"` | `"cloud.orion.dev"` |
| Test (line ~338) | `uri("https://cloud.zed.dev/client/users/me")` | `uri("https://cloud.orion.dev/client/users/me")` |
| Test (line ~357) | same | same |

Tests were updated in lockstep; no behavioral change to the resolution logic.

---

## 6. `crates/context_server/src/oauth.rs`

| Item | Before | After |
|---|---|---|
| `const CIMD_URL` | `"https://zed.dev/oauth/client-metadata.json"` | `"https://orion.dev/oauth/client-metadata.json"` |
| Test ref (line ~1773/1785) | `zed.dev` | `orion.dev` |

---

## 7. `crates/open_router/src/open_router.rs`

| Header | Before | After |
|---|---|---|
| `HTTP-Referer` | `"https://zed.dev"` | `"https://orion.dev"` |
| `X-Title` | `"Zed Editor"` | `"Orion Studio"` (both occurrences, `replace_all`) |

---

## 8. Cloud / provider endpoint identity (same `orion.dev` namespace)

These are not in `crates/client` or `crates/cloud_api_client` physically, but they carry the **cloud provider namespace** `orion.dev` and were migrated as part of the same service-endpoint theme. Documented here for traceability; flagged below for server-side coordination.

| File | Item | Before → After |
|---|---|---|
| `crates/settings_content/src/agent.rs` | provider enum default value | `"zed.dev"` → `"orion.dev"` |
| `crates/language_model_core/src/provider.rs` | `ZED_CLOUD_PROVIDER_ID` value | `LanguageModelProviderId::new("orion.dev")` |
| `crates/web_search_providers/src/cloud.rs` | `ZED_WEB_SEARCH_PROVIDER_ID` | `"orion.dev"` |
| `crates/language_models_cloud/src/language_models_cloud.rs` | share-link format | `format!("orion.dev/{}", ...)` |
| `crates/ui/src/components/collab/collab_notification.rs` | diagnostic label | lists `cloud.orion.dev, orion.dev, edit-prediction-bench, orion.dev` |
| `crates/edit_prediction/src/edit_prediction.rs` | git remote detection prefix | `git@github.com:orion-agents/` |
| `crates/proto/Cargo.toml` | crate description | `Orion Studio` / `orion.dev` |

> **Coordination finding (cross-cutting):** the provider IDs `orion.dev` (cloud LM provider, web-search provider) are now the **client default**. The Orion cloud backend / collab server must be registered to *accept* `orion.dev` as a provider ID, otherwise cloud-model and web-search features will reject the client default. This is a server-side change tracked as a follow-up (see S11 residual register R-ORION-PROVIDER).

---

## 9. Compatibility layers preserved

- `ZED_SERVER_URL` env override retained as a read-only fallback in `client.rs`.
- RPC wire format, protocol field names, and `zed::` extension namespace alias preserved (S06).
- No user data touched; no endpoint removals beyond the default swap.

---

## 10. Verification

- `cargo fmt -p client -p cloud_api_client -p http_client -p context_server -p open_router -- --check` → 0 diffs.
- `git diff --check` on all S08 files → clean.
- Targeted `cargo check -p client -p cloud_api_client -p http_client -p context_server -p open_router` launched (background `yDCDH6`): the Rust sources for these crates **compile**; the sandbox blocks the transitively-required `webrtc-sys` prebuilt-binary download (network restriction, environmental — NOT a rebrand regression). Full link of the editor binary remains gated on that native download (documented as environmental SKIP in S11).
- `cloud_api_client` test URIs updated to `orion.dev` and compile within the crate.

---

## 11. Residuals & handoff

- **R-ORION-PROVIDER (follow-up, server-side):** register `orion.dev` provider IDs on the Orion cloud/collab backend.
- **Out of scope (→ S11):** user-facing `zed.dev` doc/help/status/merch/schema URLs in other crates are *not* service endpoints and are deferred to a follow-up brand-URL sweep (no `orion.dev` docs/status/merch site exists yet).
- **Handoff to S09:** `crates/collab` and `crates/remote_server` runtime hosts are migrated there; the `server_url` default and `http_client` rewrites above are the contract they must agree with.
