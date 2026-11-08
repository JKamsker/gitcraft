# Shared brief: implementing visualizations of gitcraft issue #175

You are one of six parallel agents. Each implements TWO of the twelve visualizations in
issue #175 (full text: `/Users/jonas/.t3/worktrees/gitcraft/insights-briefs/issue-175.md`;
read it, including the "UX / technical constraints" and "Minimum acceptance criteria"
sections, which apply to every item).

gitcraft is a Rust (edition 2024, toolchain `cargo +1.95.0`) egui 0.36 desktop git client,
~300k lines. Read `AGENTS.md` in your worktree.

## The scaffold that already exists (commit 646092a, branch `feat/insights-175`)

Read these files first; they are the contract:

- `src/insights/mod.rs`: `Provenance` (source, scope, tips, collected, partial, stale; builder
  methods; `check_tips`), `Evidence` (Observed / Inferred / Unknown), `folder_at`.
- `src/insights/history.rs`: `history::read(repo, rev, limit) -> Result<History, String>` —
  last N commits with per-file numstat (renames, binaries; merges carry no files), plus
  `History::provenance()`.
- `src/repo/insights/mod.rs`: `Kind` (12 views), `InsightsView` (holds one `State` per view,
  `table: bool` = table fallback mode), `InsightsAct` (has one variant per view wrapping that
  view's `Act`), dispatch to per-view functions.
- `src/repo/insights/page.rs`: the page chrome (header with close / reload / table toggle,
  nav list). Your view draws below the title.
- `src/repo/insights/common.rs`: `Slot<T>` (start / accept: only the latest answer is taken),
  `slot_state`, `loading`, `failed`, `unavailable`, `empty`, `provenance(ui, &Provenance)`,
  `Mark` + `legend` + `paint_mark` (marks differ by shape, never colour alone),
  `evidence_segment` (solid/dashed/dotted lines by Evidence), `heat`, `diverging`, `tint`,
  `text_on`, `lerp`, `heading`, `table_row` (simple clickable table rows).
- `src/repo/insights/heat_grid.rs`: virtualized rows×columns grid with pinned labels,
  tooltips and a keyboard cursor; returns `Picked { cell, row, column }`.
- `src/repo/insights/graph.rs`: node-link widget: `Graph { nodes, edges }`, `Node`, `Edge`,
  `NodeShape`, `arrange(&graph, Arrange::{Columns, Circle, Force}) -> Vec<Pos2>` (unit
  square; compute once when data arrives, ideally in the background, and store it),
  `show(ui, salt, &graph, &positions, selected, min_height) -> Picked { clicked, hovered }`
  with `Pick::{Node(i), Edge(i)}`.
- Each view has a stub file `src/repo/insights/<view>.rs` with this fixed interface (keep the
  names and signatures; `mod.rs` calls them):

  ```rust
  #[derive(Default)] pub(crate) struct State { .. }
  #[derive(Clone, Debug)] pub(crate) enum Act { .. }
  impl RepoTab {
      pub(super) fn <view>_load(&mut self, ctx: &egui::Context)   // first show + Reload
      pub(super) fn <view>_act(&mut self, act: Act, ctx: &egui::Context)
      pub(super) fn <view>_back(&mut self) -> bool                 // Esc: deselect first
      pub(super) fn <view>_ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>)
  }
  ```

  The view's state lives at `self.insights.as_mut().unwrap().<view>` (an `Option<Box<..>>` on
  `RepoTab`; handle `None`). The view issues its own acts as
  `Action::Insights(InsightsAct::<Kind>(Act::...))` (import `super::InsightsAct`).

## How the codebase works (what you'll need)

- Background work: `self.then(ctx, move || work(), move |tab, result, ctx| { .. })`
  (`src/repo/messages.rs`). Never run git or network on the UI thread. Pair with
  `Slot::start`/`accept` so a stale answer is dropped. `RepoTab` is in `src/repo/mod.rs`;
  child modules of `repo` can read its private fields (`self.path`, `self.refs`,
  `self.worktrees`, `self.prs`, `self.ci`, `self.detail_hash`, `self.hosting`,
  `self.versions`, …) — explore as needed.
- Git: `crate::git::run(Some(&path), &[args])`, `git::resolve`, `git::merge_base`,
  `git::diff_files`, `git::range_diff`, … (`src/git/`). Remote repositories (WSL/SSH) go
  through the same calls; keep using them, never `std::process::Command` for git.
- Navigation to evidence (push onto `acts`): `Action::SelectHash(hash)`,
  `Action::OpenDiff(DiffKey::Commit { hash, path })`,
  `Action::OpenDiff(DiffKey::Range { from, to: Some(to), path })`,
  `Action::OpenDiffOfLines { hash, path, lines }`,
  `Action::FileView { path, rev: Some(sha), tab: file_view::FileTab::{File,History,Blame} }`,
  `Action::BlameAt { path, rev, line }`, `Action::OpenPr(number)`,
  `Action::OpenCi { sha, run: Some(id) }`, `Action::OpenWorktree(path)`,
  `Action::Versions(VersionsAct::..)`, `Action::OpenCollisions`, `Action::OpenUrl(url)`
  (`src/repo/actions.rs`). A diff / file / CI / PR opened from the page is drawn over it;
  Esc returns to the page.
