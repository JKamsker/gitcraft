# History layout and presentation filters (#167, #171 items 1, 8, 11)

## Prior work and TableBuilder evaluation

The starting checkout is `78f4dc8b`. `25854b0` is not its ancestor; it is on
`t3/integrate-codex-branches`. Its descendants include `978fb9bb` (adaptive refs floor)
and `88821fc4` (message-tooltip ghost guard), as well as decoration indexing and
message-tooltip work. Those changes were inspected, not blindly cherry-picked: their
adaptive refs behavior does not solve independent width preferences or overflow, and
copying the whole branch would import unrelated features. This change preserves the
starting checkout's ghost preview, graph order, test lane, remerge badges, row selection,
branch pills and drag/drop.

`graph_view/layout_tests.rs::table_builder_prototype_supports_custom_paint_but_owns_another_width_state`
exercises the installed egui_extras 0.36.2 TableBuilder with constrained/clipped initial
and remainder columns, custom painting and 1,000 virtualized rows. Its column-local
persistent resize state would need replacing to meet independent saved preferences;
whole-row hit targets and lane/pill connectors spanning cells also need custom handling.
The existing renderer therefore keeps its virtualization and interactions and uses a
small bounded allocator instead of maintaining a second table width state.

## Width and interaction policy

All sizes are logical egui points, so UI zoom changes the viewport, not the preferences.
Description is flexible with a 140-point floor. Other columns have independent saved
preferences and these bounds:

| Column | Minimum | Maximum |
| --- | ---: | ---: |
| Branch / Tag | 70 | 2000 |
| Graph | 60 | 2000 |
| Commit | 50 | 400 |
| Author | 80 | 800 |
| When | 50 | 400 |
| Date | 50 | 400 |

The Author floor reserves an avatar, robot and some name space. Description absorbs all
spare width first. If the preferred widths do not fit, Graph, Branch / Tag, Commit,
Author, When, then Date surrender their slack in that fixed order. Legacy `last_resized`
and `column_push` settings remain readable but do not affect allocation. Neither viewport
compression nor a different divider writes unrelated preferences.

A divider takes separate starting snapshots of saved preferences, actual displayed widths,
Description allocation, available width and pointer press origin.
The initial offset within its 10-point hit area is retained. Each frame uses total pointer
displacement from that origin, including after overshooting and returning from a limit.
Dragging exchanges only its own displayed width with Description; growth stops when
Description reaches its floor. It does not cascade into other cells. A drag back to its
origin restores the starting preference and Description allocation, including automatic Graph
mode. The saved column width remains independent of all other saved column widths.

Description's exchanged space is an explicit persisted `description_space` preference,
containing its requested width and the viewport width of the gesture. At viewport `v`,
its allocation floor is `max(140, requested - max(0, gesture_viewport - v))`, capped so
visible column minima still fit. It receives any remaining space above that floor too.
The fixed squeeze order is unchanged. This separates a deliberate Description exchange
from automatic compression: with Refs/Graph preferred 280/100 and displayed 150/60 at
600 points, shrinking Refs 20 saves Refs 130, retains Graph preference 100, and requests
Description 160 at viewport 600. The next frame and release display Refs/Graph 130/60
and Description 160. At 580 Description gives back its 20 first; at 640 Graph can return
to 100. Revisiting 600 gives exactly the same layout regardless of intermediate widths.
Viewport changes never write this preference; another intentional divider gesture replaces
it, returning to the gesture origin restores it, and Reset columns clears it. No temporary
compression is copied into unrelated column preferences.

Integration with incoming dev's automatic refs fitting: capture `DragStart.prefs` after the
viewport's `fit_visible_refs` adjustment and `DragStart.shown` after final allocation. Keep
incoming `save_drag` disabling `auto_refs_width` for a changed Refs gesture. Also snapshot
and restore the original automatic-refs flag when returning to an unchanged gesture,
alongside the original raw width; do not overwrite the incoming badges, lane labels or lazy
message tooltip. The follow-up branch intentionally does not merge dev.

