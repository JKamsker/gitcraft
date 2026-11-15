# PR #222 review-fix validation

Application candidate: `80619a96a857c66301b2adecb19e19ba558a994d`.
Base dev: `b249b2ae3d299855ea20ba77b86681b0f692d850` (plain merge).
The evidence commit adds only this directory; application checks bind to the candidate above.

## Review findings from #225

- CI blocker: narrowed internal parser/history visibility; forge exports 166 and insights 302, with budgets matching. Remaining additions are consumed across domain boundaries by the CI reader and repository UI. Existing unrelated budgets were preserved. The old Windows result guard failed because arch fitness skipped its prerequisite tests; no Windows test executed in that run.
- H1: stable binary/crate plus cargo target plus test path identity; conflicting repeated outcomes remain visible as an ambiguous failure row. Parser and history regression tests cover both cases.
- H2: delayed bare outcomes for `--nocapture`, preserving a pending test across another named completion as LibtestParser does.
- M3: capture job logs to 64 MiB rather than the normal 16 MiB; only a proven capture overflow becomes partial evidence. Synthetic tests cover oversized successful output, truncation and other stop kinds. No real process-control test was introduced or run.
- L4: cargo fail-fast leaves an incomplete report.
- L5: a binary header without an announcement/summary leaves an incomplete report.
- L6: timestamped cargo workspace and Windows shard/serial fixtures include hashed executable names, delayed outcomes, panic output and precompiled-shard progress. The latter exposed and fixed a false binary-header recognition.
- L7: direct shard binaries get their stable executable name as suite identity.
- Additional boundary: capped output before any test report is incomplete.

## Results

- `cargo +stable fmt --check`: pass.
- `python3 -m unittest discover -s ci -p 'test_*.py'`: 125 passed.
- `python3 ci/rust_file_size.py --revision HEAD`: 2699 Rust files, zero errors.
- `python3 ci/complexity_policy.py --revision HEAD`: pass.
- Architecture checker: 45 tests passed; candidate check zero violations.
- Application filter `insights::ci_path::`: 50 passed, including the repository evidence UI tests.
- Application filter `forge::cli::capped`: 3 passed.
- Application filter `ci::timing::logs`: 1 passed.

Rust commands ran through `nice -n 19 cargo +stable`, using the dedicated target/TMPDIR,
with two build jobs. Application test commands used `--locked --lib`,
`--config 'profile.dev.package."*".opt-level=0'` and `CARGO_PROFILE_TEST_DEBUG=0`.
The first broad filter run had 49 passes and one environmental failure: system Git
2.43 rejects `--no-lazy-fetch`. The same filter passed all 50 using a privately
extracted Debian Git 2.47.3 package, without changing or skipping tests.

`ci/check.py --dry-run --test insights::ci_path` was inspected; the allowed fast
checks were run individually under the task's resource restriction. Full suites,
Windows/live forge suites and all-target Clippy were left to CI. No new execution
of those gates is claimed. A fresh independent review and final PR CI remain
required before merge; this document records implementation verification only.
