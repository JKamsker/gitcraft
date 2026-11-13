# CI and releases

Green CI on a pull request should predict a green `dev` build and release
([issue #211](https://github.com/JKamsker/gitcraft/issues/211)). This page covers
what runs, what is required, and how to handle a failure.

## One verification contract

`.github/workflows/ci.yml` is the only place verification is defined. It runs for
pull requests (the merge commit), merge-queue candidates (`merge_group`), pushes to
`dev`/`master`/`main` and `v*` tags, nightly on `dev`, and by hand.

| Stage | Jobs |
| --- | --- |
| Fast preflight | `rust-file-size / check` (tooling tests, file-size, complexity-policy, and architecture fitness gates), `lint` (rustfmt, strict Clippy for every package) |
| Correctness matrix | `test (ubuntu-22.04)`, `test (ubuntu-latest)`, the Windows build plus ten complete, disjoint shards (`windows-tests.yml`) aggregated as `test (windows-latest)` |
| Release smoke | `package-version` computes a dev prerelease version the way Release does. `package-smoke` builds the Windows zip and installer, the tarball, AppImage, `.deb` and `.rpm` through `package.yml`, the workflow Release builds with. |
| Live services | `forges`: real Forgejo and Gitea, run for changes to `src/forge/`, `ci/forge-server.sh`, `Cargo.lock` or `ci.yml` (`ci/changed_paths.py`), nightly, and by hand |
| Required check | `CI result`: fails unless every job succeeded. Only `forges` may be skipped. |

`CI result` and `test (windows-latest)` always run (`if: always()`). A required
check that is *skipped* counts as passing, so these jobs fail on a failed, skipped
or cancelled dependency instead of being skipped. A cancelled job is reported as
cancelled, not as a test failure, and never as green. `ci/require_results.py`
implements this and `ci/test_ci_gates.py` tests it.

A newer push to a pull request cancels the older commit's run. Merge-queue
candidates, pushes, nightlies and manual runs each get their own concurrency group
(the commit), so they never cancel each other.

### Release

`.github/workflows/release.yml` repeats no tests. Its `CI passed for this commit`
job (`ci/await_ci.py`) waits for a CI run of `ci.yml` with the **same head SHA**
from a `push` or `merge_group` event. That run's `CI result` must succeed.
Such a run used that commit's own workflow files. Runs for other SHAs, pull-request
merge commits and other workflow revisions never count. The packages come from
`package.yml`, the same workflow the PR smoke runs. Versions and the RPM form
(`0.3.4-dev.6` → `0.3.4~dev.6`) come from `ci/release_version.py`.
`ci/test_release_version.py` keeps the historical `invalid version "0.3.4-dev.6"`
failure as a fixture.

Workflow-level tests pin all of this: `ci/test_workflow_gates.py` (every job
depends on the size gate, no softened or retargeted required commands, every CI
job required by `CI result`) and `ci/test_workflow_policy.py` (triggers,
concurrency evaluated for concrete events, conditional jobs, release gating).

## Merge queue and branch protection (repository settings)

These are repository settings, not files. An admin enables them for `dev` under
Settings › Rules (or Branches):

1. Require a pull request before merging, and block force pushes and deletions.
2. Require status checks to pass, with **`CI result`** as the required check.
   You can keep `test (windows-latest)` and `test (ubuntu-latest)` required too,
   since their names are unchanged. Turn on "require branches to be up to date" if
   the merge queue is not used.
3. Require a merge queue: merge method of your choice, build concurrency 1–5,
   "only merge non-failing pull requests", and a status check timeout of about
   180 minutes. `ci.yml` runs on `merge_group`, so the required check reports on
   the exact candidate.
4. Do not allow bypassing these rules, except for a documented emergency
   override. After such a push, the `push` CI run on `dev` is the follow-up
   validation, and Release still waits for it.

Verify the result with an account that can read rulesets (`gh api
repos/JKamsker/gitcraft/rulesets`). Then open two pull requests that are each green
alone but incompatible together, and check that the queue rejects the second one.

## Before you push

```sh
python3 ci/check.py                     # CI's gates + compile every test target
python3 ci/check.py --test repo::stack  # + focused tests for what you changed
python3 ci/check.py --full              # + the whole suite
```

These are the exact commands CI runs (`ci/test_check.py` pins them). Cargo runs at
the lowest priority. On Windows, run the CI shards natively with
`python ci/windows-test-build.py`, `python ci/test-shards.py --check-only` and
`python ci/test-shards.py <shard>`.

## When a job fails

Read the failure before you edit anything:

1. Open the exact failed job: `gh run view <run-id> -R JKamsker/gitcraft --log-failed`,
   or the `CI result` job's summary, which lists every failure with its class:
   `deterministic-regression`, `timeout-or-hang`, `infrastructure-or-environment`,
   `possible-flaky` or `unknown`.
2. Download the reports: `gh run download <run-id> -R JKamsker/gitcraft -p 'test-report-*'`.
   Each `report.json`/`junit.xml` gives per-test outcomes, panic messages, SHA,
   attempt, runner image, and Git and rustc versions. On a watchdog timeout it
   also holds the running and unfinished tests and a snapshot of the owned process
   tree (command lines of stuck `git`/PowerShell children). `logs/` has each
   process's capped, redacted output.
3. Reproduce it locally with the same filter (`cargo test --locked <name> -- --exact`,
   or the shard on Windows). Make a scoped fix, rerun `python3 ci/check.py`, then
   push once.
4. A re-run that passes is evidence of intermittency, not of harmlessness. Say
   which kind of evidence you have: a source fix, or a no-change retry. The summary
   keeps the failed attempt and marks the test `possible-flaky`.

### Windows hang diagnostics

`ci/test-shards.py` runs every libtest process under `ci/libtest_watchdog.py`.
Each process runs inside a kill-on-close Job Object, so only processes it started
are listed or stopped. A heartbeat names the finished/total count and libtest's
long-running tests. The shard has an 18-minute budget, before the 20-minute step
timeout. A process with no test starting or finishing for 10 minutes is stopped.
In both cases the shard fails with a report instead of a bare `KeyboardInterrupt`.
These limits are bounds for diagnosis. Never raise them to make a hang pass.
The `repo::stack`, `repo::test_lane`, `repo::reflog` and ACL hangs are still
unexplained. They need native Windows repro runs with these diagnostics.