When minima cannot fit, one horizontal scroll area contains both the header and the
vertically virtualized rows. Headers, row cells, clips and handles derive their bounds
from `Cols::cell`, `description`, and `border`; cell padding is six points. Every metadata
cell and the WIP description are clipped. Long names, subjects and metadata use ellipses.
Graph fitting remains automatic until its own divider is dragged. Double-clicking that
divider or choosing “Fit graph column to its lanes” restores fitting. Reset columns keeps
visibility settings; hiding a column leaves its saved width untouched.

The attribution robot now follows the avatar in Author, before the name, retaining its
agent/mixed color and attribution tooltip. Description reserves no robot space. Hiding
Author hides its avatar and robot together. Hovering the avatar/name area shows the full
raw author name and email even if the name is truncated.

## Filter policy and identity verification

Settings → Commit graph contains global filters. The history's “History filters” popover
contains per-repository additions; “Override global history filters” replaces the global
rules, including allowing an empty override. Preferences use the main worktree repository
key and egui persistence. “Show hidden” reversibly bypasses both author and namespace rules.
The popover and its hover summary distinguish author rows hidden **in loaded history**
from namespace-only commits counted across the current history scope. Matching protected rows are counted
separately in the popover.

The default T3 author rule requires BOTH raw author name `T3 Code` and email
`t3code@users.noreply.github.com` (email comparison is ASCII case-insensitive). This pair
was read using `git show -s --format='%an <%ae>'` on a real local
`refs/t3/orchestration-v2/checkpoints/...` commit; it is not inferred from S16, which shows
no email. A similarly named author, a human author with a Co-authored-by trailer, or any
other agent attribution does not match. User names are exact and case-sensitive, emails
exact and case-insensitive, one entry per line; surrounding whitespace is trimmed.

Ref patterns are full `refs/` patterns, one per line, with default `refs/t3/*`. Invalid
patterns are reported and ignored. Each pattern becomes a separate `--exclude=...` argument
before `--all`, alongside the existing stash/gitcraft/notes/prefetch exclusions. Roots are
excluded, not ancestors: commits also reachable from a visible branch or HEAD remain.
The namespace-only count is the difference between unfiltered and filtered `rev-list
--count` walks. On a Hide/Solo branch scope, namespaces are not roots and that count is
zero. Refs moving between the two read-only count walks can temporarily affect the count;
a subsequent reload refreshes it.

History initial loads and pagination retain the same namespace snapshot; changing rules
reloads history and invalidates the older pagination generation. Pages additionally carry
the namespace snapshot and are rejected immediately when it differs from current rules,
even before that replacement reload finishes. Search resolves the same
namespace roots before using the existing safe search API. Attribution and backlinks use
the same namespace exclusions, including repository overrides and reveal mode. Exact
single-commit reads and rewrite/checkout operations do not apply presentation rules.

## Graph compaction and safety

The raw paginated history and operational Commit objects always retain their original
hashes, parents, refs, trees and authors. Filtering builds a separate layout projection:

1. Keep visible ref tips, HEAD, selection, explicit navigation/search targets, stash bases,
   and matching commits with an unloaded traversed parent **while `has_more` is true**.
   In first-parent mode only the first parent is traversed; side parents intentionally
   omitted by Git are never pagination frontiers. With a complete scope no missing parent
   can be resolved by loading another page.
2. For a hidden commit, follow **every** traversed parent path to its nearest visible
   ancestors (all parents normally, only first parents in first-parent mode).
   Iterative post-order traversal and cached frontiers avoid recursive stack overflow on
   long generated histories. Stable de-duplication preserves first-parent ordering.
