# Resume notes: issue #175 (12 insights visualizations)

Not part of the PR: this branch only holds the briefs and these notes.

## Branches on origin (JKamsker/gitcraft)

- `feat/insights-175` — the integration branch, linear (one commit per feature). It has
  the scaffold, 10 views (ci_path, deploy, evolution, churn, ribbons, review_heat,
  constellation, conflict_radar, coupling, transform) and integration fixes
  (diff `pending_line`, `InsightsView.pr`, radar merges each pair once via
  `collision::merge`). Combined `cargo test --lib -- insights ci::`: 188 passed.
- Agent branches (all based on scaffold 646092a): `-ci`, `-history`, `-review`, `-branches`,
  `-patterns` (all done and cherry-picked), and `-analysis`:
  - `9d48a112` feat: change impact graph — done per the agent, NOT yet cherry-picked or
    reviewed.
  - `3ff60df7` wip: test coverage view — unfinished, never built. Finish it per
    `common.md` and the coverage section of the analysis brief (lcov/Cobertura, revision
    rule via a `<report>.sha` sidecar or recorded revision, otherwise unknown/stale; per-test
    only from lcov TN; test-lane verdicts; fallback; tests).

## Remaining steps

1. Cherry-pick 9d48a112 onto `feat/insights-175` (resolve `src/insights/mod.rs` with
   `python3 .insights-notes/union.py src/insights/mod.rs`), finish coverage, then squash
   the WIP into one `feat(insights): add the test coverage map (#175)` commit.
2. Impact and coverage should use `self.insights.pr` (the PR the page was opened over)
   besides `self.pr_view`; transform too (it picks from `self.prs` only).
3. Remove the two `#![allow(dead_code)]` lines (`src/insights/mod.rs`,
   `src/repo/insights/mod.rs`) and fix whatever is unused.
4. Optional shared improvements the agents asked for: a max-height option for
   `heat_grid::show` and `graph::show` (all wrap them in `allocate_ui` now), owned text for
   `Mark::Glyph`, multi-select in `graph::show`, and a boundary marker in the heat grid.
5. Full local checks: `cargo +1.95.0 fmt --check`, `cargo +1.95.0 clippy --all-targets -- -D
   warnings` (on macOS only the pre-existing dead-code errors in `src/desktop_entry.rs` and
   `src/diagnostics.rs` are expected), and the full `cargo +1.95.0 test --lib` (diff tests
   too: `DiffView::arrived` changed).
6. Independent adversarial review of the exact candidate, fix findings, then run the app
   to look at the page.
7. One PR `feat/insights-175` → `dev` on JKamsker/gitcraft ("Closes #175"), link it with
   `link_pull_request`; only then let CI run.

## Build environment

`export CARGO_TARGET_DIR=<a dir outside the worktree> CARGO_PROFILE_DEV_DEBUG=line-tables-only`;
toolchain `cargo +1.95.0` (the default stable cargo here is too old for edition 2024).
A fresh check takes ~40 s; a lib test build ~90 s and ~5 GB of target dir. The disk is tight.
