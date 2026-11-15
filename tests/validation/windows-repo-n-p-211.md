# Windows repo-n-p lifecycle follow-up (#211)

PR #244 run 43716540020, attempts 1 and 2 (jobs 132826305465 and
132829933684), repeats the practice cleanup failure previously seen on #223.
Attempt 1's stale-answer test failed deleting its fixture with Windows sharing
violation 32. Attempt 2's watchdog reported practice and configured-force-refspec
push cases unfinished after 745 seconds; its Job Object snapshot contained only
Rust PID 1996 (28 threads). This establishes neither the blocked frame nor a
subprocess prompt. No Windows execution is available locally.

Both practice cases and every live-tab push case now use the #246 `bounded`
supervisor: the 90-second body deadline covers setup, Git, synchronous polls,
assertions, watcher shutdown, worker draining and directory cleanup. `bounded`
opts into thread-local stages and pins an immutable default connection context.
Settings are seeded with `put_for_test`, with no process-wide `Settings::set`.
Deferred-only push tests also seed settings; they start no live watcher/fixture.

A live-tab guard calls the existing test-only `close_and_wait` seam on every
exit, including errors and assertion unwinding. It cancels tab jobs, drops the
watcher, and drains the detached receiver until all job senders disappear, with
a 10-second drain budget. A returned teardown error is a warning. The outer
supervisor also covers a blocked destructor, which precedes the seam's timer.
Fixtures are removed after tab guards. Practice's root guard now also cleans up
on early errors. Only Windows sharing/lock violations 32/33 are retried, with
20/40/80/160/320/640 ms backoff (1.26 seconds total); missing paths succeed, other
errors and persistent sharing violations warn and retain the fixture. Cleanup
errors never replace the test result. An owned-file Windows regression verifies
nonfatal cleanup while deletion is denied, then retries after that handle closes.

Read uploaded `[hang-stage TEST]` entries for fixture creation/Git, practice
check/reload, Push dispatch, poll and close/wait/remove scopes. Unmatched begins
and `[hang-deadline TEST]` locate the outstanding stage. #246's existing watchdog
then attempts a bounded thread-stack minidump, records the owned process snapshot
and stops its owned tree; process tracing and always-upload reports remain
unchanged. A future blocked worker requires the stacks to establish its cause.
Assertions and production behavior are unchanged; Linux repeats cannot prove
resolution of a native Windows stall.
