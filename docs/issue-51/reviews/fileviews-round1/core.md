# Fileviews independent adversarial review — core ROUND 1

Verdict: **FAIL**. Two staging-publication defects were reproduced on owned fixtures. A Windows security-descriptor omission and remote-tool portability gap also remain. Standalone macOS guarded-save and reset --keep focused tests passed; that narrower result does not certify the batch or untested platforms.

Review target: `c82112a5ab8ba461c487c3266462a81896c85d33`; application `5df74dbb3792a34ee0b1d13c301c9aa552aa6c69`; base `a850de26b0a0778ea96fe34cd1498e6e78928492`. Checkout `/Users/jonas/.t3/worktrees/gitcraft/t3-e06bdc42` was clean at target. Candidate versus application differs only in the three checked-in validation artifacts. This review read original #40/#44/#50 bodies and the candidate validation MD/JSON. No tracked files edited, agents spawned, CI run, host-wide process selection/signals used, or unsafe legacy process tests invoked.

## B1 — P1: index publication trusts a replaceable lock pathname and can report success after corrupting the index

Source: `src/git/file_comparison/transaction.rs:117–158,196–204` and remote script `:260–278`.

Local reservation retains a file descriptor but `IndexLock` retains only the pathname. After alternate-index apply and final verification, production writes/syncs the reserved descriptor, closes it, then blindly renames **whatever currently occupies `index.lock`** over the real index. Neither final publication nor error cleanup checks lock identity. Therefore replacing that directory entry after lock acquisition makes a successful mutation publish another file's bytes; error cleanup can also delete the replacement owner's lock. The resolved index parent is not held through directory-relative operations. `read_index` separately checks symlink metadata before a following `File::open`, leaving an additional type/link race.

**Reproduced**, using the existing guarded-patch test executable plus a Git wrapper restricted to its owned `/private/tmp/gitcraft-file-views-test-*` fixture. Immediately after successful alternate-index `git apply`, the wrapper renamed the fixture's `index.lock` to `review-displaced-lock` and wrote `FOREIGN-LOCK-BYTES` at the original lock name. Production finished its patch function successfully, installed those bytes as `.git/index`, and the next assertion's `git show` failed with `fatal: .git/index: index file smaller than expected`. Recorded published prefix hex: `464f524549474e2d4c4f434b2d4259544553`. The unchanged test alone passes; its “preserves an existing lock” assertion covers a preexisting lock before reservation, not replacement after reservation.

Remote is weaker: after reservation, `cat "$d/index" >"$lock"` reopens/follows the lock pathname. A replacement symlink can redirect/truncate an unrelated writable file before `mv -f` installs that symlink as the index; the `own=1` trap blindly removes a foreign replacement lock. This is direct source evidence; no live SSH/WSL mutation was run.

Required correction: pin the real index parent, retain original/lock identities and access metadata, perform no-follow regular-file reads, and make publication and cleanup conditional on the owned identities. Preserve foreign replacements and displaced bytes on every failure. A pre-rename identity check alone must not be advertised as an atomic identity-conditioned rename; demonstrate the remaining publication race is contained/recoverable. Reuse the standalone saver’s held-directory/create-only/recovery principles where coherent. Test lock replacement by regular file and symlink, parent replacement, cleanup after failure, and a last-moment foreign creator with recording/synthetic hooks before owned runtime fixtures. Preserve all unrelated index entries, work bytes, modes and locks, and never report success after publishing a foreign inode.

Evidence: `/private/tmp/gitcraft-fileviews-review-core-r1/index-lock-race-test.log`, `index-lock-race.jsonl`, and fixture-only wrapper `bin/git`.

## B2 — P1: staging bypasses configured Git index access policy

Source: same local lock creation/publication `transaction.rs:117–158`; remote `umask 077` reservation/publication `:260–278`. The newly created lock inode replaces the index without applying `core.sharedRepository` or preserving/verifying captured ownership/access metadata. Snapshot stamps cover index **bytes**, not its permissions/ACL. Ordinary filesystem umask becomes the final local index policy; remote always starts private.

**Reproduced configured-policy violation** under a normal 022 umask:

- Fixture configured `core.sharedRepository=0600`, original index mode0600. Real lens publication changed it to0644 on its first successful stage operation and kept0644 thereafter. Native `git add` in an independent owned fixture with that same setting kept0600. Thus the lens defeats an explicit privacy policy, rather than merely undoing an ad-hoc chmod.
- Fixture configured `core.sharedRepository=group`, original index0660. Lens published0644, losing group write. Native Git published0664, retaining group write. Remote's forced0600 similarly removes expected shared-repository access.

