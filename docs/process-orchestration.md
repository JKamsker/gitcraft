# Subprocess I/O and bounded jobs (#160)

GitCraft uses one lazily started Tokio event-loop thread for migrated subprocesses. Tokio
was already a dependency for MCP; this enables asynchronous processes, pipes, timers and
channels without moving UI state into a runtime.

## Limits

- Repository jobs: **8 workers, 512 queued**, shared by all tabs. At most **6 background
  mutation/polling jobs** occupy workers, leaving capacity for interactive reads. Submission
  never waits for a worker. Overload and panic use the same typed failure continuation,
  releasing operations' busy state and reloads' loading state.
- Reload leaf reads: **8 shared workers, 64 queued**. Outer reloads join these from the
  repository job pool, so waiting parents cannot occupy their own readers. Leaves must not
  recursively submit leaf work. Single-tab parallelism remains, without seven scoped OS
  threads for every reload.
- Managed processes: **8 active, 256 queued**, with at most **6 long/background commands**
  active. Interactive reads can pass background commands blocked by that limit and use the reserved slots.
  Oldest runnable requests dispatch first so reads cannot starve mutations, while the
  background limit reserves capacity for interactive reads. Capacity counts both channel
  entries and the scheduler's pending list. No task or thread
  is created just to wait for a process slot.
- Operations started by `run_ops_ex` hold their repository key across the
  entire command sequence. Other tabs' operations with the same key wait FIFO, including
  between commands. Other paths and reads can run. External tools still use Git's locks.
  Local linked worktrees share their common Git directory as the key; remote tabs use the
  main worktree discovered by their initial read. Unity setup writes use the same queue.
- stdout/stderr are drained concurrently with stdin and child exit notifications. Each
  captured stream is limited to **64 MiB**. Overflow stops/reaps the command and reports an
  error rather than silently returning truncated data. Watched stderr feeds the existing
  progress reader/repaint callback; stopped watched commands retain partial output in Activity.
  Binary/blob reads have an explicit 512 MiB memory bound. Local patch exports stream straight
  to a file, bypassing the capture bound; remote exports retain the persistent-shell protocol.
- Remote polling uses egui timers and schedules one background look at a time. No thread
  stays parked between looks, and a slow look cannot accumulate duplicate polls.

Eight slots retain single-tab parallelism while avoiding the process/thread storms measured
at 50/100 tabs. An initial 256-entry job queue rejected the 100-tab burst; 512 admits that
initial work while bounding subsequent panel reads. Excess requests are explicitly rejected.
These are conservative defaults, not a claim to have found universal optimal limits.

The existing synchronous Git APIs wait for event-loop results on bounded workers.
`processes::Service::submit` also supports multiple outstanding tickets without waiting caller
threads, as used by worktree-status batches and regression tests.

## Lifetime

Dropping a ticket requests cancellation instead of aborting the task owning its child.
Cancellation/deadline checks run while draining both pipes; stopped children are killed and
reaped. Unix children own a process group. Windows children start suspended, enter a
kill-on-close Job Object, then resume, preventing descendants from escaping during startup.
The small Win32 implementation lives behind a safe API in `gitcraft-os`.

Timeouts include time waiting for a process slot. Cancelled queued operations send their typed
completion without waiting for a worker. Failed reloads cancel and join unfinished sibling
readers before clearing the loading state.

Tabs own cancellation scopes inherited by jobs and reload readers. Closing a tab cancels
queued work and migrated children. Suspension renews the scope/result channel so late results
cannot repopulate a resumed view. App exit cancels jobs, drains/joins the process loop and
closes idle persistent shells. Shutdown also cancels/joins parallel bisects and waits for their
remote stop requests. No process-owner futures are aborted. Inherited open pipes have
a drain deadline; timed Git calls retain their completed result while cleaning up helpers.

Watched standalone SSH/WSL streams use the event loop for ready/banner handling, PID markers
and progress. Cancellation gives the PID marker a bounded grace period, then uses the existing
remote stop protocol before returning. That blocking protocol runs on at most two Tokio
blocking workers, apart from the I/O loop. Failed startup preserves authentication stderr.
Credential-save callbacks use the same two-permit budget and start when the ready marker
arrives, without blocking pipe draining.
On Windows, Tokio's anonymous process pipes themselves use blocking I/O internally: the
runtime allows at most `3 × active slots + 2` blocking workers (26 at the default limit),
covering both output readers, stdin and cleanup without pipe starvation. This is a fixed
budget per service, independent of queue depth. Unix pipes use event-driven I/O.

Unix cleanup directly signals the child's process group through the OS helper crate and
reports signaling errors. A descendant that deliberately calls `setsid` can escape that
group; tracking snapshots alone cannot guarantee containment of daemonized descendants.
This PR does not add cgroups or another OS containment policy. Windows Job Objects cover
descendants before they can run.

