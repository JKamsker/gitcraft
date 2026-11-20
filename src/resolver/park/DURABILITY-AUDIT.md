# Parking recovery: durability audit (#205)

Scope: the merged implementation of `src/resolver/park.rs` and `src/resolver/park/`
(PR #200), plus the agent-run record in
`src/repo/resolver/parking/record/persistence.rs`. This audit was done by reading the
code. It describes what each transition writes, flushes and renames, where a crash can
stop it, and what restart finds then. No process was killed and no power was cut for this
audit; see [What is not proven](#what-is-not-proven).

Labels: **[code]** read from the code at the cited function. **[observed]** measured on a
real system (named). **[believed]** documented or widely reported platform behaviour that
this repository has not tested. **[gap]** a weakness found here and not fixed in this
slice.

## Primitives

| Primitive | Where | What it does |
|---|---|---|
| `atomic_write(path, bytes)` | `journal/receipts.rs` | Writes `path.pending` (create, truncate), `sync_all`, renames it over `path`, then `sync_dir(parent)`. Boundaries: `atomic-temp-synced`, `atomic-renamed`, `atomic-parent-synced`. |
| `record(dir, phase)` | `journal/receipts.rs` | Reads every receipt first (`latest`), creates `journal-v1/` if needed, `sync_ancestors(journal-v1)`, then `atomic_write(journal-v1/<n:08>.json)`. Receipts are append-only. |
| `latest(dir)` | `journal/receipts.rs` | Reads receipts in name order. Only `*.json` counts, so a `.pending` temp file is never a receipt. A torn **final** version-1 receipt is copied to `.torn` and replaced with `Uncertain`. Any other unreadable receipt, or an unknown version, is an error. |
| `sync_dir(dir)` | `journal/dirs.rs` → `crate::dir_sync::sync` | Flushes a directory's entries. Unix: `fsync` on a read-only fd. Windows (since this slice): `FlushFileBuffers` on a directory handle opened with `FILE_FLAG_BACKUP_SEMANTICS` and `FILE_ADD_FILE` access, or `FILE_ADD_SUBDIRECTORY` if that is denied. Before this slice it did nothing on Windows. The outcomes are as follows. If another process holds the directory without write sharing (`ERROR_SHARING_VIOLATION`), the open is tried 5 times, pausing 20, 40, 60 and 80 ms, then the call fails. Access denial is never retried. If the open succeeds but the flush answers error 1, 50, 87 or 120, the result is `Flushed::Unsupported`: the filesystem does not implement directory flushes. That result is skipped with a diagnostic note, which `effects::note` keeps for the test and recording harnesses; production has no log. Any other flush error, including `PermissionDenied` from the flush itself, is a failure. |
| `sync_ancestors(path)` | `journal/dirs.rs` | `sync_dir` on `path` and then each parent up to the filesystem root. Parents are tried once, with no retry. Boundary `directory-synced` after each one. On Windows the climb stops at the first ancestor (never `path` itself) whose **open** fails. One reason is access denied for both entry-adding rights, which means this process cannot have created anything there. The other is `ERROR_SHARING_VIOLATION`: another process holds the directory without `FILE_SHARE_WRITE`. This was observed on `%LOCALAPPDATA%` during a harness run. A flush that was opened and then refused is never skipped. **[gap G8]** If the stopping ancestor is one gitcraft created (`gitcraft/`, `parked/`, `parked-recovery/<id>/`), every later receipt retries its entry. While another process keeps holding that directory, the entry stays unflushed. |
| `sync_existing` / `sync_file` | `journal.rs` | Opens an existing file without truncating it (write access on Windows, because `FlushFileBuffers` needs it) and calls `sync_all`. |
| `sync_tree(dir)` | `journal.rs` | Flushes every regular file under `dir` unless `durability::proven` shows it is already flushed and unchanged. Then `sync_dir` on every directory, children before parents. Errors on a non-regular entry. |
| `durability::copy` | `durability.rs` | `fs::copy` to a new owned output, `sync_all` (on Windows through a writable handle, after clearing and then restoring the output's READONLY bit), then `remember`: a content, identity and mtime stamp written with `atomic_write` under `durable-v1/`, plus `sync_ancestors`. |
| `publication::publish` | `publication.rs` | Makes a fresh `.gitcraft-restore-<n>/` directory next to the live file, copies the bytes there with `durability::copy`, then `sync_dir(staging)` and `sync_dir(parent)`. Re-checks the target, renames the copy over it, then `sync_dir(parent)`, `sync_dir(staging)`, `remove_dir(staging)`, `sync_dir(parent)`. |

All renames use `std::fs::rename`. **[observed in the local 1.99.1 toolchain's
std source (`sys/fs/windows.rs`); toolchain-specific]** There it calls
`MoveFileExW(MOVEFILE_REPLACE_EXISTING)` first, and on access denial falls back to `SetFileInformationByHandle(FileRenameInfoEx, POSIX_SEMANTICS)`.
Other std versions may order these differently, and newer ones may try the POSIX-semantics rename first. Nothing here depends on the order. No rename asks for write-through. The rename itself is
not flushed. It becomes durable through the `sync_dir` calls that follow it.

## Park (`park::park`)

| # | Step (function) | Written | Flushed | Renamed |
|---|---|---|---|---|
| P0 | `locking::acquire` | `gitcraft/parked-locks/<id>` (created, never truncated) | no | no |
| P1 | `git update-index --no-split-index` (split index only) | live index (by Git) | Git's choice (see [Git](#git-durability)) | Git's choice |
| P2 | `reserve` | `gitcraft/parked/`, `parked/<id>/` | not yet | no |
| P3 | `record(Archiving)` | `journal-v1/00000000.json` | `journal-v1` and all ancestors, then the receipt file and its directory | `.pending` → `.json` |
| P4 | `archive::keep`: index | `<id>/index` + `durable-v1/<h>.json` | file, then evidence (atomic), then evidence ancestors | evidence only |
| P5 | `keep`: `trees`, `commit-tree`, `update-ref refs/gitcraft/parked/<id>` | loose objects, ref, a temporary index | Git's choice: by default **not** loose objects, **not** refs | Git's choice |
| P6 | `keep`: raw files, state files | `raw/<path>`, `MERGE_HEAD`… (+ evidence each) | each file and its evidence; `raw/` subdirectories not yet | evidence only |
| P7 | `keep`: manifest | `parked.json`, with `atomic_write` (since slice 3; before: `std::fs::write` in place) | the temp file, then its directory | `parked.pending` → `parked.json` |
| P8 | `unchanged`, `in_the_way` (reads only). On error: `forget_locked` | — | — | — |
| P9 | `sync_tree(dir)`, `sync_ancestors(dir)` | — | every unproven file (`parked.json` again), every archive directory, all ancestors | — |
| P10 | `record(Archived)`, `unchanged`, `record(Clearing)` | receipts 1, 2 | as P3 | as P3 |
| P11 | `git reset --hard -q HEAD` | live worktree, index, removes `MERGE_HEAD`… | Git does not flush worktree files. Index only with `core.fsync=index` | Git's choice |
| P12 | `clean_baseline` → `atomic_write(clean-baseline.json)`, `sync_tree`, `record(Parked)` | baseline, receipt 3 | yes | yes |

What restart (`park::list` → `discovery::manifest` → `journal::recover_locked`) finds after
a stop at each point:

- **Before P3 completes:** `parked/<id>/` exists with no receipt. If a `.pending` file
  exists, it is ignored. `manifest` sees no `parked.json` and `latest == None`, so it
  returns `Ok(None)` and nothing is listed. The live worktree has not been touched. The
  empty reservation stays on disk. **[code]**
- **P4–P6:** the archive is partial and `latest == Archiving`. `manifest` sees no
  `parked.json` and returns `Ok(None)`, so nothing is listed. The worktree is untouched.
  After P5, `refs/gitcraft/parked/<id>` can exist with no listed archive. Nothing finds it
  automatically, and nothing removes it after a crash: `forget_locked` runs only on
  returned errors. **[gap G1: orphaned reservation and ref after a crash during
  archiving; they waste space but lose no data]**
- **After P7, before P9 (process kill):** `parked.json` is complete and
  `latest == Archiving`. `manifest` returns the archive and `recover_locked`
  (`_ => Ok(true)`) lists it, even though the worktree was never cleared. Bring back is
  then refused by `blocker_clean`, because the merge state files are still live
  (`busy_with`). Drop to branch works and creates the branch from the snapshot.
  **[code; real kills at `archive-manifest-written`, `archive-reachable`, `phase:Archived` and `phase:Clearing`, `checks/kill.rs`]**
- **Inside P7 (kill after the temp file is flushed, before the rename):** `parked.pending`
  exists, `parked.json` does not. Same as P4–P6. **[code; real kill, `checks/kill.rs`]**
- **After P7, before P9 (power loss):** `parked.json` was published by a flushed temp file,
  a rename and a directory flush, so it is either absent (same as P4–P6) or complete.
  **[G2 fixed in slice 3]** Before that, it was written in place and could be torn; a torn
  manifest made `manifest` return `Err("Invalid recovery manifest …")` and `list` fail for
  every archive of the repository. Archives written that way can still exist, so
  `manifest` now treats an unreadable manifest whose last receipt is `Archiving` as an
  incomplete reservation: not listed, left on disk untouched, with a diagnostic note. The
  worktree was never cleared at `Archiving`, so nothing is hidden that is not still live.
  Past `Archiving`, or with no receipts at all (a legacy archive whose worktree may have been
  cleared), it still fails closed. **[code; `checks/discovery.rs`]**
- **After P9, before P10:** everything archived is durable, but the latest receipt is still
  `Archiving`. Same as "after P7, process kill".
- **P10 (`Archived` or `Clearing` durable) and during P11:** the archive is complete and
  durable before the reset starts. A partial reset can leave the worktree half-cleared.
  Restart lists the archive. Bring back is refused (`blocker_clean`: the operation is in
  progress, or the worktree is dirty) and points the user to the archive. Raw bytes,
  index and metadata are in `raw/`, `index` and the state files. **[code]**
- **After P11, before P12:** the worktree is clean, but Git did not flush it. After power
  loss it may hold stale or partly written files. They can be recovered from the archive.
  With no baseline, `clean_bytes_guard` falls back to the `cat-file --filters` bytes, which
  is more conservative. **[code]**

## Bring back (`restore::bring_back`) and copy resume (`transaction`)

| # | Step (function) | Written | Flushed | Renamed |
|---|---|---|---|---|
| R0 | `acquire`, `locate`, `identity`, `validate_required`, `latest` | — | — | — |
| R1 | `capture_untouched` | evidence for read-only tracked files, index and state files | evidence (atomic) and its ancestors | evidence |
| R2 | `record(Restoring)` | receipt | yes | yes |
| R3 | `git checkout -q --no-overwrite-ignore` (if needed) | worktree, index, `HEAD` | Git's choice | Git's choice |
| R4 | `git read-tree -m -u head snapshot^{tree}` | worktree, index | Git's choice | Git's choice |
| R5 | `transaction::prepare` → `atomic_write(restore-plan.json)` | full before/after images | yes | yes |
| R6a | `resume`, index entry | `index.lock` (create_new), then evidence | lock file, evidence | `index.lock` → `index`, then `sync_dir(git dir)` |
| R6b | `resume`, other entries → `publish` | `.gitcraft-restore-<n>/bytes`, evidence | copy, staging and parent before the rename; parent and staging after it | copy → live path |
| R7 | `verified`, `sync_live` | — | every live tracked, changed or raw file unless proven; index and state files; then `sync_ancestors_of` every distinct file parent, deepest first, and the git dir. On Windows a live parent this process may not add entries to is skipped. Neither the restore nor Git, running as this process, can have created anything in such a directory. A parent that another process holds without write sharing fails the restore, and the archive is kept. | — |
| R8 | `record(Restored)`, `record(Cleanup)`, `retire` | receipts | yes | see cleanup |

Restart in `bring_back`, when `latest` is `Restoring`, `Restored`, `Cleanup` or
`Uncertain`:

- **R2–R4 (no plan):** if `verified` passes, the work is done (see R7). Otherwise, Git is
  retried only when `blocker_clean` proves the worktree is still in its parked state. A
  partly applied checkout or read-tree fails that check and gets `guidance(dir)`: manual
  recovery, archive kept. **[code; documented in RECOVERY.md]**
- **R5–R6 (plan durable):** `resume` checks every entry against its exact before or after
  image (bytes and attributes), and the archive sources against the after image, before it
  writes anything. Then it applies only the entries that are still in their before state.
  Repeating it is harmless. A user edit (neither image matches) stops it with no writes.
  **[code; recording harness]**
- **Stop between creating `index.lock` and renaming it:** on restart `resume` sees
  `index.lock` and refuses. The lock is never adopted or removed. Manual step: confirm Git
  has stopped, then remove the lock. **[code]**
- **Stop inside `publish`:** a `.gitcraft-restore-<n>/` directory remains next to a live
  file. A retry makes `<n+1>` and never adopts the old one. The leftover shows as an
  untracked entry in the worktree. `blocker_clean` uses `--untracked-files=no`, so the
  leftover does not block a retry. **[code; RECOVERY.md]**
- **After all copies, before `Restored`:** `verified` passes, then `sync_live` runs, the
  receipt is written and the archive is retired. **[code]**
- **R7 cost (G3, fixed in this slice):** in #200, `sync_live` called `sync_ancestors`
  once per tracked file: (tracked files × path depth) directory flushes, most of them
  repeats. On Windows a directory flush took about 0.4 ms on an idle NTFS NVMe volume and
  about 2 ms right after a rename **[observed]**. At that cost, 10,000 files at depth 8 come
  to about 30 s. `sync_live` now flushes the files first, then each distinct directory once
  (`journal::sync_ancestors_of`). The guarantee is the same: every directory above a live
  file is flushed before `Restored` is recorded. The recording harness now sees fewer
  `directory-synced` boundaries.
- **Why `sync_live` still flushes every tracked directory:** `sync_live` cannot know which
  directories changed. Git's `checkout` and `read-tree` create and rename files anywhere
  between the current branch, `HEAD` and the snapshot, and their changes are not recorded.
  On restart, `sync_live` runs again with no record of which steps already ran. The copy
  plan lists only gitcraft's own writes. Restricting the flush to the `diff-tree` paths
  plus the plan entries would miss directories that Git's branch switch changed. So each
  distinct directory is flushed once instead.

## Cleanup (`journal::retire` / `retire_paths`) and drop (`park::drop_to_branch`)

| # | Step | Written | Flushed | Renamed |
|---|---|---|---|---|
| C1 | `retire`: `update-ref refs/gitcraft/parked-recovery/<id>` if missing | ref | Git's choice (not by default) | Git's choice |
| C2 | `retire_paths`: `create_dir(parked-recovery/<id>)` (or `reserve`) | empty directory | not yet | no |
| C3 | `rename(parked/<id>, parked-recovery/<id>/parked)` | — | — | directory, inside the common Git dir |
| C4 | `sync_ancestors(destination)`, `sync_dir(destination parent)`, `sync_dir(source parent)` | — | both sides of the rename | — |
| C5 | `update-ref -d refs/gitcraft/parked/<id> <snapshot>` (conditional) | ref | Git's choice | — |
| D1 | `drop_to_branch`: `sync_tree`, `record(Dropping{branch})` | receipt | yes | yes |
| D2 | `update-ref refs/heads/<branch>`, `retire`, `git config branch.<b>.description` | ref, config | Git's choice | Git's choice |

- **Stop after C2:** an empty reservation. Restart (`recover_locked` → `retire` →
  `retire_paths`) accepts an **empty** destination and renames into it. It refuses a
  non-empty one ("collision"), and both copies are kept. **[code; journal tests]**
- **Stop after C3, before C4 (power loss):** **[believed]** on journaling filesystems
  (ext4, xfs, btrfs, NTFS) a rename is one logged metadata operation. After recovery the
  archive is either at the source or at the destination, not both and not neither.
  `locate` and `identities` scan both places, so either outcome is found, and `retire`
  finishes the move. FAT and exFAT do not journal, so this does not hold there.
- **Stop before C5:** both refs exist. Restart repeats `retire`. It removes the original
  ref only while it still points at the snapshot. **[code]**
- **Stop before the `D2` description:** the branch and the retired archive exist, but the
  branch description that names the archive path is never written on restart. The row
  menu still shows the path. **[gap G4, cosmetic]**

## Forget (`park::forget_locked`)

This runs after a park that failed with an error. It deletes the ref first, then calls
`std::fs::remove_dir_all(parked/<id>)`, with no flush and no fixed order. **[gap G5]** A
crash during the recursive removal can leave `parked.json` without `journal-v1/`.
`latest == None` then lists it as a parked merge while the worktree was never cleared,
and its ref is already gone. Bring back is refused by the live operation state. Drop to
branch would point a branch at a snapshot whose objects may still exist but are no longer
pinned. Renaming the archive aside before the recursive delete would close this.
**[code; not exercised]**

## Agent run record (`record/persistence.rs::keep_record`)

The order is: `ws.preserve()` (keeps the scratch worktree), `create_dir_all(dir)`, then
`receipt(agent-archive-v1.json)`: a unique temp file with `create_new`, `sync_all`, a
rename and `sync_dir(dir)`. Then `Workspace::keep` copies `original/` and `resolved/`, then
`sync_tree(dir)` (files are flushed through a writable handle on Windows), then
`receipt(run.json)`. `read_record` falls back to the version-1 intent when `run.json` is
missing or invalid.

- **[gap G6]** `dir` (`gitcraft/resolver/<id>`) is created with `create_dir_all`, and its
  entry in its parent is never flushed. After power loss the whole record directory can
  be missing. The scratch worktree is kept by `preserve`, so the agent's work survives at
  its original location. Only the published copy is lost.
- Windows: `sync_dir` in this file did nothing before this slice. It now flushes, like
  the journal's, and `sync_tree` flushes every directory of the kept copy strictly. If a directory
  of the copy cannot be opened for flushing, `keep_record` now fails on Windows. That
  happens, for example, while another process holds the directory without write sharing.
  Before this slice it was skipped silently. The scratch worktree
  is still preserved in that case.

## Git durability

The default for Git 2.53 is `core.fsync=committed,-loose-object` (per `git help config`).
Only pack files are flushed. **Loose objects, refs and the index are not.** The park
snapshot and staged commits come from `commit-tree`, so they are loose objects. Their pins
(`refs/gitcraft/parked/<id>`, `parked-recovery/<id>`, the drop branch) are refs. **[gap
G7]** After power loss, a snapshot ref can point at a missing or empty object even though
the archive's `raw/`, `index` and metadata are durable. In that case, Drop to branch and
the snapshot-tree comparison in `unchanged` fail closed, and recovery means copying the
raw files out by hand. A possible fix, not made here: run the snapshot's Git commands with
`-c core.fsync=committed,reference` (or `added`), or run `git fsck`-style object checks on
restart.

## Platform assumptions

- **Linux ext4** (default `data=ordered`): `fsync` of a file flushes its data and inode.
  `fsync` of a directory makes creates and renames in it durable. A rename is atomic in
  the journal. All of this assumes barriers are on (not `nobarrier`) and that the device
  honours flush/FUA. **[believed]**
- **xfs:** the same model. A directory `fsync` is needed for new entries. **[believed]**
- **btrfs:** `fsync` goes through the log tree. A directory `fsync` persists its entries.
  Rename-plus-fsync durability on btrfs has had kernel bugs in the past. **[believed,
  uncertain]**
- **NTFS:** metadata (create, rename, delete) goes through `$LogFile`. File data is not
  journaled. `FlushFileBuffers` on a file flushes its data and metadata. **[observed:
  Windows 11 26200, NTFS on C: and D:]** `FlushFileBuffers` on a directory handle succeeds
  with `FILE_ADD_FILE` or `FILE_ADD_SUBDIRECTORY` access. With read-only access it fails
  with `ERROR_ACCESS_DENIED` (`dir_sync/tests.rs`). Standard users get `ERROR_ACCESS_DENIED`
  when opening `C:\Users` or `C:\Windows` for those rights. `C:\` grants only
  `FILE_ADD_SUBDIRECTORY`. Opening a directory for flushing requests write-class access, so
  it conflicts with any other handle on that directory that lacks `FILE_SHARE_WRITE`. Before
  ancestor climbs tolerated this (see `sync_ancestors`), 3 of 6 full recording-harness runs
  on this machine failed. The 2 runs whose logs were kept show that some other process held
  `C:\Users\Jonas\AppData\Local` that way, and the opens failed with
  `ERROR_SHARING_VIOLATION`. The
  directory being flushed is never skipped. When another process still holds it after the
  open has been tried 5 times (about 0.2 s), the
  operation fails and keeps the archive. **[believed, not proven]** A directory flush writes out the
  directory's index and the NTFS log up to the rename's records. This is the guarantee
  that renames depend on. **Rename write-through:** `MOVEFILE_WRITE_THROUGH` affects only a
  move done as copy-and-delete (across volumes), per Microsoft's `MoveFileExW`
  documentation. Every rename here is within one volume (same directory, a staging
  directory beside the target, or inside the common Git directory). The flag would add
  nothing, and std cannot set it without `unsafe`. So the code relies on NTFS's logged
  rename plus the directory flushes that follow it. The Windows policy "Turn off Windows
  write-cache buffer flushing" voids every guarantee here.
- **SMB shares and WSL `\\wsl$` (9P):** **[observed, Windows 11 26200]** On
  `\\localhost\D$\…` and on `\\wsl$\<distro>\tmp` (also `\\wsl.localhost`), files write,
  rename and flush, and the directory opens with `FILE_ADD_FILE`. But the directory's
  `FlushFileBuffers` answers `ERROR_INVALID_FUNCTION` (1). That result is
  `Flushed::Unsupported`: noted, then treated as done. Parking works there, but nothing
  makes those directory entries durable. That depends on the server: the SMB server's
  NTFS, or the Linux side of 9P. Run the ignored test
  `dir_sync::outcome_tests::a_real_smb_or_9p_directory_reports_unsupported_flush` with
  `GITCRAFT_TEST_UNFLUSHABLE_DIR` to repeat this.
- **ReFS, FAT32, exFAT, cloud-filter folders:** **untested.** Mounting a FAT or exFAT VHD
  needs admin rights, which this run did not have. If such a filesystem answers one of the
  "not implemented" codes (1, 50, 87, 120), the flush is skipped with a note, as on SMB.
  Any other error fails the operation and keeps the archive. FAT and exFAT do not journal
  renames.
- **APFS (macOS):** std's `sync_all` uses `fcntl(F_FULLFSYNC)` **[code: std
  `sys/fs/unix.rs`]**, which also flushes the drive cache. A plain `fsync` would not. The
  same call is made on directory descriptors. **[believed; macOS not run for this
  audit]**
- **All platforms:** other processes such as antivirus, indexers and editors can hold
  handles without `FILE_SHARE_DELETE`. That makes a rename over a live file, or of the
  archive directory, fail on Windows. The operation then returns an error and keeps the
  archive. **[code: errors propagate; not a crash concern]**

## Kill and restart (slice 3)

`checks/kill.rs` runs each case as a real owned child process of the test binary, aborted
with `std::process::abort` right after the named effect; `checks/kill_child.rs` holds the
child and its spawn. The parent waits for that one child, then restarts through production
`list`, `bring_back`, `drop_to_branch` and `forget`. For every case: tracked, untracked and
ignored files, the index and `MERGE_HEAD` are byte-exact, live or in a discoverable archive;
`list` gives the same answer twice; an `index.lock` the child left is neither adopted nor
removed; after the archive is dropped to a branch, listing and dropping again change no
byte under the fixture. **[observed: Windows 11 26200 NTFS, local runs; CI runs it on Linux and Windows in the `recovery recording` job]**

| Killed after | Restart finds |
|---|---|
| P2 `archive-reserved`, P3 `phase:Archiving`, P4 `archive-index-copied`, P5 `update-ref refs/gitcraft/parked/…`, P6 last `archive-metadata-copied` and a `raw` copy, P7 manifest temp flushed | Nothing listed; live worktree untouched; explicit `forget` removes the reservation and ref (G1 stays: nothing removes them automatically) |
| P7 `archive-manifest-written`, P9 `archive-reachable`, P10 `phase:Archived`, `phase:Clearing` | Listed; bring back refused (merge still in progress); drop to branch keeps it |
| P11 `git reset`, P12 `phase:Parked` | Listed; brought back byte-exact |
| R1 evidence, R2 `phase:Restoring`, R5 `restore-plan-durable`, R6b `destination-temp-durable` / `destination-published`, last `restore-copy-applied`, R7 `live-restore-synced`, R8 `phase:Restored` / `phase:Cleanup` | Brought back byte-exact (resumed against the plan where one is durable) |
| R4 `git read-tree` (no plan yet), R6a `index-lock-synced` | Bring back refused with the archive's path (`Interrupted recovery at …`); drop to branch keeps it |
| C1 recovery ref, C3 `archive-moved`, C5 original ref removed; D1 `phase:Dropping`, D2 branch ref, `archive-moved`, original ref removed | Finished on restart; recovery ref and archive kept, original ref gone |

`checks/kill_cases.rs` adds two cases after a kill. A foreign file placed where the archive
would move (C1–C2) makes `list` (after a restore), bring back and drop refuse with
`Recovery archive collision: <path>`, leaving both the foreign file and the archive
byte-exact; once the user moves it away, recovery completes. A receipt of an unknown version
after a kill at R2 makes `list`, bring back and drop refuse with an error naming the receipt,
writing nothing; a torn legacy final receipt is kept as `.torn`, read as `Uncertain`, and
recovery completes.
## What is not proven

- That any flush reaches stable media. That depends on the device and controller honouring
  cache flushes, and no power was cut for this audit.
- The on-disk effect of `FlushFileBuffers` on an NTFS directory. Only its success and its
  access requirements were observed.
- Real process termination anywhere but at an effect boundary. Since slice 3, `checks/kill.rs`
  aborts an owned child process (`std::process::abort`: no unwinding, no destructors, no
  explicit unlock) at one boundary of every transition above (31 boundaries: P2–P12,
  R1–R8, C1–C5, D1–D2) and restarts through production entry points **[observed: Windows 11
  26200 NTFS; CI runs it on Linux and Windows]**. A kill inside a syscall (a half-done `write` or `rename`) is not
  produced, and Git is the recording fake, so a kill inside a real `git reset` or
  `read-tree` is not either. A process kill also keeps everything already handed to the OS;
  it says nothing about what a power cut keeps.
- Torn writes inside a sector, and reordering of writes that were not flushed. The analysis
  above assumes the worst case, any subset of unflushed writes, only for the items marked.
- How Git itself behaves when interrupted (checkout, read-tree, reset, update-ref) beyond
  its documented `core.fsync` defaults.
- ReFS, FAT, exFAT, cloud-filter folders, WSL `drvfs` (`/mnt/c`) and macOS. On SMB and
  `\\wsl$` the directory flush is observed to be unsupported, so directory durability there
  depends only on the server.

Repeating the Windows observations: run the `dir_sync` and `ancestor_tests` unit tests (see
the PR) unelevated on Windows. The ancestor test walks up from `%TEMP%`, so it reaches
`C:\Users`, which denies entry-adding rights.
