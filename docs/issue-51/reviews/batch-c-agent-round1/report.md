# Batch C agent features — independent R1

**FAIL — four blocking findings within the assigned three-idea scope.**

Candidate: `b9e5d2d4bf8151af4debe55f90bcc1782e752f04`, branch `t3/working-state-publication-features`. All source references below mean this exact commit, read from `/tmp/gitcraft-c-agent-b9e5d2d4`, extracted through immutable Git objects. Nineteen relevant archive files were independently byte-compared to `git show <candidate>:<path>`; blob IDs and SHA-256 values are in `/tmp/gitcraft-51-batch-c-agent-review-r1-manifest.json`. No mutable deferred-replay work was read or accepted.

Scope: #48 **bake-off between attempts**, **gitcraft as an MCP server** (attribution and ghost approval additions plus direct interactions), and **agent turns as review steps** (reviewed hunks and inverse/revert). Read original #48/#49 in `/tmp/gitcraft-issue51-issues.json` and the platform baseline audit. This is not acceptance of the imported history `0de454f…` or internals `3e5b685…` implementations, or the other four Batch C ideas. Those remain separate review/integration dependencies.

## C-AGENT-1 — P1: inverse rollback can displace a concurrent editor save and report an owned rollback

Source: `src/git/drafts/working_transaction.rs:53–59,72–110,253–266`; reached by `src/git/drafts/turn.rs:175–204`, `src/repo/repository_tools/turns.rs:75–84,163–172`.

`Item::rollback()` first reads/checks whether the working pathname still contains the inverse's published inode and bytes (`owned()`). It then unconditionally exchanges the saved backup and working pathname, or moves the working pathname to its spare for newly created files. The rename does not enforce the identity that was just observed, and nothing validates the displaced file after rollback. It sets `applied=false` and returns `Ok`. The caller consequently reports “Owned changes rolled back.”

Concrete allowed schedule for an existing affected file:

1. Inverse turn publishes its replacement, leaving the old file under `.gitcraft-exchange-*`.
2. A later path/HEAD/index guard fails, triggering rollback.
3. `owned()` observes our published inode/bytes and returns true.
4. An ordinary external editor saves new contents at that pathname before the exchange.
5. Rollback exchanges the backup into the pathname and moves the editor's current contents to the hidden spare. It returns success, omitting the retained foreign-file location from the error.

The same race exists with an in-place write after the read. The editor's bytes are not necessarily deleted in this schedule: they survive at the spare, because `Drop` will not recognize them as our `after` payload. Nevertheless the live pathname has been overwritten/moved aside, and the claimed successful owned rollback conceals the recovery requirement. A safety draft was captured before the concurrent save and cannot restore that new save. A newly-created-file rollback can similarly move a concurrent save out of its pathname.

Independent evidence: `rollback_recording::exact_rollback_control_flow_reports_success_after_replacing_external_current_path` in the inert harness retains the exact `rollback()` method body, replacing only its dependencies with in-memory recording doubles. The recorded sequence is `owned observes true → external writer changes name AFTER observation → rename spare -> name`; `rollback()` returns `Ok`, the live path contains old bytes, and the spare contains the external save. This is a control-flow/operation-order proof, **not native filesystem execution or a platform runtime claim**.

Required correction: ownership decisions must remain honest across publication and rollback scheduling points. Preserve a displaced concurrent save and surface its exact recovery location, with an uncertain/attention-required outcome when ownership cannot be proved. Do not certify rollback merely because a prior read matched. Add recording schedules for replacement and in-place edits between observation and every destructive rename/unlink; native owned fixtures remain held until separately authorized. The state reviewer was notified because changelist/virtual-branch callers also depend on this transaction helper.

## C-AGENT-2 — P2: reviewed-hunk keys hide changes the user never reviewed

Source: `src/git/drafts/turn.rs:229–274,366–407`; UI filtering/marking: `src/repo/repository_tools/turns.rs:104–139`.

The key hashes only the `diff --git` filename line and the hunk body with **all `@@` lines removed**. The per-file metadata header is excluded. The session-wide reviewed set consequently aliases distinct occurrences and meaningful metadata changes:

- A turn modifies two identical snippets in one file, with identical context, at distinct line ranges. Mark only the first hunk reviewed, close and reopen the review. Both hunks have the same digest, so the second disappears without being reviewed. The pure fixture uses normal three-line surrounding context repeated at ranges `@@ -1,7 +1,7 @@` and `@@ -20,7 +20,7 @@`, both changing `-old/+new`.
- Review an ordinary content hunk in an earlier turn. A later turn has the same body but changes `old mode 100644` to `new mode 100755`. The new permission change inherits the old key and is hidden. The fallback metadata-only entry is not emitted when a content hunk already begins with that file's header.

