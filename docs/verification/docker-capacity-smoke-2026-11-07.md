# Rootless Docker recovery route — blocked before startup

Status: first tiny owned-container smoke could not start. No application test,
source staging, binary staging, Cargo compile, broader retry, or CI ran.

The officially authorized `debian:trixie-slim` pull succeeded. Immutable digest:
`debian@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2`.
The local image ID is
`sha256:596c6d80c879dc94eae22cb9c59a5e18f66f4f9722e170892324c282b4eb19bc`.
Runtime Docker and network guidance were read; the official registry/CDN hosts
are allowed. Daemon/credential settings were preserved and not disclosed.

An ordinary initial Docker CLI info invocation itself exhausted threads
(`runtime/cgo: pthread_create failed`). A subsequent direct-exec client with
`GOMAXPROCS=1` and `GODEBUG=netdns=go` succeeded at pulling and inspecting the
image without changing any service. This bounded client was also used for the
first container attempt.

The requested container had `--rm --init --user 1001:1001 --cap-drop ALL`,
no-new-privileges, Docker's default private PID namespace, network none,
read-only root filesystem, private writable tmpfs /tmp, and synthetic
HOME/USER/LOGNAME. Proxy variables were empty in the networkless container.
No host mounts, credentials, Docker socket, or personal HOME were exposed.
The ordinary shell fixture would have read its own namespace UID maps and
waited for only its own child/grandchild, then exited exact 37.

Docker returned 125 before the fixture ran. The daemon reported:

```text
failed to create task for container: failed to start shim: start failed:
runtime: failed to create new OS thread (have 6 already; errno=11)
fatal error: newosproc
```

Thus actual mapped UID, fork/wait capacity, and exit propagation were not
verified. The conditional one exact application test was not run. This
failure happens before entering the requested UID and blocks this Docker
route; it does not prove the kernel quota responsible.

The exact owned failed receipt
`74fd6face1cc0b48ab07d55a3728f79f932799711104f75bcffb625d915b4a4f`
was inspected afterward and returned `No such container`, confirming --rm
cleanup. No other container or host PID was selected, inspected for cleanup,
signaled, or modified. Visible process and zombie counts remained 31,318 and
31,302 before/after. The daemon has unsupported cgroup resource enforcement,
so no unsupported resource-limit claim is made.

There was no security-policy or approval rejection. All failures were ordinary
runtime resource errors. No denied-action bypass, privilege escalation,
service mutation, or destructive workspace restart occurred.

The JSON preserves the exact command, requested mounts/isolation, immutable
image binding, owned receipt, cleanup result, counts, and raw log hashes.
