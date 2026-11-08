# Process safety: remote STATIC R2

**STATIC FAIL**, exact candidate `bc2521c6a8422571e67ae2da00e381bde48f6de5`. Previous R1 target: `6a289fd8fe90b50750d639958f70f1c48337b8c4`.

Scope: Python/Perl remote supervisors, Shell bounded input/output, authenticated protocol/lifecycle and endpoint retirement, plus disposition of earlier findings within that scope. Native Unix/Windows ownership/endpoints and bisect/caller/shutdown findings remain with the separately assigned reviewers. This report does not certify those areas.

Evidence: immutable `git archive` snapshot `/private/tmp/gitcraft-process-safety-remote-r2-bc2521c6`; immutable Git diff against R1; source/control-flow inspection and synthetic interleaving reasoning only. All citations below were checked against that exported tree. No code, supervisor, endpoint, anchor, child fixture, test, build, CI or external service was executed. Writer-reported recording/compile gates (8 app + 4 OS, Python 5, Perl 8 and all-target check) are inherited evidence, not independent executions or runtime passes. The writer's uncommitted marker hardening is excluded. Source hashes are recorded below and in the snapshot's `review-source-hashes.json`.

## Findings

### R2-B1 — P1, R1 B2 remains open: Shell writes can block before cancellation/deadline handling

`src/remote/shell.rs:133-144` writes/flushes all setup before starting the connect deadline; `176-185` writes an idle ping before its read timeout; `204-215` writes/flushes a full request before entering cancellation/deadline-aware reads. The new retirement ACK adds another synchronous unbounded write at `252-258`. `supervisor::write_request` is still just blocking `write_all(...).and_then(flush)` (`supervisor.rs:241-243`). A transport which remains alive but stops consuming stdin can pin the worker indefinitely, including with a <=64 MiB stdin payload expanded into JSON/base64. The supervisor's request deadline does not bound an incomplete request or the local pipe write.

Use a cancellation/deadline-aware writer and retain exact transport ownership through cancellation and writer shutdown. Cover setup, ping, request, flush and ACK; do not abandon a blocked writer thread. Preserve ambiguous-delivery/no-retry behavior. Pure recording cases must include repeated would-block, partial write followed by cancellation, and pending flush. No live stalled transport was launched here.

### R2-B2 — P1, new: a retirement ACK can coalesce with the next request and kill the pooled broker

`Shell::run` writes `A <nonce>\n` and returns without receiving a response/fence (`shell.rs:250-266`). The same shell can immediately be reused for the next JSON request or ping. Byte-stream pipes/SSH do not preserve the write boundary. A legal read can therefore return this single chunk:

    A oldnonce\n{"nonce":"nextnonce",...}\n

Python's broker accumulates through the first chunk containing a newline and then requires exactly one newline (`owner.py:290-303`). Perl similarly requires the whole received buffer to match one line (`owner.pl:249-256`). Both reject the coalesced ACK + request as pipelining and exit before servicing the next request. The previous command has completed; the following user operation is lost/unknown and deliberately not automatically replayed. An ACK plus a ping has the same problem.

Use an incremental bounded line reader that retains bytes after the first complete frame, or add an ACK response and consume it before returning the shell to the pool. Account for arbitrary split/coalesced reads, ACK + request, ACK + ping and rejected/wrong-nonce ACK. Test the actual broker frame reader, not only isolated string writers. This is a source-level byte-stream schedule proof, not an observed live failure.

### R2-B3 — P1, new: successful stream output can be silently truncated by the finite final forwarding drain

After observing a normal exit code 0, both backends finish the group, drain captured pipes, and flush queued stream output only until one shared ~300 ms deadline (`owner.py:241-283`; `owner.pl:208-242`). If any `Forwarder.pending`/Perl pending bytes remain, the code still publishes the original code 0 and returns 0. Overflow from adding post-stop drain bytes is ignored (`owner.py:267-268`; `owner.pl:232-233`). A slow but healthy transport can receive a prefix of the output, followed by successful transport exit; the caller has no signal that the remainder was dropped.

The static counterexample is a command which exits 0 after producing output faster than the transport can deliver it, leaving a nonempty forwarding queue after the finite final drain. This is below the configured total capture bound and does not require a malicious command or a disconnected consumer. Normal Git line streams or AI responses can therefore become false successful partial evidence.

Keep command exit/ownership receipt separate from output-delivery completion. Continue bounded forwarding until a defined delivery deadline/EOF policy, then return an explicit partial-output/transport error if queued bytes remain, a forwarding error occurred, or an add overflowed. Do not turn partial output into success. A durable stopped receipt should still be publishable independently of whether output delivery succeeds. Recording cases must exercise the run-level terminal disposition with code 0 and undelivered bytes, including a healthy slow sink; the current primitive Forwarder test does not check that disposition.