- Tests: unit tests next to the code (`#[cfg(test)] mod tests` or a `tests.rs` submodule).
  Real repositories: `crate::git::fixture::Repo::new("name")?` with `.commit(msg, &[(path,
  Some(text))])`, `.git(&[..])`, `.rev(..)` (`src/git/testlane/fixture.rs`). UI smoke tests:
  see `src/repo/insights/tests.rs` (`draw`, call `crate::theme::install_fonts(&ctx)` first) and
  `src/repo/doctor/tests.rs` (waiting for background work with `tab.poll(ctx)`).
- Style: match the surrounding code — doc comments in plain prose describing what a thing is
  ("The commits read, newest first."), comment density like the scaffold, no section-heading
  comments, no emojis. Keep files under ~800 lines; split into a directory module
  (`<view>/mod.rs` + parts) when bigger. No `unwrap()`/`expect()` in non-test code paths that
  can fail at runtime. Clippy is run with `-D warnings`.

## Requirements for each of your two visualizations

1. Pure, testable analysis in `src/insights/<view>.rs` (or a directory), no egui there, with
   unit tests on synthetic data AND at least one test against a real fixture repository.
   Include a test that a revision change (a tip moving / new commit) is detected or yields a
   different result, and a test of the fallback when the integration/data is unavailable.
2. The UI in `src/repo/insights/<view>.rs` (or directory): the picture, a provenance bar
   (`common::provenance`), a legend, hover details, keyboard reachability, a table fallback
   when `self.insights.table` is on (and a list/table somewhere for anything only reachable
   with the mouse), loading / error-with-retry / unavailable states, and click-through to the
   precise underlying evidence (commit, diff/hunk at the exact revision, file at revision, PR,
   run, worktree). Respect `crate::a11y::motion::reduced(ctx)` if you animate.
3. Honest semantics: never present inferred data as observed; unknown ≠ failing; partial and
   stale data are marked. Read-only: nothing your code runs may change the repository, the
   forge or CI (no fetch, no checkout, no reruns). Network reads only through the existing
   forge/CI tooling (`gh`/`glab` wrappers in `src/ci`, `src/forge`).
4. Bounded work: limits on commits/files/pairs read, with the limit shown as `partial`.
5. A UI smoke test that opens your view in a `RepoTab` (`RepoTab::new_deferred` on a
   nonexistent path for the unavailable state, and ideally on a fixture repo for the real
   state) and draws it in both picture and table mode without panicking.

## File ownership (to keep the six branches mergeable)

- You own: your two `src/repo/insights/<view>.rs` files (or directories) and new files
  `src/insights/<view>*.rs`. You may add `pub mod <view>;` lines to `src/insights/mod.rs`.
- You may add NEW files elsewhere for genuinely new low-level reads (e.g. `src/ci/<x>.rs`,
  `src/git/<x>.rs`) and the one-line `mod` declarations for them.
- Do not edit `src/repo/insights/{mod.rs,page.rs,common.rs,graph.rs,heat_grid.rs,tests.rs}`
  or other existing files beyond one-line module declarations. If a shared widget has a real
  bug or lacks something essential, work around it locally and describe the fix you'd want in
  your final report (the integrator applies shared changes). Do not remove the
  `#![allow(dead_code)]` lines in the two `mod.rs` files.
- Don't touch the other views' files.

## Building, testing, disk

- Use your own target dir and toolchain, always:
  `export CARGO_TARGET_DIR=<given below> CARGO_PROFILE_DEV_DEBUG=line-tables-only`
  then `cargo +1.95.0 check --all-targets`, `cargo +1.95.0 test --lib insights` (plus any
  other test filter relevant to code you add), `cargo +1.95.0 clippy --all-targets -- -D
  warnings`, `cargo +1.95.0 fmt`.
- On macOS, clippy reports pre-existing dead-code errors in `src/desktop_entry.rs` and
  `src/diagnostics.rs` (Linux-only code). Ignore exactly those; everything else must be clean.
- The disk is shared and nearly full. Before each test build check `df -h ~`; if under 8 GB
  free, stop building and report. Never run `cargo clean` on another dir, never build
  release, never run the whole test suite (only filtered `--lib` tests).
- Do not launch the GUI app. Do not start, rerun or wait on GitHub CI, do not open PRs, do not
  comment on issues.

## Committing

- Work only in your worktree on your branch. Commit each completed visualization separately
  (author is the configured git identity; verify with `git log -1 --format='%an <%ae>'` that
  it is `Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>`). Conventional commit
  subject like `feat(insights): add the CI critical path view (#175)`, a short body, and end
  the message with the line
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- After each commit: `git push -u origin <your branch>` and verify
  `git ls-remote origin <your branch>` equals `git rev-parse HEAD`. (`origin` is the fork
  `JKamsker/gitcraft`; pushing a feature branch triggers no CI.)

## Final report (your last message)

Keep it short: branch and final SHA; per visualization: data source, what it shows, how the
click-through works, the honesty/partial/stale handling, tests added (names) and their result;
the exact check/clippy/test commands you ran with pass/fail; known limitations; any shared
widget fixes you want the integrator to make.