Independent exact-source pure tests `reviewing_first_identical_hunk_also_hides_second_location` and `changed_permission_header_stays_hidden` confirm both cases. Both distinct occurrence patches print the same digest; the metadata-change fixture also prints its inherited digest. No Git repository, app constructor or process was executed.

Required correction: disambiguate simultaneous repeated occurrences while preserving intended unchanged-hunk tracking across turns, and bind review state to relevant file metadata. An executable-bit, addition/deletion or other header change must reappear even when text is unchanged. Recording/pure tests must cover repeated context, one-of-two marking, later insertion/location shifts, and mode-plus-body changes. Persisted old keys must not silently bless ambiguous new occurrences.

## C-AGENT-3 — P2: failed/interrupted build launches stay Running forever with retry disabled

Source: `src/repo/bakeoff/mod.rs:386–469,475–513,529–540`; `src/agent/bakeoff/build.rs:13–17,51–77`; `src/repo/bakeoff/view.rs:225–268`; launch failure: `src/repo/agents/mod.rs:183–199`.

The grid enters `Built::Running` before preparing the disposable worktree. On successful preparation it installs an empty status file, then calls `start_launch()`. That function returns no status to the bake-off; a failed program/PTY launch only sets `terminal_error`. The status stays empty. If a launched terminal exits/is closed before the shell writes its receipt, or writing the receipt fails, it likewise never becomes a terminal outcome.

Every poll maps missing, unreadable, empty and malformed receipts to `Built::Running`. There is no terminal-lifecycle signal or unavailable/interrupted outcome. `build()` refuses another run while Running, and `build_cell()` renders its retry button only when `built != Running`. The inner “Stopped or closed its terminal? Starts the build over” Running arm is unreachable. Build-all calls the same guarded method. The user must reset/reopen the comparison (discarding its comparison state) to recover.

Reproduction at source/recording level: prepare-workspace success followed by a recorded `Terminal::spawn` failure; empty receipt → polling Running → spinner and no Build-again action forever. The same state follows a terminal that exits before receipt creation. Independent exact-source pure test `missing_empty_and_unreadable_status_have_no_terminal_failure_state` confirms all nonreceipts map to Running; the rest is direct reachable callback/UI control-flow evidence. **No terminal, build command, timeout or cancellation fixture was run.**

Required correction: track a specific build launch/lifecycle and distinguish launch failure, completed status, and stopped/unknown execution; permit an explicit safe retry when the old launch is known finished or under an appropriate ownership-aware retry policy. Do not imply process completion merely from receipt absence or use host process selectors. Preserve the existing request-generation/status-path safeguards when adding outcomes.

## C-AGENT-4 — P2: aggregate “everything since last looked” review remains missing

Source: `src/repo/agents/turns.rs:185–232`; `src/repo/repository_tools/mod.rs:271–274,337–346`; `src/git/drafts/turn.rs:276–365`; `src/repo/repository_tools/turns.rs:104–139`.

The original idea asks for review of a single turn **or everything since the user last looked**, with unchanged reviewed hunks hidden. The new hunk-review route accepts exactly one `turn.parent → turn.commit`. Marks persist as a session digest set, but no review boundary/cursor or aggregate review action is recorded or exposed.

The pre-existing “Everything since this turn” item opens ordinary `Action::Compare { base: turn.commit, target: None }`. It does not use `Data::Turn`, the persisted reviewed set, or hunk-review actions. It requires the user to remember/select a baseline and does not hide previously reviewed hunks. Thus marking turn 4, receiving turns 5–8, and returning later provides no aggregate filtered-review flow; the user must inspect separate turns or manually compare without the new review state.

Required correction: expose an aggregate review with an explicit, repository/worktree/session-bound boundary (persist the last-reviewed/last-looked boundary or let the user explicitly select and retain it), reuse correct reviewed-hunk matching, and clearly identify whether the endpoint is the last saved turn or current files. Test new turns arriving and baseline changes while the dialog is open; stale callbacks must not advance another session's boundary.

## Reachable behavior and retained coverage

**Bake-off is a real implementation, but incomplete because of C-AGENT-3.** The grid/picker, 2–4 attempt comparison, files/size/dependencies, side-by-side file access and hunk/file assembly are present in `src/agent/bakeoff` and `src/repo/bakeoff`. New builds use the immutable compared tree and common base: `build::workspace()` validates object IDs, creates a synthetic commit and a new detached registered worktree, disables hooks for that worktree-add command, and does not move the source branch/index. The user supplies the command explicitly. Workspaces are intentionally retained for inspection/removal rather than force-cleaned. Request generation plus status path checks reject stale poll results, and stale preparation callbacks do not launch a build into a replacement comparison. This is source verification, not fresh actual-Git/build validation. Retention and real build execution across supported hosts still require the held gates.

