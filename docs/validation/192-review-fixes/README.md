# Issue 192 review-fix validation

Application source: `582f05163fc7f9c8bc322f541d4bd0df54e5ad5d`. All 17 commands in validation.json passed against this
immutable candidate. The evidence commit changes documentation alone; application identity
is checked before pushing, binding these gates without claiming new test execution.

The independent read-only review of `5726d95810d6438819ac51054c9f4b3826fed95d` is complete
and replaces a re-review, as instructed by the maintainer. All five findings were fixed:
remote capability rejection before any presence I/O; presence collected by cancellable
workers; conservative Cargo layouts with protected unknown files and exact ambiguity paths;
asynchronous evidence comparison before preview presentation and periodic revalidation;
versioned disk observations cleared on invalidation; absent observations distinguished from
Git-reported missing paths. Two additional CodeRabbit findings were verified and fixed:
non-Linux unsupported/unavailable measurements survive stale collection, and unavailable
reason keys are translated with the existing raw-error fallback.

## Results

Formatting, strict all-target application Clippy 1.100 with `-D warnings`, physical file size,
complexity policy, architecture check on committed HEAD, and baseline growth against dev
and the previous pushed head all passed. Published architecture declarations are unchanged.
132 CI tooling tests passed. Application module executions (overlap intentional):

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

Dedicated regressions exercise the actual preview invalidation path for ignored-content,
HEAD, status and presence changes, late-result rejection, remote rejection without scheduling,
protected target/.env and target/release/libuser.a, synthetic non-Linux capabilities, reload
invalidation of totals and unmeasured-vs-missing rows. Linux-specific fixture imports are
conditionally compiled; no local Windows suite ran. No full application suite ran.

Every Rust command used `nice -n 19 cargo +stable`, the task-specific target and TMPDIR.
The locally built Git 2.49 under that target supplies the repository's existing offline
--no-lazy-fetch guard; host Git 2.43 was not modified. The pre-push wrapper plan is retained;
its gates were executed individually with application-only all-target Clippy to respect
explicit resource restrictions. No GitHub CI was dispatched, rerun or waited on during
implementation. Final-head CI remains required before merge.

## Preserved constraints

Only Git-registered worktrees are inventoried. No destructive action was added. Shared
read-only traversal and recovery purge safeguards remain intact. Unknown/incomplete external
observations block cleanup previews; retirement reclaims zero bytes now, potential later
purge is separate, and overlapping operations are never summed. SSH/WSL and non-Linux safe
sizing remain unsupported. Preview observations expire after 30 seconds and are revalidated
asynchronously before presentation; future execution must always inspect again.

Prior evidence and the recovery-lock reproducer remain in 192-resources-premerge and
192-resources-final. The owned task target directory is retained until final CI and merge,
then removed. No architecture budget increase or published declaration changed.