3. Reconnect a visible child's layout parents to those frontiers. Feed this projection to
   the existing merge-aware graph layout. Operations and detail reads use original Commit
   objects. A separate `history_index` indexes raw paginated history for operational
   first-parent traversal and ancestry; `index` indexes displayed rows for selection and
   painting only. Quick message/drop/move eligibility, fold menus/picker/stopped-state
   detection, surgery/move targets, drag/drop reachability and reordering, and bisect parent
   lookup use the raw history/index. Background rewrite plans continue reading Git directly.
   Navigation history and message editing reveal a hidden raw-loaded target through `goto`.
   A hidden merge still blocks rewriting commits below it, preserving flattening guards.
   Drag/drop requires proven negative ancestry before offering CherryPick. An omitted side
   path in first-parent mode yields unknown, never a false negative; positive loaded paths
   remain usable. Commits outside loaded ancestry can be loaded before dragging them.
4. Search and explicit navigation can reveal a hidden loaded commit without changing the
   filters. Selection remains protected during later rebuilds. A selected commit removed
   specifically by a namespace-scope change is kept as an explicit selected row. Its layout
   parents bridge omitted old ancestors on every parent path to the new visible scope; its
   operational parents stay intact. Ordinary amend/reset reloads retain the original
   behavior of dropping commits that disappeared.

Filtering never writes Git objects or refs, rewrites history, checks out a branch, changes
upstreams, or changes the raw count used as the pagination skip. The unit tests cover
consecutive hidden commits, a hidden merge with both parent paths, shared ancestry,
protected rows, frontier expansion and a 10,000-commit hidden chain. A real Git fixture
checks namespace-only reachability/count/reveal, pagination, search, attribution, backlinks,
a visible branch sharing an excluded tip, and unchanged refs/HEAD/worktree state.

## Acceptance and evidence

Original visual references inspected: #170 S1, S16 and S18. S1 is a cropped history pane
about 380 pixels wide; its exact UI scale and saved preferences are not supplied, and it
comes from the separate integration branch. The stress fixture recreates its narrow pane,
large preferred refs column, constrained graph, crossing lanes and all trailing columns.
It does not claim an exact recreation of the unavailable old runtime settings.

The automated UI matrix uses the production history renderer at 380, 900 and 1800 window
pixels and 100%, 125%, 150% zoom. It includes long Unicode names, long branch names,
attributed/unattributed rows, multiple lanes, selection, all metadata columns and horizontal
overflow. Set `GITCRAFT_HISTORY_SHOTS=/tmp/gitcraft-167-171/area-a` to save the nine matrix
PNGs plus three narrow right-scroll views. Geometry assertions also inspect actual painted
text clip rectangles. This is macOS headless egui coverage; native Windows/Linux and a
human-driven desktop session are not claimed.

Commands use `cargo +1.95.0`, `CARGO_BUILD_JOBS=3`, and the existing
`CARGO_TARGET_DIR=/Users/jonas/Documents/gitcraft/target`. The complete suite is intentionally
left to the parent integration run.

Validation results and the per-checkbox checklist follow.

### Per-checkbox acceptance report

The checks below apply to this agent's assigned scope. “Verified” means focused automated
coverage plus inspection of generated production-renderer PNGs, with the native-platform
and original-runtime limitations stated above.

#167:

- [x] Divider grab anywhere in hit area: real egui pointer tests start at −4/+4 points,
  exercise every divider, overshoot constraints, return, and return to the exact origin.
- [x] Resizing does not overwrite unrelated preferences: allocator and pointer tests assert
  every other saved width after each drag step.
- [x] Shrink/expand restoration, including dragging a different divider while narrow:
  `a_different_divider_in_a_narrow_window_never_saves_compression`.
- [x] Resize policy independent of last divider: all bars and multiple viewport widths
  produce identical allocations for the same preferences.
- [x] Minima/overflow prevent overlap and inaccessible columns: tiled geometry tests,
  horizontal scrolling, and narrow left/right PNGs at all three scales.
- [x] Headers/content/clips/dividers aligned across sizes/scales: shared geometry, actual
  painted-text clip assertions, and the nine rendered width/zoom combinations.
