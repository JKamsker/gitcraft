# Object storage ownership protocol: static review handoff

Frozen product source: **9191812c849ae39403aea6b420d50a636349d86d**, branch
`codex/object-storage`, based on b5df68070a6df9d91cd29ca5d6270db80c9040c9.
`manifest.json` pins the complete changed-file inventory, tree, and evidence hashes.
This notes branch is evidence only; do not merge its review directory into product.

Requested decision: whether the corrected local ownership and new Windows/owned-helper
source safely permit ordinary fixed-candidate protocol fixtures. **No new remote-helper
runtime has executed.** Full tests are deferred because they include that fixture.
The blocked old c805f6e harness was never written/run and must not be retried.

## Availability contract and actual entrypoints

* `src/bin/gitcraft-object-storage-helper.rs:2` is a supplied, installable headless
  binary, invoking `app::object_storage_helper` (`src/app/mod.rs:617`), then
  `helper::entrypoint` (`src/git/object_storage/helper.rs:203`). No daemon or GUI.
* No args serves one JSON request then exits. `--capabilities` prints protocol,
  package version, Linux identity capability, birth-time requirement and 4 MiB limit.
  Unknown args return 2. The GUI executable also retains an early exact hidden
  `--gitcraft-object-storage-helper` dispatch via askpass startup, before a window.
* Cargo auto-discovers the dedicated binary; `default-run=gitcraft` preserves normal
  app startup. Existing release builds build both binaries. Cargo deb/rpm assets now
  include the helper. No workflow or workflow trigger was changed.
* `README.md:652` documents building the matching checkout's dedicated binary,
  installing to the remote account's PATH, and checking `--capabilities`; packages
  provide `/usr/bin/gitcraft-object-storage-helper`. WSL installs inside its distro.
  The app never deploys/installs a helper. Missing/incompatible helpers refuse with
  actionable setup text; inspection remains available.
* This helper server supports Linux filesystems with birth-time identities. Local
  Windows independence uses native held sharing handles, separately from the helper.
  macOS/other helper servers refuse explicitly. No native Windows runtime PASS claimed.

## Client, wire format, execution and ownership

* Inspector/UI prepare routes through `independent::prepare`; remote paths reach
  `helper::prepare`/`request_target` (`helper.rs:64,111`). `RemotePlan` captures the
  exact repository and connection Profile context. Explicit UI confirmation calls
  `helper::execute` (`:132`), which rejects a changed Profile before dispatch.
* Protocol is `gitcraft-owned-objects/1`, max 4 MiB request/reply, JSON stdin/stdout.
  Request has protocol/repo/optional Ticket, with unknown fields rejected. Ticket
  carries repo, primary, transaction, actual ancestor/store/alternate identities,
  refs/index/reflogs/worktrees, and serialized borrowing topology. Hex transaction
  length is 1..64. Ref snapshot additionally capped at 512 KiB.
* Only constant shell text invokes `git -c
  alias.gitcraft-owned-objects=!gitcraft-object-storage-helper gitcraft-owned-objects`.
  Repository paths and tickets travel as JSON stdin, never interpolated shell text.
  Existing `object_storage_request` (`src/git/command.rs:1233`) and
  `comparison_remote` (`:1079`) provide existing SSH/WSL transports, captured
  `connections::spawn`, 20-second deadline and bounded output. No new process
  controller, process selector, host PID signal, deployment, live account or host.
* `helper::serve` (`helper.rs:224`) reads <=4 MiB, parses, calls `process` (`:151`),
  serializes one <=4 MiB complete reply, and exits. Absolute local repo paths only.
  A prepare request creates no independence stage/removal. It reads topology,
  captures actual ownership, and exports a ticket. Execute recaptures actual handles
  and exact ticket, rejects any difference, then calls the guarded local algorithm.
* **Handles are not retained between prepare and execute helper processes.** The
  ticket pins each retained identity as device/inode/filesystem birth seconds+nanos
  (`held.rs:190`). Missing birth-time support refuses. Execute compares freshly
  captured actual identities and the refs/topology snapshot against the ticket;
  all its handles remain held throughout execute/copy/proof/recovery. Please assess
  this temporal contract explicitly; no claim of cross-process held handles.

