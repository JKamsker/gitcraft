# Workflow CORE B3/B4 delta review R3

**PASS within static/pure/recording scope** at exact `3e43ced28232183108491acb76bca08a2166f15f`, parent `fe88e1dc52828c278515e2a7fa9cb6c6347ad1c4`. Both remaining CORE R2 residuals are closed. No additional source correction is requested in this bounded review.

Scope is only the two residuals in the full prior R2 report and their direct integration semantics. Read the full R3 handoff/status and exact Git-object delta. Only `src/repo/review_workflow/sharing.rs` and `src/repo/review_workflow/intents.rs` change. The other four CORE and nine Content/UI closures remain carried within their previous limits; their gates were not re-executed or broadened. No claim of final combined acceptance or lifted runtime hold is made.

| Finding | Disposition | Exact correction and independent evidence |
| --- | --- | --- |
| B3: selected A becomes consistently captured B | **CLOSED** | `sharing.rs:26–65` obtains effective endpoints through the frozen command source. New `capture_destination` (`:137–152`) compares that frozen fetch endpoint with the observed selection before accepting push/context, and rejects mismatch. A byte-identical extraction of both production functions with recording-only mocks refuses observed A/frozen B and accepts observed B/frozen B. |
| B4: exact4MiB budget silently omits remaining notes and marks fresh | **CLOSED** | `intents.rs:231–235` returns explicit refusal before reading another relevant record after budget exhaustion. The old break is removed. Exact collector extraction refuses513 valid8192-byte notes after512 reads. The new pure test also proves exactly512 notes succeed when complete, remaining notes in the next selected source refuse, and refusal clears sync freshness. |

B3 preserves legitimate split fetch/push routes and literal endpoint strings, including spaces. The new production helper test covers those cases and propagates a later context-check refusal. `Target::guard`, transfer binding, fetch cache transaction and literal endpoint argv remain byte-identical to the accepted R2 portions. A name absent from the frozen source or ambiguous effective URLs still fail through the existing endpoint lookup. The removed early live-remotes check is replaced by the comparison inside the frozen snapshot, closing the precise ordering gap rather than moving it.

B4 checks only relevant refs (after namespace filtering), including across local and selected-cache roots. Reaching exactly the limit with no relevant record left is accepted; otherwise no partial `Reading` escapes as `Ok`. Existing `State::accept` (`intents.rs:321–344`) clears `synced` on error, and the worker callback (`:483–486`) refreshes it only when the result succeeded. Existing source isolation, field validation, bounded per-record reads, quarantine and selection-generation handling remain preserved.

## Independent permitted checks

Evidence directory: `/private/tmp/gitcraft-workflows-core-r3-independent`.

- Copied the supplied exact-candidate test artifact and independently verified SHA256 `65c87e60a9295cbe2eb8b83734294a692b23a6be7a934a4aa0c51b87b28faf17` against the handoff status. This reviewer did not freshly build the full application artifact.
- **Six individually selected exact pure/recording tests PASS**, one selected test each, zero failures. The two new tests are `sharing::tests::r3_pure_sharing_selection_is_bound_to_the_frozen_endpoint_snapshot` and `intents::tests::r3_pure_intent_exact_budget_refuses_remaining_notes_without_fresh_sync`, beneath `repo::review_workflow`. Four retained direct tests cover cache CAS/source confinement, selected-source same-ID isolation, quarantine/field limits and selection-generation completion. Exact names/results are in `selected-tests.json`, `selected-results.json`, `selected-tests.log`. Test bodies were inspected; no broad runtime filter or application constructor was used.
- **Two independently assembled, newly compiled exact-source probes PASS**, Rust1.95.0. `sharing-probe.rs` embeds production `Target::capture` plus `capture_destination`; recording mocks provide all Git/source behavior. `intent-probe.rs` embeds production `collect_selected` and the previously preserved pure card-validation scaffolding, with the original513-card boundary. Extraction line/hash provenance is in `extractions.json`. Logs confirm A/B refusal and explicit budget refusal after512 reads. No real Git/helper/transport/process-control call occurs in either probe. The scaffolding-only compiler dead-code allowance is recorded separately from application lint.

The source files used by these probes were obtained directly from immutable Git objects, not the moving worktree. The accompanying manifest verifies the two-path delta, source hashes, inputs, extracted functions, test artifact and logs. Other production files are unchanged from fe88 by the exact commit diff, so this delta does not reopen their prior scoped closures.

## Supplied evidence and limits

Writer status/log hashes were independently verified. Supplied gates are two new pure tests, four selected preservation tests, two source probes, locked check, formatting, and **qualified** clippy `--locked --lib -- -D warnings -A dead_code`. The check reported six baseline macOS dead-code warnings. These build/lint gates are writer executions, not new reviewer executions, and qualified lint is not the enabled CI all-target strict lint.

No actual Git/helper/Docker/HTTP/SSH/network/discovery constructor, application runtime, process cancellation/timeout, native fixture, full suite, CI, PR, subagent or tracked-file edit was used. Previously authored ignored runtime fixtures remain held/unexecuted. Shared process/credential acceptance and the exact final combined-candidate checks/review remain parent-owned. This scoped PASS closes only the two named residuals and supports integration of this commit with the existing finite checklist.
