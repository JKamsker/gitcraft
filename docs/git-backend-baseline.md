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
python3 ci/bench-processes.py --label git-cli --backend cli --sanitize-path \
  --modes local-tabs remote-tabs --counts 1 10 50 --repeats 3 --repo .
python3 ci/bench-processes.py --label git-native --backend native --sanitize-path \
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
Its PATH contains a literal relative `~/.dotnet/tools` entry; the native guard correctly
falls back rather than allowing Gitoxide to search a repository-relative executable path.
`--sanitize-path` drops relative PATH entries in both comparison fixture processes, matching
the application's child-process safety policy. Both resolve the same `/usr/bin/git` here.

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
