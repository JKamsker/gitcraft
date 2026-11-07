# Object storage review handoff

Candidate: `c805f6eded6e150b62e3acd44a85b109d582a1ae` (tree `f029597db8b70eb952dc68a5a0ab5848378b5050`), based on `b5df68070a6df9d91cd29ca5d6270db80c9040c9`.

Disposition: **FAIL for the identified static ownership guard gap; overall independent review incomplete. No runtime reproduction or PASS claim.**

The root directed a reporting-only stop after a platform account-security block. No focused harness was written, built or executed. No new Cargo compilation, process-control experiment, path-substitution experiment, CI use or security scan occurred in this review.

## Blocking static correctness finding

The independence snapshot retains repository/object-store *paths*, alternate bytes, topology strings and refs, but no filesystem identity for the repository root, metadata directory or object-store ancestor directories. `prepare()` and `current()` test only the final components `objects`, `objects/info` and `objects/info/alternates` for being symbolic links. Neither checks `.git`, the repository root or their ancestors. The topology comparison consists of path strings, availability booleans, alternate bytes, worktree text and promisor state. It does not establish directory identity.

Consequently a `.git` ancestor symlink or a replacement directory with matching metadata can satisfy the implemented guards. The deliberate operation then acquires/removes a path-named lock, invokes Git, creates a backup and removes `objects/info/alternates` by resolving those mutable names. Local `detach()` calls `hard_link`, re-reads contents and then `remove_file`; the local branch does not even compare the identity of the alternate and backup files. The remote branch checks `-ef` at one point but still resolves mutable ancestors for subsequent `rm`. `restore()` similarly resolves the current names.

This is a definite absence of the ownership/ancestor guard required by the task, established directly in source. The concrete concurrent replacement sequence and consequences have **not** been executed. A robust fix must bind the inspected metadata/store identities and use contained, identity-safe operations for destructive rename/unlink/restore; adding only another textual recheck leaves a later replacement window. Unsupported platforms/transports should refuse independence while retaining read-only inspection. Add owned-fixture regression coverage for `.git`/ancestor links and replacements, including substitution between final verification and removal, before a later fixed-candidate review.

Exact source entry points:

- `src/git/object_storage/independent.rs:12` — `pub(crate) struct Snapshot`
- `src/git/object_storage/independent.rs:37` — `pub(super) fn prepare`
- `src/git/object_storage/independent.rs:98` — `fn current(`
- `src/git/object_storage/independent.rs:243` — `fn detach(`
- `src/git/object_storage/independent.rs:286` — `fn restore(`
- `src/git/object_storage/independent.rs:313` — `pub(super) fn make_with_checkpoints`
- `src/git/object_storage/topology.rs:93` — `pub(super) fn read(`
- `src/repo/object_storage/mod.rs:75` — `pub(super) fn make_object_storage_independent`

## Existing evidence independently checked

The author's four immutable log files match the SHA-256 values recorded in `/workspace/work/object-storage-evidence/c805f6eded6e150b62e3acd44a85b109d582a1ae/manifest.json`:

- fmt: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- clippy: `de4d3ea5c4d630f59b771416e03bd26254472dcd0d74edad29dfee3068d158a4`
- full tests: `dab44e78bc63d0a340a2fc269fc3435943fd470e6ec7868393a89f6774ce22e2`
- test list: `b30bc3c496a30e8e688d28d6f3bfe0f5fb9f7b7de8f027c576d98dc60161c0f6`

The list contains 884 unique test names. The full log reports 881 passed, 0 failed, 3 ignored and 0 filtered. The source tree matches the manifest. These are verification of existing author evidence, not independent test execution. The claimed 869-name baseline retention was not separately compared in this stopped review.

## Static scope and remaining work

Directly inspected core modules: `src/git/command.rs` object-storage diff; `src/git/object_storage/{mod.rs,files.rs,independent.rs,topology.rs,tests.rs}`; `src/repo/object_storage/mod.rs`. Small integration/glue changes, remaining hostile-environment cases, root/private ref object completeness, cancellation/timeout behavior, malformed/budget edge cases, native platform paths and whole-combination behavior remain unapproved. Existing tests provide useful coverage but none exercise repository/metadata ancestor replacement. The initial static note has been corrected to state this honest scope.

Notes branch: `codex/review-object-storage`. Application source branch remains untouched. A new exact-candidate ordinary correctness/adversarial review is required after fixes; this handoff cannot approve the final combined candidate.
