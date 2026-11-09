# Issue 178: initial ownership phase

This is the baseline + representative terminal-owner phase of the umbrella,
not completion of its typed operations, persistence, settings or async phases.

## Immutable source and provenance

- Base: `760c79bb69ffc3af5401612bd9c48dcd254eca54` (`dev`, checked again before assembly).
- Tested application: `0c2b68e1a00107a92a14e57c8905b390145d5d70`.
- Metrics correction: `0922187c78f6ea86b6825ce379cb4b828f08ff63` (only tooling/docs).
- Consolidated implementation: `a532ed25578a7e3db5bb0af04bd16ecbf0332839`.
- Preserved work branch: `t3/improve-code-quality` at `0922187c`.
- Final source branch: `refactor/178-terminal-ownership`, built from the base with
  one baseline/tooling commit, one owning-state commit and one local Hide commit.
  Every unit was pushed and its remote SHA verified before the next unit.

The consolidated implementation's **entire tree** equals `0922187c`'s tree.
Its application files equal the tested `0c2b68e1`:

```sh
git diff --exit-code 0922187c a532ed25
git diff --exit-code 0c2b68e1 a532ed25 -- src crates Cargo.toml Cargo.lock build.rs
git diff --exit-code 496f61b2 a532ed25 -- src/repo/mod.rs src/repo/actions.rs src/repo/apply.rs
```

All three commands passed. The final evidence commit adds/updates documentation
and evidence artifacts only; bind the prior application gates using the same
application-file comparison instead of claiming a new full test execution.
The [manifest](architecture-178-evidence.json) records application subtree IDs
and SHA-256 hashes of all retained logs, including saved before/after binaries'
hashes in the timing results. Source checksums bind source equivalence, not binary
identity across rebuilding/reordering commits.

## Local checks

Host: macOS 27.2 arm64; Rust/Cargo 1.95.0; Git 2.56.0; default features unless
stated. No GitHub CI was started or awaited during implementation.

| Check | Actual result |
| --- | --- |
| `cargo +1.95.0 fmt --check` | Pass |
| `python3 -m unittest discover -s ci -p test_architecture.py -v` | 5 pass: literal/comment scanning, fields, Git config independence, raw export-attribute source reads |
| Regenerate the architecture baseline at the pinned base | Reproduced; corrected history policy then regenerated in the checked-in baseline |
| `cargo +1.95.0 test --locked --no-run` | Pass |
| `cargo +1.95.0 test --locked --lib repo::terminals::tests -- --nocapture` | 6 pass, including owned-session drops without PTYs and repeated Hide intent |
| `cargo +1.95.0 test --locked -- --nocapture` | **Not green:** 3,338 pass, 3 fail, 10 ignored; all 3 failures reproduce individually with the saved pre-change binary |
| `cargo +1.95.0 test --locked -p gitcraft --test checkpoint_cli --test mcp_stdio` | 1 checkpoint + 6 MCP integration tests pass |
| `cargo +1.95.0 test --locked -p gitcraft-os` | 2 pass |
| `cargo +1.95.0 test --locked --doc` | Pass (0 doc tests) |
| `cargo +1.95.0 clippy --locked --all-targets -- -D warnings` | **Fails** on 6 pre-existing macOS dead-code warnings in `desktop_entry.rs` / `diagnostics.rs`; these files equal the base |
| `cargo +1.95.0 clippy --locked --all-targets -- -D warnings -A dead_code` | Pass, using the historical macOS allowance; strict lint is not claimed green |
| `cargo +1.95.0 check --locked --all-targets --no-default-features` | Pass |
| Same fallback check with `--target x86_64-pc-windows-gnu` | **Blocked:** `ring` needs absent `x86_64-w64-mingw32-gcc`; no Windows build/test pass claimed |

The three unchanged host failures are:

1. `agent::checkpoint_tests::the_script_for_other_machines_saves_what_gitcraft_saves`:
   expected checkpoint message, got no record (line 174).
2. `agent::checkpoint_tests::the_script_reads_escapes_and_blank_lines_as_gitcraft_does`:
   expected escaped path/message, got empty record (line 277).
