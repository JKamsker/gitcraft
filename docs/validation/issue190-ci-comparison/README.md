# CI comparison scout evidence

These are copied-source model checks and independent source reviews. They are
not final combined application, GUI, network, or provider integration checks.
No GitHub CI or application build was run by this agent for these units.

Implementation snapshots, in order:

- `94803b7182366c84d71e53ff17c10c8cb7dcd11c`: pinned GitHub attempt endpoints and bounded same-SHA history.
- `12fcdad8e281d23e5c1253c43fc148d37a244b5b`: conservative alignment and separate outcome diagnostics.
- `c5f141b7ab412daf660d70ad8afd148241a8f95a`: baseline resolution and comparison assembly; missing-SHA regression.
- `3d3a390d7539468c384ad3633de6dfde77d98e05`: pinned UI controls, ownership/selection guards, matrix-axis and prior-order regressions.
- `77a168c0b72532efdbbe7da45581e5f2e9cc56e8`: stale-reply matrix across run/attempt/baseline changes and unused-import correction.

The source archive binds 15 actual Rust files byte-for-byte to Git blobs at
`3d3a390d7539468c384ad3633de6dfde77d98e05`. Their hashes and every archived
file hash are in `source-binding.json`. The two other Rust files are explicit
harness stubs: `src/lib.rs` and `src/ci/mod.rs`. They disable Git/network access,
replace time parsing with unavailable timestamps, and supply unrelated model
types. The harness allows dead code because it copies more public API than its
selected tests exercise. It enforces Clippy cognitive complexity with `forbid`.
It does not reproduce the application's complete lint configuration.

The selected checks exercise history, alignment, diagnostics, baseline selection,
and workflow parsing. The existing timestamp-parser and Git-backed fixture tests
are compiled but deliberately not executed against these stubs. UI namespace,
snapshot, and lifecycle fixtures remain part of the combined application tests;
this archive does not execute them. In particular, the source-only stale-reply
matrix at `77a168c0` requires the integration agent's application check.

`run-fixture.py` checks archive hashes, runs the selected tests, temporarily
reintroduces three representative defects, requires assertion failures, restores
the source, repeats the selected tests, and runs strict all-target Clippy. The
negative controls remove named default-matrix bindings, admit equal-time future
runs as previous baselines, and allow empty SHA to establish same-SHA variation.
Only fixture copies are mutated. `results.json` records commands, return codes,
compiler versions, archive/runner hashes, and log hashes. Logs are gzip-compressed
without changing their contents.

Reproduce with Python 3, Cargo, and cached dependencies; choose a fresh E: output
directory. In WSL, put Cargo on PATH so its rustc child is available:

```sh
export PATH=/home/jonas/.cargo/bin:$PATH
python3 docs/validation/issue190-ci-comparison/run-fixture.py \
  /mnt/e/private/gitcraft-build/issue190-replay --cargo /home/jonas/.cargo/bin/cargo
```

Independent read-only review by `/root/lifecycle_scout` covered the API endpoints,
bounded reads, alignment/model/load units, and exact UI commit `3d3a390d`.
The reviewer found the unnamed-matrix axis permutation and equal-timestamp prior
selection defects; the corrected source and generalized regressions were reviewed
at that exact commit with no remaining blocking source finding. Root also found
the missing-SHA diagnostic defect; its regression covers missing current and
historical SHA at both job and step levels. Failed-list retention is separately
covered. These reviews claim no build or runtime execution.

The user-facing scope and explicit remaining unknowns are documented in
`docs/ci-run-comparison.md`. Issue #190 must remain open for its remaining scope.
Final combined compiler/runtime results and exact-candidate review belong in the
integration evidence, not inferred from this fixture.
