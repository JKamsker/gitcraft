# Issue 189 local validation

Application/test candidate: `79b2269e8102ea2e536697b78f4b305002308acb`.
Base includes `d3a759bd` and the normal merge of `dev` at `eabb0ad1`.
The subsequent evidence-only commit changes documentation; bind these application gates
only after verifying `git diff --name-only 79b2269e HEAD` contains documentation paths only.

Environment: Linux, stable Rust/Cargo 1.100.0, Git 2.50.1. The system Git 2.43 cannot
run this repository's existing `--no-lazy-fetch` command envelope; Git 2.50.1 was built
locally under this task's disposable target directory. No system tools were changed.
All Rust toolchain commands ran at nice 19, with target and TMPDIR under
`/home/jonas/work/gitcraft-targets/189`. That directory is removed at handoff.

## Results

- Strict touched-crate Clippy passed: `cargo +stable clippy --locked --package gitcraft --all-targets -- -D warnings`.
- `cargo +stable fmt --check` passed.
- All 161 selected Rust tests passed, none ignored. Exact selection:
  `cargo +stable test --locked --package gitcraft --lib -- git::testlane repo::test_lane git::stream repo::detail repo::load git::repository_tools::tracking repo::tracking --test-threads=2`.
- All 132 Python CI tooling tests passed: `python3 -m unittest discover -s ci -p 'test_*.py'`.
- `python3 ci/rust_file_size.py --revision HEAD`: 2,786 tracked Rust files, zero errors.
- `python3 ci/complexity_policy.py --revision HEAD`: zero errors.
- Architecture fitness `check --revision HEAD`: zero violations. This used the release
  checker built with `nice -n 19 cargo +stable run --release --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD`.
- `python3 ci/arch_baseline.py --base origin/dev --revision HEAD` passed; the comparison
  against prior pushed `8eb0063f` also passed. This feature changes no architecture budgets
  or increase declarations; incoming tracking declarations were preserved by normal merge.
- `python3 ci/check.py --dry-run` confirmed gate commands. The task's explicit resource
  restriction overrides its all-package Clippy/compile-all plan: equivalent touched-crate
  gates and the stated affected module groups ran manually. No full application suite,
  Windows suite, forge-server suite, CI dispatch, rerun, or CI wait was performed.

The synthetic cancellation-token test passed before live owned-child cancellation.
The branch was pushed and its remote SHA verified before the wider process tests.

## Acceptance evidence

The real-Git staged tests cover the forgotten helper despite passing working-tree tests;
partial staging and source index/status/ref preservation; additions/deletions/renames/modes;
unborn and split indexes; conflicts and submodules; ignored assets; copied-index races;
intent-to-add freshness; suppressed checkout hooks; live cancellation, tab-lifetime
cancellation, and cleanup. Existing test-lane crash-sweep tests passed in the same group.
UI tests cover named accessible controls, stale-on-restage and late generation guards,
plus the responsive commit-panel matrix. Standard egui controls retain keyboard access.
Both en-US and de catalogs include the new application text.

V1 is local-only and explicitly rejects SSH/WSL and submodules. Dependency preparation
runs through the user's command inside the snapshot. Environment evidence hashes process
variables and effective Git config; external tool/dependency changes still require a rerun.
Independent adversarial review and final CI/merge remain the next delivery phase.

## Retained logs

- `architecture.txt`: SHA-256 `79be5263788791d63791fb17540614c884a050d808118dfc90705bf9948a0f5a`
- `clippy.txt`: SHA-256 `b289fccda57075c1ef93d642bb04965a45d4141b02646d00db1c049ff982dbc6`
- `module-tests.txt`: SHA-256 `1963d31d224756d5589e8ad3f4673f86795a4355e38ce8994bd348cd19511a8b`
