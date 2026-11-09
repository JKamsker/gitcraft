# Historical provenance qualifications

This addendum qualifies historical evidence without rewriting any executed runner, result, log, or prior archive.

## Completion-stage execution identity was not collected

The archive at `docs/validation/pr184-followups-20261109/` contains aggregate `full-results.json` SHA-256 `986d4ff06221d304cf22a4d98117811dd916ae3583e2faede48c381d6c254e14`. It validates tested commit `1eee7e703ce629441eee166b2718021394c9fe09` and was added by documentation successor `f5919c97296560951166eea1ed43f15425bcaf27`.

Its original twelve successful gates and four completion gates were separate execution stages. The archived completion runner copied the initial manifest's UID, GID, supplementary groups, and tool versions. Those fields establish initial-stage metadata only. **Completion-process UID, GID, supplementary groups, and tool versions are unknown from this archive.** The statement in the historical README that execution identity and groups are recorded must be read with this qualification. No different completion identity or actual group drift is established. The native index fixture's conditional supplementary-group branch remains unproven.

## Historical clean-status attribution has a limit

The historical runners bound HEAD, tree, Git clean status, and produced executable hashes. They did not independently hash every consumed tracked checkout file against immutable blobs; assume-unchanged or sparse-index conditions could therefore conceal a source edit from that attribution check. No such historical edit or archive corruption is established. Historical gate results remain historical results, with this attribution limitation.

The successor full runner compares every tracked file's raw bytes, executable mode, and symlink target against the immutable candidate tree before every gate and after execution. It rejects hidden/sparse index flags, missing entries, unsupported entry types, and observation errors. These are observations at the recorded boundaries; they do not assert an immutable filesystem throughout command execution. Its source-checker bytes are snapshotted, hashed, archived, and bound to the execution record.

## Historical tool behavior remains preserved

The original assembler did not use an expected-old ref update. The successor uses compare-and-swap for initial creation and each advancement, preserving a concurrent writer's ref rather than overwriting it.

The original package builder checked individual commits without enforcing the complete advertised chain. The actual historical chain was independently inspected and matches its advertised base, eight ordered commits, sole parents, and terminal tested commit. The successor pins its own base and ordered feature plan and checks complete parent lists.

Prior package builders did not require nonblank review-task IDs and structured reviewer identities. Their actual archived source and execution approvals have populated identities and distinct nonblank task IDs. The successor rejects malformed identities and requires distinct tasks for source and execution approval. These remain trusted task records; strings alone do not authenticate a reviewer.

Prior inheritance verifiers checked archive hashes but did not independently reconcile committed execution bytes with approval records. Their actual archived bytes and approvals were independently inspected and agree. The successor also checks tested commit/tree, result and runner digests, source/execution independence, and exact archived verifier/package design approvals before producing an external proof. No new execution is attributed to any historical documentation successor.

## Superseded provisional source runs

Provisional full runs for `67e20ff44a1b1ab5cefc0287549411fd62216d39` and `a0f3586fc2ee033dca3a2081c3e5a4fd698946d9` were superseded while independent review identified additional source corrections. Their clean-state guards reject the changed candidate after the in-flight gate settles. No passing gate from either provisional run supplies validation of the final changed application. Their original work branches, manifests and logs remain preserved on the execution machine. The final archive accepts a separate complete run only.

The provisional `67e20ff4` library result contains seven failed tests that relaunch their own executable, reporting unavailable workspace claims or OS error 2. A concurrent focused rebuild replaced the still-running test executable, removing its original pathname. This is an execution-coordination error, not evidence validating or disproving the final source. The second provisional candidate was also superseded during subsequent source/build changes. Final validation must run build and runtime stages sequentially with no concurrent rebuild.

Preserved provisional manifest digests are `90847f65c83cc0445c0295e8d2f76bd9c220e1e066d395664823f90e46f74bfd` (`67e20ff4`, library 4,970 passed / 7 failed) and `85d588fd0cc0f532226800ca394833dbeab5534ee621932db355a90e47b16430` (`a0f3586f`, library 4,978 passed / zero failed). Both manifests terminate with “Candidate dirty or changed during runtime-gitcraft-lib”. A successful library summary in the second run does not overcome its failed source-state binding and supplies no accepted final gate. The corresponding raw library-log SHA-256 values are `9a37c273a7e3bb6defb645f297dc27f015849d3a21c4d68eab1578bf3a72b29c` and `5609942511e666b7a709974fe8f19da0f3348581bd205476d2dbb6cd8001a478`.
