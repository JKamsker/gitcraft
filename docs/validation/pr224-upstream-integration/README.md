# PR #224 upstream-picker integration

Validated committed source: `7e186611bbfb848d41d9077b8e613fecb4dd1237`. Plain merge of dev eabb0ad1 (#239), preserving
published declarations from both branches. Combined repo-core symbol budget 613 has a
fresh declaration; previous values remain untouched. Settings 181/sidebar exports 37
remain, and git preserves dev's corrected 1025 export ceiling.

Passed on the committed merge:
- Release arch-fitness: 0 violations.
- Growth gate against origin/dev and previous PR head e2d4be8d: passed.
- Formatting and strict root all-target Clippy (`-D warnings`): passed.
- Rust file-size check: 0 errors.
- Wider tests with `cargo +stable test --locked -p gitcraft --lib <filter> -- --test-threads=2`:
- `repo::dialogs`: 52 passed.
- `repo::sidebar`: 30 passed.
- `repo::graph_view`: 91 passed.
- `terminal`: 51 passed.
- `git::worktree_cleanup`: 41 passed.
- `repo::branch_cleanup::worktrees`: 17 passed.

All Rust commands used +stable at nice 19, two jobs, isolated target/temp beneath
/home/jonas/work/gitcraft-targets/224, removed after delivery. No full, Windows or
forge-server suite ran locally. Prior final head e2d4be8d had fully green CI; that
is prior-head evidence, not a claim that this merged head passed CI. No new independent
review was required for the plain merge and declarations. Evidence commit adds only
this directory; source identity is verified against the named candidate.
