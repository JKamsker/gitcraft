# Parking recovery and implementation handoff

This implementation includes original slice `9cd34bd9e08084aa642040d331df1b72dc50c5b0`
and its safety-review repairs. Existing Parked/run/workspace schemas are retained.
Journal readers accept version 1 and 2; new receipts use version 2. Receipts are
written to a temporary file, synced, renamed, then their directory is synced.
Temporary files are not receipts. A malformed final receipt with an identifiable
version-1 prefix is preserved as `.torn` and replaced with an explicit Uncertain
receipt. Interior corruption, unidentifiable versions, and unsupported versions
fail closed; preserve the archive and seek manual recovery rather than editing
journal history to assert success.

Before reset, archive files, directory entries and all ancestors are synced, and
live raw/index/operation bytes are checked again. Restore syncs live regular files,
index, metadata and affected directories before recording Restored. A Restored
receipt alone never proves that current live files match: restart verification
compares the original index, metadata, tree and raw bytes before retirement.

A restore copy plan records archive identity and exact before/after images of all
tracked files, original index and operation metadata. After interruption, Bring
back resumes these copies only when every captured target is still in one of its
permitted states, source bytes match, HEAD matches, and no index lock or unrelated
operation blocks it. User edits with different bytes stop all writes. No automatic
rollback reset can discard new work. An interrupted Git checkout/read-tree without
a durable copy plan is retried only with the existing clean/status/raw-byte guards;
otherwise it requires explicit recovery. Locks are never adopted or removed on
restart merely because their contents resemble the intended index.

## Recovering an uncertain state

The graph retains recovery rows after successful restore/drop. The row menu offers
**Copy recovery folder path**, **Bring it back**, and **Drop it (keep its files on a
branch)**. Drop preserves the Git snapshot on a branch and leaves byte-exact raw
files, original index and metadata in the archive; Git filters can make snapshot
bytes differ from the raw files. A branch description also gives the archive path.

If Bring back reports uncertainty, first save current worktree files (including
ignored/untracked files), the current index and operation metadata elsewhere. Stop
editors/Git operations. Remove an index.lock only after independently confirming
its owner has stopped. Restore the recorded preconditions before retrying, or use
Drop and recover selected raw files manually. The archive's `raw/`, `index`, and
operation files remain available; never replace a live index or operation metadata
while Git is running. A partially applied copy may resume directly; a partially
applied Git transition can require manual recovery. We do not promise automatic
rollback of an arbitrary interrupted Git command.

All foreground park/restore/drop/forget mutations and background reconciliation
hold a nonblocking OS file lock keyed by the common Git directory and archive id.
Independent handles/tabs share this lock. Busy discovery retains the row and skips
reconciliation; callers may retry. Locks live at `gitcraft/parked-locks/<id>` and
are permanent: never delete or replace their inode. OS handle closure releases
ownership without PID inspection. The lock stays outside both archive locations,
so moving the archive cannot split ownership. Internal helpers require their
caller's lease and never reacquire it. A receipt cannot recreate a missing archive.
Directory-only remnants cannot shadow a retained archive with `parked.json`.

Required raw files, index and recorded operation metadata must all be regular
files before any checkout, Restoring receipt or copy-plan publication. A missing
required source is a failed precondition, never an intended deletion image.
Repairing that source permits a clean retry. A subsequent interrupted mutation
retains explicit recovery state and must pass the restart byte guards.

Archives start at `<git-common-dir>/gitcraft/parked/<id>` and move to
`<git-common-dir>/gitcraft/parked-recovery/<id>/parked` (use the row menu's actual
path, especially for linked worktrees). Discovery scans both locations. Cleanup
pins `refs/gitcraft/parked-recovery/<id>` before movement, syncs destination and
source directories, and only then conditionally removes the original parking ref.
Restart repeats this operation safely, including when ref removal failed. Source
and destination collisions preserve both copies. There is no automatic expiry.

To remove a retained recovery deliberately, first export/verify any bytes you wish
to keep and finish pending recovery. With Gitcraft and other Git writers stopped,
verify the archive's id/snapshot and both refs. Remove the original parked ref if
still present only with its expected snapshot, then remove the recovery ref with
`git update-ref -d refs/gitcraft/parked-recovery/<id> <snapshot>`, then delete only
that id's recovery directory. A failed conditional ref deletion means stop and
investigate. Interruption leaves either a discoverable archive (which may recreate
its pin on reload) or an already explicitly discarded recovery; repeat inspection
before continuing. Never delete a different ref or use recursive wildcard cleanup.

## Evidence and limits

Run the isolated safe harness with:

```
RUSTFLAGS='--cfg recovery_recording' cargo test --offline \
  --manifest-path src/resolver/park/checks/Cargo.toml -- --nocapture
```

It compiles the production parking modules against fake Git boundaries, performs
actual file/copy/rename/sync operations in owned temporary directories, injects
ordinary errors and abrupt control-flow stops after actual production effect boundaries, and invokes production park,
bring_back, drop_to_branch, list/restart and copy resumption. It checks original
raw binary/CRLF, index and metadata bytes or a discoverable byte-exact archive,
ignored-file preservation, repeated discovery and drop, edit refusal, incomplete
legacy receipts and unknown versions. Effects are recorded where they execute;
these tests do not infer behavior from a parallel phase model. Existing synthetic
receipt tests remain supplementary and were not run as application integration tests.

The abrupt recording mode panics past production error handlers, so explicit
failure cleanup cannot mask the recorded stop. Rust RAII still releases handles
(including archive locks); no process is killed, and this does not reproduce
arbitrary termination inside a syscall. This is error/interruption evidence,
not power-loss or process-kill evidence.
Directory fsync is Unix-only through std; Windows directory durability and rename
replacement remain unvalidated. Durability also depends on the filesystem/device
honoring sync and Git's object/ref persistence configuration. Raw restore publication reserves a private temporary directory beside each live
file, writes and syncs the raw bytes there, rechecks the target, and renames on
that destination filesystem. Both affected directories are synced. Archive
movement stays within the common Git directory. Interrupted staging directories
are left intact and never adopted or overwritten on retry; they can consume
space until explicitly inspected and removed. External Git/editors must be quiescent during
mutations: byte preconditions do not provide an OS-wide transaction or exclude a
concurrent editor between checking and writing. Path/type changes fail closed.
The copy plan stores additional byte images on disk and is not space bounded.

No real process tests, full application builds, CI, pushes or PR operations were
performed for this repair. Integration must run combined builds/regressions,
platform checks, full-tree size enforcement, and independent adversarial review on
the immutable final candidate. Existing agent cleanup/ownership protections are
unchanged; this slice only fixes invalid visibility on the Drop trait method.
