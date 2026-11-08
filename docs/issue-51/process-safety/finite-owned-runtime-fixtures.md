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

## Opaque Unix Command environment

Prospective command, **NOT EXECUTED** (same private TMPDIR and clean parent variables):

```sh
cargo +1.95.0 test --offline --locked -p gitcraft --lib remote::owned_runtime::environment::owned_runtime_opaque_command_environment -- --exact --ignored --test-threads=1 --nocapture
```

A Service-owned fixed current_exe driver receives synthetic inherited/set/remove keys,
raw non-UTF-8 key/value bytes and PATH through Command.env. It never calls process-global
set_var/remove_var. Seven snapshots use the original opaque std Command, directly through
existing Service/OwnedChild: inherit, override/empty/non-UTF-8 set, remove (including PATH),
env_clear with new values, empty env_clear at launch, inherited PATH lookup and explicit PATH
lookup after clear. PATH resolves only a fixed-name symlink to current_exe inside the
exclusive scratch namespace; no external executable or server is involved. Custom UTF-8
arg0 is checked exactly; non-UTF-8 coverage is for both environment key and value.

Each child returns exact serialized synthetic fields on stdout and an exclusively created,
synced receipt in its private cwd. Assertions include the fixed libtest stdout prefix,
exact payload bytes, empty stderr, status0, absent-vs-empty values and the exact
post-startup environment key set for empty-clear (platform contract below). The parent
rechecks all seven exact receipts and the driver's exact summary.
Snapshot dispatch uses arg0 so even an entirely cleared environment reaches the child
body. Driver dispatch requires a distinct arg0, and nested commands remove driver mode:
an arg0/env regression fails instead of recursively starting another driver.

The retained driver has a 100s budget; snapshots each have 12s budgets and 8KiB output
limits and immediately exit. Their own retained Service owners clean up each snapshot;
the driver Service joins before reporting its summary. Parent Ticket/Service Drop cleans
up on errors, and scratch is retained unless all assertions succeed. This fixture has no
descendant lifetime boundary or additional canary, and does not duplicate the Darwin
anchor/canary fixture. It asserts Unix Command semantics, not native_owner request
serialization (whose production API intentionally does not capture env_clear/arg0).

### Empty-clear diagnostic follow-up (runtime still held)

Parent-run candidate `3813b0f94105146424e59cb2b73298c50f8a69ca` had byte-equal
fixture sources to `49a9da66`. Its endpoint and claim/fence fixtures passed; its
environment fixture failed at outer driver status101. The retained receipts confirm
inherit/set/remove/clear tracked fields, then empty-clear has all tracked fields absent
but `empty_environment=false`. PATH cases were not reached. Evidence remains in
`/tmp/gitcraft-51-native-runtime-3813b0f9/validation.json` and `2.log`; this is a failure,
not an environment or suite pass.

The diagnostic delta at `1a328125` added sorted raw environment **key names only** to
the empty-clear receipt. That diagnostic candidate expected an empty key list and
`empty_environment=true`. Exact receipt/stdout checks remain. Readable escaped names appear in a strict
receipt assertion; outer driver status/stop failures now include its already capped
synthetic stdout/stderr, preserving inner assertion details. Unknown values are neither
recorded nor printed. Other cases omit this optional field and preserve their receipt
format. No new launch, process-control, parent environment mutation or production change.

Local Rust1.95.0 source: `library/std/src/sys/process/env.rs:24-47` captures no inherited
entries when clear is set; `library/std/src/sys/process/unix/unix.rs:393-408` installs
the captured environment for the fork/exec path. `library/test/src/lib.rs:180-183`
uses the normal static runner; its secondary invoker environment entries belong to the
separate panic=abort subprocess path at731-734. These reads do not establish which
unknown key exists in the failed binary.

Apple's published [CFRuntime.c](https://github.com/apple-oss-distributions/CF/blob/main/CFRuntime.c)
calls `__CFStringGetUserDefaultEncoding` during initialization, and
[CFStringEncodings.c](https://github.com/apple-oss-distributions/CF/blob/main/CFStringEncodings.c)
contains its environment-writing implementation. This supports a CoreFoundation startup
injection hypothesis, not attribution of the installed framework's behavior in this
failed run. This was the diagnostic hypothesis, with no key exemption at that candidate.

### Observed macOS startup contract correction (runtime still held)

The parent preserved/reviewed diagnostic candidate
`1043c8a76c426cf5f80403c94d51015beea9a34b`, with fixture sources equal to `1a328125`,
then ran the exact held environment fixture. It failed101 as expected under the prior
empty-key contract. Its actual empty-clear receipt has **exactly one key**,
`__CF_USER_TEXT_ENCODING`; all tracked synthetic variables and PATH are absent and
raw `empty_environment=false`. Evidence is preserved unchanged at
`/tmp/gitcraft-51-env-diagnostic-1043c8a7/receipt.json` and `runtime.log`.

The corrected test expectation uses `cfg!(target_os = "macos")`: this linked macOS
executable must report exactly that singleton key and raw emptiness=false after framework
initialization. Other Unix targets must report an empty key list and raw emptiness=true.
macOS does not accept an empty-or-singleton alternative. The observed key and the primary
CoreFoundation source above justify this explicit platform startup contract; no production
Command/environment defect is established by the two failed fixture expectations.

Snapshot collection remains unchanged. Full sorted actual key-list and entire receipt
equality reject every name outside that exact set, including any original inherited key
other than the single CF startup name. Every tracked synthetic value, removal, clear, non-UTF-8 value,
arg0 and both PATH cases retains its previous expectation. Unknown environment values,
including the CF value, are never recorded; there is no wildcard key/value filter,
parent environment mutation, production sanitizer or new runner. This fixture now
distinguishes an empty environment passed to std launch from the platform startup state
observed inside the linked application executable.

No new runtime run is performed by the implementation agent. The corrected candidate
requires parent delta review and one exact environment run; endpoint/claim passes and
both environment failures remain bound to their original immutable evidence.

Build/lint logs and final immutable source hashes are handed to the parent outside the
repository. Parent runtime outcomes above do not imply a corrected-fixture or suite pass.