Both injected existing tests **passed** because their assertions omit access metadata. A plain chmod0600 without sharedRepository was also tested, but native Git likewise resets it to0644; that weaker observation is deliberately not the regression claim. No index ACL/native Windows preservation is claimed tested.

Required correction: define and apply Git-compatible configured index access policy and preserve any additional access restrictions/ACLs that this custom publication promises; otherwise fail closed before replacing the original. Capture/validate access metadata against concurrent policy drift. Include explicit0600, group sharing, changed ACL/owner/mode, absent index and remote-equivalent tests. Do not copy standalone-output privilege semantics blindly: Git's own index policy must remain coherent.

Evidence: `private-index-test.log`, `shared-index-test.log`, `index-mode.jsonl`; native comparison results recorded by reviewer command: shared0600→0600, sharedgroup→0664. All paths under `/private/tmp/gitcraft-fileviews-review-core-r1/` unless noted.

## B3 — P2 source blocker for full Windows policy claims: SACL/integrity policy is never captured or copied

Source: `src/standalone/windows_metadata.rs:23–38` requests only `Access | Owner | Group` for both capture and copy. `src/standalone/access.rs` stores/compares that returned truncated descriptor, and `save.rs:145–159` consequently treats equality of that subset as full preserved access policy. New replacement files inherit their own native security metadata; no code detects or preserves source SACL entries.

