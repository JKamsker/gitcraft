# Scout verification evidence

These are local unit checks and source reviews, not the final combined application
gate. No GitHub CI was started. Final combined lint, runtime tests, and exact
candidate independent review remain the integration agent's responsibility.

## Prompt displayed-step budget

Source revision: `84a15b24af458c14e81f68068b1dce13b0d90999`.

The actual parser and regression source are copied byte-for-byte from that commit
into a minimal standalone crate. `results.json` records their SHA-256 values,
commands, compiler version, and result codes. The harness stubs localization,
redaction, and the unrelated `Step` fields; it establishes the parser's budget,
record ordering, retained edit contents, and notice behavior. It does not establish
application integration or redaction behavior.

The regression covers 21 combinations: below, exactly at, and over 2,000 displayed
steps, including single edits, 200-edit batches, summary plus edits, and multi-file
patch records. Removing only the common `truncate(2000)` causes the regression to
fail with 2,001 retained steps; restoring it passes. Clippy with cognitive threshold
25 and `forbid` passes for the copied parser, harness, and test.

Reproduction (Python 3 and Cargo with the cached `serde_json` dependency):

```sh
python3 docs/validation/complexity-scout-20261111/run-prompt-budget.py \
  . /tmp/prompt-budget-evidence --cargo /home/jonas/.cargo/bin/cargo
```

The initial Windows harness also passed, failed its negative control, and passed
after restoration; its temporary source directory was
`C:/Users/JONASK~1/AppData/Local/Temp/gitcraft-prompt-budget-0c3d8966491c48fcaf1edb65a1d003ba`.
The committed logs are the subsequent reproducible Linux run above.

## Earlier backend and compiler checks

`backend-6cf35a6a-tests-corrected.log.gz` records all 10 backend answer tests passing
on immutable source snapshot `6cf35a6a937c4819f45f2091b933cddc748d3103`.
The earlier `cargo test` filter selected zero tests; the corrected binary filter
was `git::backend::answer::tests`. The retained corrected log is the runtime evidence.

`clippy-9b32fb70.log.gz` records a successful all-target compilation of snapshot
`9b32fb70c78650b7cc9d904de6ef5bbe645e351c` with:

```sh
CARGO_TARGET_DIR=/home/jonas/gitcraft-target \
  /home/jonas/.cargo/bin/cargo clippy --locked --all-targets -- \
  -W clippy::cognitive_complexity
```

This was a warning inventory, not a passing lint gate. In particular, its remaining
reference-answer complexity and config predicate type warning motivated follow-up
`ccc4aefe850d941f66b77150bce1b85959072858`. It predates the Insights refactors and
prompt budget fix. Their combined compiler/runtime validation is pending.

## Independent source reviews

The scout reviewed root policy/workflow commit
`3b916c429b35969378b6bf5f8a5283d06c1b0116` and process capture/completion refactor
`69d2358553275e766377aee72d8787ad743adee4`. No blocking source defects were found.
The policy review identified the Windows-only Clippy coverage improvement and the
need to execute workflow dependency regressions in the reusable gate; both were
reported to the integration agent. The process review compared cancellation,
owned-child cleanup, capture limits, native-controller EOF, and completion ordering
against the prior implementation. It is a source review, not a runtime pass.