### R2-B4 — P1, R1 B1 partially fixed: ownership handshake emission still blocks before nonblocking mode

The main stream loop now uses bounded queues and nonblocking raw writes; the post-stop forwarding loop is finite. That is a real correction to R1's main-loop/post-stop blocking issue. However Python still writes/flushes the ownership marker synchronously at `owner.py:198-203`, then sets fd 1/2 nonblocking at `204-205`. Perl does the same at `owner.pl:177-180`. The anchor has already established its session and can already have published a durable record. A blocked handshake write pins the supervisor before it opens the user gate or services EOF/deadline/authenticated control. No user command has started in this interleaving, but the gated live anchor and supervisor can remain indefinitely, and an endpoint stop request times out rather than receiving cleanup.

Include initial readiness/ownership frames in the bounded nonblocking emission state machine. Do not release user code until its ownership frame is fully emitted, but keep servicing EOF/deadline/control while waiting. Initial serve/stream READY and pooled protocol output should also have explicit bounded delivery/disconnect behavior, rather than assuming a tiny write cannot block. Record a full destination before handshake, then controller EOF and deadline; assert the same owned anchor is retired and no user release occurs.

### R2-B5 — P1 file-safety race, new retirement code: receipt validation is followed by unpinned pathname deletion

Python `retire` validates a receipt and then calls unlink on `<path>/input`, `<path>/control`, `<path>/terminal`, followed by pathname rmdir (`owner.py:70-76`). Perl does the same (`owner.pl:72-85`). The endpoint directory is not retained/opened without following a link, and receipt read/delete are not tied to one directory identity. The final component's `O_NOFOLLOW` in receipt reading does not prevent the parent directory from being a symlink or being swapped after the check.

A filesystem actor with this user's rename permissions can move the checked directory and replace its name with a symlink between receipt validation and unlink. Fixed-name unlink then removes files in the replacement directory; rmdir failing on the symlink afterward does not undo them. This finding assumes same-user path mutation/rename, not that an unrelated UID can bypass a correctly private directory. The task explicitly requires preservation of unrelated data across path substitution. Endpoint authentication and literal fixed filenames do not establish object identity during deletion.

Pin the expected private endpoint directory using no-follow handles, validate the receipt relative to that retained identity, and unlink only fixed entries relative to it. Check owner/type/permission and expected artifact identity as appropriate. If safe identity-preserving removal of the final directory entry cannot be established, retain an empty artifact rather than delete a substituted path. Do not introduce global scans or PID-based retirement. Extend recording/filesystem-synthetic tests to substitute the directory between receipt validation and each removal, asserting zero foreign-object deletion. The current recording tests assert only the target strings and “no receipt => no unlink”; those do not exercise identity substitution.

### R2-B6 — P2, current SHA: non-line-start or oversized ownership markers can disclose the capability and miss ownership capture

`Marker::feed` accepts the marker only at the beginning of a complete stderr line (`supervisor.rs:157-175`) and forwards other complete lines verbatim. It also forwards the whole unclassified pending buffer once it exceeds 4096 bytes (`177-179`). The remote owner emits its marker without a leading line delimiter (`owner.py:198-201`; `owner.pl:177-178`). A connection/proxy prelude lacking its final newline can therefore produce:

    prelude__GITCRAFT_OWNER__ O1 <nonce> <path> <token>\n

The real endpoint token becomes ordinary captured stderr and no lease is accepted. An oversized unterminated prelude/partial marker can likewise be flushed before classification. This is a static parser counterexample; I did not run the parser or connect to a proxy. Later user-command stderr cannot replace an already established owner, which remains a positive property.

Treat handshake framing as private regardless of preceding arbitrary transport text, split boundaries, invalid frames and overlong buffers. Emit a delimiter or use a robust prefix scanner, and never flush candidate private protocol bytes as ordinary output. Require bounded failure handling and an explicit end-of-stream policy. Pure cases should cover every split, no-newline prelude, oversized prefix, invalid token/version/nonce and EOF mid-marker. The parent says confidentiality hardening is still uncommitted; it must be reviewed at its later preserved SHA before closing this finding.

### R2-C1 — P2 lifecycle gap: Perl fallback-signal failure can skip reaping

`owner.pl:101-102` calls the direct-child fallback outside an eval/finally scope, then reaps afterward. If group stop fails and the fallback signal also throws, the reap call is skipped. Python's corresponding wait is in `finally` (`owner.py:90-96`). The receipt is not published in the Perl error path, so this is not a false successful-stop claim; it is incomplete error-path owner retirement/reaping. Always attempt the exact owned child reap, preserve the first and fallback errors, and never retry numeric signaling after retirement. Extend the injected operations test so both group and direct fallback refuse; the existing recording refusal case only refuses the group.

