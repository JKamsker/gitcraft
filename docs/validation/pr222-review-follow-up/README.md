# PR #222 follow-up validation

Application candidate: `5344e7aa7819599a8bb6032e866e8a9e790473eb`.
Base dev: `b249b2ae3d299855ea20ba77b86681b0f692d850` (already merged).
This evidence commit changes documentation only; the application gates bind to the
candidate above, with application/configuration files verified unchanged afterward.

## Review provenance and fixes

The user supplied the completed independent read-only review of `5f37ce09` and
explicitly designated it as replacing an additional re-review. Its three verified
follow-ups are addressed here. The original H1/H2/M3/L4/L5/L7 fixes and the
forge/insights budgets (166/302) were confirmed by that review.

- CI lint failure: the oversized-output test now returns `Result` and propagates
  errors with `?`. Scanning all added Rust code, including multiline calls, found
  zero `unwrap()` or `expect()` calls. Failed CI lint job: run 43651481462,
  job 132621043513; its exact diagnostic names capped.rs:81 and unwrap_used.
- Contradictory stdout: delayed bare outcomes remain provisional until reconciled
  with summary totals and named failure lists. Contradictions remain unknown or
  ambiguous and the report is incomplete. Regression coverage includes CodeRabbit's
  exact sample in `fixtures/stdout-outcome.log`, panic output, a named failures list,
  a summary contradiction without a named failure, and direct named outcomes.
- Allocation bounds: at most 8192 identities per parsed log, 1024 bytes per name or
  suite, 16384 bytes per parsed line, and 32768 entries across the fetched evidence.
  Exhaustion is incomplete; later logs are not fetched after the aggregate budget
  is spent. Cached parsed logs use Arc, including cloned JobEvidence. Selection
  merges sorted map cursors without allocating a union and retains the best 200
  row candidates before allocating their observation histories. Tests cover entry
  and byte limits, the shared aggregate budget, Arc pointer identity, priority
  displacement and exact omitted-row counts.
- Windows shard termination: direct shard headers open a shard report; only the
  runner's `Passed N tests in <shard>` completion marker closes it. Success and
  fail-fast fixtures replace the impossible continue-after-failure sequence.
- CodeRabbit headerless identity finding: a new announcement without a suite header
  clears the previous cargo identity; the same-name tests remain distinct.
- CodeRabbit retry finding: current/baseline/history read failures retain a retry
  action. The UI regression clicks retry, verifies no duplicate while loading,
  and accepts a successful replacement without changing the job selection.

## Checks on the application candidate

- Exact gitcraft-package command from ci/clippy_packages.py passed:
  `nice -n 19 cargo +stable clippy --locked --manifest-path Cargo.toml --package gitcraft --all-targets -- -D warnings`.
  Local stable is cargo/rustc 1.97.1; the failed CI run used 1.100.0. Final CI must
  verify the candidate with CI's toolchain.
- `cargo +stable fmt --check`: passed.
- Fast tooling tests: 125 passed.
- Rust file size: 2704 tracked Rust files, zero errors.
- Complexity policy: zero errors.
- Architecture check on `--revision HEAD`: zero violations; budgets unchanged.
- Focused application tests: 28 passed, zero failed or ignored. The libtest
  includes were log_evidence, test_history, repository evidence_tests, capped
  CLI output and ci::timing::logs; no full application suite was run.

Cargo used the task's dedicated CARGO_TARGET_DIR and TMPDIR, two build jobs and
nice 19. Clippy used CARGO_PROFILE_DEV_DEBUG=0. Focused tests used
CARGO_PROFILE_TEST_DEBUG=0 and `--config 'profile.dev.package."*".opt-level=0'` to
limit local compilation cost. Full/Windows/live-forge suites remain CI's work.
The ci/check.py plan was inspected and the task's permitted gates run individually.