**MCP attribution and ghost-card additions are reachable; no additional scoped blocker found.** `mcp/tools/mod.rs::all()` registers `attribution`; it resolves a commit first and reuses explicit authorship records, returns unknown rather than guessing, distinguishes commit-only declarations from recorded ranges, caps listed ranges at 2,000 and reports omitted ranges. It does not return transcripts. `repo/agent/mod.rs::agent_plan` builds the ghost preview in the planning worker for approvals, and `app/agent_card.rs:118–149` displays it alongside the Approve/Deny card. Hard reset, branch deletion, drop commit and force-push map to the existing ghost planner where applicable; unavailable previews are labelled. Shared undo/robot attribution, destructive approval/replanning, the guard digest, and the explicit unverified self-named agent identity are retained. This observation does not independently certify all legacy MCP write/undo/forge/process paths or the imported rejected route baseline.

**Turn review/inverse routes are reachable, but fail C-AGENT-1/2/4.** Context menus in the Uncommitted changes turn strip open the new dialog; reviewed state uses a worktree/session-qualified ref and CAS update, not a silently overwritten shared file. Preview/mark callbacks are bound to repository path plus dialog request ID. The inverse is applied to current file snapshots in a private index, refuses overlapping/stale work, captures an Earlier versions safety draft, preserves the source index, and requires explicit acknowledgement. It refuses unsupported remote/non-native atomic exchange instead of falling back to unsafe writes. Limits include 2,000 affected files, 8 MiB per file, 32 MiB aggregate patch/files and 1 MiB stored review data. Native transaction correctness still needs the correction and held gates; these source limits do not establish every subprocess/memory/cancellation boundary.

## Evidence and execution limits

New independent harness: `/tmp/gitcraft-c-agent-review-r1-harness/{review.rs,extraction.txt,review-tests,result.log}`. Four distinct pure/recording counterexample tests passed. The earlier three-test exploratory pass was superseded by the final four-test log, not added to that count. Exact source `Hunk`, `digest`, `split`, `Built`, `read_status`, and the `rollback` method body were extracted; only visibility was adjusted for the standalone module, and rollback dependencies are explicit in-memory doubles. The harness uses a fixed `sha2` rlib and Rust 1.95.0, and never constructs RepoTab/terminal/Git contexts or invokes the program under test.

To reproduce the permitted inert checks:

```sh
rustup run 1.95.0 rustc --edition 2024 --test /tmp/gitcraft-c-agent-review-r1-harness/review.rs -L dependency=/Users/jonas/.t3/worktrees/gitcraft/t3-61ff14cf/target/debug/deps --extern sha2=/Users/jonas/.t3/worktrees/gitcraft/t3-61ff14cf/target/debug/deps/libsha2-87be76f5bd933e8d.rlib -o /tmp/gitcraft-c-agent-review-r1-harness/review-tests
/tmp/gitcraft-c-agent-review-r1-harness/review-tests --nocapture
```

Writer logs were inspected and SHA-256 recorded in the manifest; they were **not rerun**:

- `gitcraft-c-b58acb6a-bakeoff.log`: 21 tests; mixed pure and actual Git/build-shell/PTY UI fixtures. Relevant bake-off source is unchanged through this candidate.
- `gitcraft-c-e2501e27-mcp.log`: 1 actual Git attribution fixture. Attribution and preview source is unchanged through this candidate.
- `gitcraft-c-561dd065-agent.log`: 31 mixed operation/Git and pure flow/UI tests. Reviewed agent/MCP/card/ghost source is unchanged through this candidate.
- `gitcraft-c-97ab0cec-{review,inverse,rollback,turn-ui}.log`: 1 passing test each, using actual Git and, where relevant, native files or headless UI. Turn logic/dialog source is unchanged, but the shared working transaction has changed since that commit; these logs do not certify its exact current implementation.

These historical logs have writer filename/source-diff binding, not an independently established original executable receipt. They are neither new independent pure gates nor acceptance of their actual execution under the present hold. No actual Git fixture, build-under-test, agent/MCP execution, HTTP/provider call, process/PTY/native fixture, cancellation/timeout test, full suite, CI, PR, tracked edit or remote mutation was performed for this review. Real build results, Windows/SSH/WSL behavior, native races, end-to-end agent approvals and exact combined integration gates remain unexecuted here.
