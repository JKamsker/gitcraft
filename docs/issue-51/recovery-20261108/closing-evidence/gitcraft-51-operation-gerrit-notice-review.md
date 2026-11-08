Independent bounded adversarial source review: **FINDINGS — one P2 issue.**

Exact range: `c2da523eb77e724a2aeefecdbc34ebea7e5d415c..918889d5bd6925efb6cad7791cb0706bc0c4f123`; candidate tree `7b4b406787118021edd9c89319129b5cf87308fa`. Canonical raw baseline: `771cbd9924fbdda106a36718c551b95305a9a269`. Exactly ten source paths differ. Both preserved unit commits have the requested Jonas Kamsker author and committer identity.

P2 OP-NOTICE-1 — keep unknown operation errors opaque

`src/repo/notice/operation.rs:35–36` sends every generic failure through `push_preflight::display::diagnostic`, even when `Failure::capture(error, None)` has no producer-owned push/Gerrit witness. The pre-existing push adapter's `dynamic` branch at `src/push_preflight/display.rs:427–439` accepts any string ending in ` Git blob; consider Git LFS.`; its prefix is empty and its “size” value is unconstrained.

Consequently, a provider error `provider opaque Git blob; consider Git LFS.` reaches that branch through ordinary `op_done`. By static source/catalog inspection, German projection replaces its body with `Git-Blob mit provider opaque; erwäge Git LFS.` (`locales/de/interaction-guards.ftl:252`). The new fallback is shared by status, long-finished and retained activity, so all three can rewrite unknown provider details without source provenance. The old generic op_done status interpolated the error unchanged, and the old activity fallback did not invoke the push diagnostic parser. Raw tuple identity is preserved, but the required opaque presentation boundary is not. Existing hostile-lookalike tests do not cover this suffix.

Keep generic fallback text opaque, or attach a finite push-owned presentation captured by the actual producer before using this dynamic adapter. Add an inert retained EN-DE-EN provider-suffix case across status, finished and activity. This finding was derived without executing application code or tests.

Other scoped source conclusions

- Actual CHECK still invokes the unchanged validator once with the unchanged Git execution closure. Dynamic Gerrit fields come from the same successful log inside that invocation, without another Git query or error-suffix provenance. Only finite expected-error/hash/reason/Change-Id fields survive the log closure.
- Complete final error equality is checked before selecting a finished headline. A changed tail sharing the first line cannot transfer the witness. Failed log reads and cancellation do not mint dynamic sources; later checks replace the operation carrier, including with None.
- The original raw headline selector is byte-identical to 771 after its name/visibility/module qualification adaptations. Status and long-finished raw operation names, outer strings and flags retain that baseline contract; display_operation and catalog resolution occur at projection.
- Independent inert AST comparisons found 153 unchanged original function bodies, exactly the nine enumerated presentation/transport changes, ten byte-identical original test modules, the exact old activity-tests prefix, and all 204 original assertion invocations intact. The check_expected body is byte-identical after its single callee adaptation. The op_done token skeleton is equal after removing only status/finished writes and the explicit new receipt handoff; its guards, raw auth/control extraction, callbacks and bookkeeping order survive. These are bounded supporting comparisons, not a whole-program proof.
- Activity still redacts and bounds raw failures before source equality checks. A changed/bounded/redacted error retires its source; a repeated plain equal background failure explicitly replaces the old source with None. Raw Outcome, as_text/copy, ids, repeats and status linkage retain prior source logic.
- Notice keeps private immutable Arc presentation, raw-only equality, plain/default None semantics and clear-before-edit retirement. Existing queue/parked/Toast consumers are unchanged in this unit. Actual Gerrit dialog warning paint uses its finite exact adapter without mutating the server or review plan.

Evidence verification

Independently recomputed all ten old/new source SHA-256 pairs against immutable Git blobs, verified the exact binary patch bytes against this range, and verified the candidate tree, all seven listed artifact hashes, manifest/proof agreement and gate-list agreement. All fourteen supplied gate-log SHA-256 values match. The all-target locked/offline check log has a successful dev-profile completion; all thirteen selected test logs contain their exact requested test name followed by `... ok` and `1 passed; 0 failed`. The JSON companion retains full commands, environments and immutable source/evidence bindings. Recorded execution passed, but it did not cover OP-NOTICE-1.

The gate logs are inspected supplied receipts, not tests rerun by this reviewer. Their metadata/hash agreement does not independently attest the execution environment. Warnings remain; there is no strict-lint claim. The remote SHA is recorded by the handoff; no fresh network verification was performed here.

Limits and prior constraints

Read AGENTS.md, all ten candidate source files and their complete diff, the handoff/proof/gate scripts and JSON, relevant unchanged display/catalog/control context, and the accepted prior Notice/parallel/custom review reports. Used inert source parsing, immutable Git-object reads, hashing and report creation only. No test, compiler, product runtime, process/Git-under-test, App/RepoTab construction, discovery, native window, CI, network or delegated agent was run. No repository source or branch was edited; only these /tmp reports were written.

This is a bounded unit review, not combined application/full-suite/final-candidate or consolidated-PR acceptance. Prior Notice/parallel/custom reviews remain accepted context, without a fresh whole-application audit. Prior workflow six-P2, interaction INT-FINAL-1 and root-selected nested legacy worker strings remain OPEN. OP-NOTICE-1 is a new generic-parser expansion within this unit, not a request to close those broader inventories.
