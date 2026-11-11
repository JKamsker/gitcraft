# Issue 198: standalone and integration-test slice

Assigned base: `a4d841713af3d31178a3b9212fd3c5adcb4172dc`.
Scope: `src/standalone/**` and `tests/**`. This slice does not complete issue 198.

## Ownership and behavior

- Save transaction orchestration stays in `save.rs`. `save/reservation.rs` owns
  reservation identities, recovery directory lifetime, release and teardown.
  Mutable reservation/recovery fields remain private; callers use retain/restore
  operations. No persisted data formats or public APIs change.
- Save regression fixtures are grouped by reservation, transaction, Unix policy,
  macOS policy and Windows policy. `include!` composes these complete test
  functions into the original `save::tests` module so exact shard filters remain
  valid. No application type or implementation is split across these test files.
- UI harness and localization tests retain their original module names. The UI
  state, worker lifetimes, transitions, rendering and shutdown stay together.
- Windows metadata protocol, fixture recording and diagnostic localization tests
  retain their original module names. The constant native helper script moves
  byte-for-byte into `windows_metadata/policy.ps1`, loaded with `include_str!`.
- MCP subprocess/repository/window fixtures move to `tests/support/mcp_stdio.rs`.
  Integration test functions stay at their original root; child handles remain
  private behind close-stdin and exit-observation methods.

## Windows investigation

Reviewed existing evidence:
`docs/validation/pr184-windows-standalone-20261109/README.md` and
`failed-core.log.gz`, plus the existing Windows rehearsal investigation.
The archived core trace reports six native helper timeouts and two UI assertions
whose rendered error is the same subprocess timeout. Native metadata snapshots
launch PowerShell and compile a native C# policy class using Add-Type. Parallel
fixtures therefore compete for expensive runtime/compiler resources. This is a
supported contention hypothesis, not proof of the historical root cause.

A test-only resource guard now serializes native metadata helper execution within
one libtest process. It is acquired before launching the child, released on all
ordinary/error returns, and protects occupancy rather than fixture state. Native
policy assertions remain real. Production behavior and the existing 20-second
child deadline are unchanged; there are no retries or timeout increases. Existing
CI serial/isolated groups remain untouched. Cross-process or machine-wide resource
contention is outside this guard. UI fixture settlement still has its original
bound. Native Windows validation must establish the effect before anyone removes
existing CI isolation or claims a root-cause fix.

## Local evidence

- `python3 tests/validation/standalone_slice.py`: PASS; 51 existing functions
  (including fixture helpers) retain equivalent normalized bodies after the
  explicit private API adaptations. This is a lexical extraction audit, not Rust
  type checking. Original UI/metadata module names and MCP test placement remain.
- The native PowerShell script bytes are unchanged; SHA256:
  `b2cb41ed96448b7090e41c0df121741cce57946992cd207c4f9fadc6f74b6ac5`.
- Owned Rust inventory: 29 files, none over 500 lines, maximum 471. Before/after
  oversized files: save 1250→289, UI 639→363, Windows metadata 633→249,
  MCP stdio 531→359. Files still above the preferred 300 lines keep coherent
  responsibilities (UI lifecycle/rendering, MCP contract assertions, existing
  directory/model/registration owners).
- `rustfmt --edition 2024 --check` on every owned Rust file: PASS.
- `git diff --check`: PASS.
- A standalone `rustc --edition 2024 --test` harness imports
  `windows_metadata/fixture_resources.rs`. Its pure recording test passes: eight
  synchronized threads record 16 operations, maximum one active resource owner,
  and successful resource reacquisition after error returns. No native command,
  process control or risky process test runs.

The integration owner must run full compilation/tests and independent adversarial
review on the combined immutable candidate, plus native Windows ACL/UI fixtures
under established process containment. No Cargo build, native process test, CI,
remote push or PR was performed for this slice. Root checker/CI enforcement,
repository-wide decomposition, other architecture acceptance work and final
combined validation remain outside this assignment.