- [x] Hide/show preferences, graph fit and reset preserved: author hide/show UI test,
  graph-fit save regression, compact-graph fit test and existing reset/settings persistence
  tests. Existing graph double-click and header reset actions remain in production.
- [x] Focused sizing/interaction regressions and representative sequences: pointer-driven
  drags, narrow/restore geometry and visual PNG review. No human-driven native session is
  claimed.

#171 item 1:

- [x] S1 visual reference inspected and representative narrow-pane stress rendered at
  380 pixels and all requested zooms. Exact old saved widths/UI scale were not supplied;
  the test reproduces the geometry stress, not a pixel-identical dev.27 application.
- [x] Boundaries/clipping/ellipsis/minimum overflow use the #167 allocator and shared cells;
  no competing history policy was introduced.
- [x] Subminimum viewport has horizontal access, with shared header/row scrolling; right
  scroll screenshots verify the trailing metadata remains accessible.
- [x] Width preferences preserved through resize/drag; header/row/handle agreement covered
  by geometry and real-pointer tests.
- [x] Full author name/email hover works on a clipped name; `author-tooltip.png` and
  `author_hover_and_hidden_column_keep_details_and_preferences` verify it.

#171 item 8:

- [x] Robot moved immediately after avatar and before name in Author.
- [x] S18 arrow followed; no commit-composer co-author control was changed.
- [x] Original robot renderer, attribution tooltip and Agent/Mixed colors retained; space
  is reserved before truncating the name.
- [x] Description's old badge gap removed. Matrix alternates attributed/unattributed rows;
  narrow right-scroll and `author-hidden.png` verify narrow and hidden Author states.

#171 item 11:

- [x] Global exact names/emails plus persisted per-repository additions or full override;
  effective-rule, serialization and per-repository isolation tests.
- [x] Configurable namespace exclusions default to `refs/t3/*`, applied before `--all`;
  initial history, pagination, search, attribution and backlinks share the scope. Tests
  include an ordinary branch that shares an excluded namespace tip.
- [x] Precise T3 default identity verified from actual metadata and documented; Show hidden
  reversibly reveals both filter kinds. Loaded-author and namespace-only counts remain
  distinguishable; similar authors and human co-writer commits are not caught by default.
- [x] Display-only safety: fixture asserts unchanged refs, HEAD and worktree. Raw history
  drives pagination, operational commits retain their parents, protected rows survive,
  navigation/search reveal targets and stale namespace search results are rejected.
- [x] Graph-aware compaction removes rows and reconnects all parent paths; hidden merges,
  consecutive commits, pagination boundaries, long chains, original-parent preservation
  and selected namespace-only projection have regression coverage. No dimming substitute.
- [x] Full author/email hover verified independently of displayed truncation.

### Validation results

- Final focused combined run: 93 tests passed, none failed, including the blocked-divider
  preference regression and temporary-navigation cleanup.
- `cargo +1.95.0 fmt --check`: passed.
- `cargo +1.95.0 clippy --locked --lib --tests`: passed. Remaining warnings are existing
  macOS dead-code warnings in `desktop_entry.rs` and `diagnostics.rs`; no CI or lint policy
  was changed.
- The full integrated suite and Windows/Linux native validation remain with the parent;
  no issue/PR was created, no refs were pushed, and no issue state was changed.

PNG evidence directory: `/tmp/gitcraft-167-171/area-a/`.

- Nine main renders: `history-{380,900,1800}-{100,125,150}.png` (all combinations).
- Three narrow right-scroll renders: `history-380-{100,125,150}-right.png`.
- Three constrained filter popovers: `filters-380-{100,125,150}.png`.
- Author states: `author-tooltip.png`, `author-hidden.png`.
- Validation logs: `focused-final-tests.log`, `clippy.log`.
- Machine-readable image paths and dimensions: `evidence.json`.

