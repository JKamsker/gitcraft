# Issue 171 area B: primary forge and remote visibility

Scope: item 4 and item 5 only. Original S12/S13 were inspected directly; #167,
#171, original #170, AGENTS.md and docs/development.md were read. No PR, push,
issue edit, delegation, CI change or author-filter implementation is included.

## Acceptance coverage

Item 4:

- [x] Per-repository primary forge via a remote's context menu: “Set as primary
  forge”. A star marks the active source (including the automatic fallback).
  The accessible row name, tooltip and selected menu item identify it as primary.
  A visible, keyboard-focusable remote-actions button exposes the same menu.
- [x] Persist selection through egui's persisted repository settings. Catalog
  detection happens in the background; the latest preference is applied on
  arrival. PR list/details/files/create/review, assigned issues/refresh, CI
  status/runs/jobs/log/actions, forks/approvals, merged PRs/health, PR seen/version
  state, scope-drift issues, CODEOWNERS reviewer suggestions, stack previews and
  default links use or guard the selected hosting.
  Generic commit/file links default to primary; explicit branch links continue to
  identify the branch's own remote. Numbered state is cleared on rebind.
- [x] Explicit usable selection → usable origin → first usable remote. Local or
  unrecognized origin does not block a usable fork. Unique same-URL renames follow
  the new name; ambiguous rename/removal returns to automatic; same-name URL edits
  follow that name and invalidate old results. A stale second tab follows a renamed
  source by URL or waits for fresh detection rather than acting on upstream or the
  previous URL; reloads are stamped so they cannot overwrite a newer preference.
  Detection/selection is local and
  works offline; network reads resume through existing network policy. A failed
  remote read fails the reload instead of silently deleting a preference.
- [x] Selection writes only gitcraft's UI preference. Normal Push/Fetch transport
  routing, Git remote config, refs and branch upstreams remain unchanged. Explicit
  Create PR/stack publication uses the selected forge's remote, as the action
  explicitly publishes there. Existing destructive operation confirmations remain.
- [x] Vancete origin / JKamsker fork regression uses the same PR number on both
  repositories and proves only the current result survives. Generation-bound
  senders reject stale results/continuations, including A→B→A. A slow reload uses
  the preference at arrival. Other tabs rebind before draining old result channels.
  Same-repository renames/unrelated remote edits let a posting review settle;
  selecting another hosting invalidates the old lifetime. Stale creation/publication
  completions release their busy slot without executing
  an old continuation against a new repository. An old saved issue neither
  refreshes #N on another repository nor generates a cross-repository Closes #N.

Item 5:

- [x] Persist independent “Hide/Show remote branches in sidebar” and
  “Hide/Show remote branches in graph” controls in each remote context/actions menu.
  Hidden sidebar groups keep their header, count, visible hidden label, accessible
  state and restore actions. The primary star remains visible on a hidden group.
- [x] Sidebar hiding removes branch rows without changing history. Explicit graph
  hiding restricts Git's revision roots, removing exclusive remote history and
  its ref pills. Shared history and local HEAD remain visibly identified; this is
  ancestry filtering, not dimming rows. “Show all” restores graph groups without
  overriding the independent sidebar preference.
- [x] HEAD is always a graph root, including detached HEAD. A real Git fixture
  verifies that hiding both remotes removes remote-only history but preserves
  local HEAD and its ancestry, with identical refs/config before and after.
- [x] Individual Show overrides a hidden graph group; Solo takes precedence over
  group hiding. Removing Solo restores the group and individual choices. Coverage
  includes multiple remotes, overlapping names (`my` / `my/fork`), hidden checked-out
  branches, detached/unborn HEAD, persistence and local/remote pill coalescing.

## Validation and evidence

The final focused run and command results are recorded below. Builds used
Rust 1.95.0 and `CARGO_BUILD_JOBS=3`. Development reused
`/Users/jonas/Documents/gitcraft/target`. After the parent detected cross-worktree
artifact reuse, final validation used an APFS clone at
`/tmp/gitcraft-167-171/target-b`, touching `src/lib.rs` to force a fresh compilation
of this worktree. The binary inventory and every focused filter's nonzero test
count were checked. The complete suite was not run here. Final tests use
Homebrew Git 2.56.0 and GNU sed 4.10 through the provided PATH.

