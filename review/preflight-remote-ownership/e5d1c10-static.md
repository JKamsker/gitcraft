# Independent corrected-candidate static review: PASS to recording stage

Candidate: **e5d1c100f6f20918f3f7c3564d28234cc64a3133**, relative to failed
e0c95b5f8e31020b6ce5bbea88af09dfaca16747. Reviewed immutable Git blobs and full
correction diff; all five correction-manifest hashes match. No compiler, recording
test, shell protocol, fake/live transport, CI or repository mutation executed by
this reviewer. Original FAIL report and its candidate remain immutable.

Both blocking source findings are corrected:

* `failure_action` now returns OwnedUnknown for **every** actual Shell::run error
  before considering legacy NotSent/retry/stop variants. `exec_mode` has no owned
  retry path. Known context, pool/connection and cancellation failures before
  Shell::run remain NotDispatched. Even accidental not_dispatched on a Running
  owner transitions to Unknown rather than claiming zero delivery. Legacy public
  reader behavior remains separate.
* Receipt reads create a fresh OwnedDispatch inside the original pending owner's
  captured Context and use direct owned transport. New repair metadata reads use
  direct owned transport too. Failure/uncertainty in a receipt read retains the
  original pending record and does not invoke the legacy asynchronous stop path.

The additional receipt observation is also corrected: host writes the full five
fields to a private temporary outcome then renames it; reader strictly requires
the full version/nonce/u8 exit code/operation kind/full actual OID-or-unknown schema
and terminating newline. Truncation or extra fields cannot retire pending state.
Compare-and-remove nonce retirement protects a concurrent replacement owner.

Recording sources now exercise the actual shared write_request helper with zero,
partial-through-closing-brace and full-write/failed-flush writers; each is classified
Unknown in owned mode with zero retries/stops. Seven ownership/writer/context cases
and two strict-receipt/registry cases are present. These have not executed yet.

This static PASS permits running those pure recording/state checks first. After
they pass, proceed to the proposed bounded, owned temporary-repository host-script
and fake SSH/WSL fixtures, covering successful completion, lock/hook refusal,
connection ambiguity/late receipt, split-index rotation, merge conflict and retained
recovery. Record no owned retry/legacy stop selection. No live accounts/hosts/user
repositories, old blocked harness, new process controller or CI.

This is a source/protocol-stage verdict, not a full runtime/feature acceptance or
native-platform validation. Prior local green evidence remains bound to its old
source. Any minimal shared transport extraction for the object helper must pin its
exact source and retain these corrected ownership/writer semantics.