## Mutation surface, proof and recoverability

* `independent/local.rs:50` accepts conventional single-worktree clones only and
  refuses promisor, missing/broken/malformed/cyclic/HTTP donor, symlink store and
  incomplete states. No donor write, fetch, repair, prune, GC, or repack.
* `make_transaction` (`local.rs:267`) locks ownership, pins Profile/refs/topology,
  enumerates required refs/root refs/private refs/reflogs/index objects, copies a
  self-contained non-thin/no-reused-delta pack <=128 MiB, indexes strictly in a
  private held stage, verifies actual required-OID coverage and current snapshot,
  then installs create-only. Git commands use no-lazy-fetch, no replacements,
  no optional locks, and cleared inherited repository/object environment.
* Linux `held.rs` retains every ancestor from `/`, repo, `.git`, objects/info/pack
  and actual alternate FD; all opens reject symlinks and compare actual dev/inode.
  Git uses own-process descriptor paths. Stage mkdir/open, pack install hardlinks,
  backup rename and recovery operate relative to actual retained descriptors.
  Pack/index install never overwrites different existing data. Alternate detach
  uses NOREPLACE rename then verifies the moved actual inode. A raced different
  leaf is relinked create-only and leaves recoverable evidence, never unlinked.
* Linux recovery (`held.rs:321`) publishes captured bytes from a newly created,
  flushed actual inode into held info with a create-only link. A newer alternate
  is never overwritten. Backup identity and captured content are verified again
  through final proof. Recovery path is derived from the actual held info location.
* Windows `held_windows.rs` opens every actual ancestor without DELETE sharing,
  rejects reparse points, and holds the actual alternate read/write with READ-only
  sharing. It uses existing safe `safe_read::final_path` API (no new unsafe call).
  Stage dirs remain held. Staged pack files are protected while copied into
  create-new destinations; actual installed handles stay protected through proof.
  Existing files require exact byte equality. Detach makes/flushed create-only
  protected backup then truncates the exclusively held actual alternate, removing
  borrowing semantically. Restoration writes only that same exclusive actual file.
* Only after removal: required object set and refs must still match, bounded
  read-only `fsck --connectivity-only --no-dangling` must pass with borrowing absent,
  and physical ownership plus backup proof are checked again. Failure restores
  borrowing where possible, or reports exact retained recovery evidence without
  overwriting newer metadata. Stages/backups remain; donors are untouched.

## Outcome inventory and subsequent fixture plan

* Prepared(ticket) means only inspection/preparation; no copying or removal.
* Completed(transaction,message) is accepted only for the exact prepared transaction.
  Server sends it only after guarded local proof succeeds. Complete protocol Err
  reports refusal/failure; the action client adds recovery/uncertainty instructions.
* Nonzero/missing helper, incompatible reply, cancellation, deadline/disconnection,
  malformed/truncated/oversized response or mismatched transaction never produces
  success. Execute reports **no completion proof**, known deterministic backup path,
  and no automatic retry. Remote operation may outlive lost client response; the
  app deliberately makes no rollback/success claim in that state.
* Existing corrected Linux candidate a3b4d7d passed full 886+3, 889 names, including
  twenty object-storage cases (held ancestor/store identity replacement, recovery,
  stale refs/index/alternates, failed proof, donor removal and readonly transport).
* After independent STATIC PASS only: run the newly preserved
  `fixed_owned_helper_roundtrips_ssh_and_wsl_and_checks_completion` fixture
  (`tests.rs:822`) through controlled fake ssh/wsl.exe and the actual protocol
  server/client, owned temp donor/borrower pairs only. Then root/private-ref fixture
  (`:922`), targeted group, full test/list and exact fmt/strict Clippy gates.
  Record protocol request/completion/uncertain/refusal outcomes with owned fixtures
  as review directs; no previously blocked old-candidate harness.
* Native Windows-only three fixtures (`tests.rs:760,775,797`) are preserved for final
  native platform gates. Current cross-target metadata-only adapter compiles the
  actual held Windows module with warnings denied; this is not runtime evidence.

No CI was dispatched, no live SSH/WSL host/account contacted, and no actual user
repository was mutated during development.
