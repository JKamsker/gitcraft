# PR 200: unresolved native Windows test hangs

## Evidence and scope

Run 42937657050, jobs 130261272087 (`repo-g-m`) and 130261272011
(`repo-rest`), used merge candidate 163627b22e2010d8112e98a3520d138eb78e7946.
Both shard steps reached their existing 20-minute timeout. Aggregate job
130268961293 failed downstream; it does not establish a separate test defect.

The graph matrix test
`repo::graph_view::layout_tests::all_bars_sequential_pointer_exchanges_preserve_siblings_and_origin_after_auto_fit`
reported running over 60 seconds at 01:42:12 UTC, then **passed** at
01:44:14 UTC. That warning alone is a negative control against identifying every
60-second warning as a permanent hang. The palette and PR comparison tests
reported over 60 seconds at 01:43:21 and 01:42:41 respectively, with no completion
before cancellation at 02:00:33. These logs contain no stack or last-stage trace
establishing where those tests stopped.

## Source investigation

The palette fixture creates seven commits through `git::fixture::Repo`, opens a
live `RepoTab` (including a background filesystem watcher), and polls for settled
history. The PR fixture creates four commits, opens a live tab, and polls for the
failed comparison and successful comparison's seen receipt. Both eventually
drop a live tab and remove their repository. Their wait loops bound iterations
or elapsed wall time, but **a synchronous poll, Git call, or destructor can block
inside that bound**. This is a hypothesis to distinguish, not a demonstrated
cause. The graph fixture constructs deferred tabs without opening a live watcher
and exercises 48 cases and six dividers per case.

The palette's fixture/open/settle code already appears in `760c79bb`; versions
fixture history includes `9da1c058`, `105033ac`, and `cea7dfb5`. The issue 198
extraction in `9e145dab` moved polling/submission into core seams. Inspection
included the task pool queue lock and cancellation paths, command-source and
command-cache mutexes, synchronous local Git dispatch, and watcher construction
and teardown. No evidence links a particular lock or process ownership failure
to these two Windows hangs. Recovery changes under separate review are outside
this diagnostic slice.

## Diagnostic behavior

Only these three tests opt into `repo::hang_diagnostics`. Their stderr contains
`[hang-stage TEST] begin LABEL` and `end LABEL (ELAPSED)` pairs; `unwind` denotes
panic unwinding, and `end` denotes return, not assertion success. A final unmatched
`begin` identifies the outstanding scope. Nested scopes report fixture Git calls
on the test thread, each poll with its input state, assertions/comparison stages,
and explicit teardown in the original drop order. Matrix scopes identify width,
auto-fit, scale, offset, divider, and harness construction/destruction.

The trace is thread-local: background worker Git calls are not traced or claimed
to be traced. A poll stall with no nested Git entry still needs a native stack or
more focused instrumentation; a completed poll with loading state remaining
needs worker/scheduler evidence. This is deliberately test-only diagnostics,
not a functional fix, timeout adjustment, retry, coverage reduction, or new
isolated shard. Tests and runtime assertions retain their existing coverage.
No native Windows execution is available locally; Linux passes cannot establish
resolution of a Windows hang. Parent validation and independent review remain
required before publishing a final candidate.

## Lane follow-up (#211): blocking calls and owned lifetimes

The `dev` run 43704238820 (job 132790729702, d24ac13d) and PR #238
run 43671605139 (job 132689023607) stalled in the lane tests. The former
finished 185/186 tests and stopped at 676 seconds. The reported Job Object
contained only the Rust executable; this does not identify the blocking Rust
frame or prove a root cause. Prompt/editor and pipe-holder explanations lack
support in these snapshots.

The live lane tests now seed `show_test_lane` through `put_for_test`, avoiding
`Settings::set` and its process-wide profile application. Their bodies run under
an immutable default connection context inherited by background work. The rebase
test explicitly drops its first live tab before rebasing and reopening.

Eight live lane tests, the Git lane rebase test, the stack conflict-resume/abort
test and the ghost rebase-key test supervise their entire owned body with a
90-second `recv_timeout`. This includes fixture setup, synchronous Git, blocking
polls, assertions and teardown; the inner elapsed-time loops alone cannot bound
these calls. A worker panic is propagated to libtest. A timed-out worker is never
joined, since that could hang again; CI's existing owned-process watchdog is
responsible for terminating it and its descendants. Local libtest fails without
waiting for that worker, and process exit ends it. No production timeout changes.

Read `[hang-stage TEST] begin/end/unwind LABEL` in the uploaded per-process log.
The last unmatched begin locates a fixture Git operation, opening/settling poll,
start, rebase, reopen, assertions/actions or cleanup. `[hang-deadline TEST]`
prints the active scope chain independently of the blocked worker. Existing
`GITCRAFT_TEST_PROCESS_TRACE=1` remains enabled for all Windows shards: correlate
its process launch/completion entries with the stage, rather than assuming that
a missing Git child establishes which Rust lock is blocked.

On Windows Actions, the deadline reporter leaves a 15-second diagnostic window.
The watchdog recognizes the deadline marker immediately, saves a bounded
10-second `MiniDumpNormal` attempt of its still-owned direct child, records the
Job Object snapshot, then terminates only its owned process tree. The same dump
attempt runs on ordinary shard/stall expiry. `report.json` records `thread_dump`
status/path/error; `NN-stacks.dmp` is uploaded alongside logs and JSON/JUnit by the
existing always-upload step. Open the dump with WinDbg and run `~* kb` to inspect
all thread stacks (matching executable/PDB symbols improve frame names). These
stack dumps can contain stack memory; treat them as diagnostic artifacts. Dump
failure is recorded and never converts a timeout to success. No extra GitHub CI
retry or longer test budget is introduced. Native Windows resolution remains
unproven until these diagnostics and repeated Windows runs establish it.
