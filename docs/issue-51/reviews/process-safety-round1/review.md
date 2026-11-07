# Process-safety prerequisite: independent STATIC R1

**Disposition: STATIC FAIL for the complete prerequisite.** Candidate `6a289fd8fe90b50750d639958f70f1c48337b8c4`, branch `t3/safe-process-supervision`, base `43707ea3`. This disposition does not authorize a full suite, live SSH/WSL, or native Windows runtime claims. No supervisor, anchor, endpoint, user command, child fixture, test, build, CI, or external service was launched for this review.

The immutable tree was exported with `git archive` into `/private/tmp/gitcraft-process-safety-review-6a289fd8`. All source references below refer to that exported candidate, never the changing writer checkout. The adjacent `review-source-hashes.json` binds all 39 changed files plus the unchanged Windows Job and safe-reader files examined here. Parent verified author and remote SHA; I did not independently rerun those checks. I read all seven candidate `docs/issue-51/process-safety/*.md` documents and the prerequisite plan `/tmp/gitcraft-51-process-safety-plan.md`. Earlier checkpoint documents contain historical “not yet migrated” statements; the actual candidate code, rather than those historical statements, determines the migration findings below.

Evidence levels: the issues below are **static source/control-flow findings and deterministic interleaving arguments**, not observed runtime reproductions. Reported writer recording gates (8 application + 3 OS; Python 3 + Perl 3) and Windows cross-check are inherited evidence, not independently executed here. The review does not turn compilation or recording checks into a live pass.

## Blocking findings

### B1 — P1: remote stream backpressure prevents the owner from reaching its stop/deadline logic

Python `src/remote/supervisor/owner.py:167-172,217-219` performs blocking writes/flushes on the transport's stdout/stderr from the same thread that checks deadlines, controller EOF, and authenticated clients. Perl `owner.pl:143-146,177-182` does the same. A full transport pipe or stalled SSH channel therefore blocks the sole owner loop; a finite request deadline alone cannot stop the user command, and an authenticated control request cannot be serviced while that thread is blocked. The readiness/ownership output before opening the gate can similarly pin a gated anchor indefinitely.

This is reachable from production cancellation, not only an artificial consumer: `src/processes.rs:587-611` stops polling the transport pipes while it waits for `stop_remote`; the remote supervisor can fill the pipe and block writing before it reads that stop connection. The control call times out and correctly reports unknown cleanup, but neither that reporting nor a later local transport kill proves a bounded remote stop on a stalled network. No claim is made that every cancellation takes this interleaving.

Both post-termination drain loops also write/flush synchronously (`owner.py:225-242`, `owner.pl:190-202`). The nominal 300 ms drain bound does not cover those blocking calls. They can prevent publishing the atomic terminal receipt even after group termination, turning a completed stop into indefinitely unavailable recovery evidence. In pooled non-stream mode the large final payload is emitted after finishing the group; that particular final write does not leave an active command, but can still stall the broker/transport.

**Correction:** make all transport emission nonblocking/deadline-driven with a bounded queue and an explicit overflow/disconnect error. Continue prioritizing EOF/deadline/control while output is backpressured. Finish the owned anchor and publish its durable terminal receipt independently of final output delivery. Do not implement a detached, unjoined blocking writer as the replacement. Retain honest partial-output/error reporting.

**Recording-first regression:** inject a transport sink which repeatedly reports would-block, together with controller EOF, deadline expiry, and a valid stop request. Assert owner termination/reap and receipt publication happen without a successful output write, and no further signal occurs after retirement. Exercise both Python and Perl paths, including handshake and post-stop drain.

### B2 — P1: pooled request writes have no cancellation/deadline while blocked

`src/remote/shell.rs:209-215` calls synchronous `write_all` and `flush` before entering `protocol_line` and its deadline/cancellation logic. The same unbounded blocking write pattern exists in initial setup (`133-144`) and idle ping (`175-185`). `supervisor::write_request` (`src/remote/supervisor.rs:235-237`) correctly treats partial/flush failures as ambiguous, but has no way to make a blocked write return. Requests can contain up to 64 MiB of stdin before base64/JSON expansion (`supervisor.rs:193-215`). A live SSH/WSL process which stops draining its input can pin this worker indefinitely, even after cancellation or expiry; its owning `Shell` cannot be dropped/finished until that call returns.

**Correction:** use cancellation-aware bounded asynchronous/nonblocking writes or an explicit owned transport writer whose blocked write can be interrupted by retiring that same owned transport. Cover setup, ping, and dispatch. Preserve the current rule that any attempted/partial/flush-error dispatch is unknown and must never be replayed. A timeout which merely abandons a writer thread is insufficient.

