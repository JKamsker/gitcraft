# Native process-safety static review R2

**STATIC FAIL. This is not authorization for runtime fixtures or the full suite.**

Candidate **bc2521c6a8422571e67ae2da00e381bde48f6de5**, base **43707ea3** (full base in manifest). Reviewed only immutable `git show` material from `/Users/jonas/.t3/worktrees/gitcraft/t3-dcbba117`; copied candidate files to `/tmp/gitcraft-process-native-r2-source`, verified against Git objects, and recorded blob/SHA-256 identities in its `manifest.json`. The moving checkout and uncommitted source were not reviewed. No commands that launch the implementation, tests, helpers, children, signals, compilers or CI were run. Official API documentation was consulted. This reviewer owns native Unix anchor/C helper, Windows launch/Job, native endpoints/receipts/retirement and their Service integration; other reviewers own interpreter backends, transport, callers and workspace policy.

## Prior-finding disposition

| Prior 6a289 finding | Exact bc252 disposition |
|---|---|
| Windows Job termination acknowledgement waits only direct child | **Unresolved**, N1 below. The recording launch driver does not add Job-empty proof. |
| Snapshot TID → OpenThread → Resume without opened-handle owner validation | **Unresolved**, N2 below. Retained process HANDLE assignment is an improvement, but resume still opens an unvalidated numeric TID. |
| Native helper post-submit `?`/Err can exit before its Service has reaped | **Structurally corrected for ordinary Rust returns.** `native_owner::entrypoint:445–456` now runs Service shutdown/join, cleanup-Service shutdown and helper retirement after every `helper()` return; `Service::shutdown:345–354` joins its thread, and dispatch shutdown drains tasks rather than aborting them (`processes.rs:265–276`). This does not protect against the parent's forced termination of that helper: **new N3**. No runtime/error injection was performed. |

## Blocking findings

### N1 — P1: a stopped receipt can precede Windows descendant termination

`crates/gitcraft-os/src/process_job.rs:33–35` only calls `TerminateJobObject`. `owned_child.rs:148–150,183–196` treats that return plus `Child::wait()` for the direct child as complete cleanup. Neither this path nor Service checks that the rest of the Job has terminated. `processes.rs:672–676` returns after `finish`; `native_owner.rs:416–434` then writes a durable `state:"stopped"` receipt. Descendants with closed output pipes bypass the drain as well.

Counterexample: direct child has already exited, while a Job descendant remains with pending I/O. TerminateJobObject initiates termination; waiting the already-ended direct child is immediate; a terminal receipt is published and worktree cleanup may proceed before that descendant's termination completes. Holding/closing the Job handle preserves termination authority but is not a completion acknowledgement.

Microsoft describes Job termination as TerminateProcess for each member; externally called TerminateProcess is asynchronous and pending I/O can delay final process exit. [TerminateJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-terminatejobobject), [TerminateProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess).

Required correction: retain the Job while observing actual whole-Job completion using an appropriate handle-bound accounting/completion design, bounded by a cleanup deadline. A direct-child exit or successful terminate request must not become `stopped`; deadline/error must remain Unknown with ownership/recovery preserved. Do not assume a generic wait on the Job means empty: Microsoft documents a particular time-limit case for Job signaled state. [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), [Job accounting](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information).

Recording prerequisite: direct-child-exited + descendant-pending + terminate-success must produce no receipt until a separately recorded complete-Job event; test completion timeout, query error and retained-handle/Job closure order. The current recording Control only records “signal” then “reap anchor”; it cannot catch this.

### N2 — P1: resume can act on an unrelated reused thread ID

`process_job.rs:63–89` takes a system thread snapshot, matches `th32OwnerProcessID` to the retained child's PID, opens the snapshot's numeric TID with only `THREAD_SUSPEND_RESUME`, and immediately calls ResumeThread. There is no validation of the **opened thread handle's** owning process/lifetime.

Static race: the suspended child's primary thread is terminated externally after snapshot capture; its TID ceases to designate that lifetime and can be reused. OpenThread can open the new thread, and ResumeThread changes that unrelated thread's suspend state. Suspension of the intended process does not prohibit its external termination. Assigning the original process HANDLE to a Job does not authenticate a subsequently opened thread.

