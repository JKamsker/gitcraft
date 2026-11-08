# Visualization UI-R1-1 correction review

**Scoped verdict: PASS — UI-R1-1 is resolved at application commit `33a74a5ed917a4ef27d9944f67ee5799dbc8de30`. Full visualization batch remains FAIL.**

Read-only independent review used `git show`/`git diff` for immutable objects in `/Users/jonas/.t3/worktrees/gitcraft/t3-768b22a8`. Mutable working-tree application files, including separate lifecycle edits, were not reviewed. Compared the relevant correction against original reviewed visualization commit `39d8c4cecd63fbb225d6a32f8dac1186aab538ea`. Documentation followup `15b217bd` was checked: its diff from application commit 33a74a5e changes documentation/evidence files only.

## Correction assessment

- `src/repo/graph_walk.rs:105` now chooses `self.selected.or(focused)` instead of ignoring the graph's focused/list-stop row.
- `src/repo/graph_walk.rs:114–124` explicitly detects WIP, stash, invalid or absent origins and enters the loaded HEAD when it is a real commit, otherwise the first real loaded commit. WIP/stash rows remain excluded from topology edges, but no longer trap all arrows on their empty adjacency lists.
- Real commit origins retain edge walking (`:125–128`), so entry through the pseudo-row does not replace subsequent topology navigation with row traversal.
- `src/repo/graph_view/mod.rs:425` passes the existing `ringed` origin into graph_walk. No-selection navigation therefore follows the supplied focused row instead of raw row zero.

This resolves the original default dirty-repository WIP trap, stash trap and ignored no-selection focus origin. No concrete remaining issue within UI-R1-1 was found.

## Evidence reviewed

Reviewed the added tests in immutable `src/repo/graph_view/readability_tests.rs`: synthetic ArrowDown events through graph rendering assert WIP→HEAD→parent and stash→HEAD; a synthetic key through arrow_keys with supplied focused row 5 asserts selection of its parent row 6. The latter covers origin propagation, not native Tab traversal or screen-reader behavior. Tests intentionally inspect emitted Select actions and update the synthetic cursor instead of starting Git reads against their nonexistent fixture paths.

Read `15b217bd:docs/issue-51/reviews/visualization-round1/parent-check.json` and decompressed its tracked `parent-topology-key-tests.log.gz`. The reported command is `CARGO_BUILD_JOBS=2 cargo +1.95.0 test --locked --lib topology_arrows -- --nocapture`, bound to 33a74a5e; log shows **2 passed, 0 failed, 0 ignored**. Independently verified uncompressed log SHA-256 `6ce31932f30268fb48a3efdd785c28db2f235d07156097869e36510f6f65ec85`, matching parent-check.json. No tests were rerun for this review.

## Remaining verdict boundary

UI-R1-2 (linked-worktree band persistence), UI-R1-3 (release narration overcount), UI-R1-4 (mode toggle loses pending narration) and UI-R1-5 (chained operations erase pending feedback) remain open. This scoped PASS does not approve those findings, lifecycle changes, native accessibility/audio, other reviewers' findings or the full combined candidate.

No edits to tracked files, commits, pushes, CI or process tests occurred.
