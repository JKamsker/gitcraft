# Object storage: STATIC FAIL

Exact application: `f2b7968b45d90afc1bd2f7d2347482a2463f0742`, base
`b5df68070a6df9d91cd29ca5d6270db80c9040c9`. Tree/remote/clean checkout,
23 source hashes, three compile-log hashes and 20 commit authors verified.
Patch SHA-256: `378f6c8da1a1923379b4073f97b88a36964d0921575b5807558a2577914cee4b`.

1. **P1 OBJECT-PROTOCOL-1:** helper.rs:92 routes new mutation through
   command.rs:1089 `comparison_remote -> remote::exec`. Its timeout reaches
   session.rs:128/163 host `ps/awk -> kill`. The PID guard does not attest ownership.
   Use the independently accepted minimal owned transport; no legacy stop/replay.
2. **P2 OBJECT-PROTOCOL-2:** the same worker is spawned before the first `active()`
   check. An already cancelled execute may still send its ticket. Cancellation or
   deadline returns while actual remote work continues. Preserve completion ownership
   and check cancellation at the actual pre-send boundary; uncertainty text alone
   does not prevent dispatch or retain that ownership.
3. **P2 OBJECT-WINDOWS-3:** held_windows.rs:24/82/190 samples actual linkcount=1
   with READ sharing, then truncates the original at :272. The asserted exclusion
   of later hard links assumes DELETE access is required. Microsoft's
   FILE_LINK_INFORMATION remarks say “No specific access rights are required to set
   this information.” A linkcount sample is not proof against alias creation before
   truncate. The native assertion has not run. Prefer no-content-mutation rename of
   the retained original, no-replace publication and identity-bound create-only
   recovery. This is a blocking unsupported ownership contract, not a claimed native
   exploit reproduction. Author agrees the DELETE-access inference is invalid.

Linux descriptor-relative ownership, create-only backup/recovery, required-object
coverage and exact ticket recapture have no additional confirmed static blocker.
The helper is supplied as a dedicated headless Cargo/deb/rpm binary with documented
matching-build/PATH setup. Exact protocol/transaction success and unknown-outcome
wording are appropriate, subject to the transport corrections above.

Exact fmt/strict Clippy/Windows metadata-only compile evidence is verified. Earlier
886+3 full results bind only to a3b4d7d. This reviewer ran no Cargo, helper, native,
application/process-control fixture, signal or CI operation. **No helper/native/full
runtime approval is granted. The old c805 harness remains prohibited.** Source/hash
inspection and compile-only work may continue under root's resource policy.

Risk: high impact, critical likelihood given failed execution-boundary properties,
partial protection, hard recovery, moderate overall confidence (native unexecuted).
Recommendation/workflow: **block**. See `review.json` for boundary counterexamples,
controls, exact limits and the three-step correction/review plan; `binding.json`
contains immutable source/log bindings. No source edits in author/integration trees.

The assess-patch-risk rubric informed this review. Its shared schema/storage resource
and validator were unavailable through the exposed plugin; the JSON is a custom
review artifact and is not claimed plugin-schema-validated.
