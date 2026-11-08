# Independent internals UI review — round 1

**Verdict: FAIL. Four blocking findings in the three assigned features.**

Immutable candidate: `3e5b685a17d9351ae59dd89cd4c148358c9644bd`, base `a850de26b0a0778ea96fe34cd1498e6e78928492`. Reviewed the clean `t3/advanced-git-repository-tools` checkout at `/Users/jonas/.t3/worktrees/gitcraft/t3-3a66e6f0`; HEAD and tracked cleanliness rechecked after probes. All source locations below refer to this candidate. Requirements: original issue #49 from `/tmp/gitcraft-issue51-issues.json`. Writer handoff and `/tmp/gitcraft-advanced-validation/results.json` read; writer gates are prior evidence, not new execution by this reviewer.

Scope: publishing submodule dependencies before their parent; submodule conflict graphs/resolution; shared/borrowed topology and independence. Other internals features and general routing belong to other reviewers. No tracked edits, CI, full suite, external writes, subagents, host PID selectors/signals or real process-cancellation tests. No native clickthrough, Windows runtime, SSH or WSL execution is claimed.

## Blocking findings

### INT-UI-R1-1 — P1: selecting an annotated tag records a tag object as a submodule commit

Source: `src/git/repository_tools/submodule_conflict.rs:101` collects `%(objectname)` from all containing refs, including annotated tags; lines 118–122 accept them through `merge-base`, which peels tags. `resolve` lines 197–211 again validate via ancestry and `<oid>^{commit}`, but lines 212–216 write the **unpeeled input OID** into the mode-160000 parent index entry. `src/repo/repository_tools/submodule_conflict.rs:45` exposes those candidates directly.

Repro: create base/ours/theirs and an existing merge containing both, put an annotated tag on that merge, open the conflicted submodule graph, choose the tag candidate, acknowledge and record. The chosen gitlink contains the tag object's hash, rather than the commit hash. A gitlink must identify the submodule commit; this result can be committed into the parent and is unsuitable for the normal submodule checkout flow.

Confirmed using the exact copied test artifact, with a PATH adapter that added an annotated merge tag to the existing owned fixture and supplied that tag as the tested choice. The real production `read` and `resolve` ran unchanged. The test passed while recording:

```
commit: aeef6cfa37817a4eb813db9a8c0cc122b70c96e7
tag:    bf8b1b7ef1dcfe0d5b3e4a62ef195c4f9a7c1998
index:  160000 bf8b1b7ef1dcfe0d5b3e4a62ef195c4f9a7c1998 0 lib
object type in child: tag
```

Evidence: `/tmp/gitcraft-internals-ui-r1/tag-oid.log`, `tag-oid-record.jsonl`, `git-wrapper/git`. The fixture adapter also kept the fixture's branch ref pointing to the peeled commit so Git's branch-type restriction did not short-circuit production resolution.

Fix: canonicalize every candidate/manual choice with `rev-parse --verify <oid>^{commit}`, validate ancestry of that canonical commit, and record/display the canonical commit OID. Add an assertion that the selected gitlink object itself is a commit, covering annotated-tag candidates and pasted tag object IDs.

### INT-UI-R1-2 — P1: stage validation is not coupled to index publication

Source: `submodule_conflict.rs:182–195` compares the parent snapshot and conflict stages. It then runs child ancestry/object commands at lines 197–211 before unconditional `update-index --add --cacheinfo` at lines 212–216. No parent-index ownership or compare-and-swap spans the last validation and write. Git's own update-index lock protects its write, but it does not validate the application's earlier expected stages.

Repro: prepare the graph; while recording, another client resolves the same path after the application's last `ls-files --unmerged` and before its update-index acquires the index lock. The application silently replaces that newer resolution. This is independent of whether the originally selected merge contains both old sides.

Confirmed with the exact artifact and a deterministic owned-fixture PATH adapter: immediately before the production update-index command, the adapter successfully wrote a new stage-0 resolution. Production then overwrote it and returned success; its existing stale-check test still passed. Evidence `index-race-record.jsonl`:

```
injection_code: 0
before: 160000 4e7e551e06490718d9b62715d724219907685b82 0 lib
 after: 160000 29f0b6f1331ed78a60e77466c0feac2c2e775808 0 lib
production update-index exit: 0
```

Evidence: `/tmp/gitcraft-internals-ui-r1/index-race.log`, `index-race-record.jsonl`, `git-wrapper/git`. This introduces no signals or uncontrolled processes; both writes target the test's own temporary parent.

Fix: validate expected stages while holding ownership of the parent index transaction, then publish that exact transaction without overwriting a replacement index. Keep expensive child checks outside the critical section and repeat the index comparison within it. Add a deterministic competing-writer assertion that the newer resolution survives and the application returns a stale-context error.

