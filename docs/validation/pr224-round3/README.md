# PR #224 round-3 validation

Application source: `10e348bcdec2cdb24dc449282d42e638f7c588da`, including the plain merge of `origin/dev` at
`7fb31c5224dd0fc64c040cc54c321d5fe129a98b` (#221, #222, #223 and #235).
The evidence commit adds only this directory; application/test source identity is
checked against that source commit.

Linux toolchain: Rust/Cargo **1.100.0**, Git **2.56.1**. Every Cargo command used
`+stable`, `nice -n 19`, two build jobs, and target/temp files beneath
`/home/jonas/work/gitcraft-targets/224` (removed after delivery).

Passed:
- `cargo +stable fmt --check`.
- `cargo +stable clippy --locked --package gitcraft --all-targets -- -D warnings`.
- `python3 ci/rust_file_size.py --revision HEAD`: 2782 files, 0 errors.
- `cargo +stable run --locked --manifest-path ci/arch-fitness/Cargo.toml -- check --revision HEAD --json <owned-target>/architecture.json`: 0 violations.
  Combined repo-core symbols budget: 610 (dev's 609 after #221, plus #224).

Focused commands: `cargo +stable test --locked -p gitcraft --lib <filter> -- --test-threads=2`.
All **69 distinct tests passed**. No full, Windows or forge-server suite ran locally.

| Filter | Passed |
| --- | ---: |
| `git::worktree_cleanup` | 40 |
| `terminal::location` | 3 |
| `repo::branch_cleanup::worktrees` | 17 |
| `repo::keyboard::tests::commit_tree` | 1 |
| `repo::detail::extras::more::tests` | 6 |
| `settings::worktree_archives::tests` | 1 |
| `commit_tree_preference_defaults` | 1 |

The independent read-only round-3 review of `4172360b` is complete and replaces
re-review. Its three findings are fixed by shared per-entry native file locks for
capture/rollback/restore/purge; a mandatory final tab/terminal/task use check under
that lock; and canonical temp ancestry in the Unix race test. The reviewed deletion
algorithm is unchanged. New regressions attempt restore and another purge during
child-fd traversal, verify independent lock contention/release and capture exclusion,
and reject active nested terminals/tabs/tasks and unavailable terminal observations.

Locks are persistent sibling files, never unlinked, avoiding split lock identities.
They exclude cooperating Gitcraft operations across windows/processes. Residual Unix
limitation: an external, noncooperating process can rename an already-open directory
out of recovery; the fd still names that moved directory, so traversal may continue
there. Descriptor anchoring prevents redirection to replacements, but does not freeze
the location of an open directory. Final presence checks cover observed app usage,
not arbitrary external processes or usage started after observation. Purge remains
separately user-confirmed and logged, never automatic.

Earlier evidence and independent reviews remain in `../pr224-round2`; no round-3
finding is skipped. Follow-ups remain native non-Linux terminal cwd observers,
recovery UI grouping/size polish and relative-submodule capture (fails safe/rolls back).
