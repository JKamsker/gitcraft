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

## #37.12 fresh-clone filter-repo cleanup

Graph tools → Clean history in a fresh clone is a reachable local-only wizard.
Capability detection and execution use the shared bounded managed Git runner,
with a tab-owned cancellation token and serialized mutation task. The source is
never rewritten: an exclusively created output folder contains a fresh mirror
cloned with `--no-local --no-hardlinks` and empty hooks/templates. No filter-repo
`--force` or source override is offered. The owned clone/maps remain after error
or cancellation; no recursive cleanup can delete pre-existing output.

Configuration is limited to 32 literal paths and 32 literal text replacements,
4096 bytes per field and 64 KiB total. Traversal/control paths and rule delimiters
are rejected. Rules are written to a newly created file (0600 inside a 0700
folder on Unix), removed through RAII after rewrite, and hidden in diagnostics.
Windows uses inherited ACLs; users must choose a private output parent. Source
inspection of [filter-repo's parser and blob replacement code](https://github.com/newren/git-filter-repo/blob/main/git-filter-repo)
confirmed last-delimiter parsing and binary-blob exclusion; regex/glob/callback
execution is deliberately not exposed.

The report includes added/deleted/rewritten refs, `fsck --full --no-reflogs`,
removed-path history checks and residual literal pickaxe searches. These are
bounded checks, not an assertion that all secrets are gone. The source mirror's
origin is removed before returning the final ref report. Execution has bounded
output (4 MiB) and stage deadlines (up to 600 seconds); timeout/cancel retains the
owned partial result for inspection.

A separate button opens publication/revocation guidance. The wizard never writes
an external remote. It directs deliberate per-ref publication through the existing
guarded publisher or terminal after review/coordination and secret revocation.
No competing publisher is added. Messages, binary blobs, historical rename names,
LFS assets, forks/caches and forge-managed refs require the disclosed extra review.

Three pure tests cover literal bounded configuration, complete ref deltas and
failed-check/redaction semantics. A headless test renders the actual result and
requires a click before publishing/revocation guidance appears, including failed-
check warnings. Their source invokes no App/RepoTab discovery or process/filesystem
fixture. A real owned filter-repo fixture checks source preservation, rewritten
content, path removal, ref changes and replacement-file cleanup; it is compile-only
under the hold and requires installed git-filter-repo when explicitly executed.
