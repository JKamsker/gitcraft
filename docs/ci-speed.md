# CI speed measurement (2026-11-15)

Baseline: [PR #248 run 43730650718](https://github.com/JKamsker/gitcraft/actions/runs/43730650718)
(first attempt failed; successful core retry is attempt 2)
(head `8b8a76b4e49e913f039133a06efba95eeb25701f`, merge checkout
`c7fba584cef707444506738316342ab38063c871`). Comparison:
[dev run 43728729496](https://github.com/JKamsker/gitcraft/actions/runs/43728729496)
(commit `bc8ed6a45264c3ff06160cd31e3b3e692847d2c9`).

GitHub job-step timestamps (seconds; excludes queue time):

| Windows build step | PR baseline | dev baseline | candidate 45ba60ed |
| --- | ---: | ---: | ---: |
| Run actions/checkout@v4 | 8 | 7 | 8 |
| Run actions/setup-python@v5 | 1 | 0 | 0 |
| Run dtolnay/rust-toolchain@stable | 26 | 21 | 24 |
| Run Swatinem/rust-cache@v2 | 25 | 22 | 2 |
| Verify compiler diagnostics and build inventory | 2 | 3 | 3 |
| Verify the test watchdog's Job Object containment on native Windows | 17 | 16 | 15 |
| Verify owned-child thread stack capture on native Windows | 1 | 1 | 1 |
| Clippy and complexity gate | 398 | 527 | parallel (542s job) |
| Build test binaries | 1110 | 1388 | 902 |
| Verify complete and disjoint Windows test shards | 4 | 4 | 4 |
| Windows doctests (not included in compiled test executables) | 19 | 16 | 17 |
| Upload Windows test binaries | 18 | 21 | 10 |

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

## First attempt, retry, and eventual green

The first attempt's Windows build completed at 16:03:14; core started at
16:03:17, a **3s handoff**, and failed at 16:13:38 after **621s (10m21s)**.
The failing test was
`standalone::model::tests::existing_result_is_kept_and_computed_conflicts_require_a_choice`:
`process timed out and was stopped`. `CI result` failed at 16:13:56, making
first-attempt time-to-result **39m19s** from run creation at 15:34:37.

Core's retry started at 16:15:05 (87s after the failed shard ended), with job
setup at 16:15:06; the retry's whole-job duration is 546s. The run became green
at 16:24:39, **50m02s after creation**. The apparent 11m54s gap obtained by
comparing the build with the retry includes the failed test execution and retry
delay. It is **not runner queueing**. The timeout remains unresolved reliability
evidence; the passing retry and this speed change do not prove it harmless.
The successful core timings below describe retry execution, not first-attempt
success. Build and non-core job timings are retained from the successful jobs.

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

## Candidate measurements

[Run 43741657210](https://github.com/JKamsker/gitcraft/actions/runs/43741657210)
verified `45ba60edf771549190492182c3462a8ed47408aa` (merge checkout recorded
in its logs). Windows cache was cold. Every Windows shard passed on attempt 1.

| Metric | Baseline | Candidate |
| --- | ---: | ---: |
| Windows build job | 27m15s | 16m35s |
| Longest successful Windows shard | 9m06s core retry | 7m49s core, first attempt |
| Build + longest shard service time | 36m21s | 24m24s |
| Run creation to Windows aggregate success | baseline first core failed | 25m38s |
| First aggregate result | failed, 39m19s | failed, 25m50s |
| Time-to-eventual-green including retries | 50m02s | 43m33s |

The candidate's first attempt failed the unrelated, unchanged Ubuntu-latest test
`a11y::tests::repo_view::the_graph_is_reached_with_tab_and_read_aloud`.
The failure log reports `could not run process: Text file busy (os error 26)`.
Ubuntu latest reran from 17:58:25 to 18:15:36 and passed with no source changes;
`CI result` passed at 18:15:50. This retry is unresolved reliability evidence,
not proof the failure is harmless. Full wall-clock is therefore **43m33s**,
not a claimed sub-30-minute green run. The measured complete-verification path
before that retry is 25m50s; the final documentation head must pass fresh CI.
No checks are deferred to nightly to hide this test failure.

Cargo timing artifact: normal library 127.40s (62.6s frontend, 64.8s codegen),
library test target 211.14s, test-build total 902s including dependencies and other
targets. Target times can overlap and must not be summed as a critical path.
The native Windows Clippy job took 542s, independently of the test build.


## Runtime symbolization probe

An isolated Linux fixture was compiled and intentionally failed under stable Rust
1.100.0 with `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`,
and `RUST_BACKTRACE=1`, using `nice -n 19 cargo +stable test --offline`.
The native backtrace retained the named test and source location:

```text
2: ci_speed_backtrace::tests::intentional_failure
         at ./src/lib.rs:5:9
3: ci_speed_backtrace::tests::intentional_failure::{closure#0}
         at ./src/lib.rs:4:29
```

The fixture's expected failure is diagnostic evidence, not an application-test
failure. Windows reasoning: the unchanged build manifest/upload logic explicitly
includes each reported executable's adjacent PDB; native Windows CI runs its
owned-child stack capture regression test before building the binaries. Line
tables retain names and source lines, while full local-variable/type information
is intentionally omitted. The actual candidate artifact contains six PDBs, one beside each of the six
reported test executables, including both gitcraft harnesses. Native stack-capture
checks and all Windows shards passed. Linux runtime evidence plus those PDBs is
not a claim that a Windows failing-test backtrace was executed.

The measurement/symbolization documentation follow-up changes no application,
Cargo, CI tooling or workflow files relative to reviewed commit `45ba60ed`.
Application gates bind to green run 43741657210 for that commit, preserving the
failed first-attempt evidence above. Independent read-only review passed on
45ba60ed; its P2 baseline-attribution finding is corrected here. A documentation
update does not constitute new execution of those application gates.

## Follow-up: Windows-target Clippy on Linux

Decision: **retain native Windows Clippy**. This evaluation is a separate
follow-up to #249, not a replacement gate or a reason to delay its merge.

The exact locked default-feature graph was attempted on Linux with stable Rust
1.100.0, two workers, lowest OS priority and an isolated target/TMPDIR:

```sh
rustup target add --toolchain stable x86_64-pc-windows-msvc
nice -n 19 cargo +stable clippy --locked --all-targets \
  --target x86_64-pc-windows-msvc -- -D warnings
```

It failed in `ring 0.17.14`'s build script: `cc-rs: failed to find tool
"lib.exe": No such file or directory`. The cold attempt's log spans about
129 seconds before that error; this is a failed local experiment, not a passing
CI benchmark. Clippy avoids the final target Rust link, but Cargo still executes
build scripts that compile and archive target C code. Installing Rust's target
standard library does not install those C/SDK tools.

The locked graph includes these relevant native build requirements:

| Crate | Build requirement / implication |
| --- | --- |
| `ring` | Target C/assembly compilation and archive tools; observed MSVC `lib.exe` failure. A compatible MSVC/clang-cl environment and target headers/libraries would require separate provisioning. |
| `onig_sys` | Oniguruma's C sources compiled through `cc`; requires the target C compiler and runtime headers. |
| `tree-sitter` and C/C++/Go/JS/Python/Rust/TypeScript grammars | Runtime/parser/scanner C sources compiled through `cc`; the Rust-only target installation is insufficient. |
| `winresource` / root build script | Windows resources need a resource compiler/SDK; the root treats missing resource compilation as a warning, so success would not prove native resource verification. |
| `windows` / `windows-sys` | Rust bindings are not the observed compiler blocker. |

`libgit2-sys`, `curl-sys`, `libz-sys` and `openssl-sys` are not in this locked
project graph; its native Git feature uses `gix`. They must not be cited as the
cause of this failure. A GNU target would additionally require MinGW tools
(absent on this machine), and selects `target_env=gnu`, not the native job's
`target_env=msvc`. Its minimal probe passed the expected cfg selection, but a
full GNU dependency-graph build was not claimed or used as MSVC parity evidence.

### Deliberate lint probes

An isolated Git repository on scratch branch
`scratch/windows-clippy-equivalence` held intentional violations; none were added
to gitcraft or merged. Each command used `--all-targets -- -D warnings`:

| Probe | Windows MSVC target on Linux | Linux native target |
| --- | --- | --- |
| Private unused `#[cfg(windows)]` function | Reports `windows_dead_marker` | Excluded |
| Private unused `#[cfg(unix)]` function | Excluded | Reports `linux_dead_marker` |
| Unused variable in a Windows-only `#[cfg(test)]` branch | Reports `windows_test_marker` | Excluded |
| Unused variable in a Unix-only test branch | Excluded | Reports `linux_test_marker` |
| Windows-gated `Permissions::set_readonly(false)` | Reports `permissions_set_readonly_false` | Excluded |

The permissions probe took 0.244s for cross-Msvc (expected failure) and 0.133s
for Linux (success), with warm fixture artifacts. These tiny-fixture times do
not measure gitcraft's dependency graph. The earlier cfg probe initially waited
for the full attempt's Cargo lock, so its first 78.719s is not compiler time.
The GNU and Linux cfg probes then took 0.100s and 0.092s respectively.

These probes confirm that target-code cfg/dead-code/test analysis can catch the
classes of Windows-only warnings seen in #244 and #242. They do **not** establish
identical full-job coverage. A second scratch probe placed Windows-only and
Linux-only dead functions in `build.rs`: cross-Msvc reported
`linux_host_marker` and excluded `windows_host_marker`, because build scripts
compile for the Linux **host**. Native Windows compilation selects the opposite
host cfg. Proc macros/build dependencies also execute on the host. The existing
job additionally checks all four tracked Cargo packages (including independent
workspaces and the recovery-recording cfg); a single root Cargo invocation would
not replace that inventory.

### Runner-time decision

The measured #249 native Windows Clippy job took **542s (9m02s)**, while existing
Linux lint took **144s (2m24s)**, in parallel. A viable full cross-target job did
not complete, so its added Linux CI time and net savings cannot honestly be
measured from these failed/local probes. The adopted change saves **zero runner
minutes**: native Windows remains required. Deleting it would nominally remove
9m02s of Windows runner use, but adds unknown Linux compilation/SDK provisioning
cost and loses native-host compilation coverage. Even after solving toolchain
provisioning, host equivalence would need its own explicit verification contract.
That is a fragile trade for a job already outside the measured critical path.

This documentation-only follow-up changes no workflow, Cargo or application
files. Application verification remains bound to reviewed commit 45ba60ed and
run 43741657210, with the failed attempt retained. No successful full cross-target
Clippy build or new application test run is claimed.
