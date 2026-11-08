# Clone, cleanup and backlink implementation receipt

Base: `940cacbc31f36b040dfcb4f347afc9bc3698ed45`.
Scope is the original #37.7, #37.12 and #50.4 text in `source-issues.json`.
No CI, PR, publishing, program-under-test Git/helper/CLI/HTTP/SSH/PTY/process tests,
or broad suite is authorized during the global runtime hold.

## #37.7 clone and sparse checkout

- `app/clone.rs` adds optional positive depth, blobless and cone folders to the
  existing form/credential retry/managed clone worker. Initial checkout downloads
  are part of the Clone action. Connection setup/profile authentication and
  per-repository worktree roots are unchanged.
- `git/clone_options.rs` validates literal bounded cone paths and uses current
  watched Git for cloning, including progress/cancellation and existing failed-
  clone cleanup. The inspected `origin/codex/clone-options` branch is provenance
  for cone-state/path/dirty guards only, not an accepted recovered implementation.
- Graph tools → Sparse checkout and downloads reads the current folder tree and
  sparse state on the tab's task pool. Applying uses the mutation queue, checks
  HEAD/patterns/settings again and refuses dirty/untracked/ignored data and
  operations in progress. Root files are always included. Non-cone conversion is
  disclosed before applying.
- Shared `repository_command` and `remote_request` enforce `--no-lazy-fetch` and
  the inherited no-lazy environment for passive reads. Streaming, timed, bounded,
  watched and regular routes use those builders. The remote overview's batched
  script is guarded too. Native local reads remain unchanged (metadata/object
  database reads, with no fetch API). Git lacking `--no-lazy-fetch` fails closed
  on guarded operations; no claim of old-Git read compatibility is made.
- Download permission is synchronous, thread-local and restored with RAII. A
  sparse apply can receive consent for that operation only. Other queued reads
  never inherit it. Missing-object diagnostics direct users to explicit consent.
  A separate consented fetch downloads reachable objects from `origin` using
  `--refetch --no-filter`; it also updates remote-tracking refs and does not
  deepen shallow history. Per-file downloads and arbitrary promisor selection
  are not offered; other promisor remotes require the terminal.

Pure/recording tests cover argv separation/options, unsafe cone paths, fail-closed
local/remote builders and consent restoration including unwind. Their source was
audited for process/filesystem work before execution: only fixed
`Command("/recorded/git")` and production extras are constructed; no PATH lookup,
App/RepoTab constructor, Git, helper, socket or filesystem fixture is invoked.
The owned real-Git clone/consent fixture is authored and compile-only while held.

Validation receipt with immutable commits/log hashes is supplied separately in
`/tmp/gitcraft-51-orchestration`; compile/pure gates do not imply runtime approval.
