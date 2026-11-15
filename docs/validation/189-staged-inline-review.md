# Issue 189: extra inline-review fixes

Application/test candidate: `82c9a55ba61f1032a47c1d9dafcb508017aa2a2f`.
Base `origin/dev` remains `8e69afb7df4141eada62495793a9645d181abe0e` (#240/#243,
including `d3a759bd`); fetched again after the wider run. No rebase or rewritten
published history. Subsequent evidence commits change documentation only. Bind
these gates to a later head only after verifying
`git diff --name-only 82c9a55ba61f1032a47c1d9dafcb508017aa2a2f HEAD` contains documentation paths only.
The [round-2 evidence](189-staged-round2.md) remains historical evidence.

## Disposition

The API returned 10 top-level CodeRabbit comments. Each was treated as untrusted
review data and checked against current code. One raw-byte documentation comment
was already addressed in `4c20f01b`; the other nine findings were fixed. All 10
threads received individual replies, listed below. No suggested external reviewer
commands were executed. This shared worktree-ownership and failure-reporting fix
set is returned for independent review before CI/merge.

- Scratch attempts reserve an exclusive random destination, retrying collisions.
  Local creation is explicitly nonrecursive (0700 on Unix); remote creation uses
  the existing remote command owner and atomic mkdir. Git add no longer uses
  --force. An add error never invokes forceful worktree removal: only empty rmdir
  is attempted. A partially registered/nonempty attempt remains for crash recovery.
  Real-Git and pure filesystem regressions retain a preserved same-PID worktree's
  uncommitted bytes, refuse existing destinations, clean empty failed attempts and
  allocate independent destinations. Successful attempts keep existing RAII cleanup.
- Timed command execution returns bounded collected lines alongside its completion
  Result. Staged results keep stream/timeout errors, command, tree, elapsed time and
  the last 200 output lines; errors and lines pass through existing redaction. The
  UI shows the failure and diagnostics without calling it user cancellation. A
  deterministic failing-stream test and the actual UI test passed. The command
  helper moved to a cohesive small module to keep the runner below 500 lines; its
  unused alias helper visibility was narrowed.
- Captured absence is now an index expectation. Production verification always
  compares the copied bytes or absent state. A real-Git ABA test proves that a new
  helper-bearing index is rejected even when removing it restores the old fingerprint.
- Link resolution has one shared 1,000,000-step traversal/queue budget across the
  manifest. Both popping and enqueueing components check cancellation/deadline and
  consume the budget, bounding pending expansion too. Exhaustion has en-US/de text.
- Checkout, CheckoutPull, CheckoutRemote and CarryCheckout invalidate before dispatch.
  A deterministic action-state test covers all four before any operation starts.
- Config discovery retains symlink spellings and adds canonical file targets before
  parent-watch registration. Path keys canonicalize the closest existing ancestor
  and append missing components. Real-Git/Unix tests cover link/target watches,
  symlinked prefixes, nested missing parents and stable identity as parents appear.
  Include tests compare identity rather than raw spelling, including macOS aliases.
  Existing pure Windows verbatim-drive/UNC normalization tests passed on Linux.
- The guide already documented raw indexed bytes; its filter workaround now requires
  removing the effective staged/info filter declaration before running the command,
  then preparing assets. en-US/de isolation text now includes all indexed files,
  including unchanged tracked files, and excludes unstaged/untracked content.

| Original comment | Verified response |
| --- | --- |
| [4577621685](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621685) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577708924) |
| [4577621687](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621687) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577708987) |
| [4577621688](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621688) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709031) |
| [4577621691](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621691) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709089) |
| [4577621692](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621692) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709138) |
| [4577621693](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621693) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709205) |
| [4577621694](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621694) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709262) |
| [4577621696](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621696) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709328) |
| [4577621698](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621698) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709388) |
| [4577621704](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577621704) | [Reply](https://github.com/JKamsker/gitcraft/pull/242#discussion_r4577709443) |

## Validation

Linux; stable Rust/Cargo 1.100.0; Git 2.50.1 built inside the disposable task target.
All Rust gates ran serially at nice 19, using
`CARGO_TARGET_DIR=/home/jonas/work/gitcraft-targets/189` and TMPDIR below it.
The target is removed at handoff; no system tools changed.

- **747 passed, zero failures, two existing ignored** in the wider affected-module
  run: [output](189-staged-inline-review/tests.txt). The ignored native HTTPS gate
  keeps its containment prerequisites; the existing review-contract GC hold remains
  ignored. No test was newly ignored or weakened.
- **12 deterministic synthetic/preflight checks passed first**, then repeated within
  the wider selection: [output](189-staged-inline-review/synthetic-tests.txt).
- Strict touched-crate all-target Clippy: [output](189-staged-inline-review/clippy.txt).
- `nice -n 19 cargo +stable fmt --check` passed.
- **132 Python CI tooling tests passed**: [output](189-staged-inline-review/tooling.txt).
- Committed-HEAD size: **2,808 Rust files, zero errors**; complexity policy: zero errors.
- Committed-HEAD architecture: **zero violations**:
  [output](189-staged-inline-review/architecture.txt).
- Baseline checks against origin/dev and previous candidate `4c20f01b` passed.
  No architecture budgets or increase declarations changed in this fix set.
- `python3 ci/check.py --dry-run --test ...` confirmed CI's plan. The task's explicit
  resource limits override its all-package Clippy/compile-all defaults; equivalent
  touched-crate gates and the affected groups below ran manually. No full application,
  Windows or forge-server suite ran. No CI dispatch, rerun or wait occurred during
  implementation; CI and merge follow independent review of the final candidate.

Wider command (repo::direct_tests is a directory; its actual owner modules are selected):

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
  repo::rebase repo::rewrite repo::symbol_biography repo::publish \
  resolver::workspace repo::detached repo::checkout_blocked \
  repo::review_workflow::contracts repo::review_workflow::choreography \
  repo::review_workflow::radar --test-threads=2
```

Synthetic selection:

```sh
nice -n 19 cargo +stable test --locked --package gitcraft --lib -- \
  git::testlane::runner::command::tests git::testlane::staged::bounded \
  git::testlane::staged::preflight::names git::testlane::staged::preflight::links::tests \
  git::testlane::staged::extra_review_tests::command_failure \
  git::testlane::scratch::allocation::tests::exclusive_claim \
  git::testlane::scratch::allocation::tests::reservations \
  processes::deadline::tests --test-threads=2
```

Strict/static gates:

```sh
nice -n 19 cargo +stable clippy --locked --package gitcraft --all-targets -- -D warnings
nice -n 19 cargo +stable fmt --check
nice -n 19 cargo +stable run --locked --release --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD
python3 ci/rust_file_size.py --revision HEAD
python3 ci/complexity_policy.py --revision HEAD
python3 ci/arch_baseline.py --base origin/dev --revision HEAD
python3 ci/arch_baseline.py --base 4c20f01b --revision HEAD
```