3. `git::external_tool::tests::real::in_wsl_the_windows_tool_gets_wsl_localhost_paths`:
   `/private/var` versus `/var` path in the macOS fixture's simulated WSL argument
   (line 957).

The saved base binary reproduces the same assertions and exit 101 for all three;
their implementation/test files are unchanged. They were not skipped in the full
run. No new line-coverage percentage or real SSH/WSL/forge coverage is inferred.
The full suite includes existing owned-PTY agent/custom-command/handoff tests;
the new state-transition tests ran first without launching any processes.

## Metrics and change amplification

See [results](../architecture-results.json) and the documented scanner limitations.
`RepoTab` fields: 188 → 185; constructor: 208 → 205 lines; root dispatcher:
422 → 419 lines; root action variants: 165 → 162. Global largest-function size
and function-length p95 are unchanged. Total Rust LOC increases by 275, including
143 lines of owner tests: smaller files alone are not the goal.

Four terminal state fields become one private owner. Three launch sites share one
append/select/error/visibility transition. Handoff cannot bypass paste safety or
write selection directly. Popout/lifecycle callers use explicit methods. Adding
the real Hide intent (`496f61b2` → `a532ed25`) changes **none** of the central
state/constructor/action/dispatcher files. Rust privacy prevents reintroducing
direct collection/selection writes from collaborators.

## Timing/resource smoke checks

These are **test/debug-profile smoke measurements**, dependencies optimized at
level 2. They are not release latency baselines or statistical performance claims.
Both binaries were saved before timing; frozen input revision is the base, with
identical input hash. Nine batches target 50 ms each, before then after for each
case. No compilation or profiling ran during timing. Logs include process peak
RSS from `/usr/bin/time -l`; that includes setup, not steady-state tab memory.

| Case | Before median ms | After median ms |
| --- | ---: | ---: |
| Create 20 deferred tabs | 0.7450 | 0.7486 |
| Palette items | 0.0877 | 0.0884 |
| Terminal parse build output | 6.6670 | 6.7311 |
| Terminal paint frame | 0.2318 | 0.2347 |
| Repository reload metadata | 11.7736 | 11.6147 |
| History layout (1,500 commits) | 2.1616 | 2.1896 |
| Open frozen diff | 53.6710 | 50.1257 |
| Stage/unstage cycle, initial pair | 13.0919 | 18.8735 |
| Warm graph frame | 6.2146 | 6.2245 |

The initial staging outlier was investigated with four alternating after/before
runs, each 15 batches targeting 100 ms: **14.1871 / 14.1398 / 12.9507 / 13.1466 ms**.
The slowdown did not reproduce; retain the original outlier as well as repeats.
Most paired differences are around 1%; do not interpret the diff improvement or
small positive changes as a refactor speedup/regression. Peak RSS across pairs
remains roughly 53–65 MB. No production Git/graph/diff execution algorithm changed.
Active concurrent-tab I/O, production startup RSS, SSH/WSL and release timings
remain unmeasured here; historical workflow reports are separate evidence.

## Independent review and evidence

Round 1 reviewed exact `0c2b68e1` against the base. No terminal behavior or ownership
findings; one P2: history metrics inherited local rename/diff settings. The fix
pins the history policy, reads raw blobs rather than export-modified archives,
and adds real Git fixtures. The baseline was regenerated; quoted hotspot commit
frequencies remain 108 / 71 / 66. The final consolidated candidate must receive
a fresh independent review before PR creation; its exact SHA and verdict belong
in the PR validation record. No blanket claim of umbrella completion is warranted.

The [compressed raw evidence](architecture-178-logs.tar.gz) contains build/test/lint
logs, failures, benchmark inputs/results and local benchmark orchestration scripts.
Inspect without extracting the whole archive:

```sh
tar -tzf docs/validation/architecture-178-logs.tar.gz
tar -xOzf docs/validation/architecture-178-logs.tar.gz suite.log
tar -xOzf docs/validation/architecture-178-logs.tar.gz bench/results.json
```

The benchmark scripts record absolute paths to the saved session binaries and
inputs; adjust those paths to reproduce on another checkout. The benchmark test
itself and its fixture generation remain the existing application code.
