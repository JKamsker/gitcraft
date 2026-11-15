# PR #224 publication integration

Validated committed source `00b78c0c0f9e0afb4a22c6d10e0f17bb948a75e0` plain-merges dev
`8e69afb7df4141eada62495793a9645d181abe0e` (#240 and #243). Every declaration
published on either branch is preserved unchanged. Fresh declaration
224-243-combined-repo-core-symbols-614 covers the combined coordinator budget of 614.
Policy changes from dev are retained; no application conflict required manual resolution.

Passed on committed HEAD:
- Release arch-fitness: 0 violations.
- Growth gate against origin/dev and previous pushed head 9a9f632f: passed.
- Formatting, strict root all-target Clippy with -D warnings, physical file size: passed.
- Selected overlap tests:
- `repo::dialogs`: 52 passed.
- `repo::sidebar`: 30 passed.
- `repo::graph_view`: 91 passed.
- `terminal`: 51 passed.
- `push_preflight`: 72 passed.
- `git::worktree_cleanup`: 41 passed.
- `repo::branch_cleanup::worktrees`: 17 passed.

Test command: cargo +stable test --locked -p gitcraft --lib <filter> -- --test-threads=2.
Tests used CARGO_PROFILE_TEST_DEBUG=0 (debug symbols disabled, unoptimized test code and
assertions remain enabled). Every Rust command used nice 19, two jobs, +stable and the
isolated target/temp beneath /home/jonas/work/gitcraft-targets/224, removed after delivery.
No full, Windows or forge-server suite ran locally. Prior head 9a9f632f had 24/24 CI
checks green; this evidence does not claim CI passed for the new merged head. The final
evidence commit adds this directory alone, with source identity verified. Independent
reviews remain complete; this plain merge and combined declaration need no new review.

The first push_preflight attempt passed 67 tests and failed 5 because git-lfs was
absent locally (including LFS history-repair failures). Extracting Ubuntu's git-lfs
3.4.1 package into the owned target directory and adding its binary to PATH resolved
all 5; the full 72-test push_preflight filter passed. No application code was changed.
The original failed log is retained as missing-lfs.log.gz. The extracted tool and
package are removed with the target directory. Other already-passed filters were not
repeated; the final report records all 354 selected tests passing.
