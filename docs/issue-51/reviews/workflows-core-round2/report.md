# Review workflows CORE fixed-objection review R2

**FAIL — two residual defects within B3/B4; four source corrections PASS.** Exact candidate `fe88e1dc52828c278515e2a7fa9cb6c6347ad1c4`, tree `02bc2690f089bbb670ae40e28479f77473d0634f`, prior reviewed candidate `bf6089b684597d878d3c2f256b7c69d64c6785a0`. This is a bounded review of the six original CORE findings, not acceptance of all ten features, the combined branch, shared process/credential code or runtime containment.

Source was read from an immutable archive at `/private/tmp/gitcraft-workflows-r2-fe88`; source references below are relative to that exact tree. The accompanying manifest hashes and verifies scoped source against Git objects. Read the full original CORE R1 report, full R2 handoff/status, original issue35 and snapshot AGENTS.md. No tracked edits, subagents, CI, PR, actual Git/helper/Docker/HTTP/SSH/network/process-control/native fixtures, full suite or application-discovery constructors were used. Repository Git metadata/object reads for source inspection are distinct from application Git fixtures.

| Original finding | R2 disposition | Evidence and boundary |
| --- | --- | --- |
| B1 host hooks/filters during reproducer preparation | **PASS, source/pure scope** | Raw tree/blob export replaces scratch checkout. Exact-function/object-reader recording and inert argv tests pass; real sentinel fixture remains held. |
| B2 mutable Docker name; entrypoint/deadline/cancel/cleanup | **PASS, source/recording scope** | Immutable full IDs, explicit entrypoint, bounded control calls and recorded cleanup uncertainty. Five selected container recordings and scoped cancellation test pass. Real Docker/process behavior remains unvalidated. |
| B3 transfer routing/context binding | **PARTIAL / FAIL, residual P1** | Post-capture mutable-name/rewrite window is corrected. A selected-remote-to-capture window still accepts another endpoint; exact capture method recording reproduces it. |
| B4 selected-source intents, bounds and completion ownership | **PARTIAL / FAIL, residual P2** | Original cross-cache shadowing/poisoning and late callbacks corrected. Exact aggregate-budget exhaustion silently omits valid remaining notes and can be marked freshly synced. |
| B5 immutable backport roots | **PASS, source/pure scope** | Direct immutable commit/tree refs retain actual objects, including applied tree; pure root assertions pass. GC/rewrite fixture remains held. |
| B6 late AI form ownership | **PASS, source/pure scope** | Request and captured-form checks preserve intervening edits; cancellation invalidates request and approval. Deterministic completion test passes. |

## B3 residual: selected A can become captured B before freezing

`src/repo/review_workflow/sharing.rs:26–29` checks that the user-observed `Remote { name, url }` is still present. `git::Remote` includes both fields and derives equality (`src/git/refs.rs:206–210`). Only afterward, at `sharing.rs:30`, does it capture the frozen `CommandSource`. The effective URLs are read from that captured source (`:31–58`), and `source.check_context()` (`:59`) only establishes that configuration has not changed since this later capture. Neither the captured destination nor the completed capture is compared with the observed selection.

Concrete interleaving within the original destination-binding defect:

1. The UI selected origin at A; the initial `git::remotes` comparison sees A and succeeds.
2. External configuration changes origin from A to B before `CommandSource::capture` begins.
3. `CommandSource::capture` consistently freezes B; its before/after configuration checks succeed.
4. `Target::capture` returns B. Later guards see the same B configuration, and the transfer correctly follows the *wrong captured* B endpoint rather than selected A.

Independent pure evidence: `/private/tmp/gitcraft-workflows-core-r2-evidence/sharing-capture-race-probe.rs` embeds the byte-identical production `Target::capture`, with recording-only Git/source mocks placing the configuration change precisely between steps1 and3. It prints:

`Observed origin at A; production capture accepted fetch=B push=B with unchanged captured context`

This is a deterministic source-ordering/recording reproduction, not actual Git routing, a native race test or a live transfer. The original R1 runtime observation is historical and was not rerun.

**Minimal correction:** bind the observed remote identity to the same frozen configuration snapshot used for transfer, and refuse an A→B change during capture. Compare the captured remote identity/effective endpoint with the observed selection using consistent URL semantics, rather than merely checking that the later snapshot is internally stable. Preserve the new literal effective-endpoint argv and private command context. Add the above recording boundary to the existing test; it needs no real transfer.

The rest of B3 is materially fixed: `sharing.rs:67–117` checks the captured context and sends literal captured endpoint strings, `CommandSource` flattens includes/freezes effective routing and binds actual execution (`src/git/command/source.rs:31–216,358–402`; `src/git/command.rs:155,787,913–925`). Successful fetches import only the selected namespace via atomic CAS after postflight validation. The postflight message acknowledges possible completed side effects. Existing credential handling is retained. These observations do not accept the separately owned shared credentials/process prerequisites.

## B4 residual: exact budget boundary hides valid notes without a warning

`src/repo/review_workflow/intents.rs:211–273` introduces the requested4MiB aggregate bound. At `:260–262`, it breaks the current source loop immediately when successful card reads reach that exact limit. It does not inspect whether that source still has entries, increment quarantine/rejection status or return an explicit partial/refusal result. `Reading` therefore looks complete. The Sync callback treats successful `Ok(Reading)` as fresh (`:483–486`), even though valid selected-source cards were omitted.

Independent pure evidence: `intent-boundary-probe.rs` embeds byte-identical production `Card`/`Seen`/`Reading`, validation/source-selection helpers and `collect_selected`. An empty local source and513 valid distinct selected-cache JSON records of8192 bytes each reach the budget after512 records. The record padding is accepted JSON and all validated card fields satisfy their individual bounds. Output:

