# Git read backend measurements (#163)

The repeatable benchmark is the production `RepoTab::reload` path, its bounded job/read
workers, graph updates and 2 ms message polling loop. It is not a synthetic `gix` versus
`git` microbenchmark. `ci/bench-processes.py` accepts `--backend cli` to force the reference
CLI path, `--backend native` to enable eligible local reads, `--repo` to clone real history
for every tab, and `--repeats` to run fresh workload processes. Default fixtures retain the
existing one-commit reload workload. Remote mode defaults to the test-only loopback shell transport;
it exercises pool/framing contention but does **not** measure SSH, authentication or network
latency. See [the SSH transport measurements](ssh-transport.md) for real OpenSSH evidence.

Reproduce with Rust 1.95 or newer, Git and Unix `ps`:

```sh
python3 ci/bench-processes.py --label git-cli --backend cli \
  --modes local-tabs remote-tabs --counts 1 10 50 --repeats 3 --repo .
python3 ci/bench-processes.py --label git-native --backend native \
  --binary target/process-bench/git-cli-tests \
  --modes local-tabs remote-tabs --counts 1 10 50 --repeats 3 --repo .
```

Use the exact same saved binary for both modes; compare a fresh workload process for every
case. No compiler or other test suite should run during measurement. Fixture init/clone and
deferred tab construction occur before measured reload timing, CPU deltas, managed process
deltas and operation samples. OS resource peaks are sampled over the whole workload process,
including fixture setup. File-system caches are warm; “fresh process” does not mean a cold
OS disk cache. Fixture source trees are disposable independent clones with shared read-only
object storage; the source repository is never modified.

This host inherits `GIT_PAGER`, which is allowed because these captured reads do not page.
Repository/config/authentication overrides are never stripped from the benchmark environment.
Its PATH contains a literal relative `~/.dotnet/tools` entry **after** the selected Git.
The guard accepts it because it cannot affect executable lookup; relative entries before
Git cause explicit CLI fallback. Both modes resolve the same `/usr/bin/git` here. The optional
`--sanitize-path` fixture switch is unused in the recorded comparison.

For a disposable local OpenSSH server, `--ssh-host` replaces the loopback host and
`--ssh-log` counts its `Accepted publickey` records during each reload. `--rounds 2`
records the initial reload and a warm reload in the same process, after secondary panel
jobs settle. The same sampled process/thread/RSS peaks accompany both rounds. The harness
closes its workers, shells and managed masters after timing. See the SSH document for
server creation and the baseline configuration that explicitly disables multiplexing.

`latency_p50_ms` and `latency_p95_ms` describe completion times across the tabs within a run,
measured from the common submission start. The one-tab percentiles are the same observation.
Reload wall time includes polling; the heartbeat gap measures message-loop responsiveness,
not GPU/native-window rendering. Peak OS threads and parent RSS are sampled about every
10 ms plus sampler overhead, so very short peaks can be missed. Parent CPU excludes Git
children, while descendant-process peaks include their helpers. Managed subprocess deltas
are actual local process-service spawns; SSH's persistent shells and the remote Git children
are outside that service counter. Per-operation `cli:*` counters record Git command
invocations, including remote commands through persistent shells, and may include attempts
that fail before process creation. Typed read counts and CLI subcommand counts are different
layers and must not be added together. Gitoxide may invoke Git once per process to discover
installation configuration; this startup helper is outside the managed-process and CLI
timer counters. OS process sampling can observe it.

Timing ends after initial loads are applied. Subsequent panel reads may still be queued;
this is why the operation counters capture an observed startup workload rather than a fixed
number of hand-picked calls. All latency samples are bounded to the latest 1024 calls per
operation. Operation rankings below use the CLI run to select actual repeated work, rather
than imposing a native-operation target percentage.

The original pre-native debug harness was saved before backend source edits and measured at
source revision `d7b3757a` on 2026-11-07 (Apple M4 Max, macOS 27.2, Rust 1.95.0, Apple Git
2.54.0). Its one-commit baseline is in [git-backend-before.json](git-backend-before.json).
It establishes the unchanged reload/process architecture before this work; same-binary
CLI/native comparisons isolate the backend change from build and instrumentation effects.

WSL is unavailable on this macOS host. Loopback remote comparisons establish that the new
native layer keeps remote operations on the remote route; they cannot establish WSL or
real-network performance. Native Windows/Linux CI covers platform behavior, and the Linux
SSH harness exercises a real temporary OpenSSH server. Native status, diff, log, mutation,
clone/fetch/push and unsupported contexts deliberately retain the CLI; these results do not
claim complete native Git coverage or a universal speedup.

