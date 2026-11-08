# Visualization corrections: independent UI review R2

**Verdict: FAIL — one blocking P2 finding (UI-R2-1).**

Immutable documentation tip: `cfc96d8ff949e35d0de5ea643e76b1551b874769`. Application: `bc201e42d3480c4cc9525c948a69fb06afd3ac73`. Correction base: `883c14d8ebe8f38cd2c404cc87de05e21c07e66c`. Assigned worktree: `/Users/jonas/.t3/worktrees/gitcraft/t3-54b1a48a`, branch `t3/visualization-review-fixes`. Reviewed source was extracted with `git show` at the immutable tip into `/tmp/gitcraft-visual-ui-r2-source`; mutable worktree files were not the review source. The documentation-tip/application diff contains only the correction evidence directory.

Scope: prior UI-R1-2/3/4/5, successful-operation celebration identity, preservation of the accepted UI-R1-1 keyboard correction, minimap disclosure/focus and relevant reduced-motion behavior. Read original #46/#39/#38 bodies, the R1 UI report and scoped keyboard correction, and the candidate correction README/manifest/receipt. Other reviewers own analytics/content, resource lifecycle, shared graph ownership and export-worker corrections. This is not acceptance of their scopes or of the final combined application.

## Blocking finding

### UI-R2-1 — P2 — A queued no-op merge-mode Pull can celebrate the preceding merge again

**Source:** `src/repo/celebration.rs:61–70`, especially `:69`; `:129–139`; `src/repo/operations.rs:117–131`, `:307–314`; `src/repo/running.rs:142–155`.

The new mutation-worker observation pins the successful operation's **after** HEAD, but `Event::Merge.before` still comes from `self.refs.head`, the last accepted UI reload. A following queued operation starts immediately after a reload is requested, before that read must be accepted. Consequently its actual starting HEAD can already differ from the UI snapshot. An exit-success result plus the pinned after HEAD does not prove that this operation moved HEAD or completed a merge.

**Concrete sequence:**

1. UI HEAD is A. Start a merge which produces M with ordered parents `[A, topic]`; queue a merge-mode Pull while the merge is busy. The relevant remote is already at M, so the eventual Pull is up to date.
2. Merge succeeds. Its worker observes M and arms its successful event. `op_done` requests the asynchronous reload, then `after_op` starts queued Pull before that reload is accepted. UI `refs.head` remains A.
3. Pull's celebration candidate records `before=A`; actual worker HEAD before and after Pull is M. Pull succeeds without changing the repository and its observed HEAD is M.
4. When M is loaded, `hash != before` is true and `M.parents.first() == A` is true. The Pull's candidate emits “Merge completed” and the optional sound for a merge completed by the preceding operation. If both pending candidates are confirmed together, audio receives two cues for one merge; the visible timer is also renewed. This also occurs when an external merge advances HEAD before the UI reload and the user runs an up-to-date Pull/merge.

The original #46 requirement is a flourish for an operation that finished the relevant event. Success of an up-to-date Pull is insufficient evidence that this Pull finished a merge. This is incorrect feedback rather than a native-audio implementation claim.

**Independent deterministic evidence:** `/tmp/gitcraft-visual-ui-r2-evidence/stale-before-probe.rs` embeds the candidate's production celebration state and methods through `confirm_celebration` unchanged, with small RepoTab/settings/egui/audio adapters. It models actual before=after=M, UI before=A, M parents `[A, topic]`, and a successful `pull --no-rebase`. Its recording output is:

```
actual_before=already-merged actual_after=already-merged ui_before=old-ui-head no_op=true celebration_cues=1
```

The probe asserts both the no-op and the emitted cue. This is a copied-production-logic pure probe, not an interaction in the compiled application, a real network Pull or native speaker validation. The source control flow above establishes the production queued-operation route. Existing no-op tests use an accurate UI baseline, so they omit this stale-before case.

**Required correction:** supply the actual operation-worker before and after HEAD observations to celebration confirmation and require a relevant actual transition. `run_steps_counted` already captures actual `head_before` for normal `Undo::HeadMove` operations (`operations.rs:523–526`) and returns worker-observed before/after metadata (`:575–583`); reuse or make this a typed result rather than authorizing a transition from stale UI state. Preserve post-operation identity pinning and independent pending events. Add deterministic queued merge→up-to-date Pull coverage with delayed reload acceptance and exactly one cue total; add external HEAD advancement followed by a no-op and zero cues for that operation. Keep successful real merge, fast-forward Pull exclusion and later unrelated HEAD movement coverage.

## Prior finding dispositions