**Recording-first regression:** a pending/would-block writer must allow cancellation and absolute deadline to finish the request, close its lease/retire its owned transport, and return ambiguous delivery without retry. Include a large request, zero-byte pending setup, and pending flush.

### B3 — P1: native helper may exit on an endpoint error before its target is stopped/reaped

The native helper submits the real command at `src/remote/native_owner.rs:365-372`. It then has post-submit error returns for accepted-socket timeout setup (`384-389`) and listener failure (`404`), in addition to normal fallible control flow. Those paths do not explicitly cancel and await `ticket`/Service retirement. `Ticket::drop` only requests cancellation (`src/processes.rs:144-147`); it does not join. The early helper entrypoint immediately invokes `std::process::exit` (`src/main.rs:10-13`, `native_owner.rs:437-443`). Static Service storage is not a cleanup barrier at process exit.

On Unix, the target's C anchor has neither a controller-EOF lease nor a parent-death mechanism: it waits for its child and then pauses (`group_anchor.c:59-67`). If the helper exits before its Service event loop handles the dropped ticket, an active target/anchor can remain without a live authenticated endpoint. On Windows, closing the helper's Job handles is a different kernel fallback; it does not establish the Unix behavior or justify a universal successful-stop claim. No runtime endpoint error was induced here.

**Correction:** install an explicit post-submit owner scope/finalizer. Every helper exit after submission must cancel and await the same ticket/Service owner, preserve any cleanup uncertainty, and join the helper Service before returning to the process-exit entrypoint. Avoid fallible early-return paths which bypass that finalizer. Do not publish a terminal receipt for an unconfirmed finalization.

**Recording-first regression:** inject accept failure and accepted-socket setup failure immediately after submission. Record cancellation, final owner result, and join before helper return; inject refused cleanup and verify no stopped receipt. Test controller EOF before and during submission without launching the real helper yet.

### B4 — P1: bisect workspace ownership is only in-process; a losing process can clear the winner

`Workspace::open` uses `claim` backed only by this process's `ACTIVE` mutex (`src/parallel_bisect/workspace.rs:42-43,70-95,391-405`). The heartbeat check is separate from creation. A valid static interleaving is:

1. Two app processes claim locally and both pass the heartbeat check on an absent/stale workspace.
2. Both clear the old directory before either starts a heartbeat.
3. Process A creates the private directory and begins setup; process B's exclusive `make_private` fails because A created it.
4. B returns the error from `open`, dropping its already-constructed `Workspace`. Drop invokes `close`/`clear` (`233-247`). B can now read A's new `.owner` files, authenticate and stop A's real running tests, remove A's worktrees and directory, and delete its refs (`191-207,363-389`). If A has not yet recorded tests, the directory can be removed beneath A's setup instead.

The record/receipt checks establish which endpoint a record describes. They do not establish that this claimant owns the workspace currently being cleared. In addition, `refuse_if_in_use` considers a fresh heartbeat “ours” solely when its PID equals this app's PID (`256-264`); two client machines can have the same app PID while operating on one SSH repository. That bypasses the fresh-heartbeat refusal. This is not an allegation of PID-based signal selection: the signal API now uses authenticated endpoints, but those endpoints can belong to another active bisect.

**Correction:** establish an exclusive cross-process, random run/lease claim on the target machine covering recovery, creation, dispatch, heartbeat and cleanup. A failed claimant must only release its own claim, never auto-clear a directory another claimant created. Bind the workspace identity/records to that claim, revalidate it before destructive cleanup, and serialize late record publication against clearing. A PID equality exemption is not a substitute for a unique run capability. Keep legacy/unknown record preservation.

**Recording-first regression:** deterministic two-claimant schedules for the four steps above, equal PIDs from different clients, failed creation and Drop, stale-claim recovery, replaced claim, and late owner publication. Assert the losing claimant sends no stop request and performs no deletion of the winning workspace. Do not use two real Git/app processes as the first test.

### B5 — P1: Windows primary-thread resume trusts a stale snapshot ID without validating the opened handle

The candidate retains `crates/gitcraft-os/src/process_job.rs:69-93`: enumerate a thread snapshot, match the entry's owner PID, `OpenThread` by that numeric thread ID, then `ResumeThread`. The process is initially created suspended and assigned to its private Job, which prevents it from racing by spawning its own descendants. It does not prevent external termination (including an enclosing Job or another cancellation source) between the snapshot and `OpenThread`. If that thread ID is reused in that interval, the handle opened can refer to a different process's thread; no queried handle-owner identity is checked before resuming it.

