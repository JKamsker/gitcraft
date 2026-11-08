# Visualization/audio UI adversarial review — round 1

**Verdict: FAIL — five blocking functional findings.**

Reviewed immutable commit `39d8c4cecd63fbb225d6a32f8dac1186aab538ea`, branch `t3/history-visualization-audio`, worktree `/Users/jonas/.t3/worktrees/gitcraft/t3-680bf798`, against baseline `a850de26b0a0778ea96fe34cd1498e6e78928492`. The application commit is `0b5718ce575d3ba6332ca0ae6ea96f90d6493073`; `git diff --name-only 0b5718ce..39d8c4ce` contains only `docs/issue-51-visualization-audio.md`. Worktree was clean before and after review.

Scope: exact #46 graph-band/minimap/celebration requirements, #39 topology navigation/narration/audio, #38.14 explanatory motion, their graph/focus/layout/lifecycle integration and new native dependency wiring. Other reviewers own analytics/export correctness. Read original issue bodies from `/tmp/gitcraft-issue51-issues.json`, AGENTS and the writer evidence document. No edits, CI, full suite or unsafe/process-control tests were performed. Findings below are source-verified reproducible control/data-flow defects; the existing focused test executions did not reproduce these missing cases because their fixtures omit them.

## Blocking findings

### UI-R1-1 · P1 · Topology navigation cannot leave the default changes row in a dirty repository

**Sources:** `src/repo/graph_walk.rs:37`, `:104`, `:112`; `src/repo/graph_view/mod.rs:406`; `src/repo/load.rs:662`.

`Topology::build` entirely skips the WIP pseudo-commit (and stash rows), so those rows have neither parents nor children in Topology. `graph_walk` starts from `self.selected.unwrap_or(0)`. The topology branch of `arrow_keys` replaces the ordinary row navigation and returns unconditionally, including when topology.walk returns None. `restore_selection` normally selects WIP when opening a dirty repository.

**Concrete reproduction:** open any repository with at least one commit and uncommitted work; enable Walk graph topology; keep the default changes-row selection, Tab to the graph and press Down/Up/Left/Right. Every direction produces “No loaded commit in that direction”; no real commit is selected. Choose a parent or child is empty on the same row. Selecting a stash has the same trap. On a clean graph with no selection, the same implementation also ignores the `ringed` row supplied by the graph list and starts at raw row zero, even if keyboard focus is elsewhere in the scrolled list.

**Impact:** the feature intended to give keyboard/screen-reader users topology navigation breaks the ordinary entry point on dirty repositories. This is not an acceptable loaded-boundary condition: HEAD is already loaded and the WIP commit records HEAD as its parent (`load.rs:568–574`).

**Required correction:** define WIP/stash entry behavior, for example Down enters loaded HEAD/base, or resolve the first real commit from the actual focused row. Preserve focus/selection identity rather than silently falling back to row zero. Add actual graph-harness keypress tests from default WIP, stash, no-selection/scrolled focus and normal real-commit selection. The current test directly calls graph_walk from real rows; its AccessKit harness only inspects a label and does not press an arrow.

### UI-R1-2 · P2 · Branch-band preferences do not persist for linked worktree tabs

**Sources:** `src/repo/mod.rs:532`; `src/repo/branch_bands.rs:35`, `:39`, `:259`; `src/repo/worktree_tab/mod.rs:205`.

Constructor loading is `Preferences::load(ctx, &path)`, using the opened worktree path. Saving is `insert_persisted(id(self.repo_key()), ...)`. Once the repository has loaded, repo_key resolves to its main worktree's path. No later load/reconciliation of branch_bands exists when worktrees become known, and continue_from does not transfer these preferences.

**Concrete reproduction:** create/open a linked worktree at `/repo-feature` whose main worktree is `/repo`; choose GitFlow or pin a branch in Graph tools; close/reopen the linked-worktree tab or restart the app. The preference was written to the `/repo` key but reopening looks up `/repo-feature`, so the tab starts in Compact/default pins. Opening the main worktree can show the setting that appeared lost in the linked worktree. Two linked tabs can also display inconsistent preferences for their shared repository.

**Impact:** #46.1 promises repository-scoped model/band persistence and stable presentation across reload/reopen, but a supported repository-entry path loses it. This is a persistence defect, not a limitation stated in the issue or evidence.

