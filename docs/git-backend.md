# Local Git reads and compatibility fallback

GitCraft uses gitoxide (`gix`) for a deliberately small set of repeatedly refreshed local,
read-only operations. The `native-git` Cargo feature is enabled by default. Set
`GITCRAFT_GIT_BACKEND=cli` to compare or diagnose against the installed Git, or build without
that feature to remove gitoxide entirely. The public Git operation APIs keep their existing
results and error behavior.

| Operation | Execution | Compatibility boundary |
| --- | --- | --- |
| Refs, local/remote branches, tags, HEAD | In-process synchronous gitoxide reads | Includes packed refs, unborn/detached HEAD, annotated tags, remote names containing slashes, and upstream ahead/behind/gone state. |
| Remote names and fetch URLs | In-process synchronous config reads | URL rewriting, legacy remotes files, missing fetch URLs, or control characters requiring CLI display parsing use Git. |
| Worktree-private Git directory used by status/state refresh | In-process synchronous discovery | Linked worktrees retain their private state directory. |
| History/log, status porcelain, diffs | Installed Git | Exact graph ordering/decorations/encoding and filters/rename/submodule/status semantics remain authoritative. |
| Mutations, hooks, signing, LFS, merge/rebase, external tools | Installed Git | Existing per-repository mutation ordering and cancellation are preserved. |
| WSL and SSH repositories | Git on the repository's machine | Native local paths are never used to reinterpret remote repository paths. |
| Clone/fetch/push | Installed Git through the existing asynchronous subprocess runner | Credentials, SSH configuration, host-key verification, proxies, TLS, streaming and cancellation retain their established behavior. |

Native library calls perform synchronous filesystem reads and CPU work. Repo reload already
runs them in the bounded background read pool from #160. Calling these functions is not
asynchronous I/O. Remaining local processes use #160's `tokio::process` runtime; remote
commands keep the remote executor and shell pool, with SSH transport reuse from #158.

The small dispatch layer is organized around a typed `Read` request and capability checks,
with native and CLI implementations per operation. A native error runs the compatibility
implementation and returns its result, including its original error. No write operation is
inferred from command strings or silently reinterpreted.

Native reads also require Gitoxide’s inherited PATH to select an executable Git before any
relative or empty search entry, and to select the same Git as GitCraft’s sanitized/login-shell PATH
(symlink-equivalent paths are accepted). A Finder launch selecting different Apple/Homebrew
Git installations, or a relative/empty inherited PATH entry before the selected Git, uses the CLI with reason
`git-executable-context` before initializing Gitoxide. Installation discovery is cached by
Gitoxide; a later executable change likewise retains CLI execution. Windows also retains
the CLI when `EXEPATH` could select a different installation configuration, or when the app
sanitizes the inherited PATH that Gitoxide's helper would otherwise use.

Native reads require a local, trusted repository, parsed with strict config validation and
`safe.directory` ownership checks. Git installation, system, user, include/includeIf and
repository configuration are loaded. An active GitCraft profile or inherited `GIT_*`
repository/config overrides uses the installed Git, so its exact effective context wins.
Non-UTF8 reference/config values, local (`remote = .`) upstreams, ambiguous upstream
abbreviations, symbolic refs requiring Git's resolution rules, shallow/replacement/graft
histories, and upstream graph comparisons exceeding 100,000 aggregate commit visits per
refs read also fall back. Repeated visits to shared history count toward that total budget. Each operation reopens the
repository/config: branch changes and edited configuration are visible on the next read.
No global mutable repository object or credential session is shared between repositories.

To locate Git's installation-specific configuration, gitoxide may invoke the installed
Git once per application process and cache those configuration paths. This initialization
runs synchronously on a background read worker through gitoxide, outside the managed
subprocess service. Subsequent native reads load files in-process. Managed subprocess
counters exclude that one-time discovery; OS process sampling can include it. The feature
therefore reduces repeated subprocesses without claiming that native initialization never
starts Git.

`git::backend::metrics()` returns bounded per-operation counters and the latest 1024 elapsed
microsecond samples, including native/CLI fallback counts, categorical fallback reasons,
and `cli:<subcommand>` invocation counts (the `subprocesses` field counts attempted CLI
invocations, including remote commands and failures before spawn). These contain no
repository paths, Git arguments,
URLs, commit messages or credentials. They are included in copied diagnostics and in the
opt-in process/reload benchmark output. Read counters and CLI counters are separate views:
a fallback read can execute more than one CLI invocation, so their counts must not be added
together as a single operation denominator. Native failures intentionally use a categorical
reason, rather than retaining filesystem/config error text that could contain private data.

Parity tests invoke the native implementation directly and compare real Git output; they
cannot pass merely because dispatch fell back. Fixtures cover unborn and detached HEAD,
packed refs, Unicode names, branch/tag collisions, nested annotated tags, diverged/gone
upstreams, remotes with slashes/spaces, linked worktrees, non-UTF8 fallback and unsupported
URL config. Existing CLI status/diff/history tests continue to cover those compatibility
paths. Benchmark usage and measured results are in
[git-backend-baseline.md](git-backend-baseline.md).

Native networking remains experimental rather than a production routing option. See
[the async transport prototype](async-git-transport.md) for its tested protocol scope and the auth,
host-key, TLS, proxy and cancellation gaps that must be closed before replacing Git's
network path. There is no asserted “90% native” coverage target.