Integration-sensitive shared files: `repo/load.rs`, `repo/mod.rs`, `repo/messages.rs`,
`repo/navigate.rs`, `repo/detail/extras/mod.rs`, and `settings/mod.rs`. Area B's branch/remote
visibility must continue supplying the explicit `log_from` branch roots. Area D's context
menu implementation was not changed; its existing tests are included in the focused run.

## Independent review follow-up

All three findings against integrated `e9a6812d` are fixed in this original worktree:

- [x] P1 operational ancestry: raw-history index at every rebuild, raw first-parent and
  drag reach inputs, unchanged merge barriers, navigation reveal at EOF. Regression coverage
  includes a real drop/undo replay preserving a hidden commit, editable visible ancestors,
  fold/surgery eligibility, and an invisible merge that must still prohibit flattening.
- [x] P2 post-release geometry: a persisted Description exchange distinct from column
  preferences; exact allocator assertions for every divider at five viewport widths plus
  production-renderer pointer/release/shrink/restore assertions at 100/125/150% zoom.
- [x] P2 first-parent frontiers: active mode and `has_more` distinguish intentionally omitted
  side parents from genuine unloaded first parents. Tests include a real Git merge/log,
  both complete and paginated histories, and next-page resolution.
- [x] Asynchronous namespace acceptance: a real Git fixture holds an actual page worker's
  stream lock across a rule change while search and backlinks are also in flight. It
  asserts no stale page/search/backlinks are applied, replacement reload/count and reveal
  agree, and refs/HEAD/worktree remain unchanged.

Native human-driven desktop drags and Windows/Linux execution are still not claimed.
The automated evidence drives the production egui renderer and real Git subprocesses on
macOS. Final counts, copied-executable inventory, screenshots and logs are recorded in the
follow-up evidence manifest.

Follow-up validation uses private `/tmp/gitcraft-area-a-followup-target`, seeded with
`cp -cR /Users/jonas/Documents/gitcraft/target/debug` on APFS, followed by touching
`src/lib.rs` and a Rust 1.95.0 build with `CARGO_BUILD_JOBS=3`. It never uses the parent's
`/tmp/gitcraft-167-171/target`. Cargo's compiler-artifact manifest path and `fresh: false`
were checked, the executable copied out, and all 11 new regression names verified in its
3,087-test inventory before execution. This avoids the shared-target binary collision.

- Production fix and relevant existing suites: 332 passed, zero failed. This includes
  graph/columns, history filters, drag/drop, quick edits, fold, surgery, navigation,
  lifecycle, parallel bisect, details/extras, ghost previews, tab loading, graph order,
  fetched PR tips, namespace readers, authorship, and settings.
- Final `cargo +1.95.0 fmt --check` and `cargo +1.95.0 clippy --locked --lib --tests`
  passed. Only existing macOS dead-code warnings remain in `desktop_entry.rs` and
  `diagnostics.rs`; no lint policy was changed.
- After the final test-only lint cleanup: all 11 new regressions passed again separately.
  No further broad suite was run, as requested by the parent during integrated checks.
- Original 17-render matrix regenerated in `followup/`, plus 18 gesture images:
  `post-drag-{600,900,1800}-{100,125,150}.png` and
  `restored-drag-{600,900,1800}-{100,125,150}.png`. These gesture widths are logical points;
  their PNG widths include the requested zoom factor. The original matrix retains its
  fixed physical widths. Numerical clip measurements are in the regression log.
- `followup-test-inventory.txt`, `followup-tests.log`, `followup-final-regressions.log`,
  `followup-clippy.log`, and `followup-evidence.json` live in
  `/tmp/gitcraft-167-171/area-a/`. The manifest lists each PNG's absolute path/dimensions.
- Follow-ups were committed and pushed to `t3/history-graph-layout-filters` before running
  the corresponding test checks, following the parent's updated instruction. No PR or
  issue changes were made. Latest dev was not merged into this worktree.