| Final check | Result |
| --- | --- |
| Isolated `cargo +1.95.0 test --lib --no-run` | Pass; artifact marked fresh=false, manifest identifies this worktree |
| Binary inventory | 3,098 tests, including every named area-B regression |
| 23 focused filters, each exact count asserted | 227 passed, zero failed; no empty filters |
| UI interaction and render matrix | All nine widths/scales passed; 27 PNGs generated |
| `cargo +1.95.0 fmt --check` / `git diff --check` | Pass |
| Isolated `cargo +1.95.0 clippy --lib --tests -- -D clippy::all` | Pass; existing desktop_entry/diagnostics dead_code warnings only |

Evidence logs: `/tmp/gitcraft-167-171/area-b/{build.log,build-artifacts.jsonl,test-inventory.txt,focused-tests.log,clippy.log,checks.log,manifest.json}`.

The UI regression uses the production remotes sidebar/menu code, exercises primary
selection through keyboard focus/Enter and hide/restore in both locations,
asserts menu items stay inside the viewport, and renders 360/720/1200 physical-pixel widths at 100/125/150% density
(with the corresponding reduced logical viewport).

Reproduce screenshots with Rust 1.95.0 and an available egui WGPU adapter:

```sh
CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=/tmp/gitcraft-167-171/target-b \
  GITCRAFT_REMOTE_SHOTS=/tmp/gitcraft-167-171/area-b \
  cargo +1.95.0 test --lib remote_menus_and_hidden_groups -- --test-threads=1
```

Generated screenshots (27 PNGs) are in `/tmp/gitcraft-167-171/area-b/`:

- `remote-menu-W-S.png`: reachable primary and independent hide controls.
- `remote-hidden-W-S.png`: primary star and discoverable hidden group.
- `remote-restore-menu-W-S.png`: primary indication, automatic fallback and both restore actions.

W is 360/720/1200 and S is 100/125/150; every combination is rendered and its
interactive actions and viewport bounds checked. Narrow 150% and wide samples
were also inspected visually. These are focused production sidebar/menu renders,
not full-application screenshots or live network-forge screenshots.

## Integration notes

`graph_view/menu.rs` changes branch selection to use visibility-aware badges;
`sidebar/branches.rs` uses the selected primary forge's default base for Create PR.
Preserve these destination/visibility changes when combining with agent D's menu
grouping. The local remote menu wraps to the viewport; the parent's
`crate::menu_item::bounded_menu` helper can replace that wrapper when integrating.
The primary/visibility actions, keyboard closing and regression coverage must stay.
`Loaded::forge_generation` stamps the catalog's read lifetime; preserve it and
`take_forge_catalog` when merging load.rs so a slow read cannot reconcile a newer
cross-tab URL back to its old value.

## Practical limits

The focused tests use deterministic local Git fixtures and injected asynchronous
results; they do not create real PRs, post reviews, approve CI runs, or make network
mutations. Native Windows/Linux accessibility/platform behavior is not run on this
macOS host. The parent owns the integrated full-suite run and integration with the
other areas; shared load.rs, messages.rs, mod.rs and graph files need merge review.


## Review 8 adversarial follow-up

- P1: catalog rebinds retain every started release, including its initial validation,
  in-flight completion, captured hosting destination, Done state and undo recording.
  Unstarted previews are still invalidated. Publication/upload steps stop if the
  hosting repository changed (including away and back); local mutation results are
  recorded and remain undoable. Ordinary Git transport routing is unchanged.
- P2: invalidating the follow-up details/files refresh clears a completed post's
  settling/posting flags and preserves unsent text. A genuinely pending mutation
  (`posting=true`, `settling=false`) remains blocked and can receive its completion.
- Before the fixes, both new reproductions ran and failed (2 tests, zero passed).
  Afterward, all three regressions pass, including a real local release whose
  publication is stopped after a primary change and whose commit/tag can be undone.
- Private target `/tmp/gitcraft-167-171/target-b`: inventory 3,101 tests; focused
  forge-selection (10), release-dialog (5), PR-review (18), and PR-view (11) filters
  passed all 44 tests, with each exact nonzero count asserted. Rust 1.95.0,
  CARGO_BUILD_JOBS=3, GNU Git/sed PATH. Clippy with `-D clippy::all`, formatting and
  diff checks pass; only existing dead_code warnings remain.
- Evidence: `/tmp/gitcraft-review8/area-b-fixes/{before.log,after.log,focused.log,inventory.txt,clippy.log}`.
  No live forge writes, UI layout changes, CI changes, PR, or dev merge were needed.
