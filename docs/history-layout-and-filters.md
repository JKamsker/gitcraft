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

A divider takes a starting preference snapshot, available width and pointer press origin.
The initial offset within its 10-point hit area is retained. Each frame uses total pointer
displacement from that origin, including after overshooting and returning from a limit.
Dragging exchanges only its own displayed width with Description; growth stops when
Description reaches its floor. It does not cascade into other cells. A drag back to its
origin restores the starting preference, including automatic Graph mode.

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
reloads history and invalidates the older pagination generation. Search resolves the same
namespace roots before using the existing safe search API. Attribution and backlinks use
the same namespace exclusions, including repository overrides and reveal mode. Exact
single-commit reads and rewrite/checkout operations do not apply presentation rules.

## Graph compaction and safety

The raw paginated history and operational Commit objects always retain their original
hashes, parents, refs, trees and authors. Filtering builds a separate layout projection:

1. Keep visible ref tips, HEAD, selection, explicit navigation/search targets, stash bases,
   and matching commits whose parents are not all loaded. The last rule retains a visible
   pagination frontier until the next page resolves it.
2. For a hidden commit, follow **every** parent path to its nearest visible ancestors.
   Iterative post-order traversal and cached frontiers avoid recursive stack overflow on
   long generated histories. Stable de-duplication preserves first-parent ordering.
3. Reconnect a visible child's layout parents to those frontiers. Feed this projection to
   the existing merge-aware graph layout. Operations, drag/drop and detail reads still use
   the original Commit objects. First-parent mode keeps its existing layout semantics.
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