Persistent SSH pool/framing readers remain blocking, with their existing host/lane limits;
their waits now observe tab cancellation. History pagination uses cancellable bounded commands
on all platforms, trading a process per page for releasing the worker on tab close. PTYs, HTTP libraries, external
interactive tools and unrelated app workers have not all been converted. Persistent shell
reuse and SSH transport multiplexing are unchanged. Broken networks can still prevent
confirmation of remote cleanup, as with the existing transport.

## Benchmark

Requires Rust 1.95+, Git and Unix `ps` for resource sampling:

```sh
python3 ci/bench-processes.py --label optimized
# Reuse saved binaries without a compiler running during measurement:
python3 ci/bench-processes.py --label optimized-repeat \
  --binary target/process-bench/optimized-tests
```

For baseline `35a26a5`, copy the test-only `src/repo/process_benchmarks.rs` module and its module
declaration into a detached checkout. Omit the new `jobs`, `job_queue_peak` and `processes`
JSON metrics, build its lib test executable and pass that saved binary with `--binary`.
Only benchmark instrumentation belongs in the baseline checkout.

These measurements compare the implementation before/after on the original workspace base,
`35a26a5`. The PR targets `dev` and excludes that base's unrelated performance commits.
The tables record the original implementation before review follow-ups to scheduling,
Windows pipe budgets and cancellation; they are not a measurement of those follow-ups.

Fresh test processes run 1/10/50/100 production background Git shell commands
(`sleep 0.1; printf ok`) or independent one-commit repository clones with real reloads and
watchers. Remote tabs use the existing test-only loopback shell transport: real pool/framing
contention, without SSH network, encryption or sign-in latency. Fixture creation is outside
elapsed/latency timing. Tab timing ends after the initial load is applied; secondary panel
jobs may still be queued.

The runner samples parent OS threads/RSS and descendant processes about every 10 ms plus
sampling overhead, so short peaks may be missed. Descendants include Git's shell/helpers;
RSS excludes child memory. Completion/submission times and a 2 ms UI polling heartbeat are
recorded. Heartbeats exercise message polling, not GPU rendering/native event dispatch.
Optimized counters also report active/queued/spawned processes and peak job queue size.
Help → Copy diagnostics exposes live job/process counters for real sessions.

Measured 2026-11-07 on Apple M4 Max, macOS 27.2, Rust 1.95.0, Apple Git 2.54.0.
One debug-build run per case; no compiler ran during these measurements. These are
diagnostic workload measurements, not release-build or real-network performance claims.

Each pair is **before → after**. RSS is parent resident memory, in MiB.

| Workload | Count | OS threads | Descendant processes | RSS (MiB) | Peak queued jobs after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Commands | 1 | 3 → 11 | 3 → 3 | 18.6 → 21.3 | 1 |
| Commands | 10 | 12 → 11 | 30 → 18 | 19.3 → 22.3 | 3 |
| Commands | 50 | 52 → 11 | 150 → 18 | 21.9 → 23.8 | 42 |
| Commands | 100 | 102 → 11 | 291 → 18 | 26.3 → 27.8 | 92 |
| Local tabs | 1 | 8 → 23 | 1 → 6 | 27.0 → 28.8 | 5 |
| Local tabs | 10 | 103 → 32 | 41 → 8 | 34.4 → 34.7 | 105 |
| Local tabs | 50 | 248 → 71 | 189 → 8 | 43.9 → 38.3 | 512 |
| Local tabs | 100 | 505 → 121 | 369 → 8 | 61.3 → 45.4 | 512 |
| Loopback remote tabs | 1 | 23 → 34 | 14 → 12 | 31.1 → 29.4 | 6 |
| Loopback remote tabs | 10 | 147 → 41 | 20 → 25 | 36.2 → 34.8 | 110 |
| Loopback remote tabs | 50 | 620 → 44 | 25 → 25 | 58.2 → 42.1 | 512 |
| Loopback remote tabs | 100 | 1144 → 42 | 27 → 22 | 88.7 → 55.2 | 512 |

