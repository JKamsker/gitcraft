# Bisect authenticated recovery checkpoint

WIP, not runtime validated. This removes the last production host-wide signal selector (`parallel_bisect::runner::STOP_SCRIPT`). Each test has a fresh random `.owner` record and a shared cancellation token. Local tests use the packaged native endpoint, including Windows Job supervision; SSH/WSL tests use the authenticated POSIX endpoint. User code is dispatched only after its exclusive record is written. Logs retain the last 8 KiB, including after confirmed cancellation; unknown cleanup is an error, never a stopped verdict.

Workspace creation is private (Unix mode 0700 or protected Windows current-user DACL). Before removing worktrees or refs, cleanup validates all record names, authenticates each recorded owner and requires its atomic terminal receipt, then compares the exact record set and contents again in the clearing command. Legacy `.pid`, invalid/replaced records, wrong capabilities, unavailable owners and missing receipts preserve the workspace. The heartbeat's process number remains coordination/ref namespace metadata only. Repeated normal recovery can read an existing terminal receipt without signalling a retired owner.

The bisect private cache remains outside configurable user worktree roots. The new dev worktree placement helpers must be preserved for subsequent user-facing worktree features. This checkpoint does not modify them.

Pure gates on Rust 1.95.0 with CARGO_BUILD_JOBS=2:

- `cargo +1.95.0 test --locked --lib recording -- --nocapture`: 8 application and 3 OS tests passed. This filter includes only synthetic/recording tests, not the native helper entrypoint.
- `python3 -B src/remote/supervisor/recording_tests.py`: 3 tests passed with injected fake OS operations.
- `perl -c src/remote/supervisor/owner.pl`: syntax OK.
- `perl src/remote/supervisor/owner.pl recording`: 3 recording gates passed.
- `cargo +1.95.0 check --locked --all-targets`: passed during integration; final recording build compiled the subsequent test changes. Baseline macOS dead-code warnings remain.
- Source search for STOP_SCRIPT, stop_script, taskkill, kill_process_group, ps -A/-eo/-ef, pgrep and pkill under src and crates/gitcraft-os/src finds only the read-only Git lock diagnostic listing. Exact fixture PID status probes remain observations, not signal authority.

No owned-child runtime, inherited bisect/process fixture, full-suite, native Windows/Linux, live SSH or WSL gate has run. Independent static review is still required before the deliberately owned canary fixtures. Supervisor death, deliberate session escapes and externally enclosing Windows Jobs retain conservative recovery limitations. Endpoint artifact retirement and final platform/behavior review remain open. Large recovery record sets can exceed the OS argument limit; this fails before mutation and preserves evidence, but needs a scalable proof handoff before claiming end-to-end completion.