## Validation

The exact pre-change library executable passed 3,041 tests with five failures specific to
this macOS host. The final source executable, with native dispatch active in the actual
inherited environment, passed 3,063 tests and had exactly those same five failures:

- The two `agent::checkpoint_tests` remote-shell JSON parser cases.
- `git::external_tool::tests::real::in_wsl_the_windows_tool_gets_wsl_localhost_paths`.
- `git::search_query::tests::contents_regex_and_merges_on_a_real_repository`.
- `repo::drag_drop::tests::a_fast_forward_refuses_a_branch_being_rebased_elsewhere`
  (the existing editor uses GNU `sed -i` syntax).

Ten opt-in tests were ignored, including real SSH; the process-global timed-out bisect test
was excluded from both broad runs and passed separately. The real OpenSSH harness runs
separately against generated keys, pinned host-key verification and a temporary server.
The checkpoint CLI integration test, six MCP stdio integration tests, two OS helper tests and
four async-network prototype tests pass. All-target Clippy passes with `-D warnings` and
only the pre-existing macOS `dead_code` warnings allowed. Formatting, diff checks and
`cargo check --locked --all-targets --no-default-features` pass. Linux/Windows hosted CI
provides the native platform results; no local macOS baseline failures were hidden by
changing their behavior or adding ignores.

## Same-binary local comparison

Measured at fixture revision `f3c344e2` on the machine above, using three fresh processes
per case, the same saved debug executable and the unchanged inherited environment. Each
cell is **CLI → native**. Latency cells are the median of each run's across-tab percentile;
process counts are median managed process-service spawns during measured reloads.

| Concurrent repositories | Completion p50 (ms) | Completion p95 (ms) | Managed subprocesses |
| ---: | ---: | ---: | ---: |
| 1 | 89.6 → 74.1 | 89.6 → 74.1 | 23 → 19 |
| 10 | 205.1 → 176.0 | 283.4 → 241.1 | 206 → 157 |
| 50 | 676.2 → 595.9 | 1154.7 → 1007.1 | 814 → 603 |

At 50 repositories the observed startup workload used 153 native reads and 595 remaining
CLI invocations, compared with 806 CLI invocations in reference mode. Managed subprocess
churn fell about 26% and completion p95 about 13%. Initial reload completion can leave
secondary panel work queued, so these are observed startup counts rather than a promise of
an exact fixed per-repository saving. The three migrated reload reads—refs, remotes and
private Git directory—each remain covered by native/CLI parity tests.

The parent process absorbs work previously done in Git children. Its median measured CPU
was 1730.4 → 1855.5 ms for 50 repositories; that excludes child CPU and therefore cannot
establish a total CPU regression or improvement. Peak parent RSS was 101.9 → 105.3 MiB,
peak OS threads 72 → 72, and sampled descendants 8 → 9. The one-off Gitoxide installation
config helper can briefly add a child beyond the managed service's eight active slots.
The application keeps the existing bounded workers and process scheduler.

Loopback remote runs recorded **zero native reads** in both modes, retaining the remote
command route. They are a compatibility/control workload; results vary with scheduling
and are not evidence of a native remote speedup. Full local and loopback p50/p95, reload
wall time, CPU, memory, process/thread peaks, native/fallback share and compact operation
summaries are in [git-backend-results.json](git-backend-results.json). Raw per-call samples,
logs, binary hashes and saved executables remain under ignored `target/process-bench/`.

## Observed reference operation ranking

These are median invocation counts during the 50-local-repository startup case, with
latencies pooled across its three runs. Each CLI timer includes queueing and execution;
concurrent command times overlap and cannot be summed into reload wall time.

| Git operation | Invocations | p50 (ms) | p95 (ms) | Median summed time (ms) |
| --- | ---: | ---: | ---: | ---: |
| config | 213 | 9.8 | 15.2 | 2197.4 |
| rev-parse | 151 | 11.3 | 46.0 | 2577.5 |
| for-each-ref | 124 | 11.2 | 17.2 | 1585.1 |
| remote | 102 | 10.5 | 20.5 | 1243.6 |
| log | 52 | 18.6 | 23.6 | 988.1 |
| status | 50 | 73.3 | 79.2 | 3607.5 |
| stash | 50 | 9.3 | 14.4 | 480.7 |
| worktree | 50 | 8.6 | 13.2 | 467.7 |

Config is most frequent; status has the largest summed latency among these operations.
They remain explicit CLI operations pending parity for their broader configuration,
filter, index and worktree semantics. This initial migration removes repeated validated
ref/remote/directory subprocesses and leaves these measured priorities visible for follow-up.
