# PIXEL-UI-00 Baseline

Status: PARTIAL

The source/default/resource baseline is complete. A same-source pre-change runtime bundle and
repeatable performance trace were not retained, so those historical comparisons remain
`NOT VERIFIED` and are carried explicitly into UI-50 rather than reconstructed from stale data.

## Repository baseline

- Worktree: `/Users/hope/ai-project/orion-studio`
- Implementation branch: `ui/orion-pixel-atelier`
- Fork base: `origin/main@d593fdd3534b4541ba62cf38d23d3bccb56b39a8`
- Upstream integrated: `upstream/main@01acd0ee8e906dd0ec8b526fe08da94444a5e2af`
- Upstream merge: `e96fef3`
- Plan commit: `84ce9a6`
- Upstream integration conflict: `docs/src/migrate/vs-code.md`
- Resolution: retained Orion Studio product wording and adopted upstream's
  `outline_panel.folder_indicator` mapping.

The implementation branch is intentionally based on the refreshed fork main plus the six
new upstream commits. The release branch is not used for UI development.

## Worktree ownership

At branch creation, the only untracked file was
`docs/plan/orion-pixel-atelier-ui-plan.md`, created by the Orion Pixel Atelier planning task.
No user-owned source changes were present. The plan is now committed.

## Product defaults before Pixel UI

- Dynamic light theme: `One Light`
- Dynamic dark theme: `One Dark`
- Icon theme: `Orion Studio (Default)`
- UI font: `IBM Plex Sans`
- UI font size: `16`
- Buffer font: `Lilex`

These values are evidence of the pre-Pixel defaults. Theme defaults must not change before
UI-50 passes and UI-51 is explicitly executed. The icon theme name remains unchanged in v1.

## Architecture boundaries

- Colors are theme data.
- Pixel geometry is global Orion Studio Chrome in `crates/ui`.
- Project, Editor, Workspace, Agent and Terminal state remain unchanged.
- GPUI, renderer, theme schema, database and migration paths are forbidden.
- Product shell files allow style-only edits in explicitly named Render regions.

## Resource baseline

- Host memory: 16 GiB Apple Silicon.
- Free disk before implementation: approximately 165 GiB.
- Cargo build jobs: 2.
- Cargo, Clippy and visual runner must run serially.
- Existing shared `target` is reused; no parallel target directories are allowed.

## Upstream and brand checks

`git diff --cached --check` passed before the upstream merge commit.

`./script/check-orion-brand` currently reports pre-existing findings in generated legal
licenses and `docs/research/open-source-code-editors-research.md`. None of those files were
introduced or modified by the six-commit upstream merge. Legal attribution must not be
rewritten to hide those findings. Final evidence must distinguish this baseline from new
product-brand regressions.

## Runtime baseline

Current-source baseline bundle and screenshots were unavailable before implementation. The final
installed build, deterministic Night/Dawn captures and their hashes are recorded in
`PIXEL-UI-EXECUTION-RECORD.md`; they are post-change evidence, not a fabricated pre-change
baseline.

UI-52 must provide current-SHA evidence for:

1. a visible CoreGraphics window;
2. opening this repository;
3. Project, Editor, Terminal and Agent surfaces;
4. quitting and reopening normally; and
5. current bundle/binary hashes and resource usage.

Historical installed-app evidence is not accepted as proof for this implementation branch.
