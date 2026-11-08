# Process safety R2 — caller / bisect ownership review

**STATIC FAIL** on immutable candidate `bc2521c6a8422571e67ae2da00e381bde48f6de5`, base `43707ea3`, branch `t3/safe-process-supervision`.

Scope: durable bisect workspace ownership, receipt-gated cleanup, watched Git, streaming Git, AI, lost-fsck cancellation, shutdown ordering. This is a static source review, not runtime validation or authorization for owned-child fixtures. No helper, supervisor, app, test executable, subprocess-control fixture, compiler, CI or live remote request was executed. Source was obtained with `git show <candidate>:<path>` into `/tmp/gitcraft-process-callers-r2-source`; SHA-256 bindings are in `/tmp/gitcraft-process-callers-r2-provenance.json`. The plan and candidate process-safety documents were read. The worktree has uncommitted remote marker changes; none were reviewed or credited.

## P1 — prior cross-process bisect ownership finding remains open

Source: `src/parallel_bisect/workspace.rs:43,65–95,101–114,245–267,287–308,401–410`.

`claim()` still only adds a path to the current process's `ACTIVE` mutex. There is no durable, random, exclusive run/lease claim. `open()` checks a readable PID/time heartbeat, constructs a cleanup-enabled `Workspace`, calls `clear()`, then attempts the exclusive directory creation. It starts the heartbeat asynchronously only afterward. Any failure after construction runs `Drop`, which calls `close()` and `clear()` without evidence that this instance ever owned the workspace.

Concrete permitted interleaving, derived from the exact branches:

1. App A and app B resolve the same repository/common-dir workspace. Each claims its own in-process `ACTIVE` and both pass `refuse_if_in_use` while the heartbeat is absent/stale.
2. Both initial `clear()` calls complete before A creates the directory. B pauses before its own `make_private()`.
3. A creates the private directory, starts its heartbeat, and dispatches its bisect test. Its authenticated random `.owner` record now exists.
4. B resumes. Its exclusive `mkdir` fails because A's directory exists. B's constructed `Workspace` drops; `close()` invokes `clear()` without checking A's new heartbeat or possessing a lease.
5. B enumerates A's record, authenticates it with `recover_owner`, stops A's live test, removes A's worktrees, and removes A's workspace. The receipt is valid for A's test; it does not authorize B to claim A's workspace.

Even without this timing, `refuse_if_in_use` line 267 considers any heartbeat whose numeric PID equals this app's PID to be ours. Different clients accessing one remote machine can have equal local PIDs. Such a client bypasses a fresh heartbeat. A live owner whose heartbeat cannot update for 90 seconds can also be treated as stale; age is not exclusive ownership.

The random per-test record and exact-record-byte barrier are useful, but neither substitutes for workspace/run authority. **Disposition of prior `6a289` P1: unresolved**, not fixed by receipt-gated artifact retirement in this candidate.

Required correction: acquire an exclusive random durable claim before any stale cleanup/create/dispatch; retain and validate its identity throughout cleanup. A failed claimant must never construct a cleanup-enabled owner or invoke cleanup. Distinguish previous-run evidence from the current live run; equal PID must not grant ownership. Retire only the lease owned by this run. Apply the same ownership rule to `clear_leftovers`, not just `open`.

## P2 — lost-fsck cancellation overwrites an explicit unknown-cleanup result

Source: `src/git/reflog/lost.rs:134–148`; `src/git/stream.rs:119–122`; `src/processes.rs:605–618`.

The process service records failed remote stop/reap acknowledgement as `cancelled; remote cleanup unconfirmed: ...`. `stream()` deliberately returns that error even when its stopper is cancelled. However, `unreachable_commits()` stores this result and immediately executes `cancel.check()?` at line 144 **before matching the result**. With cancellation set it returns bare `Cancelled`, losing the unconfirmed cleanup evidence.

Static reproduction inputs: `result = Err("cancelled; remote cleanup unconfirmed: the supervisor did not confirm its atomic stop/reap receipt")`, `cancel.is_cancelled() = true`. The error at line 148 is unreachable; line 144 returns `Cancelled`. This covers a user-cancelled SSH/WSL lost-commit scan whose stop connection fails. It also discards local process-service errors on the same path.

Required correction: propagate execution/cleanup errors before normalizing a confirmed or pre-dispatch cancellation. Prefer a typed cleanup/termination outcome instead of a string classification. Add a pure recording test for cancelled + unconfirmed and cancelled + confirmed results. No remote fixture is necessary to establish this branch.

