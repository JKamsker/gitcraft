# Final standard user/PID namespace smoke — passed

The orchestrator authorized one final tiny standard unshare smoke after the
Docker shim startup failure was committed, pushed, and remote verified at
`82273cfb4b95abe6407de8982c80601a574a8848`. There was no security/approval
rejection to bypass; the Docker failure was an ordinary runtime EAGAIN.

Read-only inspection found current user `agent`, UID/GID 1000, has no assigned
subordinate UID/GID ranges. The only /etc/subuid and /etc/subgid entries belong
to dockerd. Therefore no distinct subordinate UID was attempted. Existing
`/usr/bin/unshare --help` documents `--map-current-user` as mapping the current
user to itself. The single permitted command was:

```text
/usr/bin/unshare --user --map-current-user --pid --fork -- /bin/sh -c <ordinary owned-child fixture>
```

The ordinary fixture read its own namespace identity/mapping, verified its
shell PID was 1, forked one child that exited 23, waited for that exact owned
child, verified status 23, and exited 37. Result: **passed, exact launcher exit 37**,
with empty stderr. Observed UID and GID maps were each `1000 1000 1`.

No mount namespace, mount-proc, host PID selector, host process cleanup,
signal, new mapping range, privileged container, service change, or denied
retry was requested. No application test, broad suite, Cargo compile, or CI
ran. Only the current user's permitted self mapping was used.

The count-only snapshot afterward was 31,318 visible processes and 31,302
zombies, matching the most recent preceding Docker snapshot. There was no
separate immediately-before unshare snapshot, so this is not a directly
measured unshare zombie delta.

This shows that self-mapped user/PID namespaces and tiny owned fork/wait work
without Docker's Go shim. It does not establish a distinct kernel UID, bypass
a controlling quota, or prove capacity for the earlier failing application
case. A later separately authorized exact test could place maintained Tini at
PID 1 inside a fresh namespace. No such test was run under this authorization.

The JSON preserves the exact command and fixture, permitted mapping,
exit/wait results, limitations, and raw-log hashes. Previous recovery evidence
remains unchanged.
