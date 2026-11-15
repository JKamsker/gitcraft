# Issue 189: round-1 review fixes and historical evidence

Superseded by the [round-2 fix evidence](189-staged-round2.md).

Application/test candidate: `15badf7feeffaf102a577c125e32ebdfc6ec61d8`. Base `origin/dev` at
`4189435a20d21f5f334332d71d7230fe184571c5` was merged normally; it includes `d3a759bd`.
The subsequent evidence-only commit changes documentation. Bind these gates to that
head only after verifying `git diff --name-only 15badf7feeffaf102a577c125e32ebdfc6ec61d8 HEAD` contains documentation paths only.
The earlier [validation](189-staged-verification.md) remains historical evidence.

The supplied independent read-only review of `f4eca635` is complete and replaces the
previously requested re-review. This larger fix set is returned for a new review before
final-head CI and merge; it adds preparation policy, filesystem preflight and shared deadlines.

## Gates

Linux, Rust/Cargo stable 1.100.0, Git 2.50.1 built under the disposable task target.
Every Rust command ran at nice 19 with `CARGO_TARGET_DIR=/home/jonas/work/gitcraft-targets/189`
and `TMPDIR` under it. No system tools were changed. The target is removed at handoff.

- **675 passed, 0 failed, 1 existing ignored**, in one combined affected-module run.
  The ignored native HTTPS gate already requires separate explicit containment approval;
  no test was newly ignored or weakened. No Windows, forge-server or full application suite ran.
- Strict touched-crate all-target Clippy passed:
  `nice -n 19 cargo +stable clippy --locked --package gitcraft --all-targets -- -D warnings`.
- `nice -n 19 cargo +stable fmt --check` passed.
- **132 Python CI tooling tests passed**.
- Committed-HEAD file-size (2,793 tracked Rust files) and complexity gates: zero errors.
- Architecture fitness `check --revision HEAD`: **zero violations**.
- Baseline growth checks against `origin/dev` and prior pushed `f4eca635`: passed.
  No architecture budget or increase declaration changed. The coordinator reuses its existing
  Git text-output API and connection context rather than adding a foreign-symbol allowance.
- `python3 ci/check.py --dry-run` confirmed the CI command plan. The task's explicit resource
  limits override its all-package Clippy/compile-all plan: equivalent touched-crate gates and
  all selected affected groups ran manually. No CI dispatch, rerun or wait was performed.

Exact combined Rust selection (`repo::direct_tests` is a directory; its actual owner
modules are included as separate filters):

```sh
nice -n 19 cargo +stable test --locked --package gitcraft --lib -- \
  processes::deadline::tests git::testlane repo::test_lane repo::layout \
  repo::direct_tests repo::detail repo::load repo::tests \
  repo::watch repo::lifecycle repo::keys repo::keyboard \
  repo::toolbar repo::palette repo::commit_changes repo::graph_view \
  repo::apply a11y::tests::repo_view app::palette git::command \
  git::rehearse git::interaction git::stream git::bundles \
  processes repo::branch_cleanup_tests repo::branch_health repo::branch_tree \
  repo::celebration repo::columns repo::commands repo::conflict_check \
  repo::file_text repo::forge_selection_tests repo::graph_walk repo::growth \
  repo::messages repo::navigate repo::push_preflight repo::quick_edit \
  repo::rebase repo::rewrite repo::symbol_biography \
  --test-threads=2
```

The synthetic deadline/cancellation checks passed before live owned-child cancellation.
The process service's signal/ownership code is unchanged; the new deadline feeds its existing
owned-child timeout path. Synthetic runtime evidence from `cbe05691` also binds unchanged
runtime files, verified by Git diff; the final combined run repeats those tests.

## Review and acceptance evidence

- P1-1: `private_index_writes_and_cleanup_never_run_source_hooks` installs real
  `post-index-change`, `post-checkout` and fsmonitor probes, then checks external markers,
  source index bytes and refs. All preparation/cleanup calls suppress hooks.
- P1-2: `symlink_targets_and_chains_cannot_expose_live_helpers` covers absolute/outward
  links, chains, `.git`, a valid internal link and `core.symlinks=false`. Capability is
  probed on the snapshot filesystem before checkout; actual link types are verified too.
- P1-3: `sparse_index_and_standalone_skip_worktree_are_explicitly_refused` uses real
  sparse-index and skip-worktree fixtures. Both are explicitly unsupported in v1.
- P1-4: `filters_lfs_and_info_attributes_are_refused_before_they_can_execute` covers
  smudge/process drivers, LFS and ambiguous attribute values; external markers prove no
  driver ran. `global_attributes_cannot_switch_from_unstaged_source_to_staged_filter_rules`
  covers relative global attributes and info-attribute fingerprinting. Global attributes
  are disabled; effective tree/info filter declarations are refused. The private index is
  written from an actually empty cwd with explicit source Git-directory/private-index
  overrides, preventing Git's racy-entry rehash from reading live source files.
- P2-5: `watcher_events_invalidate_before_debounce_and_reject_older_freshness` covers
  immediate dispatch and late-check rejection. Config-origin, linked-config classification,
  absent-file matching and drive/UNC identity tests cover watcher routing. Existing recursive
  watches are preserved; the UI shows verification in progress while validating.
- P2-6: one 20-minute preparation/queue/execution deadline, separately bounded cleanup,
  64 MiB index/info-attribute limits, 10,000-file and estimated 512 MiB checkout limits.
  Pure block-budget tests and real oversized-index/file-count tests pass, along with expired
  preparation and cleanup-after-expiry tests. Build outputs have no filesystem quota;
  the guide explicitly requires commands to bound generated files/disk use.
- P2-7: pure portable-name tests cover case/Unicode and directory-prefix collisions,
  reserved names and long/invalid paths independently of the host filesystem. Actual directory
  entries must have the expected membership/spelling; modes, symlink types and Git contents
  are checked after checkout. No silently collapsed or fallback tree receives a verdict.
- Layout: the original CI failure was reproduced locally using `cbe05691` with only
  `f4eca635:src/repo/detail/commit_box.rs` temporarily restored, then restored to the candidate.
  The unchanged test panicked at line 116 with `toolbar scrolled out at full height`.
  The final combined run passes that same resize test and the wider responsive groups.
  The compact same-row flask popup also passes keyboard-access and named-control tests.
- Original acceptance: the forgotten-helper/partial-stage test proves working-tree success
  but staged failure, then success after staging the helper. Adds/deletes/renames/modes,
  unborn/split indexes, source preservation, conflicts, submodules, ignored assets, stale
  restaging, copied-index races, cancellation/tab closure and crash cleanup remain covered.
  en-US/de text is present. User commands run through the existing test lane/process owner.

## Retained logs

- `architecture.txt`: SHA-256 `ff22ae9b9f8a10ccb0b3f076768b564ae54fea1c962447f9fe0d734fc7de9a99`
- `clippy.txt`: SHA-256 `94e4ce0edebd2d31be0a183826c4d8386b0a6b0fa4fd59bf08c0faa7549325a2`
- `gates.txt`: SHA-256 `5aca8ffa658edf29f7b1f9a4615302033a711eb9d89a01540f1702be5c01b62c`
- `original-layout-reproduction.txt`: SHA-256 `43e8a1af8f5c264e5a91d8864b7585636c34433b677dd6af1d2dfb93fc1e4c5d`
- `tests.txt`: SHA-256 `a02303edf633ea6f7d99e3c1d0fbfeb2846ba2fe8a0065f26a65a153934507f1`
