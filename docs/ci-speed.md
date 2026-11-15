# CI speed measurement (2026-11-15)

Baseline: [PR #248 run 43730650718, attempt 2](https://github.com/JKamsker/gitcraft/actions/runs/43730650718/attempts/2)
(head `8b8a76b4e49e913f039133a06efba95eeb25701f`, merge checkout
`c7fba584cef707444506738316342ab38063c871`). Comparison:
[dev run 43728729496](https://github.com/JKamsker/gitcraft/actions/runs/43728729496)
(commit `bc8ed6a45264c3ff06160cd31e3b3e692847d2c9`).

GitHub job-step timestamps (seconds; excludes queue time):

| Windows build step | PR baseline | dev baseline |
| --- | ---: | ---: |
| Run actions/checkout@v4 | 8 | 7 |
| Run actions/setup-python@v5 | 1 | 0 |
| Run dtolnay/rust-toolchain@stable | 26 | 21 |
| Run Swatinem/rust-cache@v2 | 25 | 22 |
| Verify compiler diagnostics and build inventory | 2 | 3 |
| Verify the test watchdog's Job Object containment on native Windows | 17 | 16 |
| Verify owned-child thread stack capture on native Windows | 1 | 1 |
| Clippy and complexity gate | 398 | 527 |
| Build test binaries | 1110 | 1388 |
| Verify complete and disjoint Windows test shards | 4 | 4 |
| Windows doctests (not included in compiled test executables) | 19 | 16 |
| Upload Windows test binaries | 18 | 21 |

Both restored the exact same 684,605,283-byte (653 MiB) cache, then reported
`Cache up-to-date` without saving. Both nevertheless compiled dependencies again.
PR Clippy's root-package invocation took 350s; the other three packages added
about 48s. During test compilation the first dependency started at 15:44:01;
`Compiling gitcraft` appeared at 15:54:00 and Cargo finished at 16:02:27.
Thus 599s elapsed before the root crate started, with dependencies continuing
until at least 15:55:44, and 507s elapsed after that root-crate start. These are
overlapping wall intervals, **not** separate measured lib/test compiler times:
the old log has no per-target completion timestamps. The dev counterpart took
778s before the root crate started and 608s from its start to Cargo completion.
The candidate adds Cargo timing artifacts to measure individual targets.

Core's report records 363.328s in the main library harness, 69.593s in serial
standalone tests, and 1.688s / 41.954s / 21.422s in its three isolated tests.
Moving the latter groups shifts 134.657s of test execution to repo-g-m without
changing the number of runners, test threads, selection, or timeouts. The
baseline whole-job tail is core 546s, git-a-m 406s, repo-d-f 381s, repo-g-m 166s.
The estimated rebalanced tail is about 410s, subject to runner variance.

## Decisions

- Remove the serial 398s (dev: 527s) Windows Clippy dependency; retain native
  strict Clippy for every package as an explicit `CI result` prerequisite.
- Use line-table debug information for Windows tests. This retains named/source
  backtraces, PDB upload and owned-child stack dumps. Assertions/overflow checks
  and optimization levels are unchanged. Avoid debug=0, which loses diagnostics.
- Reset the immutable Windows test cache key, separate Clippy from codegen caches,
  and allow only dev pushes to save. Fresh PR runs are deliberately cold until
  dev first populates the new cache. Incremental was already disabled.
- Keep the existing build-once artifact and low compression: upload was only 18s
  (dev 21s). nextest would replace working watchdog integration for little gain.
- Do not add sccache, alternate linkers, dependency optimization or unstable flags
  without evidence that their setup/recompilation improves this profile.
- Linux, package smoke and apt setup are outside the measured critical path.
  Leave both Ubuntu runners, release-equivalent packaging, forge selection,
  merge queue and cancellation semantics intact.
- Splitting the large crate is follow-up work: root-crate-start to completion was
  507–608s, overlapping remaining dependencies. Use the new target timings before
  choosing a boundary; this task changes no application source.

## Nightly split considered

| Job | PR / merge queue | dev push | nightly | release |
| --- | --- | --- | --- | --- |
| fmt, Linux and Windows Clippy | full | full | full | exact-commit CI gate |
| architecture, size, policy, tooling | full | full | full | exact-commit CI gate |
| Ubuntu latest and 22.04 tests | full | full | full | exact-commit CI gate |
| Windows, including serial/isolated fixtures | full | full | full | exact-commit CI gate |
| Linux and Windows package smoke | full | full | full | same packaging + CI gate |
| Forgejo/Gitea | existing conservative forge-path selection | same selection | full | exact-commit CI gate |

No checks are deferred: doing so would move regression detection past merge and
could hold each dev release until a nightly run validates that exact commit.
In particular Ubuntu 22.04 catches oldest-glibc failures, package smoke catches
installer/version errors, and Windows isolated fixtures exercise native ACL and
process behavior. Those residual risks are avoided in this candidate. The
existing `changes` job only selects the forge lane; docs-only changes still run
all mandatory checks. Any future docs-only optimization must conservatively
require the full suite for src/**, Cargo.*, ci/** and .github/**.

Before measurement, expected Windows path is build + about 7m shard tail +
preflight/queue/aggregate overhead: the parallel Clippy and rebalancing alone
remove about 9m from the baseline's serial work. Line tables and a fresh usable
cache target a further reduction. The <=30m goal remains unproven until real CI
finishes; ~20m is a target, not a measured claim.