| Prior item | R2 disposition and source evidence |
| --- | --- |
| UI-R1-1, accepted parent keyboard fix | Preserved. `graph_walk.rs:146` uses selected/focused origin; pseudo-row entry enters loaded HEAD/first real row. `graph_view/readability_tests.rs` is unchanged from the accepted correction base. Both actual-key WIP/stash/focused-origin tests were rerun successfully. |
| UI-R1-2, linked-worktree band identity | Resolved in scope. `branch_bands.rs:149–154` reconciles against resolved `repo_key()`; `load.rs:427–428` invokes it after worktree discovery, and the immutable `messages.rs` awake-poll path invokes it before accepting band output. Main/linked siblings and reopen/`continue_from` are covered by the successful linked-worktree fixture. Preferences are reloaded from one shared persisted identity. |
| UI-R1-3, nested merges counted as release integrations | Resolved in scope. `graph_walk.rs:70–108` traverses the release's actual first-parent hash, counts introductions there and reports missing parent coverage rather than treating another loaded parent as first parent. The narrated wording names the first-parent line. Expected-DAG tests include feature→main→release, direct/repeated integration and missing first parent. |
| UI-R1-4, off/on suppresses selected narration | Resolved in scope. `graph_walk.rs:272–277` cancels and clears both request hash and result while disabled; re-enable queues the unchanged selection. Controlled pending/completed Reader fixtures exercise actual checkbox transitions and repeated toggles without timing sleeps. |
| UI-R1-5, pending motion overwritten by queued operation | Resolved in scope. `graph_motion.rs:12–16,29–61` separates in-flight and successful pending snapshots, retaining the earliest successful ref baseline and combining successful fetch state. Delayed reload tests go through `op_done`/queued Push and failure handling; an unrelated operation no longer clears successful pending feedback. |
| UI-R1-5, pending celebrations overwritten | Original erasure defect resolved: `celebration.rs:14–17,87–105` retains successful pending events independently. `operations.rs:117–131` pins actual successful after HEAD before queued work starts, so a later unrelated HEAD change is not attributed to the earlier operation. Overall celebration acceptance remains blocked by UI-R2-1's stale **before** identity. |

## Additional scoped checks

The minimap now visibly and accessibly discloses its first-24-lane shape limit while retaining all-row and HEAD/tag/search representation (`graph_view/minimap.rs:79,119,131`). Its focused headless tests exercise shared focus behavior and Tab/PageDown interaction. Band rebuilds expose a loading/preview explanation rather than silently presenting the temporary compact layout as final. These address the corresponding R1 followups; no numeric frame-time or native screen-reader claim follows from these tests.

Motion marking is capped and expires (`graph_motion.rs:8–9`); reduced-motion paths retain static feedback rather than interpolating. Celebration sound and motion remain independently opt-in, and reduced motion suppresses the flourish (`celebration.rs:145–151`). Focused reduced-motion tests pass. Native reduced-motion appearance, speaker output, device failure and external screen-reader behavior were not exercised.

## Gates and evidence

Independently copied the exact test executable to `/tmp/gitcraft-visual-ui-r2-evidence/exact-tests`; SHA-256 `03ef0bbe746ebd27a01c83190781cbcd533256b3040cc1e479c821cb3f3a60d9` matches the immutable candidate manifest. Ran seven narrow filters with isolated temporary storage, the validation shell and one test thread: branch bands 4, graph walk 4, graph motion 5, celebrations 7, topology arrows 2, minimap 4, audio recording 1. **27 passed, zero failed, zero ignored in these reviewer executions.** The ordinary local-Git celebration worker fixture is included; no process-control fixture was selected.

`results.json`, raw reviewer logs, the pure probe/source/output and SHA-256 inventory are under `/tmp/gitcraft-visual-ui-r2-evidence/` (`review-evidence.json`). Independently checked the 29 candidate compressed raw-log hashes and receipt arithmetic: writer evidence reports 180 passing executions / 176 unique passing tests, with two process tests explicitly ignored. Those ignored tests were not executed by this reviewer and are not passes. Writer build/fmt/qualified-clippy results are evidence inspected, not independently rerun here. Strict `clippy -D warnings` remains a recorded failure for six baseline macOS dead-code findings; the README correctly distinguishes it from the qualified lint gate.

No tracked edits, CI, full suite, unsafe process tests/signals, native audio test or live service operation were performed. This report does not authorize those gates. The original whole-batch FAIL is not cleared: this scoped correction review itself remains FAIL until UI-R2-1 is corrected and independently checked, and other owners' reviews remain separate.
