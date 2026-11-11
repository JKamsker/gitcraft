# Architecture and change amplification

Issue [#178](https://github.com/JKamsker/gitcraft/issues/178) is an incremental
refactoring program. The first slice establishes this survey and makes the
terminal panel an owning feature. Typed operation plans/persistence migration,
other feature extractions, settings composition and shared request utilities
remain separate phases. Keep those phases independently reviewable.

## Mandatory Rust file-size policy

Try to keep each Rust file below **300 physical lines**. Files at **300–500**
lines produce actionable warnings; **500 passes and 501 fails**. Every tracked
`.rs` file anywhere in the repository is covered, including tests, examples,
build scripts, and committed generated/vendor files. Comments and blanks count.
There are no exemptions, baselines, suppressions, or warning-only hard-limit modes.

Use `python3 ci/rust_file_size.py` before committing. See the
[contributor instructions](development.md#rust-source-size) for staged and
immutable candidate checks and the exact counting convention. CI and release
validation run that same checker against the complete candidate tree.
Decompose along ownership boundaries; minification, arbitrary splits, giant inline
expressions, partial types solely for line counts, and moving the same oversized
responsibility into another hierarchy level do not satisfy the policy.

The enforcement slice alone does not complete [#198](https://github.com/JKamsker/gitcraft/issues/198).
Existing oversized files must be refactored in reviewable slices and the issue
must remain open until the full-tree gate has zero errors and the required
ownership/safety work has evidence. Coordinate typed operations with #193,
feature ownership with #194, async lifetimes with #195, diff work with #196,
and CI reliability with #190.

## Reproduce the survey

From the repository root, with Python 3.10+ and Git:

```sh
python3 -m unittest discover -s ci -p test_architecture.py
python3 ci/architecture.py --revision 760c79bb69ffc3af5401612bd9c48dcd254eca54 \
  --commits 207 --output /tmp/architecture-before.json
python3 ci/architecture.py --revision HEAD --commits 207 \
  --output /tmp/architecture-after.json --compare /tmp/architecture-before.json
```

The full [baseline](architecture-baseline.json) binds the source and Git history
to a commit and tree. Running at that revision reproduces the JSON, with no
timestamps or machine paths. It includes all tracked Rust under `src/` and
`crates/`: tests, generated-looking source and all conditional branches included.
Sources are raw blobs, so export attributes and uncommitted edits do not change
them; symlinks are excluded. Physical LOC includes comments/blank lines. Function length spans `fn` through
its closing brace. Large-function/struct lists are size rankings, not findings.

The scanner masks comments (including nested block comments), strings/raw strings
and character literals before matching declarations. It is a lexical survey,
**not a Rust parser**: macro token trees, unusual signatures containing const
blocks and multiple fields on one line can distort counts. `branch_tokens`
counts `if`, `for`, `while`, `loop`, `match`, `&&`, `||` and `?` in each body;
it is neither cyclomatic nor cognitive complexity (match arms aren't counted,
nested functions/macros are not semantically resolved). Validate suspicious
results in source. These lexical complexity scores remain advisory. The separate
mandatory full-tree physical file-size gate below supersedes the earlier guidance
rejecting a blanket LOC gate (#198).

History uses the most recent 207 first-parent commits reachable from the selected
revision, including merge deltas against the first parent. Each text file gets a
commit frequency and added/deleted line count. Rename detection is explicitly
disabled; Myers diff, no external diff/textconv and quoted paths are pinned so
ordinary local diff settings do not change the results. Repository attributes
and Git's version can still affect text/binary classification and diff results;
keep those comparable for historical trends. This window differs from a fixed
calendar range, can include older commits, and does not follow renames. A large
move inflates churn. Frequency and churn indicate investigation targets; they do
not establish bugs, ownership or feature boundaries. Median text files changed
per commit includes integrations/docs/refactors; it is **not** the median for
isolated feature changes. Compare equal windows when tracking trends.

At `760c79bb`: 948 Rust files, 308,298 physical lines, 188 `RepoTab` fields,
208 constructor lines, 422 dispatcher lines and 165 root action variants.
`repo/mod.rs`, `actions.rs` and `apply.rs` occur in 108, 71 and 66 commits of
this window. These replace the issue's older source baseline, not its historical
Oct 27–Nov 8 analysis. Measure central state/constructor/dispatcher edits in each
new isolated feature PR as well as file sizes.

## Owners and lifetimes

The baseline's `repo_tab_field_references` inventories **every** top-level field
and the files referencing `self.field`/`tab.field`. Those are lexical references,
including tests, not writes; aliases, other receiver names and nested access
are missed. Use it as an audit index, then inspect actual transitions. The
following groups record intended ownership and important dependencies; they
are a migration map, not new nested public bags of fields.

| Owner / lifetime | Current state and transition sites | Dependencies / reset rules |
| --- | --- | --- |
| Tab identity and jobs / tab | `path`, `name`, `tx`, `_jobs`, `rx`; `mod`, `messages`, `lifecycle` | Mutation key and cancellation scope; suspend replaces sender/receiver so old results disconnect |
| Loading / live interval | `opened`, `life`, `reads`, `work_read`, `submodules_read`, `recheck_after_load`, `shown`, `loading`, `load_started`, `load_times`, `reload_again`, `load_error`; `load`, `lifecycle` | Read ordering, watcher setup race, doctor timing; resume reads again |
| Repository and graph / reload | `refs`, `history`, `history_index`, `pr_graph_head`, `has_more`, `loading_more`, `log_from`, `log_order`, `history_gen`, `log_stream`, `work`, `commits`, `graph`, `graph_labels`, `graph_messages`, `graph_badge_index`, `index`; `load`, `graph_view` | Operational history remains unfiltered; paging uses history generation; suspend drops caches/stream |
| Repository metadata / reload | `worktrees`, `worktree_root`, `stashes`, `stash_picks`, `remotes`, `submodules`, `lfs`, `lfs_locks`, `lfs_storage`, `unity`, `identity` | Linked worktrees share refs but not checkout/index; partial stash selection and locks have separate owners |
| Forge/review / selection | `hosting`, `forge_selection`, `forge_remotes`, `forge_generation`, `forge_repository_generation`, `gerrit`, `prs`, `pr_error`, `prs_loaded`, `ci`, `weakened`, `dco`, `collisions`, `issue_work`, `owners`, `fence`, `drift`, `forks`, `authorship`, `history_filter`; feature modules, `forge_selection`, `load` | Selection invalidates reads; same-repository outstanding mutations can settle; never replay continuations on another forge |
| Selection/detail / view | `selected`, `stash_view`, `scroll_to`, `detail_hash`, `detail`, `detail_error`, `extras`, `all_files`; `navigate`, `detail`, `load` | Reload resolves selection by hash; detail replies must still match |
| Central pages / view | `diff`, `structural_running`, `structural_pending`, `compare`, `pr_view`, `ci_view`, `reflog`, `versions`, `doctor`, `pr_requests`, `pr_drafts`, `compare_requests`, `pr_selection_request`, `file_view` | PR/CI/diff mutually exclusive transitions; per-view request checks; PR drafts survive view changes |
| Navigation / tab | `nav`, `nav_jump`, `pending_goto`; `navigation`, `navigate` | History records places after frame actions; gone changes are skipped; paging can resolve pending selection |
| Editors/workflows / open editor | `rebase`, `drop_menu`, `split`, `absorb`, `stacks`, `release`, `hotfix`, `package`, `reword` | Blocking editors prevent view changes; push/undo continuations retain exact plans |
| Conflict/test workflows / operation | `conflict`, `conflict_generation`, `conflict_checks`, `target_watch`, `keep_both`, `bisect`, `parallel`, `rerere`, `decisions_pending`, `test_lane`, `rehearsing`, `interactions` | Conflict generation rejects old editor answers; running tests/operations prevent suspension |
| Commit editor / persisted draft | `drafts_loaded`, `commit_summary`, `commit_body`, `compose`, `amend`, `ai`, `skip_hooks`, `detached`, `dialog`, `guide` | Lazy tabs must not overwrite drafts; commit success clears composed message; one-shot hook bypass |
| Search/diff preferences / view or tab | `search`, `find`, `diff_ignore_ws`, `diff_as_file`, `diff_rendered`, `folding`, `diff_rich`, `diff_rich_for`, `fold` | Query/debounce/current view and diff request identity; folding/render caches are independent |
| Operations / active step or retry | `push_flow`, `credentials`, `last_op`, `error_card`, `busy_op`, `op_started`, `running`, `waiting`, `activity`, `network_failed`, `update_submodules`; `operations`, `running`, `push` | One mutation at a time; credential retry retains preconditions; cancellation/checkout notifications settle once |
| Timeline/agents / persisted history | `timeline`, `timeline_view`, `timeline_error`, `agent_calls`, `avatars_hinted` | Undo guards HEAD and refs; agent approval/execution shares the operation runner |
| Presentation / frame or tab | `status`, `branch_filter`, `sidebar_jump`, `sidebar_reveal`, `focus_branch_filter`, `focus_wip`, `focus_commit_summary`, `keys_blocked` | Explicit effects from feature controllers; application dialogs own key blocking |
| Network/watch / live interval | `last_fetch`, `fetched_at`, `fetch_time_asked`, `network_paused`, `watcher`, `watches`, `resuming`, `resume_select`, `resume_diff`, `poller`, `counting_worktrees`, `recount_worktrees`, `counted_at`, `fs_rx`, `fs_pending` | Offline policy, scoped watchers, debounce and generation ordering; suspension disconnects results |
| Terminal / tab including parked/hidden | `terminals`, `active_terminal`, `terminal_open`, `terminal_error` → terminal owner | Agent/custom launch and handoff share add/select/error transitions; hide preserves shells; closing/dropping releases owned PTYs |
| Composition / tab and parked tabs | `popouts`, `ghost`, `queued`, `dropped_patch`, `open_request`, `open_when_done`, `parked`, `parked_status`, `agents`, `bakeoff`, `finished`, `custom` | Explicit coordination with window/worktree lifecycle; parked active terminals prevent idle suspension |

### Terminal writes audited for this extraction

Before extraction, `terminals.rs` initializes/toggles/starts/closes/selects;
`agents::start_launch` and `custom_commands::run_custom_in_terminal` duplicate
the append → select last → clear error → show sequence. Handoff writes selection
and visibility after successful paste. `apply` directly writes selection, while
`popout::views` hides the panel. `worktree_tab` observes running shells to prevent
suspension. Tests also inject/clear PTYs. These collaborators must use the owner's
methods; its fields must remain private so future code cannot repeat that sequence.

## Typed boundaries still to migrate

`root_action_variants` in the JSON lists all root actions. `apply` routes these
to feature handlers plus orchestration (popout interception, operation queueing
and fence gates). Terminal actions move behind one feature enum in this slice;
other features should follow only with a coherent owner and explicit effects.

Operation producers include `actions`, dialogs, `commit_actions`, `rewrite`,
`surgery`, `stack`, `release`, `hotfix`, `partial_stash`, `gerrit`, `forks`,
`agent` and error fixes. The root `ops` helper constructs CLI argument lists.
`state::{Undo,PendingOp,UndoEntry}` and `Action::{Ops,UndoableOps}` carry them.
`operations::{run_ops_ex,run_steps_guarded}` execute, retry and record them;
`running`, `offline`, hooks, activity and fork guards also inspect them.

The legacy protocol includes `operations::{EXPECT,EXPECT_SYMREF,EXPECT_TREE,TRY}`,
`gerrit::{CHECK,AMEND}` and fork guard markers. Expectations run before mutation;
best-effort steps and credential retry indices have different semantics. A typed
plan migration must inventory these consumers together. Timeline records serialize
`UndoEntry` with `undo`/`redo: Vec<Vec<String>>`, optional guarded HEAD pair,
defaulted id and agent. Do not change this disk format until old fixtures decode
and round-trip with tested compatibility. This slice leaves it intact.

Background work flows through `messages::{JobSender,Msg,spawn}`, `then` and the
tab job scope. Reload/paging use read/history generations; diff/rich/rendered,
conflict, compare/PR selection, forge and watcher requests have their own checks.
PR/CI polling and remote watchers add timers. A shared request utility should
carry typed request identity and scoped cancellation while the feature decides
invalidation. Test A→B→A, close/reopen, suspend/resume and late errors, not just
different-query success.

## Tests, coverage and resource evidence

Use `cargo +1.95.0 fmt --check`, `cargo +1.95.0 clippy --locked --all-targets --
-D warnings` and `cargo +1.95.0 test --locked` for the final combined candidate.
Platform-specific checks still need their platforms. This refactor changes no
process-control code; tests must use owned child handles, never host-wide PID
selection. Terminal transition tests should use inert terminal fixtures before
the existing owned-PTY launch/handoff tests.

Existing test families: `repo::popout`, `repo::custom_commands`, `repo::agents`
(including handoff), `repo::bakeoff`, `repo::worktree_tab`, `repo::lifecycle` and
`terminal` cover the affected integration boundaries. Reload/navigation/diff/PR/CI,
operations/forks and timeline tests protect adjacent flows. Follow-up priorities:
overlapping reload plus selection, cross-view late replies, credential retry after
partial completion, canceled guarded rewrites, undo after restart and remote-host
failures. A terminal extraction does not establish those broader guarantees.

[Workflow coverage](cpu-workflows.md) reports historical 116,833/159,467
instrumented lines (73.26%, rounded in the issue to 73.3%). This includes inline
test bodies, is not current production-only coverage, and excludes other cfg
builds. Several remote/tool tests can return early without fixtures. No new
coverage percentage is inferred here. Weak paths include PTY I/O, external
credentials, native GPU/events, SSH/WSL, forge and lifecycle races.

Use the existing [workflow benchmarks](cpu-workflows.md),
[process baselines](process-orchestration.md) and [backend baselines](git-backend.md)
for load/reload, graph, diff, staging, navigation, transports and concurrent tabs.
Capture immutable binaries with identical frozen inputs; alternate before/after,
exclude compilation/profiling from timings, record CPU versus elapsed separately.
Historical results are not new measurements of this candidate. Terminal selection
and collection transitions add no polling, transport calls or tasks; measure
those directly and retain the final validation evidence alongside this document.
SSH/WSL and platform resource claims require their real hosts and remain unmeasured
when unavailable.

## Convention for new feature work

1. Put state **and** constructor/reset/result/teardown invariants in the feature
   module with private fields. Avoid exposing `&mut Vec<_>` or public mutable
   state that lets collaborators bypass transitions. Read-only views and narrowly
   scoped I/O methods are fine.
2. Give the owner a typed action enum. The root enum and dispatcher contain one
   explicit feature route. Keep ordering/gates visible; avoid global registries,
   downcasts and string-based service lookups.
3. Render from the owner and explicit borrowed inputs (`path`, context, settings,
   dock state). Return typed effects for genuinely cross-feature coordination;
   do not pass the entire mutable tab into the owner.
4. Keep palette contributions beside the feature. Compose them at named positions
   to preserve deterministic order and shortcuts. Adding a local feature action
   should not require editing root fields, constructor or dispatcher.
5. Keep Git/forge/process I/O below the UI; preserve bounded jobs, cancellation,
   offline policy and persisted formats. Dependency direction is window/tab →
   feature → Git/forge/terminal services; services must not depend on `RepoTab`.
6. Review central edit count and cross-module writes alongside metrics. Rust
   privacy and clear ownership complement the mandatory physical file-size cap.

The feature-local follow-up in this phase adds `TerminalAct::Hide`: the existing
Hide button expresses an idempotent hide intent even if another event already
hid the panel. It changes the terminal owner, its tests and its tab strip, with
**zero edits** to `RepoTab` fields, constructor, root action enum or dispatcher.
The toolbar/key still toggles (and focuses an existing popout) as before.

The [results](architecture-results.json) and [validation evidence](validation/architecture-178.md)
record the actual before/after metrics, source equivalence, local checks, timing
smoke results and limitations. The owning boundary reduces four central state
fields to one and four root variants to one; it does not eliminate the remaining
`RepoTab`, settings, operation or async hotspots. Continue those as bounded phases.
