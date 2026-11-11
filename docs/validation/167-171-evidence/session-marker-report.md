Implementation candidate: `913bf4a5b46b0e7ce6117539177cd058c807263f`
Parent: `2743133d690874512716c2037b6ab0f9c11e6a5a`; requested base: `cea7dfb5ddcc290352958b63566cfd3e515a0d9f`.
Branch: `t3/drafts-session-marker-flake`; fork remote: `https://github.com/JKamsker/gitcraft`.
Author and committer: Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>.

Only `src/drafts/tests.rs` changes. Production session locking, all fork/undo code, and the crash/clean/empty-marker assertions remain intact. The initial session is created without `end()` in an exact, single-test child. The parent waits for successful process exit before checking `second.unclean`, normal closure, `!third.unclean`, and empty marker directory. Child assertion/spawn failures propagate; there is no retry, sleep, skip, explicit unlock, or signal.

Evidence of mechanism (historical failing event was not instrumented, so the originating subprocess cannot be identified):
- The original exact isolated test passed on the unchanged parent: `baseline-exact.log`.
- `owned-inheritance.rs` imports unchanged production `src/drafts/session.rs`. It creates/locks a session, forks exactly one owned child, handshakes over a private Unix socket, drops the parent session, then starts the next session while the child retains the original descriptor. `unclean=false` reproduces the original failure condition. After releasing and waiting for that exact returned child PID, another start has `unclean=true`.
- In the same controlled inherited-descriptor window, the revised approach creates its initial session in a separate process, waits for that process, and correctly observes `unclean=true` while the unrelated fork child is still alive.
- The synthetic ownership/record-only check passed before real owned-child execution. Child reads have a five-second socket timeout, and children are waited for before any expected assertion failure. No host process enumeration, selectors, or signals are used.
- `original-inherited.log`: expected exit 101 at the original `second.unclean` condition, after child reaping/fixture cleanup. `isolated-marker.log`: exit 0. `diagnostic-results.json` records both.
- A directory/name collision was not supported by inspection: the drafts helper includes test label + process PID, only this test uses label `sessions`, and session names include PID + nanosecond timestamp.

Reproduction commands:
```
rustc +1.95.0 --edition=2024 /tmp/gitcraft-drafts-a4981112/owned-inheritance.rs -o /tmp/gitcraft-drafts-a4981112/owned-inheritance
/tmp/gitcraft-drafts-a4981112/owned-inheritance --record-only
/tmp/gitcraft-drafts-a4981112/owned-inheritance --assert-original  # expected 101, cleanup/reaping already complete
/tmp/gitcraft-drafts-a4981112/owned-inheritance                   # expected 0
```

Target isolation: APFS clone of parent target into `/tmp/gitcraft-drafts-a4981112/target`, then Cargo rebuilt gitcraft/gitcraft-os for this worktree and rebuilt gitcraft again after the test change. Rust 1.95.0, CARGO_BUILD_JOBS=3 throughout. Parent target/worktree were never written.

Focused check:
```
CARGO_TARGET_DIR=/tmp/gitcraft-drafts-a4981112/target CARGO_BUILD_JOBS=3 cargo +1.95.0 test --locked -p gitcraft --lib drafts::tests:: -- --test-threads=4
```
Result: 20 passed, zero failures (`drafts.log`).

Broader checks are recorded by immutable candidate in `checks.json` and executed by `validate.py`. A preliminary full run omitted the parent's canonical TMPDIR setting and reproduced an unrelated WSL fixture path mismatch (/var versus /private/var). `wsl-environment.json` and its two logs demonstrate default-temp failure and canonical-temp success on the same unchanged binary; preserve this environment correction in final combined gates. The corrected full run uses `TMPDIR=/private/tmp`, GNU sed in PATH, and a private copy of the parent's login-shell wrapper. The existing process-timeout test is excluded from the full suite in accordance with the parent's procedure; this agent does not run that separate host-counting test.

No CI, PR, ignored live-forge tests, or additional agents were started. No independent review is claimed. Parent must cherry-pick this candidate, combine the unrelated coder's fork/undo fix, then run all combined local gates and an independent adversarial review on the final immutable SHA before opening the consolidated PR.

Final unit status: VALIDATED locally, pushed with remote SHA verified equal to `913bf4a5b46b0e7ce6117539177cd058c807263f`; clean worktree. No independent review is claimed.

Final gate results (`checks.json`):
- Full suite exit 0: 3,324 library tests + seven integration tests + two OS tests = 3,333 passing tests; ten existing ignored tests and the existing single timeout exclusion. Library run 178.03s, complete cargo command 187.82s. Zero doctests were present.
- All-targets Clippy exit 0: `cargo +1.95.0 clippy --locked --all-targets -- -D warnings -A dead_code` (30.64s). The dead-code allowance matches the parent's existing macOS baseline; no new lint allowance was added to source.
- `cargo +1.95.0 fmt --check`: exit 0. `git diff --check`: exit 0.

Exact corrected full-suite invocation (the inherited PATH is preserved after the prefix):
```
TMPDIR=/private/tmp SHELL=/tmp/gitcraft-drafts-a4981112/test-shell CARGO_TARGET_DIR=/tmp/gitcraft-drafts-a4981112/target CARGO_BUILD_JOBS=3 PATH="/opt/homebrew/opt/gnu-sed/libexec/gnubin:$PATH" cargo +1.95.0 test --locked -- --test-threads=4 --skip parallel_bisect::tests::a_test_past_its_limit_is_stopped_and_skipped
```

`candidate.patch` contains the single cherry-pickable commit. `source-session.rs` and `source-tests.rs` are extracted from that immutable commit; the parent and requested base had identical drafts session/test sources before this unit. `test-binary.json` binds the exact library test executable; `bin/` preserves all suite executables and `manifest.json` records evidence hashes. The initial failed default-temp full run is preserved as `suite-default-temp.log` / `checks-default-temp.json`; the corrected run is `suite.log` / `checks.json`.