## P2 — AI cancellation hides native cleanup errors that lack a magic substring

Source: `src/ai/run.rs:263–278`; `src/processes.rs:628–634,672–676`; `src/processes.rs:117–139`.

AI preserves only errors containing literal `unconfirmed`. Every other `Err` becomes `RunError::Cancelled` if the user cancellation flag is set. The process service returns a failed owner finalization as `could not finish owned process: ...`, and a lost service result as `Process service stopped before returning a result`; neither contains that substring. These are not proof of a stopped/reaped target. The resulting `Cancelled` classification conceals cleanup uncertainty. A native finish error can also replace an already accumulated remote `...cleanup unconfirmed...` stopped reason at `processes.rs:634`.

Static reproduction inputs: `processes::run = Err("could not finish owned process: <recorded finish failure>")`, user cancellation true. AI selects line 265 and returns `RunError::Cancelled`. This finding does not assert that a native failure was triggered on any platform; it establishes incorrect handling when the fallible ownership API returns its documented error path.

Required correction: preserve unknown finalization/transport outcomes regardless of cancellation timing. Normalize cancellation only for explicit no-dispatch or confirmed terminal results. Propagate both original failure and cleanup failure when both occur. Pure injected-result/recording coverage should precede any platform fixture.

## P2 — stream formatting/queue errors discard the cleanup acknowledgement

Source: `src/git/stream.rs:91–94,104–120`.

When a line exceeds 1 MiB, `feed()` returns an error, the caller cancels the ticket, waits for it, discards that entire result, and returns only the line-length error. An unconfirmed remote stop or failed native finish is consequently hidden. Likewise, the bounded-queue overflow check at 116–117 returns before inspecting the process service's stopped/cleanup result at 119. The post-completion pending-line `feed(...)?` at 105 also precedes inspection of `result.stopped`.

Static reproduction: a recording stream supplies a >1 MiB line without newline; the ticket's final result is `Ok(Output { stopped: Some("cancelled; remote cleanup unconfirmed: ..."), ... })` or a finalization error. The caller returns only `stream line exceeds 1 MiB`. For the queue variant, set `overflow=true` with the same final result; it returns only `stream consumer exceeded the bounded output queue`.

The stream remains bounded and waits in the first path, but waiting is not proof of cleanup. Return the content/queue error together with any cleanup uncertainty; always inspect the terminal ownership outcome first. Cover these combinations with pure recording outcomes.

## P2 — workspace cleanup falsely succeeds and retires evidence after removal errors

Source: `src/parallel_bisect/workspace.rs:212–224,388–399`; `src/parallel_bisect/runner.rs:62–75`; `src/parallel_bisect/session.rs:233–240`.

`CLEAR` ignores failure of `git worktree remove`, `rm -rf "$b"`, worktree metadata deletion, and `git update-ref -d`, then ends with unconditional `true`. It is invoked through `sh -c` without errexit. Consequently `runner::shell_input` returns success even if removal/ref updates fail. `Workspace::clear()` then retires authenticated endpoint artifacts because `result.is_ok()`, and `close()` reports success so the session emits no cleanup warning.

Concrete static failure injection: make the removal command return nonzero (e.g. a recorded permission error for an inaccessible nested directory) and/or make a ref deletion return nonzero. Shell control proceeds to final `true`; the externally observed exit is zero and the retirement loop runs. No command was actually run to reproduce this: it follows directly from the shell's unchecked sequence and final status.

All owner receipts may correctly prove target termination, so this finding is not a claim that signals target unrelated processes. The incorrect claim is that workspace cleanup succeeded, which authorizes artifact retirement and hides remaining state. Required correction: explicitly check each mutation, retain aggregate failure details, verify remaining workspace/worktree/ref state, and retire evidence only after required removal succeeds. Cleanup failure should remain visible at session completion. The warning text currently promises the next bisect removes leftovers even when ownership remains unknown; avoid that promise.

## What the migration does establish statically

