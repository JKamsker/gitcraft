# Remote mutation scheduling evidence

Validated implementation: `28e6a8b1589c95694ec9a3c042c037518bc1829c`.
Base: `e16441b23d84ffd2d54bc9f409ae0ba4ead35983`.
Branch: `t3/serialize-remote-stash-mutations`, pushed only to `JKamsker/gitcraft`.

The scheduler classifies every keyed submission with `remote::is_remote` at
`Pool::submit_cancellable`. A typed `Local(PathBuf)` / `Remote` namespace keeps
local common-metadata keys intact and prevents collisions with local paths.
The shared remote bucket covers SSH aliases, distinct hosts and paths, and WSL
on platforms where execution recognizes it. It serializes whole operations
within a Pool; the application's mutation jobs use its singleton Pool. This is
not cross-pool, cross-process, external-Git, or remote-host locking.

`mutation_key`, tray deduplication, OWNED and owns_fetch are unchanged. The
worktree identity update was extracted for synthetic lifecycle testing.
Unkeyed reads remain concurrent; apparently read-like keyed work remains keyed
because working drafts can delete refs and tray observations can fetch.

## Submission inventory

All eleven production keyed paths reach the common normalization boundary:

- `repo/messages.rs`: spawn_mutation, including whole expectation/pop operations.
- `repo/devcontainer/mod.rs`: mutation jobs.
- `repo/dialogs/behaviour.rs`: mutation jobs.
- `repo/dialogs/bundles.rs`: bundle operations.
- `repo/dialogs/linked_machines.rs`: mutation jobs.
- `repo/email/mod.rs`: email/b4 operations.
- `repo/local_ci/mod.rs`: keyed execution (catalog reads remain unkeyed).
- `repo/saved_views/reader.rs`: working_drafts queue_mutation using tx.repo.
- `repo/ssh_setup/mod.rs`: raw repository key.
- `parallel_bisect/mod.rs`: raw repository key.
- `tray/watch.rs`: fetch/observation mutation key.

READ_POOL submissions remain unkeyed.

## Checks

All commands ran offline with the locked dependency set and
`CARGO_TARGET_DIR=/home/jonas/.t3/worktrees/gitcraft/t3-c35e2e6e/target-remote-mutations`.
No parent target directory was used.

- `cargo test --offline --locked --lib remote_mutation -- --test-threads=1 --nocapture`: 5 passed.
- `cargo test --offline --locked --lib remote_spawn_mutation -- --test-threads=1 --nocapture`: 1 passed.
- `cargo test --offline --locked --lib tasks::tests:: -- --test-threads=1 --nocapture`: 12 passed (9 scheduler tests and 3 agent command formatting tests matched by the filter).
- `rustfmt --edition 2024 --check src/tasks.rs src/repo/load.rs src/repo/lifecycle.rs src/repo/tests.rs src/tray/watch_tests.rs`: passed.
- `git diff --check`: passed.

Logs and SHA-256 hashes are preserved alongside this record. Test filters
overlap; these counts are executions, not a claim of 18 distinct tests.
The lifecycle test uses actual deferred tab initialization and suspension,
the extracted successful/empty discovery transition, and a synthetic failed
Loaded message. Pool tests record expectation/pop versus competing stash push,
alias/host FIFO, cancellation, panic recovery, unkeyed reads and local liveness.
The tab test exercises spawn_mutation itself without Git or a remote connection.
Tray tests verify distinct repository subscriptions despite shared scheduling.

Earlier compile attempts at 05eb300f and ed6e5abf exposed test-only missing Debug
assertion support and unconstrained gate channel types. These were corrected
before the passing run; the saved ed6e5abf failure log is historical evidence.

No real process-control tests, live SSH/WSL tests, full application test suite,
CI, PR updates, comments or independent final adversarial review were run.
The implementation was self-inspected; the parent provided independent design
confirmation of the central boundary, not a final candidate approval.
Separate stash data issues remain outside this change.

This evidence-only commit must have the same application files as the validated
implementation above; it binds those prior tests and does not claim a new run.