This omits object auditing and mandatory integrity labels. Microsoft documents that the integrity SID and mandatory policy are SACL ACEs and can restrict write/read/execute access: [Mandatory Integrity Control](https://learn.microsoft.com/en-us/windows/win32/secauthz/mandatory-integrity-control). Thus identical DACL/owner/group is insufficient to establish unchanged native access behavior. A source with an explicit integrity label or noninherited audit policy can be accepted and replaced with different policy without the current checker noticing. This is a source-level contract defect, **not a claimed native Windows runtime reproduction**.

Required correction: bound, capture, verify and preserve the full applicable native policy, including mandatory label/audit components, or fail closed when it cannot be safely established/preserved. If reading/copying requires privileges unavailable to the current user, return an explicit unsupported-policy error before moving MERGED. Add recording checks proving requested security-information sections, then owned native Windows fixtures with label/audit-only drift and verified descriptor equality. Do not claim Windows validated until that native gate runs.

## C1 — P2 remote compatibility gap: stock macOS SSH lacks the hard-coded fingerprint tool

Source: `transaction.rs:318,364` unconditionally invokes `md5sum` for attributes and index fingerprinting; lens loading calls this before becoming usable (`lens.rs:39`). There is no capability probe or portable fallback. macOS ships `md5`, not GNU `md5sum`; a normal remote host without GNU coreutils cannot open the lens and returns the generic fingerprint failure. Local macOS succeeds because it uses Rust MD5. This is a code-path portability finding, not a live macOS SSH test. Other remote dependencies should be assessed against actual supported shells, rather than assuming the local development PATH.

Required correction: use a verified portable hashing capability/protocol, or detect supported tools and provide an explicit actionable capability error; validate Darwin and Linux/WSL command construction/execution with bounded owned fixtures. Preserving equivalent remote staging behavior remains required; silently dropping remote support or classifying this flow as fully implemented is insufficient.

## Core behavior that survived review

- HEAD/index/work snapshots and conversion guards are substantive, not stubs: `lens.rs:24–88,117–198`; transaction rechecks HEAD OID, complete index bytes and entries, raw working bytes/type/mode, core.filemode/core.symlinks and conversion fingerprint before commit. Intent-to-add uses Git semantics, not empty-blob guessing. Conversion-dependent/binary/submodule boundaries are explicitly read-only rather than silently applying raw converted bytes. Path quoting/literal pathspecs are used. Partial-file operations modify an alternate full index before publication; no working-tree writes are introduced. B1/B2 prevent accepting the transaction as safe overall.
- Split-index compatibility was specifically challenged by converting the owned test fixture with `git update-index --split-index` before lens load. The guarded staging test still passed. No split-index failure is claimed.
- Standalone native CLI parsing preserves OsString paths, supports exact two/four arguments and optional `--`, and dispatches before repository/single-instance forwarding (`main.rs`, `standalone/cli.rs`). Checkpoint dispatch also uses args_os safely. Normal file-tool invocation does not register Git configuration; explicit Settings registration quotes literal paths, preserves unrelated keys/defaults and reports partial installation honestly (`registration.rs`).
- Standalone save guards are substantially stronger than the staging transaction: held directory-relative operations on Unix, pinned canonical ancestor handles on Windows, no-follow output opens, exact identity/bytes/stamp/access comparisons, input revalidation, create-only final installation, existing-inode recovery retained for late writers, and restore only into absence (`save.rs`, `directory.rs`). Concurrent current destinations are preserved and failure exits nonzero. Read-only outputs and aliases with inputs are refused. Unix preserves uid/gid, nonprivileged permission bits and supported access metadata or fails before moving MERGED; stripping set-id/capability/integrity signatures from edited content is intentional. macOS ACL drift is exercised natively below. Linux and Windows runtime behavior remain pending.
- Bounded file/metadata reads, explicit binary whole-side choices and conflict completion checks form real merge flows. Load/save work runs in shared background tasks; rejected/dropped workers disconnect their private receiver and surface an error. No new process selector or UI-thread child wait was found in this scoped standalone path. File limits are explicit errors rather than truncated output writes. Native file-tool window creation itself was not exercised interactively by this reviewer.
- Keep is the fourth shared reset mode, available to palette/shared menus, using existing expected HEAD/symref guards and Git's own `reset --keep`; undo also uses --keep (`repo/commit_actions.rs`). Both real native Git preservation and refusal tests passed. The guards remain separate preflight commands, like the baseline reset implementation; this review does not claim atomic compare-and-reset against an external concurrent checkout.

## Reviewer execution and limits

Runtime artifact: copied existing worktree test executable `target/debug/deps/gitcraft-5be0a566c51761f5` to `/private/tmp/gitcraft-fileviews-review-core-r1/review-tests`; SHA256 `e3316b2d2b86f2667a519340fd7cbc563837a2abb9dc65f967239cf85769cff2`. This reviewer did **not** freshly compile it. The checked-in handoff binds its application checks to5df74dbb; exact source inspection independently binds findings above to c82112a5. Runtime results are additionally bound here to the copied executable hash; do not present them as a fresh exact-candidate build.

Commands used `TMPDIR=/private/tmp`, `SHELL=/tmp/gitcraft-integration-tools/validation-shell`, Git from `/opt/homebrew/bin/git`, and integration-tool PATH. Instrumented runs additionally prefixed the fixture-only wrapper directory. That wrapper executes real Git first, modifies only explicitly named owned test repositories, and uses no process discovery, signals, SSH or cancellation.

| Reviewer run | Result | Artifact |
| --- | --- | --- |
| `review-tests --exact git::file_comparison::tests::guarded_patch_keeps_working_bytes_and_other_index_entries_and_preserves_an_existing_lock --nocapture` with original index0600 | 1 pass; mode loss recorded | `index-mode-test.log`, `index-mode.jsonl` |
| same with `REVIEW_REPLACE_LOCK=1` | 1 failure; actual index corruption after successful publication | `index-lock-race-test.log`, `index-lock-race.jsonl` |
| same with `REVIEW_PRIVATE_INDEX=1` (sharedRepository0600) | 1 pass; policy violation0644 recorded | `private-index-test.log`, `index-mode.jsonl` |
| same with `REVIEW_SHARED_INDEX=1` (sharedRepositorygroup) | 1 pass; lost group write recorded | `shared-index-test.log`, `index-mode.jsonl` |
| same with `REVIEW_SPLIT_INDEX=1` | 1 pass | `split-index-test.log`, `split-index.jsonl` |
| `review-tests standalone:: --nocapture` | 23 pass, including native macOS ACL drift/save, UI editor/close/error, registration and Windows recording tests | `standalone-tests.log` |
| `review-tests a_keep_reset_ --nocapture` | 2 pass, real Git preserve/refuse | `reset-keep-tests.log` |

All artifacts above reside in `/private/tmp/gitcraft-fileviews-review-core-r1/`. Wrapper SHA256 after all mode extensions: `bf8aed4aa41152ba82b61199484b94e863a0c9adda20c141a5903889dcf0722f`.

No native Linux/Windows, real SSH/WSL, interactive OS-window session, CI, full suite, process timeout/cancel safety suite or independent fresh build was run. Those gates remain pending; existing ignored/unsafe tests were not counted as passes. Other reviewers own SOPS privacy, rendering and broader UI semantics. History groups migration/lens/matrix in initial479f9655 feature checkpoint plus later corrections; final one-commit-per-feature assembly is parent-owned and was not treated as an implementation blocker.
