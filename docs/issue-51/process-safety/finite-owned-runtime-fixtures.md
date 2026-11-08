# Finite owned-runtime fixtures (held)

Base: `38cfeb4492a4e1896fdd4bb2d1c8ef6a33d6078e`. These are test-only additions.
No production launch, supervisor, credentials or ownership policy changes. Runtime
execution remains held until the parent preserves and independently reviews each exact
candidate. Ignored tests require the exact filter and `--ignored`; helper dispatchers
are implementation children and are never evidence of fixture success.

## Native endpoint

Prospective command, **NOT EXECUTED**:

```sh
cargo +1.95.0 test --offline --locked -p gitcraft --lib remote::native_owner::tests::owned_runtime_endpoint_roundtrip -- --exact --ignored --test-threads=1 --nocapture
```

Use a newly created private TMPDIR per invocation, and unset parent helper/test/trace
variables (including `GITCRAFT_FINITE_FIXTURE_MODE` and
`GITCRAFT_FINITE_FIXTURE_DIRECTORY`). Expect exactly one matched validating test.
The ordinary-exit case asserts exact libtest preamble plus binary target stdout,
exact stderr, status7, authenticated endpoint identity, atomic receipt and retained
record. The cancellation case rejects a syntactically valid wrong token before
cancelling the retained ticket through configure's authenticated endpoint callback.
It checks complete=false/code125 proof, no delayed inherited-descendant effect,
an independent canary's fresh challenges, exact recovery, malformed-proof refusal,
and repeated pinned receipt retirement.

All controller launches and canary handles stay in a private existing Service; target
launches stay in the native helper's existing Service. The target's inherited-group
leaf retains its direct Child in a local RAII guard (a second Service launch would
create a different group). Only that Child handle can be killed/waited by its guard;
the target Service owns the group's live anchor. No PID is read or used for control.
Ticket Drop cancels, Service Drop joins. The 12s request/poll budgets, 8KiB output caps,
3s finite leaf and 60s finite canary limit the schedule. Endpoint exchange uses the
existing 5s deadline. Failed assertions retain scratch records for exact recovery;
only success marks this exclusive random 0700 scratch directory removable. Native
endpoint directories intentionally remain after pinned receipt deletion, matching
production retirement's refusal to remove directory pathnames. Unknown helper
outcomes stay in existing controller recovery; no guessed cleanup or watchdog.

No fixture runtime results are claimed by this document. Build/lint logs and final
immutable source hashes are handed to the parent outside the repository.
