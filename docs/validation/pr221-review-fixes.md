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