### INT-UI-R1-3 — P2: valid shared donor DAGs are misclassified as cyclic and cannot become independent

Source: `src/git/repository_tools/sharing/topology.rs:125–137` uses one global `seen` set. A repeated donor, including a diamond root→B,C and B,C→D, is treated as “Repeated or cyclic alternate” and skipped. `Store` has only one `donor_of`, so the second borrowing edge is also absent from the topology display. `sharing/mod.rs:175–181` blocks independence for every warning.

A repeated vertex is not a cycle. Two clones borrowing the same transitive donor, or even redundant identical alternate entries, are valid Git object-store topologies. The feature refuses them despite being conventional, local, complete clones within the pack limit.

Confirmed with the exact independence test: the adapter duplicated its donor line before production inspection. `git --no-replace-objects fsck --full` exited 0; production inspection reported `Some("Repair the missing, cyclic, HTTP or unreadable donors before preparing independence.")`, causing the existing positive test to fail. Evidence: `/tmp/gitcraft-internals-ui-r1/duplicate.log`, `duplicate-record.jsonl`. The diamond case follows the same global-seen branch; that multi-store case was source-reviewed rather than separately executed.

Fix: represent vertices and borrowing edges separately; detect cycles only on the current DFS ancestry path. Deduplicate traversal/packing without dropping edges or treating shared/repeated donors as corruption. Cover a diamond, duplicate entries and a real cycle.

### INT-UI-R1-4 — P2: cancellation disables cleanup of already-created dependency-check refs

Source: `src/git/repository_tools/dependencies.rs:223–245` fetches heads/tags into a unique app-owned namespace and cleans it with `runner.in_repo(update-ref -d name expected_oid)`. `src/git/repository_tools/guard.rs:76–78` immediately refuses **every** runner command once the operation control is canceled. Thus cancellation after refs materialize, including cancellation between fetch and cleanup, guarantees that no cleanup command executes. A failed listing at dependencies.rs:236–239 also returns before cleanup. If fetch itself returned an error, line 249 returns it before presenting the collected cleanup failure.

Result: persistent private refs retain fetched histories/object storage, appear in all-refs inspection and can influence later enumeration. Repeated canceled checks accumulate unreachable-from-UI namespaces; there is no recovery janitor or durable cleanup record here. Parent pushing is stopped, so this finding does not claim a parent was incorrectly published.

Evidence: source review plus recording-only `/tmp/gitcraft-internals-ui-r1/cancellation-recording.rs`/`.log`. The exact production `advertised` function was copied unchanged into an adapter recording a fetch that materializes an owned ref then marks control canceled; the adapter uses the production runner's early-cancel rule. Result: `Err("Cancelled after owned refs materialized")`, zero delete commands, one owned ref remaining. This is an algorithm recording probe, **not** a real process-cancellation execution or an exact-binary cancellation claim.

Fix: register owned refs/namespaces for finalization before fetch, and perform bounded noncancelable cleanup restricted to verified app-owned names and expected OIDs. Preserve the original operation outcome plus cleanup diagnostics/recovery instructions. Test cancellation before/after ref creation and cleanup listing/delete failures with recording backends first.

## Requirement coverage and reachable behavior

| Original #49 requirement | Exact-candidate assessment |
|---|---|
| Before parent push, list unpublished gitlink commits | Toolbar Push routes repositories with listed submodules through dependencies (`src/repo/push/mod.rs:121–129`); command palette also exposes the tool (`actions.rs:674`). Backend gathers current-tree and outgoing-history gitlinks, with nested dependencies, rather than merely examining current child HEAD. UI lists all required OIDs and publication status. This is substantial implementation, subject to findings above. Other push entry routes were not exhaustively exercised. |
| Push dependencies before parent, per-repository results | `dependencies.rs:417` sorts deeper child paths first, stops subsequent pushes on failure, pins source OIDs and parent target, rechecks named fetch availability after pushing. UI renders child and parent outcomes (`repo/repository_tools/dependencies.rs`). Local named-host/stale-plan test passed. No distributed atomicity guarantee: dialog correctly explains this. |
| Name the remote checked | UI exposes remote picker, redacted exact fetch/push URLs and source/target. Fresh app-owned fetches inspect the selected fetch endpoint; a different push endpoint is not assumed to establish fetch reachability. Existing wrong-host fixture passed. |
| Conflicted gitlink opens graph around base/ours/theirs | Changes navigation (`navigate.rs:154`), submodule double-click and context menu (`sidebar/submodules.rs:45,74`) reach a real bounded text graph with labels. Open-child-to-merge and refresh are wired; existing merge candidates/full-OID input and parent recording are reachable. Findings 1–2 block acceptance. Add/delete and uninitialized conflicts explicitly direct users elsewhere; no silent claim those cases produce a graph. |
| Show shared git directories and borrowing topology, missing donors | Palette route (`actions.rs:669`), git/common directory and worktree list, donor cards, missing/error labels are implemented. Missing-donor and metadata safety tests pass. Shared vertices/edges remain incorrect under finding 3. |
| Copy needed objects before removing alternate link | Conventional local-clone path performs root/ref/reflog/index census, original replacement/graft ancestry packing, standalone strict pack/index verification, local install, then alternate detach and a post-detach object proof; failure attempts non-overwriting restoration. Existing positive test hides the donor after detach and runs fsck; it passed here. This is real production work, not a placeholder. Scope limits below still apply. |

