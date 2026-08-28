# PIXEL-UI-52 Final Gate

Status: GO-WITH-CONDITIONS

## Candidate decision

`GO-WITH-CONDITIONS` for the local opt-in Orion Studio Dev build. Visible launch, repository
open, live theme discovery, full-app Component Preview and quit/restart checks passed. The
conditions apply to exhaustive manual/performance qualification and public distribution, not
to this local installation.

## Source and scope

- Branch: `ui/orion-pixel-atelier`.
- Plan base: `84ce9a64917461fc48fd0b5ae985ee6139d6d5a0`.
- Implementation: `1fadd0ca63d78976ea02958c773a59e8eb6cadef`.
- Visual runner cleanup: `fd64ea82c48e4e08c0d79c2f6cd18b73ed009e36`.
- Implementation diff: 34 files, 1,535 insertions and 247 deletions.
- Theme fixture SHA-256:
  `bbaf045db023bf092d8b27fff48519a0bf20aa8c7c6a5f4aa0b6d1153aa0f4c8`.
- `Cargo.lock` and `assets/settings/default.json` have no diff.
- Brand delta gate: no new unapproved product-brand hit. The required upstream theme-schema URL
  is hash-bound in the existing allowlist format; the remaining 54 findings are confined to two
  unchanged baseline legal/research files.
- Non-font assets increased by only 17,713 B across the 15 changed asset files, below the 2 MiB
  stop threshold.
- UI-51: `SKIPPED-BY-DECISION`; themes remain opt-in.

## Build and installed artifact

- Main app/CLI release build: PASS in 57m03s.
- Remote server release build: PASS in 25m03s.
- Bundle/install command: `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 ./script/bundle-mac -i
  aarch64-apple-darwin`; exit 0.
- Installed app: `/Applications/Orion Studio Dev.app`, 411 MiB.
- Bundle ID: `dev.orion.OrionStudio-Dev`.
- Version/build: `1.16.1 (20260828.131522)`.
- Main executable: arm64; SHA-256
  `87354bda8d7aa369b1c0f884dac74e99bad720fbfb2f1c1477b093e276b4861e`.
- `codesign --verify --deep --strict --verbose=2`: PASS.
- Signature: ad-hoc, no TeamIdentifier. Suitable for this machine's local use, not evidence for
  public distribution, Developer ID signing or notarization.
- The installed executable contains both Pixel theme names and embedded commit `1fadd0ca...`.

## Regression gates

See `PIXEL-UI-EXECUTION-RECORD.md` and `PIXEL-UI-50-QUALITY-GATE.md` for exact focused tests,
Clippy scope, visual hashes and explicit non-verified items. The implementation commit passed
the full workspace Clippy gate. The exact post-cleanup runner source passed affected-crate
Clippy; two extra full-workspace reruns stopped before linting completed because the external
`webrtc-sys` archive download ended with TLS `unexpected-eof`. The only build warnings were the
existing macOS `__eh_frame` compact-unwind size warning and the future-incompatible dependency
notice for `block v0.1.6`.

## Runtime smoke

- First installed-process start: PASS; PID 19510 ran from the installed bundle with the
  repository path, exposed one visible window and reached 135,648 KiB RSS immediately before
  the quit check.
- Visible CoreGraphics window/open project: PASS. Project Panel displayed root `orion-studio`;
  README opened in the editor; Terminal, Agent and Project surfaces were observed in the
  installed app.
- Live theme discovery: PASS. `Settings -> Select Theme...` filtered by `Atelier` displayed
  `Orion Pixel Atelier Dawn` and `Orion Pixel Atelier Night`. Dawn previewed live; Escape
  cancelled without persisting a default change and restored the prior dark theme.
- Full-app Component Preview: PASS. Command Palette resolved
  `workspace: open component preview` and the installed app rendered Agent, Data Display and
  Forms & Input previews.
- Quit/restart: PASS. PID 19510 exited cleanly; installed app restart created PID 36562 with one
  visible `orion-studio` window and the repository argument. The app remains running for local
  inspection.
- Migration/database fatal error: none observed.
- Unrelated local warnings observed, not repaired in this visual goal: one skill lacks YAML
  frontmatter; the global tasks/debug-scenarios files are empty JSON inputs; the user shell
  reports `compdef: command not found`; and the local model/provider endpoint was unavailable.

## Live screenshot hashes

| Evidence | SHA-256 |
|---|---|
| Initial visible installed window | `8aad2de1b869a7b07c5cc29be9609e074c05deccd2929080e7f8feb891bf21fc` |
| Visible `orion-studio` Project Panel | `6375b98b6eaf6d257d0d574455938090d57c26dc2932e7a02e524ec42dcaeb72` |
| Live Night/Dawn theme selector | `e2565e031244fd6b8258c482f0627bc5751ad1cd02514acaa5d4de1fd70ca69a` |
| Restarted installed window | `55cc45ad025ae8d36ee373d4c73bbf143d974643522aa9fd1285fef4d2ad832c` |
| README editor + Terminal + Project Panel | `12c99d7b3a3e2d7860ef21660d25cde93825501cae6e6f5232171a7174c1e94e` |
| Component Preview command resolution | `7b0dd44352a0280d27849124e4dc12e6e3add7990287b95f3a5d97d612f2324f` |
| Full-app Component Preview open | `8bfb45332864a65b24b10827422ea2276b00a7a771a183d5a314482784b56be5` |

The local PNG files were deleted after hashing; the table is an audit index, not a committed
visual-baseline store.

## Resource cleanup

- Before final cleanup: `target` 42 GiB (32 GiB arm64 target, 10 GiB host release cache),
  113 GiB free disk.
- `cargo metadata` resolved the exact target directory to
  `/Users/hope/ai-project/orion-studio/target`.
- `cargo clean` removed 91,105 files and 42.0 GiB; the target directory is absent and free disk
  increased to 154 GiB.
- Forty-one exact `/private/tmp/orion-pixel-*` screenshot/log/raster entries were deleted after
  evidence hashes were recorded; zero remain.
- `/Applications/Orion Studio Dev.app` remained running and a post-clean strict/deep signature
  verification passed.

## Remaining conditions

1. Complete the remaining UI-50 exhaustive font/density/keyboard/reduced-motion matrix and a
   production-shaped before/after performance baseline before calling Pixel UI fully
   release-qualified.
2. Obtain Developer ID signing and notarization before public distribution; the current ad-hoc
   signature is local-only.
3. Treat the observed user-config warnings as a separate maintenance goal; they are not part of
   this visual implementation.

TASK: UI-52
STATUS: GO-WITH-CONDITIONS
SCOPE CHECK: PASS
BEHAVIOR CHECK: PASS FOR AUTOMATED AND INSTALLED-APP SMOKE
VISUAL CHECK: PASS FOR LOCAL RUNNER AND INSTALLED APP; EXHAUSTIVE MANUAL MATRIX NOT VERIFIED
RESOURCE CHECK: PASS FOR ASSET AND DISK GATES; BEFORE/AFTER PERFORMANCE NOT VERIFIED
