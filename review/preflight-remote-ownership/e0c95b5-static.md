# Independent static review: FAIL, source corrections required

Candidate: e0c95b5f8e31020b6ce5bbea88af09dfaca16747 on codex/push-preflight.
Base: 1c6d1611ead6f39b0ab60947a08af504cfa4aa19. Reviewed in an isolated
worktree. All ten manifest source SHA-256 values match. No compiler, recording
test, shell protocol, transport, live host, CI or repository mutation executed
by this reviewer. The previous local green evidence is not a remote-runtime pass.

## Blocking findings

1. **Write failures are not proof that an owned mutation was never sent.**
   `src/remote/shell.rs:188` maps every `write_all`/flush error to `Failure::NotSent`.
   The new `src/remote/ownership.rs:110` retries that on a reused shell, or calls it
   NotDispatched on a new shell; `remote_repair.rs:248` then retires its pending
   ownership. A failed write can follow successful partial writes; this API does
   not establish zero delivered bytes. The frame's syntactically complete closing
   `}` precedes its final newline, so a peer seeing a complete group at EOF is not
   ruled out by an unsuccessful full-frame write. A flush failure also cannot
   establish zero delivery. An actual started mutation must instead retain Unknown
   ownership, guard and recovery, without a retry. Conservatively classify any
   owned send failure as Unknown, or provide an explicit proven zero-byte boundary.
   Keep the old public exec behavior separate if compatibility requires it. The six
   pure tests currently manufacture Failure variants; they do not validate their
   classification at the actual writer boundary.

2. **Unknown-outcome receipt recovery re-enters the legacy stop controller.**
   `remote_repair.rs:56` invokes `held.owner.run` around `git::forecast_remote`.
   `ForecastCompletion::track` retains Context/tickets but does not select owned
   transport. `forecast_remote` (`src/git/command.rs:322`) still calls public
   `remote::exec`; its timeout can take `FailureAction::LegacyTimeout` and invoke
   `stop` (`src/remote/session.rs:203`), including the legacy asynchronous selector.
   This violates the new ownership lifecycle's no-stop contract on its actual
   recovery path, despite the synthetic owned-failure matrix. Use a fresh
   OwnedDispatch captured inside the original held Context and direct owned
   transport for receipt reads. Keep the original Unknown pending record/evidence
   when that read fails or is uncertain. Author acknowledged this source gap.

## Additional review observations

* `prior` currently recognizes a receipt by only a two-line prefix (`:69`), while
  `finish` writes outcome in place before removing the shared guard (`.sh:21`).
  Prefer atomic publish plus strict complete version/nonce/code/kind/actual parsing
  so a partial receipt cannot retire client ownership. The durable shared guard
  presently prevents a new host mutation during the finish window; this observation
  is not an independently demonstrated overlapping-mutation finding.
* Actual queue/connection waits are covered by synchronous `owner.run` tracking;
  cancellation is checked immediately before `begin`, after pool acquisition.
  Cancelling Running work correctly does not claim it stopped. Context is captured
  and retained, including in pending Unknown entries.
* The new host mutation route uses foreground native Git, exact branch/HEAD checks,
  coherent saved-index dependency resolution, bounded index recovery, private
  validation indexes, native capability checks and three mandatory rewrite ACKs.
  The reused HookScript generator's body differs from the green base only in
  visibility declarations, not shell content. No new selector/controller was added.
* Shared host guard is released by the host only after foreground command returns
  and receipt writing succeeds. Successful cleanup targets the unique nonce
  directory and does not remove the shared guard or a later operation's directory.
  Error paths retain recovery rather than rolling back concurrent edits.
* The local behavior change adds no-lazy-fetch for rewrite plumbing; accepted local
  transaction behavior is otherwise preserved. Native fetched-target merge keeps
  ordinary Git conflict/abort recovery and does not claim failed merges succeeded.

## Recording plan before protocol execution

After source corrections are pushed and independently re-reviewed, first run pure
state/writer-recording tests: zero-byte refusal versus partial/flush failure,
Queued cancellation before actual send, Running cancellation -> Unknown, reused
connection ambiguity -> no retry, and every receipt-read failure selecting zero
legacy stop commands. Exercise full/truncated/wrong-version/wrong-nonce receipts,
captured Context/lease retention and pending-registry retirement rules. Record
cleanup targeting only exact successful unique D+nonce, never shared guard.

Only after static PASS and recording checks should ordinary owned temp-repository
fixtures execute the remote host script/fake transports for success, refused lock,
original-hook veto, late outcome/lost response, split-index rotation, native merge
and recovery. No live accounts/hosts/user repositories, old blocked harness,
new process controller or CI.
