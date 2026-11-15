# Windows capture identity fix

Application source `28be1228e4621ee0371a201dfa0682108a6bb1e4`. The evidence-only commit changes this directory alone.

Windows CI run 43666768092, git-rest job 132673969199, reports
`Failed("kept: it changed since it was looked at. Look again before removing it.")`.
The direct capture/rollback regression passed: whole-tree rename was successful.
The final inspection rediscovered the original registration using `same_dir` after the
original was moved. That canonicalization can no longer resolve Windows path aliases;
its lexical fallback misses Git's registered spelling. Symlink ancestry reproduces the
same missing-path identity loss on Unix.

Capture now resolves and saves Git's registered spelling before moving the tree. Final
inspection uses that spelling for the registration lookup, while filesystem reads stay
in the retained tree and ZIP/recovery original-path metadata stays unchanged. No sharing,
containment or verification policy was relaxed. Rename failures still preserve content.

The new Unix regression cleans up and restores a worktree through symlink ancestry.
A Windows-gated regression exercises an alternate drive-letter case. Existing assertions
now print actual outcomes, and two existing post-restore assertions compare filesystem
identity rather than requiring Git and the caller to use identical path spellings.

Local gates: `cargo +stable fmt --check`, strict root all-target Clippy with `-D warnings`,
file-size (0 errors), architecture fitness (0 violations), and focused tests:
- `cargo +stable test --locked -p gitcraft --lib git::worktree_cleanup -- --test-threads=2`: 41 passed.
- `cargo +stable test --locked -p gitcraft --lib repo::branch_cleanup::worktrees -- --test-threads=2`: 17 passed.

All Rust commands ran at `nice -n 19`, two jobs, isolated target/temp under
`/home/jonas/work/gitcraft-targets/224`, removed after delivery. Windows could not be run
locally; confirmation requires CI. No full suite or forge-server suite ran locally.

The repo-a-c failures occur in the same cleanup fixture. The separate repo-q-r resolver
failure at flow_tests.rs:444 is recorded without changing resolver code; rerun failed jobs
only if that unrelated failure persists on the new head. No old failed job was rerun.
The earlier independent reviews remain applicable to this scoped identity fix.