**Correction:** retain the actual primary-thread handle from process creation, or query the opened thread handle's owning process and compare it with the retained child process identity before any resume. Acquire the rights needed for that identity query; once validated, retain the handle through the resume. Refuse and retire the owned child/Job on mismatch. Do not repeat a numeric lookup after ownership is lost.

**Recording-first regression:** inject a snapshot entry for the child but an opened handle which reports a different owner. Assert no resume call. Include thread-open/query errors, external child death, resume failure, and Job-assignment failure. Native Windows runtime remains unvalidated.

### B6 — P2: migrated callers can hide cleanup uncertainty behind ordinary cancellation

`src/git/reflog/lost.rs:136-148` receives the stream result, then calls `cancel.check()?` before inspecting it. For a cancelled scan whose streaming result is `Err("...remote cleanup unconfirmed...")`, the visible result becomes just `Cancelled`; the ownership uncertainty is discarded. The stream itself preserves that error (`src/git/stream.rs:119-122`), so the masking is in the caller.

AI execution preserves errors containing the string `unconfirmed`, but maps every other `Err` to cancellation when its user token is cancelled (`src/ai/run.rs:263-278`). The Service's owned stop/reap failure is `Err("could not finish owned process: ...")` (`src/processes.rs:618-624,664-666`), without that substring. A refused local group/Job stop can therefore be presented as ordinary AI cancellation.

**Correction:** preserve owner/cleanup failures before converting a normal acknowledged cancellation into the UI cancellation result. Prefer a structured cleanup disposition, or at minimum a centralized complete classification of Service errors; do not depend on an incidental substring scattered across callers. A cancelled user request is not evidence that cleanup succeeded.

**Pure regression:** pass synthetic stream/Service results with cancellation already true: confirmed ordinary cancellation, remote unconfirmed stop, refused local finish, missing ownership, and unrelated execution error. Assert cleanup uncertainty reaches the caller/UI. No child process is needed.

## Additional completion gaps already acknowledged in the candidate

**C1 — P2: unbounded artifact retention/input persistence.** Every remote request creates a new private owner directory, socket and plaintext input file (`owner.py:95-107`, `owner.pl:96-102`), and no retirement unlinks them after finishing (`owner.py:240-247`, `owner.pl:201-205`). This applies even to normal pooled reads which have no durable bisect record. Repeated bounded requests still produce unbounded aggregate disk usage; a request's stdin remains on disk after the command ends. Native endpoints also retain their per-run directory/receipt, and Unix installs its packaged helper into a never-retired random directory (`unix_anchor.rs:23-42`). Candidate docs explicitly mark retirement open. Preserve receipts while recovery still needs them, but unlink consumed plaintext input promptly and retire socket/temporary artifacts using retained directory/record identity after acknowledged cleanup. A global age/PID scan must not replace identity-safe retirement. Verify no live owner or unrelated swapped path is removed.

**C2 — P2: recovery proof still scales into command-line limits.** `workspace.rs:194-207,344-358` collects every owner pathname and full record JSON; `runner::sh_alias:40-49` embeds all of them in a single Git alias command-line argument. Limits therefore apply to a single argument as well as the total argv, with shorter native Windows limits. Long bisects retain one record per test (`session.rs:402-405`). The OS can reject clearing before it reaches the shell. The failure preserves the workspace rather than authorizing mutation, which is the correct safety default, but this prevents complete recovery for sufficiently large valid record sets. Use a bounded streamed/private retained proof artifact bound to the exact claim/record set, not argv. Cap malformed record listings/record count and preserve overflow as uncertainty. The candidate documentation already identifies this gap; the writer's later unpreserved work is not covered by this review.

**C3 — Windows stop-completion ordering is a proof gap, not a live result.** `ProcessJob::terminate` only invokes `TerminateJobObject` (`process_job.rs:35-38`). `OwnedChild::finish` waits for the direct child (`owned_child.rs:135-137,170-183`); it does not wait for Job membership to reach zero. The native helper publishes a “stopped” receipt immediately from that completion (`native_owner.rs:408-426`). No Job-wide completion/quiescence check is shown. If the receipt contract requires all members to have completed exit/released resources before worktree deletion, the candidate does not establish it. Add a bounded Job completion/accounting wait or explicitly prove the narrower contract is sufficient, with uncertainty on deadline; handle inherited pipe handles and enclosing Jobs. This review does not claim an observed native Windows descendant executing after termination. It also does not claim Unix group SIGKILL plus waiting the anchor reaps every nonchild descendant: the implemented reaping authority is the direct owned anchor.

