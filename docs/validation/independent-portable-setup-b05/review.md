Independent Tour / Portable Setup review — FAIL

Application: `b05e51c529c12cc66bb907772e66450cd4ad9cc8` relative to `ff901aad4346ee409509b60f9c6e7b12689fde44`.

Two actionable P2 findings remain:

1. `src/git/command.rs:721–724`: network setup clones retain user-global/profile-included and destination-parent URL rewrites. A recording-only fixture for the exact network command shape shows a previewed HTTPS source routes instead to another SSH host through `url.*.insteadOf`. Import must preserve its selected public source routing while adding only required captured authentication settings. See `network-rewrite-reproducer.json`.
2. `src/portability/restore.rs:398–401`: the lock lookup compares Git porcelain path bytes with Rust canonical Windows path spelling. On Windows these differ in separators and verbatim prefix. A retry after an already-locked worktree and later failure misses its lock, re-locks it and fails before remaining steps. Use filesystem identity or Windows-aware normalization. See `windows-lock-review.json`; native Windows execution remains unperformed.

Evidence verified: exact supplied manifest hash, all34 listed source hashes, all7 log hashes, full author log809passed/3ignored/0filtered with all20new names OK. The author binary and depfile bind the expected worktree, current262source/Cargo files match application, and812tests are registered. Independent read-only reuse of that existing binary ran all20focused tests:20passed,0failed,792filtered. This is not a fresh compilation or independently rerun full suite; binary source identity cannot be proven cryptographically after the fact.

Other reviewed controls include allowlisted public serialization, absent auth/key/config/command fields, pending account/key policy and stale verification rejection, explicit profile activation, create-only destinations, exact missing branch/detached commit failure, private unchanged-config retry receipts, Practice context isolation, retained partial outcomes, bounded HEAD-tree tour documents/history, observed policy wording and guarded setup actions. Their reviewed implementation and focused tests do not resolve the two findings above.

No application source edits, real user keys/accounts, real remote network transport, Cargo invocation or CI use occurred. Recording fixtures used owned temporary directories and local executables only. This scoped review does not replace independent review of the exact final combined candidate.