**Required correction:** consistently key loading/saving by the resolved repository identity, load when that identity is learned (the existing graph-order flow is a useful pattern), and define how open sibling worktree tabs observe a change. Test main plus linked-worktree save/reopen and worktree switching.

### UI-R1-3 · P2 · Release narration counts integrations made into other branches as merges into release

**Source:** `src/repo/graph_walk.rs:278–284` (especially line 283).

For each release branch, the implementation walks *all* ancestors of its tip and counts every merge that introduces a descendant of the selection. That includes merge commits made on main or another feature before those commits themselves were merged into release. It then labels the sum “selected ancestry introduced by N loaded merges into release/...”.

**Concrete complete-history reproduction:** use this DAG, where listed parents are ordered:

- R: root.
- F: feature commit, parent R.
- M: merge on main, parents R and F.
- L: merge on release/v1, parents R and M.
- refs: feature/x → F, main → M, release/v1 → L; select F.

Only L is a merge into the release first-parent line. The code's selected-descendant set contains F/M/L; release ancestors contain L/M/F/R. Both M and L satisfy the predicate, so narration says **2 loaded merges into release/v1**. The actual feature integration into release occurred once. All ancestry is loaded, so its partial-history warning does not qualify the wrong count.

**Impact:** this is a core requested topology explanation, and the screen reader receives a confidently incorrect account of the branch shape.

**Required correction:** define and compute observed integrations on the release's own first-parent line, with explicit coverage when that line is incomplete, or change the metric and wording so it accurately names ancestry-wide merge events without claiming they happened into release. Add an independent expected-DAG test for feature→main→release, direct release merge, repeat integration and incomplete release first-parent history. Current tests do not assert advanced narration counts.

### UI-R1-4 · P2 · Re-enabling topology mode can permanently suppress the selected narration

**Source:** `src/repo/graph_walk.rs:217–239`.

A pending narration stores its selected hash before its reader runs. Turning the mode off cancels the reader but leaves `graph_narration.hash` and its empty result intact. On re-enable, `commit.hash == graph_narration.hash` returns early and never queues a replacement. The same problem affects a completed result awaiting polling when the feature is disabled first.

**Concrete reproduction:** enable topology mode on a large loaded graph, select a commit so the worker is pending, switch topology off before its result is accepted, then turn it back on without changing selection. Basic shape text remains, but ahead/behind/shared-ancestor/release narration never arrives for that selection. Changing selection away and back or rebuilding the graph repairs it, which demonstrates the lost scheduling state.

**Required correction:** invalidate hash/result when cancellation invalidates the job, or distinguish completed data from the currently requested hash/generation and requeue when no valid result/job exists. Use a controlled blocked reader and real UI toggle/selection transitions to verify cancellation/re-enable, including quick repeated toggles. Avoid a sleep-based timing test.

### UI-R1-5 · P2 · Chained operations erase successful graph feedback before its confirming reload

**Sources:** `src/repo/graph_motion.rs:29–49`, `:77`; `src/repo/operations.rs:190`, `:295–302`; `src/repo/running.rs:142–155`; `src/repo/push/mod.rs:371–379`, `:411`.

`graph_motion_done(true)` arms feedback for a later repository read. `op_done` starts that asynchronous reload, then can immediately start the next queued operation or Sync's push. Every `graph_motion_begin`, including an unrelated push, assigns a new before snapshot (None for a push) and clears ready. The prior successful operation's pending snapshot is lost before its reload can be accepted.

**Concrete reproduction:** run a fetch that brings commits in and queue Push while it runs; when fetch completes the queued push begins immediately. Its graph_motion_begin clears the successful fetch snapshot, so incoming commits never get the promised new markers/highlight. Normal Sync also starts push immediately after its pull; that erases the pending pull ref/HEAD motion. A following fetch/pull can instead replace the previous snapshot and only explain part of the combined change.

**Impact:** explanatory motion is absent on existing supported operation chains; the claim that successful fetches are marked and confirmed pull changes interpolate is not true for these paths. `celebration_begin` similarly resets its one pending candidate before the confirming read if another operation starts; preserve confirmed candidates independently there as well when fixing shared operation feedback.

**Required correction:** distinguish in-flight operation capture from already-successful feedback awaiting read confirmation. Retain/merge pending snapshots by operation/read generation until consumed or invalidated; unrelated operations must not erase successful feedback. Test through op_done/queued action or Sync orchestration with delayed reload acceptance, rather than invoking begin/done/loaded only for a single isolated operation. Also test that a later failure does not celebrate/mark a previously unsuccessful action.

