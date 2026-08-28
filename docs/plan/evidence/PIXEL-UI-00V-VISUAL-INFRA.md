# PIXEL-UI-00V Visual Infrastructure

Status: DONE

Final verification is recorded in `PIXEL-UI-EXECUTION-RECORD.md` and supersedes the draft
checklist below. The exhaustive manual matrix remains an explicit UI-50 condition.

## Scope

- `crates/zed/src/visual_test_runner.rs`
- `crates/theme_settings/src/theme_settings.rs` test module

## Implementation evidence

- The diff changes theme loading from `LoadThemes::JustBase` to bundled theme loading.
- The runner diff adds exact `VISUAL_TEST_THEME` selection, appearance checks, `VISUAL_TEST_FILTER`, skipped results, unmatched-filter failure, and theme-namespaced output/baseline paths.
- The usage text preserves presence-based `UPDATE_BASELINE` semantics and warns that `UPDATE_BASELINE=0` still updates.
- The theme-settings diff adds fixture parse, hexadecimal color, registration, appearance, and name-uniqueness assertions for Night and Dawn.

## Behavior boundary

- The visible diff is limited to the visual runner and a test-only module; no production UI component, schema, workflow, or `.gitignore` change is attributed to this task.

## Accepted plan variance

The final `visual_test_runner.rs` diff is 561 lines, exceeding the task's original 250-line
budget. The coordinator accepted this variance only for the user-authorized whole-goal execution:

- exact bundled-theme selection, appearance validation, namespaced output and filter/error
  behavior required more fixture plumbing than the planning estimate;
- latent blank Editor/Agent capture false positives required fixture-readiness handling;
- the runner needed explicit application shutdown, state flush/clear and executor draining so
  `ThreadStore` and related entities are released instead of leaking after captures;
- the original runner retained every fixture with `TempDir::keep()`. Sixty-three exact
  hash-matched fixture directories (1,260 KiB total) proved that process exit was not cleaning
  them. Commit `fd64ea8` now keeps `TempDir` alive only through runner shutdown, closes it
  explicitly, reports cleanup failures and returns a failing exit status when cleanup fails;
- 11 redundant `Arc` clones exposed by full Clippy were removed before the final build.

All excess lines remain runner-only. No production state, action, handler, schema, persistence or
theme-selection behavior depends on them. This accepted variance is not a standing increase to
future task budgets.

## Validation checklist

- [x] Implementation diff inspected.
- [x] Rust formatting, release build and affected-crate Clippy passed.
- [x] Theme fixture tests and deterministic visual captures passed.
- [x] Existing default-theme compatibility passed for the retained legacy path and five bundled themes.
- [x] Valid theme, missing theme, matched filter, unmatched filter and namespace behavior passed.
- [x] Fixture cleanup remained at zero after Project, Agent and both negative CLI paths.

## Remaining gate

The exhaustive manual UI-50 matrix remains a release-qualification condition; it is not a
runner-infrastructure blocker.
