# Issue 192 platform cfg correction

Application source: `438053a9bf2fb0fb850ee059091445b278be9470`. All 17 manifest commands passed on this immutable
application commit. The evidence commit adds documentation only, retaining application
identity without claiming new test execution for that documentation commit.

Windows CI run 43710703482 (head c46eec5d) failed Clippy in Windows test build job
132801838682: marker was compiled under cfg(test), but its only caller is the Linux-only
walker. The downstream Windows summary reported that build failure; tests never started,
and no test-report artifact was available. Exact job logs are preserved here. No old-head
run was rerun. The fix restricts marker to cfg(target_os = "linux"), matching its caller.

All new source items and imports were audited for platform-only/test-only callers.
The caller audit and source-addition inventory are retained. No additional mismatch was
found. Only x86_64-unknown-linux-gnu is installed locally; no Windows/macOS compilation
or suite is claimed. No blanket dead-code allowance or behavior change was introduced.
The existing classification tests exercise marker handling through real Linux traversal.

Formatting, strict all-target application Clippy 1.100 (-D warnings), size/complexity,
committed-HEAD architecture and dev-baseline growth checks passed. Published architecture
declarations remain unchanged. Scoped application test executions (overlap intentional):

- tests-worktree_resources: 10 passed
- tests-constellation: 17 passed
- tests-repo-insights: 84 passed
- tests-git-worktree_cleanup: 42 passed
- tests-repo-branch_cleanup: 29 passed
- tests-terminal: 51 passed
- tests-a11y-tests-repo_view: 2 passed
- tests-palette: 61 passed
- tests-repo-layout: 4 passed
- tests-repo-tests: 33 passed
- tests-i18n-: 5 passed

Rust commands ran under nice -n 19 cargo +stable with the issue-owned target and TMPDIR;
fixture Git 2.49 from that target supplies the existing offline Git guard. No host install,
full suite or local Windows suite was used. Prior tooling tests and independent review
remain bound to the unchanged source documented in 192-review-fixes; this fix is one cfg
attribute. Final new-head CI is still required before merge.