## R1 disposition in this review scope

| Earlier item | R2 disposition |
|---|---|
| B1 remote stream blocking | **Partial.** Normal chunk forwarding and final drain now nonblocking/bounded, but handshake remains blocking (R2-B4), and terminal output disposition regressed (R2-B3). |
| B2 pooled write deadline | **Open.** Unchanged blocked setup/ping/request writes; new ACK write also blocks (R2-B1). |
| C1 artifact retirement | **Partial.** Exact-nonce pooled ACK and receipt-gated explicit stream retirement now exist. Private input/socket/receipt retirement is useful, but pathname identity is unpinned (R2-B5), ACK framing is broken (R2-B2), and unknown/failed operations can intentionally retain evidence. No bounded runtime-retention claim is established. |
| C2 proof argv scalability | Code/docs show proof moved to intended stdin; **not independently accepted here**, assigned to bisect reviewer for claim binding, record races, bounds and failure disposition. |
| B3/B4/B5/B6 and C3 | Native early-return, workspace claim, Windows thread/Job, caller uncertainty and shutdown disposition belong to assigned native/content reviewers. This remote R2 neither closes nor duplicates them. |

## Positive static observations and limits

Normal stream writes use raw nonblocking file descriptors with bounded per-stream queues, retain partial-write offsets, and continue polling EOF/deadline/authenticated clients. The output capture and forwarding queues each have independent 64 MiB-per-stream bounds; aggregate memory can be materially larger than one capture cap but is not an unbounded queue. Signal targets still derive only from directly owned unreaped anchors, never host-wide process selectors; TERM/KILL precede reaping. Wrong endpoint capability/identity and missing terminal receipt do not intentionally authorize stop success. The Perl receipt code is now bounded to 255. The user command's stdin remains separate from the controller lease. Partial/flush-failed dispatch is still never replayed. Passive pooled retirement uses its existing channel, preserving the no-reconnect behavior when that protocol is corrected.

The new `Options::retire_remote` is optional/defaulted. The Service attempts it after finishing its local transport, and skips it after explicitly unconfirmed remote cleanup (`processes.rs:596-621,672-685`); remote retire rechecks a complete receipt before deleting. This is a useful protocol gate but does not repair R2-B5's filesystem identity gap. Owner/retirement control connections have finite outer Service/control limits; those do not fix Shell's blocked input write or a supervisor stalled before its event loop.

The five Python and eight Perl recording gates check important primitives, but do not establish handshake backpressure, run-level output completeness, stream-read coalescing, blocked Shell writes, path substitution or double-refusal reaping. They were inspected, not executed. After corrections, preserve the exact candidate and run those new pure/recording state-machine cases first. Renewed static acceptance should precede any narrowly owned remote runtime fixture; such a fixture must retain its local transport owner and authenticated remote anchor, avoid all host-wide selectors, and explicitly cover paused output/input consumption with finite deadlines. Native SSH/WSL/platform behavior, inherited process tests, full suite, CI and arbitrary hostile repository commands remain unvalidated and are not authorized by this report.

## Immutable source SHA-256

| File | SHA-256 |
|---|---|
| `src/remote/supervisor/owner.py` | `08ae90481095c86efd2eeb548bf8f2d64e96c80d0ffbdd7f689f790bd4cc8225` |
| `src/remote/supervisor/owner.pl` | `2044428f665187cd99cd2acb37c838723e162e880c6d97ca28a841d7e4c3d766` |
| `src/remote/supervisor.rs` | `625d0f388ca732e5870dc322ab892808bb2c1715653c9ac35d5189228ab28d0b` |
| `src/remote/supervisor/recording_tests.py` | `6c518629b5b96e08b590022006055a6526c111972fbf01dafe69018cd2e16d34` |
| `src/remote/session.rs` | `5499bc9468aa097f2886ef357952a58b5ece3ad063e297c2742ae7cd0426f90e` |
| `src/remote/shell.rs` | `27b83663b7d438972ace02862673abacd2cc7aaa4eeaf69fc2533087ea0b5e0c` |
| `src/processes.rs` | `9f4328c59db62e0c343f1cccb729a7c331144eab66d4d05fce336e81dc517c70` |
| `docs/issue-51/process-safety/backpressure-and-proof.md` | `79b74bf41ade3f4444e8df5eb9cb7572ae2dddfb453f38a97874887720e1c37a` |
| `docs/issue-51/process-safety/artifact-retirement.md` | `c0bd6165f1706e045a73dc437b13cf4081480b5c35d8a11fb260e654f1a079a7` |