| Workload | Count | Elapsed (ms) | Completion p95 (ms) | Submit (ms) | Largest UI polling gap (ms) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Commands | 1 | 119.5 → 121.3 | 117.0 → 118.7 | 0.0 → 0.1 | 2.5 → 2.5 |
| Commands | 10 | 125.7 → 238.3 | 123.4 → 236.2 | 0.1 → 0.2 | 2.5 → 2.5 |
| Commands | 50 | 164.1 → 1050.3 | 161.5 → 933.8 | 0.5 → 0.2 | 5.3 → 2.5 |
| Commands | 100 | 225.5 → 1974.4 | 221.0 → 1854.6 | 1.6 → 0.3 | 6.2 → 2.5 |
| Local tabs | 1 | 95.6 → 94.5 | 93.0 → 91.9 | 0.2 → 0.3 | 2.6 → 2.6 |
| Local tabs | 10 | 234.4 → 282.9 | 231.7 → 280.3 | 0.7 → 0.6 | 6.2 → 2.8 |
| Local tabs | 50 | 1837.1 → 1187.4 | 1714.2 → 1142.8 | 2.2 → 1.2 | 7.1 → 3.1 |
| Local tabs | 100 | 3701.9 → 2342.3 | 3454.9 → 2272.6 | 28.0 → 1.9 | 33.4 → 5.2 |
| Loopback remote tabs | 1 | 196.1 → 209.1 | 193.6 → 206.6 | 0.2 → 0.4 | 2.6 → 2.7 |
| Loopback remote tabs | 10 | 1009.9 → 929.3 | 1007.3 → 926.8 | 0.7 → 0.6 | 3.2 → 2.9 |
| Loopback remote tabs | 50 | 7948.7 → 4144.6 | 5343.0 → 4001.4 | 2.3 → 1.3 | 3.8 → 3.4 |
| Loopback remote tabs | 100 | 16683.2 → 8277.5 | 14056.2 → 8060.5 | 5.0 → 2.3 | 5.2 → 4.9 |

The 100-command sleep workload deliberately trades total completion time for bounded
resources: its Git shell aliases count as long commands, so at most six run at once.
It uses 11 OS threads at 1/10/50/100 jobs instead of adding one waiting thread per job.
The process service itself uses one I/O thread; repository jobs supply eight workers.
Reloads add the eight shared leaf workers. Local OS thread counts still grow with
the native filesystem watcher per tab; remote counts include persistent shell readers.
At 50/100 tabs secondary panel work reaches the explicit 512-job cap. Initial reloads
complete, while excess secondary requests receive a visible overload error.

Compared with removing reload parallelism altogether (the first prototype), the shared
leaf pool restores single-tab load latency: the final local case is 94 ms versus 96 ms
before. More slots are not assumed to improve every repository or network.

Raw runs, metadata and saved executables are under ignored `target/process-bench/`
(`baseline.json`, `optimized.json`, and their metadata/logs). Earlier exploratory runs
used repeated tabs of one path; the tables above use independent clones.

## Validation

Regression coverage includes simultaneous large stdout/stderr/stdin; exit/spawn errors;
bounded queues/workers and immediate rejection; cancellation before spawn/during I/O;
dropped tickets; timeout/partial output; inherited pipes; reserved interactive slots;
FIFO mutation keys across workers; panic recovery; remote ready-marker boundaries;
streamed remote progress/cancellation; tab close; and rapid reload coalescing. The core child
fixture is the Rust test executable, so these process tests run on Windows without a shell.

Validated on the macOS host:

- On the benchmark base, 3,011 library tests passed, 9 opt-in tests ignored, 5 known baseline failures excluded.
  The excluded tests were also run against the saved unmodified baseline and failed there:
  the two checkpoint shell-parser cases, WSL tool path canonicalization, content-regex
  search, and the drag/drop rebase case using GNU `sed -i` syntax.
- The review-follow-up library run on `dev` passed 3,003 cases with 8 ignored and the same
  5 exclusions. It caught a Darwin zombie-only process-group error, fixed by reaping the
  child before signaling; that case and the global-process-probe bisect timeout case pass
  on rerun. The earlier six timeline/version full-suite failures did not recur.
- All 32 focused process/job/timed-command/tab-lifecycle/export tests passed, including a
  real 65 MiB Git blob read and disk export. After the Darwin fix and blocking-work budget,
  all 29 core/remote-cancel/bisect-timeout cases passed again. Regression tests also check
  descendant readiness and absence of a delayed write after cancellation.
- `cargo +1.95.0 fmt --check`, `git diff --check`, and all-target Clippy passed. Clippy used
  `-D warnings -A dead_code` for the repository's existing macOS-only dead-code warnings.
- The Windows process/job modules, their tests and the Win32 helper cross-check successfully
  for `x86_64-pc-windows-gnu` in a minimal harness using the production sources. The helper
  crate also checks directly for that target. Native Windows/Linux execution was unavailable.
  A full Windows application cross-check is blocked by the absent
  `x86_64-w64-mingw32-gcc` needed by the pre-existing `ring` dependency; CI must run the
  cross-platform tests natively.

The real checkpoint CLI test, all six MCP stdio integration tests, and the OS helper unit
test passed. These exercise the binary in addition to library/unit-test callers.
