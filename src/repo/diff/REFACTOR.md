# Diff ownership slice for #198

Base: `a4d841713af3d31178a3b9212fd3c5adcb4172dc`.
Selection provenance: the diff-subtree changes from PR #199,
head `73f56b23ff8f1186de93d29317bc64508ab8f3c3` (read-only coordination).

## Ownership and compatibility

- `load.rs` prepares immutable diff data in the background. Existing loading entrypoints
  remain reexports from `diff`; patch construction and persistence formats are unchanged.
- `presentation.rs` owns the displayed-row/patch-line map and pointer selection transitions.
  Rendering, wrapped geometry, hunk lookup and navigation use that map. It borrows the data
  so it cannot outlive fold mutation or reload. Split filler slots remain `None`.
  Existing `DiffView` fields remain compatible with the independent central-state slice;
  keyboard and fold reset callers retain their current interface.
- `rows.rs` owns viewport virtualization. Its children own line painting, review/move/hunk
  affordances, labels and accessibility regressions. `view.rs` composes the pane; its children
  own the header, options and non-text ready/error/media presentation.
- Context reading/function geometry stays in `context.rs`; patch-preserving fold mutation
  and view-index restoration have separate owners. Markdown image access, binary reading,
  Office/XML/PDF extraction, image canvas interaction, API operation/schema comparison,
  dependency parsing, table alignment and registry metadata capture each have named owners.
- Existing test modules retain their qualified names. Test cohorts use `include!` inside
  the original namespace so Windows shard selectors continue to refer to the same tests.

No edits outside `src/repo/diff/**`, including `src/repo/state.rs`.
No persisted schema or public API changes. Internal selection accepts only changed patch
lines and defensively ignores invalid indices. Code-text input uses PR #199's caret,
word/line selection and explicit Git marker affordances.

## Local evidence and limits

- Baseline: 17 Rust files above 500 physical lines. After: zero; maximum 498.
  Physical counts include comments and blank lines, across every Rust file in this subtree.
- `rustfmt --edition 2024 --check src/repo/diff/mod.rs` and formatting checks on every
  individual subtree Rust file pass; `git diff --check` passes.
- Five new presentation tests pass with `rustc --test` against the actual presentation source
  and minimal type fixtures. This isolates mapping and selection transitions from GUI/build
  dependencies; it is not an application test run.
- A standalone Rust harness compiles the actual rich API/schema/operation, dependency parser,
  table/alignment, config, JSON, tree, prose and word modules. Its 32 CSV, 32 OpenAPI JSON and
  32 npm JSON comparison outputs match the base exactly. Localization and unrelated formats
  are stubbed; YAML/TOML, network, UI and platform behavior are not validated by this harness.
- Static inventory preserves every preexisting test function name. PR #199 contributes
  twelve pointer regression tests; this slice adds five presentation tests. Existing GUI,
  Git/process and tool integration tests were not executed.

The integration owner must run the full Cargo build/tests/clippy, Windows qualified-test
shards and independent adversarial review on the combined candidate before pushing.
No CI or risky process tests were started; no push or PR was made by this slice.

## Required integration and remaining acceptance

Bring in PR #199's two `diff-pick-line` locale entries and its guide changes outside this
slice. Do not reapply its old versions of the refactored Rust files. The pointer harnesses
and changed header fixtures are already integrated here. No central state adaptation is
required; a later state-owner migration can move the retained fields and call this owner.

Repository-wide size enforcement, other oversized files, central feature ownership,
command grammar, recovery journal, settings, preflight and CI reliability remain the
responsibility of the other #198 slices. This work does not complete #198 or #196's final
combined-candidate acceptance gates.

## Adversarial follow-up

The presentation owner now coordinates invalidation of both layouts' hunk/change caches,
remaps keyboard and text/action selection on fold edits, and clears wrapped geometry at
fold/reload boundaries. Scroll-based keyboard targeting converts physical pixels through
wrapped geometry before looking up the displayed hunk. Existing state fields/interfaces
remain compatible with the central-state refactor.

Three new regressions cover populated caches before unfolding, wrapped scroll targeting
for stage/unstage/discard patch generation in both layouts, and reload invalidation without
folds. An isolated source harness runs these and eight existing pure presentation/context
tests (11 passing); targeted Clippy passes with warnings/unwrap/expect/panic denied.
The harness uses actual production mapping, fold mutation, view restoration, parsing and
patch sources, with minimal application/highlighting fixtures; it does not execute GUI
keyboard events, Git workers or a full Cargo build. The wrap renderer's denied `expect`
is removed, but its egui rendering path still needs the centralized application checks.

Only PR #199's five guide/localization files were additionally ported; no older Rust
implementation was overlaid. All 136 diff Rust files remain <=500 lines (maximum 498).
The repository-wide checker still reports 203 oversized files outside this subtree on
the inherited combined branch; those require the parent integration's remaining slices.

## Remaining re-review findings: physical targeting and geometry identity

The combined parent already contains `dbc331c0745667d87ca98283a33189e802503871`
and the diff changes from `f558d25c4d0ecb4e28918981a50926e800ff96e9`.
Comparing its diff subtree to the latter leaves only the parent's five follow-up
visibility/import/qualification repairs; none of the accepted changes were dropped.
The five guide/locale files from that fix are also tree-identical in the parent.

Keyboard scroll targeting now adds the three-line lookahead in physical pixels,
bounded by the real scroll area's viewport, before converting to a displayed row.
The enclosing hunk retains its context, even when its header has scrolled above the
viewport. Headers outside the viewport cannot become a new scroll target.
Fold row mutation issues a fresh process-wide geometry revision through the
presentation owner, included in the actual egui cache key. It cannot collide with
another folded copy of the same original source merely because row counts match.
No persisted fields or public UI entrypoints changed.

Validation for this follow-up (source harness, not application validation):

- All 11 prior pure context/presentation tests retain their names and pass.
- `presentation::tests::tall_context_keeps_all_three_operations_on_its_enclosing_hunk`
  compares complete generated stage/unstage/discard patches for both layouts and
  four physical context heights, including the review's 500..880 viewport with
  the second header at 1957/1938. It also checks nearby and bottom-edge headers,
  empty viewports and unwrapped targeting.
- `context::tests::final_one_line_unfold_invalidates_cached_geometry_with_unchanged_source_and_counts`
  uses egui's real temporary cache and headless font metrics, holding its original
  geometry Arc across final unfolding of a 3000-character context line. It checks
  unchanged source and (4,3) row counts, cache hits before/after, measured height,
  wrap coordinate transitions, split transitions and multi-row selection rectangles.
- The focused harness compiles actual production presentation, fold mutation,
  view restoration, parse/patch, wrap and line-job sources against existing egui
  artifacts. Application types/highlighting and theme wiring use minimal fixtures;
  there is no backend, event dispatch, rendering, Git mutation or process fixture.
- 13 tests pass. Direct targeted `clippy-driver` passes with warnings,
  unwrap/expect/panic denied. Restoring each original defect in temporary source
  copies makes its new regression fail (wrong complete patch / stale geometry).
- Recursive diff-module rustfmt and `git diff --check` pass. Working-tree and staged
  full-repository physical-size checks still fail on 19 inherited files outside
  this owned subtree; every diff Rust file remains at most 498 lines.

No full Cargo build, application UI/process/network tests, CI, push, PR or independent
review was performed. The implementation parent must run combined-candidate gates.