## Requirement coverage and nonblocking observations

- **Bands:** real model/pin UI, deterministic branch color keys, banded topology and ordinary/ghost painting integration exist. The tests cover merge edges and unrelated tip arrival, not linked-worktree persistence or the compact-to-banded asynchronous visual transition. Every rebuild first installs compact layout and later the band worker replaces it; consider keeping a compatible prior banded layout/loading treatment so users do not repeatedly see the supposedly stable model jump.
- **Minimap:** actual loaded graph bins, HEAD/tags/search markers, viewport rectangle, click/drag offsets and unloaded boundary are present. Existing click test exercises the minimap method in isolation, not its graph layout, drag rectangle, keyboard focus or LoadMore integration. The map caps shape at the first 24 lanes but the visible tooltip does not state that lane truncation; it should disclose the cap when graph.width exceeds it. Its direct widget_info call does not invoke the shared painted-focus-ring helper, so include this new widget in the next full #39 focus sweep. These are integration/coverage followups, not additional blockers in this review.
- **Topology/audio:** real edge walking, parent/child chooser, AccessKit shape labels, off-thread advanced narration and optional native rodio cues exist. The absence of a Git branch-creation timestamp is handled honestly as shared-ancestor date. Loaded ancestry limits are stated. Navigation from WIP and the advanced narration defects above prevent completion; basic helper tests do not cover them.
- **Celebrations:** default-off independent switches, success+later-read gates, explicit release designation and reduced-motion suppression are real. Existing tests use synthetic state and record cue dispatch rather than the whole operation/read path. Add a whole-flow success test and queued-success/read scheduling coverage. Consider documenting whether a merge performed by Pull is intentionally excluded: candidate detection currently accepts an explicit merge command/Continue merge, not Pull in merge mode.
- **Motion:** incoming badges/expiry timer, reduced-motion completion, animated pill offsets and HEAD scroll integration are substantive. Current tests validate isolated marker state and reduced scroll, not rendered moved-pill behavior, native scroll animation, offscreen/virtualized destination refs, moving multi-ref stacks or hitbox behavior during animation. Parent should preserve those gaps in final evidence instead of implying native interactions were exercised.
- **Lifecycle:** the new band/topology/narration/minimap and analysis caches/readers are not reset in existing `lifecycle::suspend` (`src/repo/lifecycle.rs:152–188`). A completed band reader can still apply its retained graph after suspension because poll_branch_bands runs before sleeping() returns in poll_as. This race/cache preservation deserves an integration test and lifecycle cleanup, especially alongside the analytics owners; I did not run the long-idle path and do not count it as an independently reproduced sixth blocker here.
- **Dependency/CI compatibility:** rodio is locked to 0.21.1 with default features off/playback on; local locked crate manifests show CPAL playback brings ALSA on Linux. All four existing Linux apt install sites in ci.yml/release.yml add libasound2-dev. No omitted Linux workflow install site was found. This is adequate source wiring, not a Windows/Linux native build pass. Native speaker/device-error behavior and macOS/Windows/Linux accessibility remain unexercised here. No new process-target/signaling implementation was found in these UI/audio modules.

## Executed verification

Copied the writer's existing application test executable to `/tmp/gitcraft-visual-ui-r1-tests`; original and copy SHA-256 both equal `02e15078252d750200ce93150155aee8f5fe2e65d9887652662e681d2eaadda2`. No compile or test-source modification was made. Executed each following filter with `--nocapture`:

| Filter | Passed | Failed/ignored |
|---|---:|---:|
| repo::graph_walk:: | 2 | 0 |
| repo::graph_view::minimap:: | 2 | 0 |
| repo::branch_bands:: | 3 | 0 |
| repo::graph_motion:: | 3 | 0 |
| repo::celebration:: | 3 | 0 |
| audio::tests:: | 1 | 0 |
| **Total** | **14** | **0** |

These tests confirm their implemented scenarios only. In particular, the topology AccessKit harness does not apply produced graph actions or exercise real arrow navigation; the single-operation motion tests cannot detect chained-operation erasure; no test asserts release narration output; the band tests do not save/reload a linked-worktree setting. Passing them does not negate the five source-verified findings above.

No app source changes, local commits, pushes, CI invocation or host-wide process inspection/signals occurred. Re-review is required on the corrected immutable application candidate and the final combined candidate.