## Boundaries, resource/cancellation limits and remaining verification

- **Explicit independence scope:** only conventional local `.git/objects` clones; rejects linked worktrees/shared common dirs, nonconventional gitdirs/bare layouts, promisor clones, missing/unreadable donors, unsafe metadata and SSH/WSL mutation. The UI and backend explain the 128 MiB complete-pack cap. Original #49 does not explicitly demand SSH/WSL mutation, so those explicit capability restrictions are not independently treated as unimplemented platform requirements. Completion claims must retain them.
- **Object safety reviewed:** unchanged alternate bytes/file identity, held ancestor ownership and no-replace installs/detach/restore are checked; donor stores are read, not mutated. Post-detach proof intentionally ignores cancellation until proof/restoration finishes. No donor-loss, ENOSPC, power-loss or syscall fault injection was executed here. Refs/config/index/reflog/root/graft snapshot rechecks and donor topology rechecks narrow stale-context races, but the UI requires other writers to be stopped. Do not infer an atomic lock across refs, index and donors.
- **Storage cleanup limitation:** Unix `held_unix.rs:250–284` may leave `.gitcraft-<nonce>.pack/.idx` on write/sync/rename errors; a successfully installed pack may also remain when later state checks/cancellation retain borrowing. Neither is object loss, but repeated failed attempts can consume disk. Recovery alternate backups are intentionally retained. This was source-reviewed, not fault-injected.
- **Ancestry/candidate cancellation is not cleared:** both dependency ancestry (`dependencies.rs:199`) and conflict containment (`submodule_conflict.rs:21`) use legacy `run_captured`/default Extra; `command.rs:149–184` supplies no timeout or operation-cancel callback. Conflict scanning stops after 100 *accepted* candidates, not 100 examined refs (`submodule_conflict.rs:111–126`), and can issue many individual ancestry processes. Bounded text reads/200-row graph and dependency count128 do not bound total traversal time. Independence binary reads do use 120s/byte caps/control; its final proof is intentionally noncancelable. No runtime latency/cancellation guarantee is claimed for the legacy ancestry paths.
- **UI work boundaries:** initial tool reads use task callbacks guarded by repository path/profile/dialog id. Dependency jobs check network refusal and operation busy state, return per-node results only to the same dialog id, and reset acknowledgment. Generic tool mutation completion closes any repository-tool dialog (`repo/repository_tools/mod.rs:327`), so opening another tool while an operation completes can dismiss that later dialog; this is an additional UI coherency observation, not one of the four acceptance blockers.
- **Portability:** new direct `tempfile` and Windows-only `winapi-util` dependencies reuse locked packages. Writer macOS build/fmt/clippy and focused tests are bound in its results file; Windows ownership module was only cross-typechecked, with no Windows runtime/full application cross-build claim. New Git root-ref/no-lazy-fetch use is explicitly guarded by a Git 2.45+ diagnostic. No new CI was run.
- Parent/content reviewer already owns inherited Git environment mismatch and legacy-graft findings; this report does not duplicate those as independently reproduced findings.

## Executed evidence

Copied the preserved candidate test artifact to `/tmp/gitcraft-internals-ui-r1/exact-tests`; SHA-256 `9bcfa1ae6c5a7ba895358301221725c8532fcb29102edf6c62317cc666f0bb6a`. Used isolated TMPDIR children and the writer's non-login validation shell. Six baseline focused tests passed: four sharing/topology tests, one named-host dependency test and one conflict-graph test. Logs/results: `baseline-sharing.log`, `baseline-submodule_conflict.log`, `baseline-dependencies.log`, `baseline-results.json` in that evidence directory.

Adversarial exact-artifact runs: duplicate-valid-donor positive test fails at production capability preparation as expected; competing-index and annotated-tag variations expose incorrect production outcomes while existing assertions still pass. Recording-only cancellation probe produces one leaked owned ref and zero delete commands. All mutation adapters restrict writes to their owned temporary Git fixtures; no tracked candidate files were edited. The report is bound to the immutable SHA above; a later correction requires scoped re-review and deterministic assertions for these cases.
