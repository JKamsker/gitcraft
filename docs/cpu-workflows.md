# CPU workflows and coverage

This extends [the first CPU pass](cpu-performance.md). The expanded suite contains **118
offline workloads**, using production functions, real Git commands and egui frames. The
baseline production code is `35a26a5`; it already includes the native syntax parser and
earlier memory improvements. Measurements use Rust 1.95.0, Apple Git 2.54.0 and an Apple
M4 Max on macOS 27.2.

Captured IDs precede the email-privacy rewrite required to publish the branch. The public
commit with the same baseline tree as `35a26a5` is `08da373`; the frozen source revision
`81163fe` has the same tree as public `a936407`. Measurement inputs and binary hashes are
retained as captured; only commit identities changed during publication.

## Results

These are medians of three serial interleaved runs per saved release binary, with seven
samples per run and batches calibrated to at least 60 ms. No compiler or profiler ran
during these comparison runs. The 106 common workloads have before/after comparisons;
12 document cases were added afterwards and have final measurements only. Full common
workload summaries are in [cpu-workflow-results.json](cpu-workflow-results.json).

| Workload | CPU before, ms | CPU after, ms | Elapsed before, ms | Elapsed after, ms |
| --- | ---: | ---: | ---: | ---: |
| JSON bulk deletion, 5,000 keys | 86.426 | 1.659 | 104.667 | 16.168 |
| GitLab owners, 1,000 files | 26.068 | 11.900 | 26.075 | 11.903 |
| Clean status, 400 files | 1.843 | 0.511 | 46.979 | 15.382 |
| Operation state | 1.455 | 0.241 | 35.706 | 6.195 |
| Identity | 1.074 | 0.487 | 27.392 | 13.006 |
| Refs | 0.834 | 0.587 | 22.173 | 15.748 |
| Concurrent metadata reload | 3.723 | 2.389 | 40.053 | 15.920 |
| Literal find, 50,000 lines | 2.302 | 1.242 | 2.302 | 1.242 |
| Unicode find, 50,000 lines | 1.694 | 0.706 | 1.695 | 0.706 |
| Image difference, 1% edited at 1080p | 7.345 | 2.537 | 7.348 | 2.538 |
| Image difference, fully edited at 1080p | 6.902 | 6.852 | 6.906 | 6.851 |
| Unified diff scrolling frame | 0.600 | 0.574 | 0.600 | 0.574 |
| Split diff scrolling frame | 0.611 | 0.574 | 0.611 | 0.574 |

JSON deletion uses about **98% less CPU**, GitLab lookup **54% less**, literal find **46%
less**, and mostly unchanged image comparison **65% less**. Fully changed images remain
effectively unchanged. The metadata reload's elapsed time falls about **60%**. UI frame
gains are small. Text/colour fingerprints match in every repeated comparison. These are
results for these inputs and this machine, not a guarantee for every repository.

Short Git wrappers varied more between runs. Aggregate medians for reflog, authorship and
stage/unstage increased by about 0.1 ms of parent CPU and 2 ms elapsed; their paths receive
no speedup claim. The full JSON includes these cases. For example, the last paired reflog
run was 13.309 → 13.212 ms, versus aggregate medians of 13.767 → 16.076 ms.
A separate three-pair exact-workload recheck (nine 80 ms samples) measured reflog at
12.214 → 12.561 ms, authorship at 11.576 → 11.621 ms and stage/unstage at
18.425 → 17.586 ms. It did not reproduce the larger aggregate slowdowns.

## What is exercised

