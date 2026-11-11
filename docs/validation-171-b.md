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

## Review 9: explicit provider destinations

- [x] GitHub and GitLab provider entry points, including PR/MR lists, merged/find,
  create/reviewers, details/threads/reviews/comments, retarget, issues, inbox,
  forks, CI reads/mutations, release lookup/create and assets, require the captured
  `Hosting` at the CLI process boundary. Repository commands name the selected
  repository; API commands name its host and use explicit project paths/GraphQL
  variables. No production CLI entry point can infer a destination from cwd.
- [x] Cross-host secondary remotes are supported. SSH tabs re-read the catalog and
  require the selected host AND path to remain present; removed/invalid targets
  fail without falling back to origin. Custom SSH hosts require a configured CLI
  login; inherited global token variables cannot establish host trust.
- [x] Same-number fork/upstream isolation is checked at the prepared Command, and
  stale details cannot construct a mutation target for a different repository
  on GitHub, Forgejo or Gitea. Local Git remote/upstream/pushDefault configuration
  remains byte-for-byte unchanged in a real repository regression.
- [x] Command syntax verified with installed `gh --help` for every used API, PR,
  issue, release and run command. `gh api` uses `--hostname`, while repository
  commands take `--repo [HOST/]OWNER/REPO`. `glab` is not installed; checked its
  official command docs individually for api, mr list/create/update, issue
  list/view and release view/create/upload. API takes `--hostname` and no
  `--repo`; the others accept `--repo` with a full URL, retaining nested groups
  and HTTP ports. Upload switches are inserted before the `--` filename marker.
  References: https://cli.github.com/manual/gh_api,
  https://docs.gitlab.com/cli/api/ and the corresponding command pages under
  https://docs.gitlab.com/cli/.
- Evidence: `/tmp/gitcraft-review9/area-b/before.log` contains the original
  implementation's failing process-boundary regression (1 test, 1 failed).
  `after-providers.log` contains 10 passing recording-only regressions, with
  argv, target environment and stdin assertions. No gh/glab process executes in
  these tests; mutation coverage performs no live forge writes.
- This follow-up changes routing/state only. Existing area-B screenshots remain
  under `/tmp/gitcraft-167-171/area-b/`; no layout change needs new screenshots.

### Review 9 release and follow-up mutation safeguards

- [x] The explicit release draft selects the primary forge remote for its Git push
  and captures that same repository for publication. Other push/fetch defaults
  and upstreams are unchanged. The before-fix test failed with `origin` instead
  of `fork`; afterward a real local bare fork receives the tag and origin does
  not. All six release-dialog regressions pass, including release completion,
  resume, changed-primary cancellation and undo from Review 8.
- [x] Publication and resumed asset uploads re-read Git's resolved push URLs. If
  they differ from the captured hosting repository (including a separate
  `pushurl` or a changed remote), the forge mutation stops with an explanation;
  local changes remain recorded and undoable. A recording-only regression
  confirms zero forge calls for incompatible destinations and an explicitly
  selected release target for a matching destination.
- [x] PR/MR creation validates the returned URL before requesting reviewers;
  GitHub edits by numeric ID, since a URL argument would override `--repo`.
  Unexpected upstream URLs cannot redirect the follow-up mutation. Valid
  `.github` repository names remain supported while traversal paths are refused.
- Current focused boundary count is **12 passed**, with all command preparation
  covered without starting gh/glab. Release evidence:
  `/tmp/gitcraft-review9/area-b/{release-before.log,release-after.log,release-focused.log}`.
  `clippy.log` passes `--all-targets -- -D clippy::all` (only existing dead_code
  warnings); formatting and diff checks pass. Rust 1.95.0, jobs 3, private target.

### Final Review 9 validation

- Exact original-provider reproduction: restored all affected provider/CI source
  files from `6e893e44`, added only the process recorder and current provider
  tests, and ran the two `every_provider_action_uses_selected_repo_and_host`
  cases. **Both ran and failed** because the selected repository was absent.
  No forge process executed. Restored all committed sources afterward and
  confirmed a clean worktree before rebuilding. Evidence:
  `/tmp/gitcraft-review9/area-b/before-both-providers.log` and the reproducible
  `baseline-providers.py` beside it.
- Final private binary inventory: **3,113 tests**, including **12** named
  `provider_boundary` regressions. Final focused runs were sequential with
  Rust 1.95.0, CARGO_BUILD_JOBS=3 and GNU Git/sed on PATH: forge filter **144
  passed, 6 live-server tests ignored**; CI filter **22 passed**; release filter
  **17 passed** (including all 6 release-dialog tests). Total **183 passed**.
  Logs and inventory are `/tmp/gitcraft-review9/area-b/{forge-focused.log,ci-focused.log,release-all.log,inventory.txt}`.
- `cargo +1.95.0 fmt --check`, `git diff --check`, and clippy with
  `--all-targets -- -D clippy::all` pass. Existing desktop/diagnostics dead-code
  warnings remain. No integrated whole-suite claim is made; the parent owns
  that run. Real authenticated gh/glab writes and real SSH login checks were
  deliberately excluded; provider command preparation, input and trust refusal
  are covered with recording-only fixtures. GitLab syntax was checked against
  official docs because glab is absent on this machine.

## Review 10: refuse incompatible publication before mutation

- [x] A publishing plan now checks its captured hosting against Git's resolved
  push destinations in the dialog's asynchronous preflight, before executing
  any step. The engine checks again before Bump, Tag and immediately before
  Push, while retaining the Publish/Upload guards. Plans without publication
  retain their existing Git transport behavior.
- [x] Refusal leaves local HEAD, tag refs and Done untouched. A mismatch introduced
  after completed local steps prevents the push and preserves those completed
  steps for undo/resumption. Both failing-before and passing-after push fixtures
  use owned local bare receivers; none can push to a live forge.
- [x] Destination identity compares the web authority separately from the path.
  GitLab subgroup/project paths retain their exact captured spelling; GitHub,
  Forgejo and Gitea owner/repository names retain ASCII case-insensitive identity.
  Scheme and port differences remain incompatible, and host case is ignored.
- The existing changed-primary undo fixture now seeds a completed local phase
  before pending publication, so it continues to test resumption/undo without
  presenting an intentionally incompatible plan to the new initial preflight.
- Actual private-target evidence under `/tmp/gitcraft-review10/area-b/`:
  `before.log` reports **3 ran, 3 failed** on the previous implementation;
  `after.log` reports **4 passed**, including the path/authority regression;
  `release-suite.log` reports **21 passed**, zero ignored/failed. Test binary
  inventory is 3,117 (4 selected + 3,113 filtered in the focused run). Formatting,
  diff checks and `clippy --all-targets -- -D clippy::all` pass; see `fmt.log`
  and `clippy.log` (existing dead-code warnings only). Rust 1.95.0, jobs 3,
  private target `/tmp/gitcraft-167-171/target-b`, GNU Git/sed PATH. No provider
  changes, live forge requests, broad 183-test rerun or layout changes were needed.
