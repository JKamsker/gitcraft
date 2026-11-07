# Native durable endpoint checkpoint

WIP: local bisect integration, independent static review, owned-child fixtures and all native runtime gates remain pending. No process runtime tests were executed for this checkpoint.

The packaged early-start helper creates a private directory, random capability, loopback control endpoint and exclusive durable record before submitting user code to the shared Service. The helper survives ordinary app/controller EOF long enough to cancel and reap its own anchored group or Windows Job, then publishes an atomic terminal receipt. Recovery accepts an exact authenticated endpoint and complete receipt; a dead/missing endpoint without receipt remains unknown. There is no PID recovery operation. The transport's direct-child ownership is restricted at call sites to this packaged helper; ordinary commands still use group/Job ownership.

Windows private directories use a protected current-user DACL. Native argument/environment words retain platform encoding. The helper has finite command and authenticated control deadlines. Deliberate daemon/session escapes, native supervisor crashes, external enclosing Windows Jobs and changed temporary-directory roots are not unconditional recovery guarantees; unknown cleanup must preserve worktree evidence.

Recording-only checks (Rust 1.95.0, CARGO_BUILD_JOBS=2):

- `cargo +1.95.0 test --locked --lib remote::native_owner::tests::recording -- --nocapture`: 3 passed, zero child commands spawned by these tests.
- `cargo +1.95.0 check --locked -p gitcraft-os --target x86_64-pc-windows-gnu`: passed after correcting the Windows feature and descriptor bindings. This is compilation, not Windows runtime validation.
- Earlier `cargo +1.95.0 check --locked --all-targets`: passed with baseline and temporarily unintegrated API warnings.

The test-only helper entrypoint is intentionally excluded from the recording filter. No inherited cancellation, bisect, SSH or WSL fixture was run.