## Positive static findings and nonfindings

* The production `STOP_SCRIPT` selectors and old raw PID termination APIs are absent from the inspected candidate. Search hits for remaining signals lead to private owned group/Job termination, Python/Perl owned-anchor cleanup, or diagnostic/test observations. Display-only/exact-PID diagnostics are not being relabeled as production termination selection.
* Unix `OwnedChild` can only be constructed by spawning; normal commands establish a new group, execute the packaged C live leader, and retain its direct Child until final group signaling. Status observation uses a private pipe, not `try_wait` on that anchor. `finish` records its terminal result and never repeats numeric signals after a reaping attempt (`owned_child.rs:21-77,94-141,155-183`; `group_anchor.c:39-67`). The specific normal-path stale-PGID interval from the old design is addressed statically. This is not a runtime Linux/macOS pass.
* Direct-controller mode is actually configured by the packaged native endpoint; inspected ordinary watched Git, streaming Git, AI and fsck callers go through the shared Service/owned execution. `Stopper` holds a cancellation token rather than a PID (`stream.rs:12-21`). No production callback/ambient-token mismatch proving a normal history Reader leak was established or assumed.
* Initial Windows target creation is suspended, the unnamed non-inheritable Job is configured for kill-on-close and assigned before resume (`owned_child.rs:38-64`; `process_job.rs:45-68`). These are meaningful safeguards despite B5/C3 and the absence of native runtime evidence.
* The remote anchor is a directly forked child, acknowledges a new session before user release, remains unreaped through TERM/KILL, then is reaped. There is no host process enumeration to select its group. A pre-establishment failure targets only the still-owned gated direct child. Refused stop is not intentionally turned into an atomic receipt (`owner.py:70-88,112-172,220-242`; `owner.pl:72-90,108-146,187-202`).
* Endpoints require exact version, nonce, constrained random path, random capability and complete receipt fields; debug omits the token. Existing streaming marker parsing is first-owner-only/exact-nonce and removed before user output callbacks. User code is gated behind initial ownership emission, so later command stderr cannot replace the established owner (`supervisor.rs:38-107,110-175`; `native_owner.rs:101-169,223-275`). This does not sandbox arbitrary code already running as the same OS user or protect against a compromised same-user account.
* The framed session checks start/owner/receipt/header identities and output lengths. Partial or flush-failed writes never authorize retry. A passive operation's timeout closes the existing controller channel instead of making another connection (`shell.rs:209-267`; `session.rs:159-196`). B2 concerns blocked writes, not a discovered accidental replay.
* Dedicated cleanup Service capacity is separated from regular processes and clears ambient cancellation (`processes.rs:372-383`). Streaming callbacks use a bounded nonblocking channel and surface overflow (`stream.rs:30-67,116-122`); line retention is bounded. Main Service capture/output/drain caps and select-based stdin flushing are present. Blocking user callbacks remain outside the stream Service loop, though watched progress notification runs synchronously there and should remain cheap.
* Recovery refuses legacy PID records, validates the full record-name set before controlling endpoints, requires an exact atomic receipt, rereads the owner record, and compares record set/content before deletion (`remote/mod.rs:40-62`; `workspace.rs:320-389`). These help against stale/replaced records; they do not solve B4's missing workspace claim serialization.
* Missing/unavailable receipt is reported as unknown and normally preserves workspace. Deliberate `setsid`/process-session escapes, supervisor death, and externally enclosing Windows Jobs remain disclosed limitations, not claims of unconditional containment.

## Required validation sequence and limits