Thread identifiers are valid only until their thread terminates, whereas retained handles bind the object. GetProcessIdOfThread requires query rights, which this path does not request. [Thread identifiers and handles](https://learn.microsoft.com/en-us/windows/win32/procthread/thread-handles-and-identifiers), [GetProcessIdOfThread](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocessidofthread).

Required correction: preferably retain the primary-thread HANDLE from process creation through assign/resume. If snapshot discovery remains, prove the opened thread belongs to the actual retained process lifetime before any resume, including process-termination/PID-reuse boundaries; comparing only a stale snapshot row is insufficient. Fail closed without touching the opened foreign thread. Keep Job assignment before resume.

Recording prerequisite: snapshot says owned TID, open returns a different-owner/lifetime handle; expect zero ResumeThread calls and only handle-based cleanup of the retained child/Job. The new `suspended::launch` driver correctly orders its callbacks but mocks `resume` as one opaque success/failure, so its four recorded branches do not cover production thread selection.

### N3 — P1: unknown native stop causes the parent to kill the only Unix owner

`native_owner.rs:231–234` configures the packaged helper as `native_controller`, with its control stdin held open; its in-process Service starts the user command in a **separate group** (`OwnedChild::start:50–52`). Native helper detaches itself (`native_owner.rs:302`, `private.rs:104`). The parent Service calls endpoint stop on cancellation, but **after a stop error/Unknown it still calls `child.finish()`** (`processes.rs:604–635`). For controller ownership, finish calls `Child::kill()` on that helper itself (`owned_child.rs:162–165`). The held `input` is not closed before this kill.

The helper is the supervisor/owner, not just an SSH transport. Killing it bypasses `entrypoint` shutdown and destroys the Service that retains the target anchor. On Unix, the target has a different group and can survive indefinitely without its controller; a quiet command need not exit merely because its output reader disappears.

Concrete static scenario: valid local helper is running a long command. An unauthenticated loopback connection keeps the helper's main thread inside its bounded-size but per-read-timeout `read_until` (native_owner.rs:390–403). Parent cancellation sends a real request on another connection, but helper has not accepted it; the parent's stop response times out. Parent immediately kills the helper while target deadline has not expired, rather than closing stdin so its EOF monitor cancels the target (`343–355`). A missing handshake, socket failure or other unconfirmed acknowledgement takes the same unsafe owner-retirement path. This scenario was **not executed**.

Required correction: distinguish native supervisor ownership from disposable transport ownership. On cancellation, deliver EOF/control cancellation while retaining the native helper; let its own Service finish and publish the receipt. Unknown must preserve the live owner and recovery, not authorize killing that owner. A bounded return may hand ownership to a durable cleanup manager, but it must retain the helper/lease until proof or a truthful unrecoverable state. No PID fallback.

Recording prerequisite: actual Service branch with stop_remote Err/timeout/missing endpoint must record control EOF/retained native owner, **zero force-kill of the owner while target cleanup is unconfirmed**, and no terminal receipt or workspace removal. Also inject ordinary post-submit helper errors to verify the new shutdown/join path separately.

### N4 — P2: native Windows terminal receipts reject valid native exit codes

`native_owner.rs:425–429` serializes `ExitStatus::code()` without conversion into `Receipt.code: i32`; `Endpoint::confirms:129–130` accepts only 0..255. Windows native termination values are DWORDs, including application codes above255 and unhandled-exception codes. Thus a fully completed native command exiting256, or with an NTSTATUS crash code represented as a negative i32, produces a terminal receipt that its own stop/recovery/retire reader permanently refuses. Once helper exits, the now-dead control endpoint cannot repair this; durable cleanup remains Unknown unnecessarily.

The 8-bit truncation in Rust's ExitStatus documentation is explicitly Unix-specific; the Windows extension represents the native u32 status. [Rust ExitStatus](https://doc.rust-lang.org/std/process/struct.ExitStatus.html), [Windows exit codes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess).

Required correction: define a platform-aware status representation or a separate normalized receipt result while retaining the full native outcome. Cleanup confirmation must depend on ownership/proof, not an accidental POSIX exit-code restriction. Recording-only receipt roundtrips should cover0,255,256 and0xC0000005 with exact endpoint identity; malformed status encodings must still fail closed.

### N5 — P2: native control “timeouts” are per-read, not an overall cleanup deadline

`Endpoint::stop:148–164` configures a 5-second socket read timeout then calls `read_until` with a4097-byte cap. A peer sending a byte before each timeout can keep that blocking cleanup worker occupied far beyond5 seconds (potentially hours before reaching the byte cap). The helper accepts unauthenticated clients with a100ms per-read timeout and2049-byte cap (`390–403`); a drip-fed line can hold the main control/receipt loop for minutes. This also supplies an ordinary source-level trigger for N3. Service awaits these blocking callbacks and shutdown joins them; no outer absolute deadline bounds them.

Required correction: enforce one monotonic deadline across connect, full request/response framing and authentication. Reads/writes must use remaining budget, or nonblocking polling with an absolute deadline; close invalid/slow clients without delaying receipt publication/other authenticated stop clients. Retain fixed byte and client bounds. Recording prerequisite: arbitrarily many partial reads/timeout resets cannot advance past the hard budget; receipt/owner state must stay conservative when budget expires. No socket/process runtime was executed here.

## Native paths that are structurally improved

- Unix ordinary execution has a real live group leader: standard Command applies cwd/env/stdio/group, pre_exec execs the packaged target C image with literal argv (`unix_anchor.rs:108–150`), and C verifies its own group, forks/execs the user process, directly waits for that child, reports a private status and stays alive (`group_anchor.c:39–67`). User child closes control FD and the exec-failure writer is close-on-exec. Rust does not reap the anchor when observing user status. Final killpg occurs before the single anchor wait, and repeated finish uses cached terminal state (`owned_child.rs:141–154`). No host snapshot supplies Unix signal targets in these paths. This addresses the old reaped-PGID interval structurally, subject to future native execution/error fixtures.
- Unix helper image installs in a random private directory with create-new/no-follow executable file; private status writer is reserved above fd2 and allocations occur before pre_exec. Build-time target image avoids a runtime compiler/interpreter prerequisite. Final artifact retirement uses the exact stored image path after Service shutdown, without a directory sweep or guessed PID. Failed installation/abrupt death may retain files; deletion failures are best effort, not cleanup proof.
- Windows uses CREATE_SUSPENDED and assigns the **retained process HANDLE**, removing the old OpenProcess-by-PID assignment race. Generic launch driver runs spawn→assign→resume, with direct retained-child abort/reap on assignment failure and Job close before child abort on resume failure (`suspended.rs:14–31`). This does not resolve N1/N2.
- Native endpoint directory/capability are created before target submission, with cryptographic random IDs, local-only TCP, strict version/nonce/token/path validation, JSON unknown-field rejection and size caps. Unix directory mode0700 and Windows current-user protected DACL are implemented in `private.rs`. Durable record is exclusive-created before target submission; failed/partial startup has no user command yet. Handshake precedes user output. Terminal receipt is fsynced to a private create-new file then renamed, and recovery confirms exact endpoint identity.
- Native retire requires a valid receipt before unlinking fixed `terminal` and exact private directory; it does not signal or scan. Unknown receipt/endpoint paths fail before deletion. These path-based operations do not establish tamper resistance against malicious same-user replacement of the private directory; that stronger threat boundary was not proven here. Partial retirement can leave artifacts. Workspace-level proof/record handling belongs to the content reviewer.
- Service owns noncancelable task completion, output/input caps, bounded drain, dedicated cleanup capacity and helper retirement ordering. However synchronous OS finish/wait and native socket framing still require boundedness/error validation; neither reported all-target check nor recording tests establish kernel/platform runtime behavior.

## Next verification gate — still static/recording only

Fix N1–N5 and preserve/push the corrected immutable candidate. Re-review the real production branches with recording backends covering Job membership completion, opened-thread identity, native-owner retention after unknown stop, full Windows receipt codes and absolute protocol budgets. Include error paths and zero signals/resumes against foreign/stale handles; counters must demonstrate no receipt before complete cleanup. Existing mocks asserting callback order alone are insufficient.

Only after all scoped reviewers give the corrected candidate a static pass should a separately agreed narrow fixture gate be considered: owned local target plus descendant and separately owned canary; canary observed only through its retained handle; no host selectors/watchdogs; readiness/stop/complete receipts; actual EOF-before/after-handshake, blocked I/O, output limits and failed-launch cleanup. Native macOS/Linux and Windows need separate platform gates, especially Darwin group/zombie behavior and Windows pending-I/O/Job/thread semantics. Deliberate setsid/daemon escapes, supervisor crash, external enclosing Windows Jobs, temp-root changes and kernel-uninterruptible operations remain explicit limitations requiring stronger containment or Unknown recovery. SSH/WSL/interpreter transport validation is outside this review.

All candidate docs correctly call the checkpoint WIP and disclose missing native runtime gates. Their reported8 app+4 OS recording tests, Python5/Perl8 gates and all-target check are **writer-reported evidence read from immutable docs**, not execution by this reviewer. This review ran zero tests. Full suite and CI remain held.