| Workflow | Inputs and timed work |
| --- | --- |
| History and graph | Frozen repository history, first/next 500 commits, layout, warm/scroll frames, commit detail and reflog |
| Repository refresh | 400 tracked files; clean, 200 dirty/untracked files, merge and rebase conflicts, linked worktree, identity, refs, overview, worktrees, stashes, remotes and concurrent metadata reload |
| Files and search | Tree/blob/history, 2,000-line blame, history message/operators/code search; literal/regex/Unicode find across 50,000 lines |
| Changes | Three frozen real commit diffs, index/worktree diffs, unified/split warm and scrolling frames, file/history/blame frames |
| Highlighting | Actual Rust and Cargo.lock files; representative Python, TSX, Vue, C#, C++, Go, Java and PowerShell source |
| Ownership | GitHub/GitLab/Gitea CODEOWNERS parsing, lookup and lint; 400 rules, 20 sections, 1,000 paths |
| Rich changes | JSON bulk deletions, YAML, TOML, Cargo.lock, CSV, Markdown prose, OpenAPI and DVC through `rich::load` |
| Documents | Compressed/uncompressed PDF, Word, Excel, PowerPoint, OpenDocument, 1,000-member ZIP, 200-cell notebooks with/without stable IDs, XML, LFS pointers and PNG through `diff::load` |
| Images | Decode/resize preparation, 1080p identical/1%-edited/fully-edited comparison, dimension mismatch |
| Local mutations | Stage/unstage, commit/soft-reset, stash/restore, checkout, merge/hard-reset and rebase/restore cycles in disposable repositories |
| Local transport | Clone/remove, up-to-date fetch and push against a disposable bare repository |
| MCP | Status, branches, log, commit, diff, blame and conflicts through argument handling, production Git reads and JSON serialization |
| Other CPU work | Weakened-check scanning, authorship metadata, Myers sparse/rewrite changes, line display limits, palette, deferred tabs and settings serialization |
| Terminal | ANSI/Unicode build-output parsing and actual row painting/font layout/tessellation without starting a shell |

Fixtures are prepared outside timing. Mutation cases include the restoration operation in
their measured cycle; they are not measurements of every corresponding UI action/reducer.
All writes target disposable repositories. The working checkout's source/history comes
from immutable object IDs in `target/cpu-bench/inputs.json`; the refs case also observes
the checkout's current branch list. Hooks and signing are disabled in mutation fixtures.
The headless frames are 1400×800, with avatars and network disabled. Content cases assert
successful production dispatch and extracted changes instead of silently timing a fallback.

All 118 expected cases have successful stack captures, recorded in
[cpu-profile-coverage.json](cpu-profile-coverage.json). One local bare-clone setup failed
before sampling and succeeded on retry; the session manifest retains both attempts.
The fixture now disables automatic maintenance and prepares its bare source through Git's
transport instead of the local hardlink/copy optimization.

## Optimizations

- JSON removal placement previously searched the remaining old keys for every deleted key.
  A reverse scan records the next surviving key once, preserving output order and duplicate
  key behavior. This removes the quadratic deletion scan.
- GitLab ownership stores rule positions by section when parsing and searches each section
  backwards for its last matching rule. Repeated section headers, exclusions, defaults and
  case-insensitive owner deduplication retain their existing semantics.
- Local operation-state checks ask Git once for the private worktree directory. Missing
  operation markers no longer start separate Git processes. Present markers still undergo
  Git's validation. Linked-worktree state is kept separate from the main worktree.
- Identity reads fetch name/email together, twice for effective/local config. Last values,
  empty and valueless entries, multiline values and local-presence flags are preserved.
  Attached HEAD reuses the object ID already returned by `for-each-ref`; detached/unborn
  repositories retain the explicit HEAD lookup.
- Identical image pixels skip unpremultiplication and channel-distance calculation. Different
  pixels, including alpha changes, use the existing calculation and tolerance.
- Short text lines avoid scanning thousands of possible character positions to enforce a
  display limit they cannot reach. Unicode lines retain the character-based limit.
- Diff row layout appends borrowed text directly and uses stack cut points and UTF-8 slices
  for changed spans, removing temporary substring allocations. Frame-level gains are small.

## Measuring and profiling

```sh
python3 ci/bench-cpu.py --label before
# Change production code, keeping the frozen inputs.
python3 ci/bench-cpu.py --label after --compare target/cpu-bench/before.json
# Alternate saved binaries for repeat runs; never compile or profile during timing.
python3 ci/bench-cpu.py --label before-repeat --binary target/cpu-bench/before-bench
python3 ci/bench-cpu.py --label after-repeat --binary target/cpu-bench/after-bench
# Collect one separate stack profile per measured case on macOS.
python3 ci/bench-cpu.py --label after --binary target/cpu-bench/after-bench \
  --profile-all --profile-from target/cpu-bench/after.json
```

