# Visualization suspension correction review: scoped PASS

Immutable candidate: **72a6a6a30fde9cce2b4a586f1597d42ff1e098f7**. Reviewed the exact commit diff and dependent source through `git show`/`git grep <commit>` only. Mutable root files were not used as review evidence. No edits, application tests, compiler, process-control fixture, CI, live host or signal executed by this reviewer. This verdict addresses **B2 (suspension ownership/cache retirement)** from `/tmp/gitcraft-51-visual-review-lifecycle-r1.md`; it is not a full-batch PASS.

## Why B2 is corrected

* `src/repo/lifecycle.rs:182–195` replaces every private visualization Reader/state during suspend: band_reader, topology_reader, topology, narration (including its Reader), minimap (including its Reader), activity, atlas, growth, postcards, motion and celebration. Replacing/dropping Reader cancels its retained running token, drops its pending closure and receiver, and disconnects already-sent or future results. An old channel therefore cannot populate the replacement reader, even if its generation counter happens to restart at the same number. The tab's old Scope and main result channel are also retired as before.
* `src/repo/messages.rs:388–394` polls feature readers only while the tab is awake. A sleeping/deferred tab cannot dispatch pending feature requests through the replacement tab lifetime, nor accept a feature result before the existing sleeping guard.
* Heavy activity/Atlas/growth/topology/minimap results are removed from the suspended RepoTab. User band preferences remain in the separate branch_bands field; rebuilding on normal resume/reload queues fresh graph/topology work. Motion/celebration state referring to prior rows/reads is also discarded.
* `has_postcard_draft` and worktree_tab::work().editor treat an open postcard as unfinished work, so normal idle/suspend checks preserve an open form rather than resetting it. This also participates in existing close/profile-switch handling. It is reasonable that a deliberately forced internal suspend clears the form; ordinary background_tick must first pass idle().

The earlier completed-before-suspend race is eliminated by dropping the entire private receiver, not merely requesting cancellation. The running-during-suspend race is covered by cancellation plus disconnected completion; the pending-before-suspend race is covered by dropping pending closures plus the sleeping poll guard. No stale-acceptance route remains in the changed code for B2.

## Regression evidence scope

The new `suspension_retires_private_analysis_readers_and_their_pending_captures` fixture is deterministic and does not spawn subprocesses. It starts a blocked band worker, queues undispatched topology, installs Atlas/growth caches, forces the sleeping phase and calls suspend, then releases the worker. It checks that the old active callback is cancelled, replacement readers are idle, caches are absent, and parked polling leaves graph/topology empty even when the worker ignores cancellation and returns success. This is appropriate evidence for the original running/pending regression.

Parent reports this fixture passed and is running relevant lifecycle/history/activity/postcard/topology/graph-view groups. Reviewer inspected the fixture but did not rerun it or certify those parent runtime results. The fixture does not separately stage an answer already sent before suspend or an explicit resume; source inspection establishes receiver replacement for the former and fresh reload queues for the latter. Additional pure coverage is welcome but is not a blocker to this scoped source verdict.

One separate preexisting behavior remains outside this correction: poll_postcards closes an open postcard when a **parked worktree** is polled with shown=false, while ordinary background tabs use poll(...shown=true). Consequently the new open-form unfinished protection should not be described as proven draft persistence across worktree parking/eviction. No new regression in that path is introduced by this correction; a future draft-preservation claim should test it explicitly.

## Remaining review status

The visualization batch remains **FAIL / not fully accepted**:

* **B3 remains open:** expanded path/identity allocations can exceed reasonable memory despite stream/node budgets. No allocation fix is present in this four-file correction.
* **O1/O2 remain open:** deep Graph clone on the UI during minimap preparation, and synchronous export ancestor opens. Cache destruction in suspend is also synchronous; this correction retires ownership correctly but supplies no proof that freeing large new caches meets frame-time goals. Address bulk disposal when resolving resource/performance work.
* **C1 remains an API/test-contract gap:** active() only guards stages/result while actual Reader close/replacement cancellation works through the ambient owned token. The misleading direct-callback ignored test must be corrected or the API changed before it can be claimed passed.
* Both explicitly ignored activity process-control tests remain pending the parent's safe ownership/containment gates. This review grants no permission to run them, no full-suite/CI pass and no final PR approval.

Author/commit evidence and parent test receipts should remain bound to 72a6a6a30fde9cce2b4a586f1597d42ff1e098f7 (or the exact next corrected candidate). B2 can be marked corrected based on this scoped independent static review; the remaining findings require their own implementation and review.
