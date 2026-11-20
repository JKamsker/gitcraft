# Release failure at 014add85 (PR #244 merge, #211)

Release run [43721621861](https://github.com/JKamsker/gitcraft/actions/runs/43721621861),
attempt 1, for `014add85891638e721151222029824e8fcb30751` (the merge of PR #244
into `dev`, 2026-11-15T13:56:24Z), concluded `failure`. No `vX.Y.Z-dev.N`
prerelease was published for this commit; the run was never re-run.

## What failed

Only the gate job `CI passed for this commit`
([job 132834908856](https://github.com/JKamsker/gitcraft/actions/runs/43721621861/job/132834908856))
failed. Both package builds (`build / package (windows-x86_64)` and
`(linux-x86_64)`) passed; `release` was skipped because the gate failed. The gate
(`ci/await_ci.py`) waited 2700 s for CI run
[43721621876](https://github.com/JKamsker/gitcraft/actions/runs/43721621876) of the
same SHA and failed at 14:42:33Z with
`'CI result' completed/failure`, as designed: it publishes only after `CI result`
passed for the exact commit.

CI run 43721621876 attempt 1 failed because one Windows shard failed:
`windows-tests / Windows tests (repo-rest)`
([job 132843503990](https://github.com/JKamsker/gitcraft/actions/runs/43721621876/job/132843503990)).
`test (windows-latest)` and `CI result` failed only as aggregates of it. Every
other job, including both Ubuntu test jobs and packaging smoke, passed.

In that shard, `ci/libtest_watchdog.py` reported 198/199 tests finished after
about 180 s, with
`repo::timeline::receipt_tests::a_squash_says_the_files_are_identical_and_a_commit_went`
still running. No test started or finished for 600 s (the stall limit), so the
watchdog stopped the process after 735.937 s
(`repo-rest: stalled`, `terminated Job Object`). No assertion failed.

## Evidence on the cause

From the `test-report-windows-repo-rest-attempt-1` artifact of run 43721621876
(`report.json`, `00-stacks.dmp`; Windows Server 2025 image `win25-vs2026`
20261103.298.1, Git 2.56.0.windows.4, rustc 1.99.1):

- The owned Job Object held a single process, the test binary (PID 3616,
  25 threads). No `git`, PowerShell or other child was alive, so the stall was
  inside the test process, not a waiting subprocess or prompt.
- The thread-stack minidump, read with `cdb` (`~*kc`), shows the test's own
  thread in `ntdll!NtQueryDirectoryFile` called from
  `KERNELBASE!GetFileInformationByHandleEx`, i.e. enumerating a directory. A
  `notify-rs windows loop` thread was in `ReadDirectoryChangesW` (a live file
  watcher), and two pools of `gitcraft-job-*` workers plus `gitcraft-process-io`
  were idle in waits. The frames inside the test binary are unsymbolized: the
  run's test binaries/PDBs are no longer among its artifacts.
- At this commit the test removed its fixture with `std::fs::remove_dir_all`
  right after `receipt_of` dropped its `RepoTab`, without draining the tab's jobs
  or stopping its watcher first (`git show 014add85:src/repo/timeline/receipt_tests.rs`).
  A directory walk on the test thread while a watcher still watches the fixture
  is consistent with that removal, but a single snapshot cannot prove which call
  was running or why it did not return. The root cause is **not proven**.
- The same job re-run as attempt 2 (14:44Z,
  [job 132847863838](https://github.com/JKamsker/gitcraft/actions/runs/43721621876/job/132847863838))
  passed in 2 min 39 s with no source change. That shows the failure is
  intermittent; it does not show it is harmless.

## Classification

- **Not a product bug** on the evidence: the stalled code is a test's fixture
  lifecycle (tab drop and directory removal), and production code was not
  implicated by any frame or assertion.
- **A test-isolation defect (CI-side), Windows-only and intermittent**: the
  test's live tab and watcher outlived it into fixture removal. It is classified
  `timeout-or-hang`, not infrastructure: the runner, toolchain and network
  behaved normally, and the only owned process was the test binary.
- **Release behaved correctly**: it refused to publish a commit whose CI had not
  passed. One CI-design note: re-running CI after the gate has failed does not
  re-run Release, so this commit stays unpublished unless Release is re-run.
  The next merge's prerelease covers it.

## Fixed by a later PR?

PR #248 (`749ccd20`, merged 2026-11-15T16:25:13Z,
`tests/validation/windows-timeline-receipts-211.md`) targets exactly this case:
it cites this run, puts all seven live receipt tests under a 90 s whole-body
supervisor with stage traces, and closes and drains each tab before best-effort
fixture removal. That removes the hazard described above. #248 itself states that
the Windows blocked frame and root cause remain unproven, so this is a
mitigation with diagnostics, not a proven fix.

Recurrence check (2026-11-20, `gh api`): of the CI runs created after #248 merged,
the 8 that failed or were re-run contain no failed `repo-rest` job, and the 25
`repo-rest` executions recorded by `ci/ci_metrics.py` for the 20-merge window
(merges #240 to #258) since then all passed. The squash receipt test has not
stalled again in that period. Five days without a repeat is evidence, not proof:
the original stall was intermittent, and the attempt-2 re-run passed too.

## How the other Release reds in the window compare

`ci/ci_metrics.py` over the last 20 merges (#240 to #258) lists five Release runs
red on attempt 1: 4189435a (#240), d24ac13d (#224), b9227f05 (#242), 014add85
(#244) and 7dc0bdf9 (#255). In each, the only failed Release job is
`CI passed for this commit`: none of them is a packaging failure. They are red
because CI for that exact commit failed, which is the gate working as intended;
the failures to fix are the CI ones.
