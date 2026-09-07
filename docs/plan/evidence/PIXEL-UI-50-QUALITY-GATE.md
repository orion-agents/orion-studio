# PIXEL-UI-50 Quality Gate

Status: GO-WITH-CONDITIONS

## Decision

The implementation is suitable for an opt-in local Orion Studio Dev build. No production
behavior regression was found by the automated and deterministic visual gates. The gate is
conditional because the plan's exhaustive manual interaction matrix and before/after
performance baseline were not available in this execution; those items are not represented as
passing.

## Verified gates

- Theme fixture: JSON parse, bundled registration, dark/light appearance, global-name
  uniqueness and hexadecimal color validation passed.
- Accessibility: Night and Dawn sampled semantic pairs passed the plan's APCA body/UI/support
  thresholds. Night body values were approximately Lc 87.9/87.6, muted text 61.2,
  placeholder 51.9, disabled 46.6, muted icons 51.9, line numbers 46.2, terminal dim 46.5,
  accent 70.4, success 70.9, warning 75.3 and error 49.4.
- Icons: all 14 allowlisted SVGs are valid XML; icon tests passed; filenames, view boxes and
  registrations remain unchanged.
- Icon scale matrix: 56 exact-size PNGs covered all 14 allowlisted icons at 10/12/14/16 px; all
  had non-empty alpha bounds and the expected dimensions. Manual contact-sheet and One Light
  Project Panel review found no clipping or unreadable silhouette.
- Shared UI: 84 UI tests and 41 doctests passed, including the new hard-shadow and Pixel Chrome
  token assertions.
- Product behavior: four Project Panel, three Terminal, one Status Bar and three Agent UI exact
  regression tests passed.
- Visual runner: final release runner SHA-256 is
  `396d2969c69031885cc8adb8bffd46bdf8914902afc6d2946fea11f1ba0b7356` and embeds cleanup
  commit `fd64ea8`.
  Night and Dawn Project/Editor/Agent captures each matched their current-SHA baselines at
  100.00% with zero differing pixels. Missing-theme and unmatched-filter cases exited 1.
- Visual fixture lifecycle: 63 stale, exact hash-matched runner fixtures totaling 1,260 KiB
  were removed. Project, Agent, missing-theme and unmatched-filter paths all left the fixture
  count at zero after the `TempDir` cleanup fix; a final commit-matched Agent run also left zero.
- Existing-theme compatibility: One Dark, One Light, Ayu Mirage, Gruvbox Dark and Gruvbox Light
  each passed Project/Editor/Agent baseline and comparison runs (30 serial invocations total,
  100.00% and zero differing pixels). Representative One Light and Gruvbox Dark captures were
  also inspected manually for contrast, clipping and global Pixel Chrome compatibility.
- Quality: `cargo fmt --all -- --check`, `git diff --check`, the affected app check and
  arm64 `./script/clippy -p orion-studio` all passed. The implementation commit also passed
  full `./script/clippy`; the post-cleanup full-workspace rerun was attempted twice but both
  attempts stopped in `webrtc-sys` while downloading its prebuilt archive with an external TLS
  `unexpected-eof`, before any lint failure. This is recorded as an infrastructure limitation,
  not as a current full-workspace pass.
- Non-font asset delta: the 15 changed asset files grew from 10,278 B to 27,991 B, a net
  increase of 17,713 B (approximately 17.3 KiB), far below the 2 MiB gate.
- Scope: no diff exists in GPUI, renderer, persistence, migration, schema, default settings,
  updater, signing infrastructure or `Cargo.lock`; no production branch depends on a Pixel
  theme name.

## Not verified

- The complete manual matrix across UI fonts 14/16/18, all densities, 1x/2x devices, reduced
  motion, every menu/modal/popover keyboard flow and every product interaction was not run.
- No trustworthy pre-change build with the same source/configuration was retained, so the
  plan's three-run startup/frame/RSS regression percentages and p95 frame-time comparison are
  `NOT VERIFIED`. Current-build measurements must not be presented as a before/after result.
- The existing `benchmarks` package is not an acceptable substitute: its full feature tree
  contains 82 `test-support` feature matches. Under the repository's `gpui-bench` contract,
  measured runs from that graph would not be production-shaped and were deliberately not run.
  A valid follow-up requires an isolated `bench-support` package and identical benchmark code
  on `84ce9a6` and `1fadd0ca`.
- CI does not currently consume these local PNG baselines; they are local evidence only.

## Conditions

1. Keep Night/Dawn opt-in; do not change fresh-install defaults in this change.
2. Complete the remaining manual matrix and repeatable performance baseline before calling the
   Pixel UI fully release-qualified.
3. Any defect found by that matrix returns to its owning UI task; UI-50 must remain production
   diff-free.

TASK: UI-50
STATUS: GO-WITH-CONDITIONS
SCOPE CHECK: PASS
BEHAVIOR CHECK: PASS FOR AUTOMATED MATRIX; MANUAL MATRIX NOT VERIFIED
VISUAL CHECK: PASS FOR LOCAL DETERMINISTIC MATRIX
RESOURCE CHECK: NOT VERIFIED FOR BEFORE/AFTER PERFORMANCE
