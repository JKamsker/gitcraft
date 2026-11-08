# Static follow-up: backpressure and recovery proof transport

WIP follow-up to 6a289fd8; independent review and all process runtime gates remain held.

The POSIX streaming supervisor previously forwarded through blocking stdout/stderr writes. A controller that stopped draining could prevent the supervisor from servicing EOF, deadline and authenticated stop. Both backends now retain bounded forwarding queues and use nonblocking writes, preserving partial-write offsets and polling control while output is blocked. Final drain is finite after group cleanup. Recording tests inject blocked and partial writers; they never execute a user command or send signals. The Perl backend also rejects noncanonical base64 input before decoding.

Recovery's final proof now travels on intended command stdin instead of argv. It contains restricted record names and hex-encoded exact bytes. CLEAR compares bounded file reads against this proof before mutation, refuses added/missing/replaced records, and never reconstructs a signal target. This removes the OS argument-size limitation from the preceding checkpoint. The protocol retains its explicit 64 MiB input bound; overflow preserves the workspace before mutation.

Exact local gates with CARGO_BUILD_JOBS=2:

- `cargo +1.95.0 test --locked --lib recording -- --nocapture`: 8 application and 3 OS tests passed.
- `python3 -B src/remote/supervisor/recording_tests.py`: 4 tests passed.
- `perl -c src/remote/supervisor/owner.pl`: syntax OK.
- `perl src/remote/supervisor/owner.pl recording`: 6 recording gates passed.
- `cargo +1.95.0 check --locked --all-targets`: passed (baseline macOS dead-code warnings).

These are recording/syntax/compile gates, not owned-child, transport runtime or platform validation. Endpoint artifact retirement, Windows production ordering recording tests and review disposition remain open.
