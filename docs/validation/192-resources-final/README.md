# Issue 192 final combined validation

Application source: `174d661bc5d54b61cf71e3a174a074f5f2051c17`. This includes the plain merge of dev #242 at
`7ed992ef18f65e74780f0c963b3254d7bea0e20d` and the scoped recovery-lock fix.
Every command in validation.json passed against this immutable application candidate.
The evidence commit changes documentation alone; application source identity is verified
before pushing, binding these gates without claiming new test execution for documentation.

## Results

Strict all-target application Clippy: 1.100, `-D warnings`; formatting; physical file size;
complexity policy; architecture `check --revision HEAD`; baseline growth against dev
and previous pushed head all passed, with no budget/baseline increase or new declaration.
132 CI tooling tests also passed. Application module executions (overlap intentional):

- tests-worktree_resources: 8 passed
- tests-constellation: 14 passed
- tests-repo-insights: 81 passed
- tests-git-worktree_cleanup: 42 passed
- tests-repo-branch_cleanup: 29 passed
- tests-terminal: 51 passed
- tests-a11y-tests-repo_view: 2 passed
- tests-palette: 61 passed
- tests-repo-layout: 4 passed
- tests-repo-tests: 33 passed
- tests-i18n-: 5 passed

No full application suite or Windows suite ran. All Rust commands used `nice -n 19`,
`cargo +stable`, the issue-specific target and TMPDIR. Git 2.49 was built in that directory
for the repository's existing offline --no-lazy-fetch guard; host Git is 2.43. The test Git
has no HTTP support; these are local fixtures. No host installation was modified.
The pre-push wrapper plan is retained; its gates were executed individually with Clippy
restricted to the application package/all targets per the explicit resource constraint.

## Recovery-lock investigation

`lock-original-failure.log.gz`: the merged wider cleanup group failed its restore/purge
exclusion test with WouldBlock. `lock-exact.log.gz`: isolated rerun passed; this was not
accepted as evidence that the failure was harmless. `lock-reproducer.log.gz`: the synthetic
owned-descriptor reproducer at `fefb9e8e33f5b786bff4aec844707fa552a00595` fails: closing the
original descriptor leaves an advisory flock held by a duplicate open-file description.
It models a transient inherited descriptor without starting or signalling any process.
The operation guard now explicitly unlocks before close. The final cleanup group includes
this reproducer and the original exclusion case; all 42 pass under normal parallel execution.
No timeout was raised, no test skipped and no Windows flake dismissed.

## V1 limits for review

Read-only observations and previews only; no new execute/archive/purge/process-control or
hibernate action. Linux descriptor/openat2 sizing only; other platforms and SSH/WSL show
unsupported/unavailable capabilities. Cargo target is the small build candidate allow-list.
Other ignored/unclassified data stays protected. Publication refs are local observations
with unknown remote freshness. External process/port absence remains unknown and therefore
blocks every cleanup plan. Retirement to recovery reclaims 0 bytes now; potential later
purge size is separate and overlapping operations are never added.

Known reloads invalidate observations, changed collection state is partial, new observations
have fingerprints, and preview detail expires after 30 seconds. There is no continuous
watcher across every registered worktree; any future execution must inspect again.
Independent parent review and final-head CI are outstanding at this review handoff.
The owned /home/jonas/work/gitcraft-targets/192 directory is available for review/fix
continuation and must be removed upon merge/completion or abandonment.