1. Correct B1–B6 and resolve C1/C2 plus the required Windows completion contract. Preserve each completed implementation with the required author, push and verify its remote SHA. Review the new immutable combined candidate; this report cannot cover uncommitted retirement/proof/Windows edits.
2. Run only pure synthetic/recording cases first for stalled read/write/flush, failure-to-acknowledge, refusal, wrong/missing identities, duplicate finalization, stale thread handles, native helper error scopes, and cross-process claim interleavings. The existing recording tests are useful but do not currently exercise these blocking/multi-claimant cases. This reviewer executed none.
3. After fixes and renewed scoped static acceptance, narrowly owned runtime fixtures can validate: one Service-owned Unix anchor with an explicitly created child/grandchild and closed/inherited pipes; cancellation before/after readiness, deadline and normal completion; exact packaged native helper with controller EOF and injected endpoint error; Python and Perl each through an owned local transport with paused output reads and finite controls; bisect with an exclusive unique scratch claim and interrupted app controller; native Windows suspended child/Job ordering, external-owned-child exit, inherited handles and Job quiescence. Every fixture must retain owned child/Job handles or a verified live anchor, have its own cancellation/deadline, and never recover a signal target using `ps`, `pgrep`, stale pid files, or broad cleanup. Inspect the fixture itself before execution. Do not run arbitrary hostile repository commands to validate it.
4. Treat direct-child status, command-group termination, terminal receipt, output delivery, artifact retirement and filesystem cleanup as distinct assertions. Receipt absence or cleanup refusal must retain recovery evidence and surface uncertainty. A passing isolated local anchor fixture would not validate remote supervisor, SSH, WSL, or Windows behavior.
5. Existing inherited process tests remain pending. This report neither ran nor authorizes them wholesale, nor any tests merely because they now compile or route through the new owner. Live SSH/WSL and native Windows/Linux/macOS gates, broader suite, process crash/escape limits, packaging on supported targets and CI remain unvalidated. Full-suite/CI authorization requires the later exact combined candidate and the user's established sequence.

The internals transport/routing review remains separate at `/tmp/gitcraft-51-internals-core-r1.md`; its findings were not mixed into this process-safety disposition.

## Selected immutable SHA-256 hashes

| Candidate file | SHA-256 |
|---|---|
| `crates/gitcraft-os/src/owned_child.rs` | `dd0bc52e2096b0df240e28d8e6d244f6e065c8968f05575f9a24f7aa13f4bd61` |
| `crates/gitcraft-os/src/group_anchor.c` | `4125f07e3721afae1acedd08ae8ec8c00c70b0ccb09630a9fbdd24f9fafafad0` |
| `crates/gitcraft-os/src/unix_anchor.rs` | `d01a11b5cc6c838a0b65e78ee8b476c78f3bedb89afe0587fd370502d6f7b325` |
| `crates/gitcraft-os/src/private.rs` | `d8ede970f38a8f2ada3ada93a020d216ee7e1f3529a1d1b388182e4a25764e27` |
| `crates/gitcraft-os/src/process_job.rs` | `15496fff34202d2b7eae1c68eda8a573d1a73f66dfeac86420d1761f4ce99e2d` |
| `src/processes.rs` | `0907d01f177409894a2d3ffa87b53482e35e45c3e7621a09f9c34bb6507fce85` |
| `src/remote/native_owner.rs` | `950be78f100c8b78d0243ca0f13e265b608e88cc8760d6bdcf839a0d177fc009` |
| `src/remote/supervisor.rs` | `c6f3e690032c11c0f5511c9ce6d52ddf0fc40fb9a30d4229396aaa65b2de7325` |
| `src/remote/supervisor/owner.py` | `aff54ba9dbbf4558d57bdec8b74f64e42ed650a33e0515d1f5f401472e855670` |
| `src/remote/supervisor/owner.pl` | `1ec334bbe6737c8e26d1de557ee0b5f25dff7be137de551965d8bccbeeb91774` |
| `src/remote/shell.rs` | `300dde1cf2566f8fcfdfc382918e63f7199c419b42264b700702b8d18187256a` |
| `src/remote/session.rs` | `fc16c07050eec71b5dda41ad71a3defdc84bf8e0edd842766545cedeb52c9edc` |
| `src/remote/mod.rs` | `4968234b4063f2c2ee50b5e1dbdd741fcefd25ee32797548d3fa905034bb9b8e` |
| `src/parallel_bisect/workspace.rs` | `379ad5bbea32a64b29686e1920fa7f54492a832addfab069e821b2d7ca03b133` |
| `src/parallel_bisect/runner.rs` | `9229cbdb754e62cc8c6aa0ed234f2fd861a38eb7338921e664912bb90d1171e7` |
| `src/parallel_bisect/session.rs` | `ec2dfe73dd460a70840e17884d12dd4b9601988288d4d6bc58402d8e190ed220` |
| `src/git/stream.rs` | `2c8ff49a9f1d1484b383dcae08eac5523a2552d4fd8b9ab59da265741c580f82` |
| `src/git/reflog/lost.rs` | `0bada1576027363f228c7dd3faaef72527edf2a5991ae13612243929a396a4b2` |
| `src/ai/run.rs` | `4690f091e0b7a530c7a395266a1e96f5637683ace42982cde44da0534de1572d` |
| `src/main.rs` | `b653320a3781bdb6a4e9c7cc5111eda703f6de47534a0960181756d6c6300153` |