The profiler waits for the exact case's timed-work marker before attaching. Its coverage
manifest records every expected case, sample path and both exit statuses, and fails if a
case is missing. Profiles use longer batches and are excluded from timing comparisons.
`sample` records process stacks, including waits; it does not profile Git child-process
stacks. Process CPU clocks exclude child CPU. Elapsed measurements include Git, I/O and
waiting. Filesystem caches are warm; batch p95 describes batch averages, not interaction
tail latency. Timing works on other platforms; automatic stack sampling currently needs
macOS's `sample` command.

## Source-line coverage: iterative work

Benchmark completeness, a captured profile for every benchmark, and source-line coverage
are separate measures. This suite does **not** cover every application workflow or 100%
of source lines. LLVM source instrumentation is measured separately from release timing.

The first workspace test run covered 115,004/159,448 instrumented lines (**72.13%**).
Adding the offline workflows covered 116,790/159,448 (**73.25%**). A coverage-driven
iteration added tests for whole-config removal, bounded large rewrites, section approval
summaries and corrupt/truncated image failures. A fresh final run covered
116,833/159,467 lines (**73.26%**). Config diff increased from **96.4% to 99.6%**, and
CODEOWNERS from **95.4% to 98.3%**. See [source-coverage-results.json](source-coverage-results.json)
for scoped totals and the largest remaining files. This is the first measured iteration
toward full coverage, not a claim of 100% source coverage.

The final workspace run has **3,003 passing library tests, five failures and eight ignored**;
the other workspace helper/integration targets pass. All five failures reproduce with the
saved pre-optimization binary:

- `agent::checkpoint_tests::the_script_for_other_machines_saves_what_gitcraft_saves`
- `agent::checkpoint_tests::the_script_reads_escapes_and_blank_lines_as_gitcraft_does`
- `git::external_tool::tests::real::in_wsl_the_windows_tool_gets_wsl_localhost_paths`
- `git::search_query::tests::contents_regex_and_merges_on_a_real_repository`
- `repo::drag_drop::tests::a_fast_forward_refuses_a_branch_being_rebased_elsewhere`

They involve shell/regex/path portability on this macOS host; they are retained as failures,
not skipped or counted as covered behavior. Formatting, all-target release Clippy with the
existing macOS dead-code allowance, the fallback all-target build check, and the native
release build pass. The native app was rebuilt and relaunched; its idle sample waits for
events, with observed idle CPU near 0%.

```sh
rustup component add llvm-tools-preview --toolchain 1.95.0
cargo +1.95.0 install cargo-llvm-cov --locked
python3 ci/coverage.py --workflows --html
# After inspecting missing lines, accumulate additional targeted test executions:
python3 ci/coverage.py --append --filter git::status::tests
```

The runner retains a JSON report even when tests fail, records command exit statuses,
and returns failure. It explicitly selects only the workload test for ignored-test runs;
it does not enable ignored live-forge or credential-store tests. Raw line totals include
inline test bodies. Excluding files named `tests` alone would not give a strictly
production-only denominator. Conditional platform/feature code needs separate builds.

Next iterations should prioritize:

1. Watcher-driven refresh, streaming history, dense merge histories, rename-heavy and binary
   changes, partial staging/stash, conflict resolution/continue/abort and rewrite actions.
2. Collision matrices, stacks/restack, timeline, drift/fence, rehearsal, parallel bisect,
   doctor and actual transport of new objects rather than up-to-date remotes.
3. Home/workspaces/inbox, dialogs, PR/CI views, accessibility, persisted startup, native
   event dispatch/GPU, PTY I/O and terminal lifecycle.
4. Forge/CI/issue/release workflows, SSH/WSL, authentication, cancellation/progress,
   credential stores, external agents/tools, update/download and desktop integration.

External integration coverage needs its real fixtures. Several SSH/WSL tests return early
without `GITCRAFT_TEST_SSH`/`GITCRAFT_TEST_WSL`; Mergiraf also needs its opt-in variable.
LFS, npm, SSH-signing and notebook-filter tests can skip when tools are absent. Privilege
and permission tests depend on the host. Test credential storage uses an in-memory
replacement, so it does not establish coverage of the production OS keyring.