- Watched Git routes local and SSH/WSL commands through `processes::run`; `follow_command` preserves service errors and `Output.stopped`, and `run_watched_recorded` returns the stop reason before considering exit code (`src/git/command/watched.rs:176–211,239–282`). This caller has no newly discovered raw-PID authority or local cancellation overwrite in the inspected path.
- Git streaming has bounded chunks, two bounded pending-line buffers, a 1 MiB unfinished-line cap, and explicit queue overflow. It uses the process service for both local and remote commands. The uncertainty problems above occur at caller result handling, not from reinstating a PID selector.
- AI runs both local and remote tools through the service, bounds stdout/stderr separately, and supplies cancellation/input/timeouts. The above classification gap prevents calling its migration complete.
- `run_owned_test` supplies a random durable record to the remote/native owner (`src/git/command.rs:643–676`). `runner::run` waits for the scoped worker and requires `recover_owner()` before converting a dispatched test into an exited/stopped/timed-out verdict (`runner.rs:91–149`). The exact pre-dispatch `Err("cancelled")` shortcut is currently limited by service code to cancellation before target spawning; other errors propagate. Workspace ownership remains the blocker.
- `clear_proof()` validates all record names before recovering any, rejects `.pid` records, and binds exact bytes. `recover_owner()` reads a bounded record within the workspace, invokes authenticated stop/receipt confirmation, and re-reads the exact bytes before returning. Unknown/changed records abort ordinary clearing. This is an improvement over numeric PID authority; it does not resolve concurrent-workspace claims.
- Retirement is receipt-gated and best effort. Remote standalone streams register a retirement callback; durable bisect owners retain it for workspace cleanup. `processes.rs:677–683` does not use failed retirement to authorize signals. Whether endpoint confidentiality and final native/remote receipt semantics are adequate belongs to the other reviewers, especially the uncommitted marker work.

## Shutdown assessment and limits

`app::after_exit` now orders task cancellation/join, bisect cancellation/join, main process-service shutdown/join, remote shutdown, cleanup-service shutdown/join, then packaged helper retirement (`src/app/mod.rs:953–959`). The main process service waits for its active `execute` tasks rather than aborting them (`processes.rs:266–276`). Remote stop/retire uses dedicated cleanup service capacity (`remote/session.rs:309–358`); it remains available while main execution and remote shutdown occur. This ordering addresses the prior concern about retiring helper artifacts before remote-owned work finishes.

This is a source-level sequencing observation, not a runtime proof of termination or a blanket shutdown pass. `parallel_bisect::clear_leftovers()` spawns an untracked thread (`src/parallel_bisect/mod.rs:46–49`), heartbeat threads are detached (`workspace.rs:104`), and the remote idle-reaper thread is detached (`remote/pool.rs:308–319`). They are not joined by the new ordered path. If deterministic completion of all recovery/maintenance activity is intended, register/cancel/join them or explicitly document that they can fail closed and preserve evidence during shutdown. Main `Service::shutdown()` discards thread-join errors, so a joined return alone must not be described as proof that every cleanup succeeded. No executed behavior of these paths is claimed.

## Required next gates before any bounded owned-child fixture

1. Fix the exclusive cross-process workspace claim and failed-claimant Drop behavior. Preserve and push that completed unit and verify its immutable remote SHA before risky process testing, per AGENTS/plan.
2. Add pure recording state-machine schedules for two apps, equal local PIDs on one remote, stale heartbeat, failed directory creation, lease replacement, and failed claimant drop. Recording must prove zero cleanup/dispatch by a non-owner.
3. Add injected terminal-result coverage for fsck, stream line/queue failures and AI cancellation: no-dispatch, confirmed stop, missing/wrong receipt, native finish failure, remote ambiguity, and lost result. Preserve unknown state and errors through the user-facing result.
4. Record failed worktree/ref/file cleanup and prove it cannot produce success or artifact retirement. Verify all receipt/owner comparisons before destructive calls without relying on shell final `true`.
5. Obtain static passes for the exact same immutable combined source from core and platform reviewers, including marker confidentiality. Existing user-reported all-target and recording gates were not independently rerun or reinterpreted as runtime proof in this review.
6. Only afterward consider explicitly authorized, tiny owned fixtures with direct child/group/Job handles, a single random private workspace, exact helper provenance, no host PID selector, no remote reconnect retry of ambiguous dispatch, and receipt-bound teardown. Start synthetic/recording checks first; preserve unknown evidence. Platform coverage must be reported separately (native Unix, Windows Job/controller, SSH/WSL Python/Perl fallbacks); do not claim any unexecuted platform. EOF, timeout, failed start, ticket drop, descendant-held/closed pipes and shutdown need separate bounded cases. Keep any external process outside the owned fixture entirely untargeted.

No runtime authorization is implied by these prerequisites. Overall status remains **STATIC FAIL** until the prior P1 and caller/cleanup blockers above are corrected and independently re-reviewed on an immutable candidate.
