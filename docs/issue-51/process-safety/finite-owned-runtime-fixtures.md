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

## Native claim and fence turnover

Prospective command, **NOT EXECUTED** (same private TMPDIR and clean parent variables):

```sh
cargo +1.95.0 test --offline --locked -p gitcraft --lib remote::workspace_claim::tests::owned_runtime_claim_fence_turnover -- --exact --ignored --test-threads=1 --nocapture
```

One namespace uses existing `Lease::native` claimants. A losing busy acquisition must
return the actual contention error without changing any namespace entries or generation.
An actual native mutation helper holds production `WorkspaceFence` through its fixed
target's ready/release barrier, effect and terminal receipt. Cancelling the retained
primary lease ticket closes its owned input, joins its EOF exit0, and invalidates its
authority. A second native claimant starts on a retained thread/token; a busy retained
primary lock after the old helper has exited proves the successor has entered the
namespace while the actual operation fence still blocks generation publication.
The test checks the pending result is absent and old generation/record unchanged before
releasing the target. Turnover completes only after real effect and terminal proof.

The successor must have a distinct generation and the same anchor. Both direct existing
WorkspaceFence acquisition and an actual native helper reject the stale grant; the
latter cannot create its validly named stale record or its target effect. The original
record/receipt survive turnover and authenticate exact recovery. A live successor fence
still verifies, pinned receipt retirement repeats safely, and fresh canary challenges
remain responsive throughout. No invented claim or generation implementation is used.

The pending thread has a cancellation/join Drop guard. Leases retain their Service
tickets, the isolated exact invocation owns the existing dedicated lease Service lane,
and a Drop guard shuts down/joins that lane on failure. All target and canary work is
retained in existing Services; ordinary polls, channel receives and lock waits are
bounded. The existing `Lease::native` handshake has its production 30s budget and its
helper's retained input supplies EOF cancellation. No process selector, timestamp/PID
takeover, signal hook or fallback cleanup is used. Failed runs preserve scratch records.

LOOPBACK/Python/Perl/SSH/WSL variants remain **unimplemented and unvalidated**. The
production remote launcher selects Python if its probe succeeds, otherwise Perl; it
has no fixed-backend test adapter. Adding an audited test-only backend selector and
repeating this schedule against that backend would be a separate source-reviewed
fixture. This native unit deliberately does not add a backend framework or policy.

No fixture runtime results are claimed by this document. Build/lint logs and final
immutable source hashes are handed to the parent outside the repository.
