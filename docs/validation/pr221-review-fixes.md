# PR #221 review-fix validation

Application candidate: `4aaac011700cd5fb10933c2b94481ad32a514a53`. Preserved work branch: `944fc1f75a167ef21fab73133f7ba3dd690ad649`.
The candidate merges `origin/dev` normally; published history is preserved.

## Scope and environment

Review input: [#225](https://github.com/JKamsker/gitcraft/issues/225), findings for
[#221](https://github.com/JKamsker/gitcraft/pull/221), originally at `1e9d08f4`.
Fixes cover CI, H1, M2, M3 and L4–L8. No finding was skipped.

Rust gates used `cargo +stable`, `nice -n 19`, two build jobs, and the dedicated
`/home/jonas/work/gitcraft-targets/221` directory. Repository tests used a disposable
Git 2.51 build: system Git 2.43 rejected the existing `--no-lazy-fetch` policy.
No full Rust, Windows, or forge-server suite was run locally.

`ci/check.py --dry-run --test repo::insights::purpose` was inspected. Its gates
were executed with Clippy restricted to the application crate and tests restricted
to the touched functionality, as requested for this resource-limited task.

## Combined candidate gates

- `cargo +stable fmt --check`: passed.
- `cargo +stable clippy --locked -p gitcraft --all-targets -- -D warnings`: passed.
- `python3 ci/rust_file_size.py --revision HEAD`: 2702 tracked files, zero errors.
- `python3 ci/complexity_policy.py --revision HEAD`: zero errors.
- Architecture checker (`check --revision HEAD`): 28 domains, 291 edges, zero violations.
  No architecture budget was increased during the fix round.

Focused tests used `cargo +stable test --locked -p gitcraft --lib FILTER -- --test-threads=2`:

- `purpose`: test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 5294 filtered out; finished in 1.65s
- `diff`: test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 5305 filtered out; finished in 0.00s
- `folding`: test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 5289 filtered out; finished in 0.76s
- `attrs`: test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 5307 filtered out; finished in 0.00s
- `i18n`: test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 5304 filtered out; finished in 0.23s

Filters: `repo::insights::purpose`, `insights::transform::diff`, `repo::folding`,
`git::attributes`, `i18n::tests`. Total: **46 passed**.

The Python tooling tests passed **125 tests**. Tooling sources are unchanged from
that execution: the `ci` tree at the preserved WIP and combined candidate is
`14459a1fb5a968891946d58fa2944aee38ba6efb`; the earlier tooling result is bound to that identical tree.

The 15 actual purpose test names were listed through the native libtest binary and
matched against `ci/test-shards.py`'s real selectors: each selected exactly once.
This checked the rename regression without running Windows suites. Purpose and
shard-selection source files are unchanged between that check and the combined candidate.

## Review checks

- Verified the original Windows failure in run `43496076054` and its CI summary:
  overlapping shard selection for `purpose::tests::repo`, before test execution.
- Verified bounded matching and size limits, background proposal revision guards,
  rename accounting, base attributes, custom settings passed through the existing
  caller boundary, thread interval edges, contextual identity and both catalogs.
- Regression fixtures include an owned bare-repository force push and fetch, a rename
  across the 80-path boundary, oversized repository rules, changed base context,
  unknown hunks, and thread boundaries.

This file is a documentation-only evidence addition after the application checks;
no new application test execution is claimed for its documentation commit.


## Independent review follow-up (de884625)

The maintainer supplied an independent read-only review of
`de8846254fa81a0e0ed45b6249a8d91009d76c93`; that review replaces another re-review.
All four follow-up findings are addressed; none was skipped or deferred.

Implementation commit: `a8e76b0f93ed667c9ac106abaedc6fde04c50ea5`, preserved on
`work/221` and verified against the remote. Combined application candidate:
`fa9595dbd9dad598b2be9c9618616a1ff5708381`. It normally merges `origin/dev`
`70d0bb8f` (including #223), without rewriting published history.

- Proposal jobs use guarded `then_or` failure completions. Recorded-job regressions
  cover rejection, retry and stale request/proposal failures; arrangement errors
  enable the existing standard-order fallback.
- Oversized base blobs cache a sentinel per path. A recorded-command test checks
  one size lookup per file and no oversized blob reads.
- The rename editor uses English/German catalog entries. The UI harness exercises
  both controls, cancellation, the original group ID and a UTF-8 rename action.
- #224 remained open at the final implementation fetch. The merged candidate's
  architecture check passes with repo-core symbols at 609 and no baseline change.
  Before merging #221, fetch/check dev again; if #224 lands first, merge dev and
  validate the composed repo-core symbols budget of 610.

Executed on this combined candidate with low-priority `cargo +stable` (Rust
1.97.1), two build jobs and disposable Git 2.51:

- Formatting, strict application all-target Clippy (`-D warnings`, including the
  repository's denied unwrap lint), file-size and complexity gates: passed.
  File-size check: 2724 tracked Rust files, zero errors.
- Architecture: 28 domains, 291 edges, 239 cross-feature mutations, zero violations.
- `repo::insights::purpose`: 19 passed, including the four new follow-up regressions.
- `i18n::tests`: 5 passed.
- `feature_failure_continuation_runs_when_a_queued_job_is_cancelled`: 1 passed.
- Python CI tooling tests: 125 passed.

Total focused Rust tests: **25 passed**. No full Rust, Windows or forge-server
suite ran locally. CI uses Rust 1.100 and remains the authority for that toolchain
and platform coverage. This evidence addition changes documentation only after
the listed application checks; its commit claims no additional test execution.
