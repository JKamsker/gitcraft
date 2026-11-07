# CPU performance: real browsing and diff workloads

The follow-up [workflow suite and coverage inventory](cpu-workflows.md) expands this pass
to 118 offline workloads, per-case stack profiles, and iterative LLVM source-line coverage.

Measured on 2026-11-07: Apple M4 Max, macOS 27.2, Rust 1.95.0, Apple Git 2.54.0.
The baseline is `81163fe`, including the earlier memory optimizations.
Following the email-privacy rewrite needed to publish the branch, its identical public tree
is at `a936407`. The earlier image-cache and title-bar commits `3d2df7d` and `c7ac524`
correspond to public `4872136` and `5d8a0d2`. IDs below describe the captured local inputs.

The benchmark exercises production code with this repository's actual history and source
files. Commit IDs, branch tips, and file revisions are frozen in
`target/cpu-bench/inputs.json`, so editing the worktree cannot change the timed inputs.

## Results

Each number is the median of three runs, interleaved between saved baseline and optimized
release binaries. Each run has nine samples calibrated to at least 50 ms per batch, following
warmup. No compilation or profiler ran during these six comparison runs.

CPU time comes from the process CPU clock and **excludes Git child processes**. Elapsed time
includes Git subprocesses, I/O and waiting. Numbers are milliseconds per operation/frame.

| Workload | CPU before | CPU after | Elapsed before | Elapsed after |
| --- | ---: | ---: | ---: | ---: |
| Read first 500 history commits | 1.808 | 1.615 | 14.203 | 13.130 |
| Lay out captured history (1,439 commits) | 0.712 | 0.724 | 0.712 | 0.724 |
| Highlight `src/app/mod.rs` | 46.146 | 14.772 | 46.145 | 14.772 |
| Highlight `src/repo/load.rs` | 43.775 | 13.895 | 43.780 | 13.895 |
| Highlight `Cargo.lock` | 17.368 | 17.583 | 17.372 | 17.583 |
| Open image decoder commit diff | 19.274 | 7.030 | 61.204 | 43.320 |
| Open image cache commit diff | 18.775 | 6.755 | 59.707 | 42.535 |
| Open title bar commit diff | 15.397 | 4.695 | 55.862 | 40.027 |
| Warm graph frame, including tessellation | 0.127 | 0.127 | 0.127 | 0.127 |
| Scroll graph frame, including tessellation | 0.170 | 0.169 | 0.170 | 0.169 |

The Rust sources use about **68% less CPU**. Preparing these three diffs uses **64–70% less
GitCraft CPU**, with **28–29% less elapsed time**. The graph remains effectively unchanged.
The lockfile is slightly slower (1.2%); the parsing cache brings it down from approximately
24.5 ms with the native backend alone to 17.6 ms. These results describe these inputs and
machine, not every language or repository.

The diffs are the actual changes from `81163fe` (`src/repo/diff/image_compare/decode.rs`),
`3d2df7d` (`src/image_cache.rs`), and `c7ac524` (`src/app/title_bar.rs`). Loading includes
Git configuration, rename lookup, patch parsing, context, syntax colours, moved-code detection
and authorship metadata. It does not cache the completed diff between benchmark iterations.

## Profiles and changes

- A macOS `sample` profile of Rust highlighting put roughly 88% of the sampled worker stacks
  in syntax parsing, mostly the fancy-regex VM. The default build now uses Oniguruma through
  syntect and two-face. `--no-default-features` retains the pure-Rust backend. Oniguruma builds
  its bundled C sources; it does not require a separately installed shared library by default.
- Generated files repeat lines such as registry URLs and package headers. A per-file cache
  reuses parser operations only when the complete parser state matches and stays unchanged.
  It holds at most four parser contexts and 128 short lines, then is dropped. Highlight scope
  operations still run against the current theme state. A reusable line buffer and joining
  adjacent spans with identical colours reduce allocations as well.
- Diff profiles also showed repeated `fork` calls. Rust 1.95 falls back to fork/exec when
  a bare executable name is combined with a changed PATH. Git is now resolved through the
  same sanitized login-shell PATH before starting it. The post-change profile confirms
  `posix_spawn` in place of `fork`. Executable lookup is refreshed on each call, handles spaces
  and raw path bytes, and skips non-executable files. Windows command construction is unchanged.
- The native app was near 0% CPU when idle, with its main thread waiting for events. The graph
  frame benchmark was already around 0.17 ms. Neither justified speculative changes.

Rendered text/colour fingerprints match the baseline for all three source files across all
six comparison runs. Additional tests compare cached output with syntect's uncached output
for Rust, TOML, JavaScript and embedded HTML, in dark and light themes, including multiline
comments and strings. Git command, credentials, diff, history, shell-path and graph tests pass
(262 targeted tests); the pure-Rust backend's highlight and command tests also pass.
Formatting, all-target Clippy, the fallback all-target build check, and the native release
build pass. Clippy allows the existing macOS platform-specific dead-code warnings.

## Reproduce and profile

Run serially from the project root:

```sh
python3 ci/bench-cpu.py --label before
# Make the production change without deleting target/cpu-bench/inputs.json.
python3 ci/bench-cpu.py --label after --compare target/cpu-bench/before.json

# Reuse saved binaries to avoid including compilation in comparisons.
python3 ci/bench-cpu.py --label before-repeat --binary target/cpu-bench/before-bench
python3 ci/bench-cpu.py --label after-repeat --binary target/cpu-bench/after-bench
python3 ci/bench-cpu.py --label fallback --no-default-features
```

To build an older revision, create a detached worktree, copy
`src/repo/cpu_benchmarks.rs` into it, add its `#[cfg(test)] mod cpu_benchmarks;` declaration
to `src/repo/mod.rs`, and copy the Unix `rustix` dev-dependency with its `time` feature from
`Cargo.toml`. Then use `--build-dir /path/to/that/worktree`; the runner keeps the original
frozen inputs and saves the resulting binary. Production changes need not be copied.

On macOS, extend one workload so there is time to sample it. In a second terminal:

```sh
python3 ci/bench-cpu.py --label profile --binary target/cpu-bench/after-bench \
  --filter highlight/src/app/mod.rs --sample-ms 1000

sample "$(cat target/cpu-bench/profile.pid)" 3 -file target/cpu-bench/profile-sample.txt
```

Use `--filter diff/open` or `--filter ui/graph-scroll-frame` for the other paths. Profiled
runs should not be used as timing comparisons. JSON stores raw batch means, median, batch
p95, CPU samples and text/colour fingerprints; metadata stores binary and input hashes.
The p95 is of batch averages, not individual-interaction tail latency.

Session evidence is under the ignored `target/cpu-bench/`: `baseline-{1,2,3}.json`,
`final-{1,2,3}.json`, `comparison.json`, saved binaries, build logs and stack samples.
The graph benchmark runs the real view and tessellator at 1400×800 with fixed history,
avatars disabled and no network or watchers. It excludes GPU work, OS event dispatch,
startup grammar loading, and network operations. Repeated runs use warm filesystem caches.