`Selected 513 valid cards; returned 512, read 512, quarantine 0 at exact4MiB boundary.`

The probe asserts the omitted card and absence of quarantine; it exits0 because it successfully reproduces the defect. No Git repository or helper was used. `/private/tmp/gitcraft-workflows-core-r2-evidence/intent-extractions.json` binds extracted source and line/hash provenance.

**Minimal correction:** when budget exhaustion leaves unvisited relevant records, return explicit refusal or a visible truncation/quarantine condition; do not present the partial set as an unqualified fresh read. Preserve the existing bound. Add an exact-boundary pure test with one remaining valid card. Merely increasing the limit would retain the defect.

The remaining B4 changes close the original demonstrated paths: namespace selection precedes parsing/deduplication (`:150–170,221–229`); `(root,reference)` identity preserves same-ID notes across local/selected source; no cache key means local-only. Malformed/oversize data is quarantined, field/path/ref/identity/expiry limits are enforced, and selection generations guard Read/Post/Expire/Sync/Publish completions (`:303–351,404–487`). Three individually selected original regression tests pass independently. This residual concerns the new bounded-reading integration, not a reopened foreign-cache finding or a requirement for cryptographic teammate identity.

## Source closure evidence for B1/B2/B5/B6

**B1:** `reproducer_source.rs:124–196` obtains raw tree entries and blobs through the inert object reader, including already-available pinned submodule objects. `:277–348` packs a bounded in-memory tar/PAX archive and the identical generated test; traversal, unsafe links/ancestors and unsupported names are refused. No host checkout, Git archive, extraction helper or build is used. File/count/blob/aggregate/time limits are explicit. `git/command.rs:515–581` restricts inert subcommands, disables hooks/pager/fsmonitor/replacement objects/transports/lazy fetch, and sets deadline/output limits. Metadata/input pins use inert object operations too. The two raw-archive tests and inert-command recording pass. No hook/filter sentinel, actual exporter Git run or Docker run was performed in this review.

**B2:** `reproducer_docker.rs:110–160` pins a validated image ID and explicit `/bin/sh` entrypoint, validates required tools, places repository commands under the internal120s timeout plus5s kill-after, and keeps no-network/read-only/nonroot/capability/resource restrictions. Control calls have15s bounds and attach135s (`:10–11,172,286,296–337`). Launch validates/retains the full64hex container ID and owner before start; start/wait/removal use that ID. Interrupted creation uses the name only to discover an ID, then validates that immutable ID/owner before removal (`:207–245`). Cleanup uses an independent cancellation scope and persists failure/uncertainty, including bounded Drop handling (`:247–277,341–385`). An interrupted create can still finish after discovery and leave a stopped container; this is explicitly recorded as uncertainty rather than unsafe by-name deletion. Five pure container recordings and the thread-local scope test independently pass. This closes the original source/recording defects; it does not establish actual Docker timeout/cancel/crash behavior or the correctness of the separately reviewed shared process implementation.

**B5:** `contracts.rs:125–156` constructs content-addressed refs to fix/target/source commits and to every applied `write-tree` object. Execution retains these roots before scratch cleanup (`:158–260`). Tree objects are actual ref targets, not OID strings in JSON. Later plan/ref rewrites cannot overwrite roots for different object IDs. The exact pure root test independently passes. The authored ignored GC/source-rewrite fixture was not run; real Git reachability after GC remains a held runtime gate.

**B6:** `reproducer.rs:54–96` rejects old requests and installs generated output only if the current form equals the captured input. Changed forms retain their edits and receive a separately offered result. Explicit Use clears approval. Stop advances request identity/cancels work; form/selection edits clear approval (`:237–320,350–455`). The deterministic late-result/edit/cancel test independently passes. No model or application constructor was invoked.

## Independent checks and supplied evidence

The supplied exact-candidate test artifact was copied to `/private/tmp/gitcraft-workflows-core-r2-evidence/review-tests`; independently verified SHA256 `fba747936ba8c153dbd59a7c25f1a654c88c76d4e76ac72c9fe792f759059343` matches the handoff. This reviewer did not freshly build the full application artifact. Source review binds to the immutable commit; rerun claims additionally bind to this binary hash and the writer's exact-candidate build attestation.

- **17 individually selected exact pure/recording tests PASS**, zero failures. Bodies were inspected before execution; no broad feature/runtime filter was used. Names/results are in `selected-tests.json`, `selected-pure-results.json` and `selected-pure-rechecks.log`.
- **Two independent extracted-source pure probes** compiled with Rust1.95.0 and exited0, asserting the two residual defects above. B3 uses only standard-library recording mocks; B4 uses existing cached serde/serde_json/md5 libraries. Extraction manifests and sources are retained. These are not application process/runtime fixtures.
- Supplied writer gates, separately attributed:28 `r2_pure_` checks, compile/check, qualified clippy (`-D warnings -A dead_code`) and formatting PASS at exactfe88. This reviewer did not rerun those broad/build gates. Supplied status/log hashes are retained in the manifest.

No actual Git/helper/Docker/network/SSH/HTTP/cancel/timeout/native fixture, full suite or CI ran here. Real raw-export hook/filter sentinels, frozen-route local repositories, backport GC/ref rewrites, UI worker integration and owned Docker success/failure/entrypoint/deadline/cancel/crash/cleanup remain HELD. Authored ignored fixtures remain unexecuted, not passing. This report neither lifts the process/runtime hold nor accepts other reviewers' nine findings or the combined implementation. The only requested followup source repairs are the two concrete residuals above.
