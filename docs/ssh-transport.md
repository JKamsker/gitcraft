# Shared SSH transports

GitCraft reuses OpenSSH transports on Unix while keeping its independent shell pools and
quick/long/tool/test execution lanes. Repository paths are absent from transport identity,
so repositories opened under the same SSH destination can execute concurrently over one
authenticated connection. Local repositories and WSL execution are unchanged. Windows keeps
its existing SSH behavior because native Win32 OpenSSH does not provide this multiplexing.

## Configuration and isolation

A private, randomly named `/tmp/gc-*` directory has mode `0700`; generated configuration
files have mode `0600`. Short socket names also accommodate OpenSSH's temporary bind suffix
on macOS. Each GitCraft process has its own directory and never adopts another instance's
managed master. No password, key content or environment value is written into these files.

A wrapper includes the original user configuration and system configuration, or only
`GITCRAFT_SSH_CONFIG` when supplied. OpenSSH reads aliases, users, ports, identities, jump
hosts, proxies, host-key verification and authentication as before. Default multiplexing
settings are applied in the final configuration pass, after the original configuration:
`ControlMaster auto`, a private `ControlPath`, and `ControlPersist 300`. Explicit user
settings, including `ControlMaster no`, `ControlPath none`, another control socket or a
persistence policy, take precedence. Existing user-managed sockets remain user-managed;
GitCraft never stops or removes them.

The context fingerprint includes the destination, configuration contents and recursively
expanded Includes (with the original user/system resolution base retained recursively),
environment, process working directory, agent socket state, and metadata
of referenced identity/certificate/trust files. It also accounts for identity public
companions and implicit certificates (with or without the `.pub` suffix), default keys, default user/system known-host files, and rejected/forgotten
credentials. The socket adds OpenSSH's `%C` destination hash. Configuration or credential
changes select new shell pools and transports; old sessions drain independently. OpenSSH's
passwd-account home is used for defaults and tilde expansion, even if `HOME` is overridden.
The UI's connection indicator is an in-memory snapshot; background `exec_if_connected`
rechecks the current context and refuses to authenticate or reuse a stale pool.

There are deliberate compatibility fallbacks:

- `Match exec`, `Match localnetwork`, and percent tokens in security-file paths depend on
  state that cannot be safely fingerprinted here. Configurations containing these use fresh
  SSH connections per operation, with neither managed transport nor persistent-shell reuse.
  Named-user tilde paths, escaped path syntax and bracket Include globs also retain fresh
  native SSH execution when the fingerprint parser cannot resolve them exactly. This is
  conservative across all included Host sections.
- Relative system Includes cannot be faithfully replayed as user Includes through `-F`.
  Such configurations keep OpenSSH's original configuration-loading and transport behavior.
- Config root filenames containing percent or environment expansion syntax keep the literal
  original `-F` handling; replaying those as `Include` could select a different file.
- Unsupported SSH clients or unavailable private storage keep the original command line.
- Interactive terminals retain ordinary OpenSSH transport behavior and honor
  `GITCRAFT_SSH_CONFIG`. Their command construction runs on the UI thread, so it performs
  no filesystem fingerprinting or SSH capability probe.

Changing the keys offered by an already-running agent does not revoke an established SSH
connection. Forgetting/rejecting a GitCraft credential invalidates its reuse context; server
or administrator revocation still follows SSH's established-session semantics.

## Concurrency, failures and cleanup

Only pooled-shell establishment is gated per context, preventing duplicate cold handshakes
without serializing commands. Prepared streaming requests must not hold that gate while
queued in the bounded process service: remote cancellation cleanup may need a pooled shell.
Streaming commands use the same multiplexing configuration, but simultaneous streaming-only
cold starts may race into redundant initial handshakes. Once established, the master is
reused. Existing shell limits/backpressure are preserved.

OpenSSH auto mode handles missing/stale sockets and falls back to ordinary connections if a
master refuses a session, including the server's `MaxSessions` limit. Thus the six quick and
three long shells usually share one default server transport, while additional tool, test
or streaming channels can use extra connections instead of failing or blocking other lanes.
This preserves the existing concurrency limits; it does not assume every server permits ten
sessions. Commands already sent are never replayed after a disconnect.

Closing, timing out or killing a command closes its channel, rather than the shared master.
Existing remote process cleanup remains in place. At application shutdown jobs and shells
are closed first; only sockets in this instance's private directory are asked to stop
listening, and that directory is removed. A crash can leave a private directory behind;
OpenSSH's idle persistence limit retires its masters. GitCraft does not sweep unrelated
`/tmp` directories or stale sockets owned by other instances.

OpenSSH's fallback and stale-socket behavior is implemented in
[the upstream mux client](https://github.com/openssh/openssh-portable/blob/master/mux.c),
with connection fallback in
[the SSH client](https://github.com/openssh/openssh-portable/blob/master/ssh.c).

## Verification and measurement

Unit tests cover private permissions/instance isolation, context invalidation, Included
configuration, relative identities, implicit certificates, explicit user settings, final-pass
settings, passwd-home compatibility, conservative fallbacks, socket length and startup-gate
concurrency/failure release. They use `ssh -G` to compare real OpenSSH configuration results
without making network connections.

`ci/test-ssh-transport.py` starts a disposable real `sshd` with ephemeral Ed25519 host/client
keys, a pinned isolated known-hosts file and `MaxSessions 10`; it never modifies `~/.ssh`.
The opt-in integration test verifies:

- Six simultaneous pooled shells and streaming commands report the same `SSH_CONNECTION`.
- Independent commands execute concurrently.
- A prepared stream leaves the pooled startup gate available for cancellation cleanup.
- Six idle pooled shells plus twelve streaming clients overflow the server limit and still
  complete through ordinary-connection fallback, preserving the original master.
- Killing one streaming client preserves the master and other sessions.
- Forced master loss followed by a deliberately stale socket reconnects successfully.

Linux CI installs OpenSSH and runs this harness automatically. Run locally with Rust 1.95+
and an installed SSH server:

```sh
python3 ci/test-ssh-transport.py
```

For measured cold/warm production repository reloads at 1/10/50 concurrent activities, pass
an already-built test harness. Each comparison uses a separate process and isolated config;
`ssh-no-mux` explicitly disables multiplexing while preserving the existing shell pools,
and `ssh-managed` enables the defaults above:

```sh
python3 ci/test-ssh-transport.py --benchmark --binary /path/to/gitcraft-test-binary
```

The harness records accepted SSH authentications from the real server log alongside the
reload benchmark's latency, process/thread and memory observations. Loopback measurements
measure handshake/process overhead; they do not establish WAN latency or resource savings
on every deployment. Native Git and reload benchmark methodology/results are documented in
[git-backend-baseline.md](git-backend-baseline.md).

### Measured production reloads

Measured 2026-11-07 on Apple M4 Max/macOS 27.2 with Rust 1.95 and Apple Git 2.54, fixture
revision `f3c344e2`. The same saved debug executable and real GitCraft history were used
for every case; the native Git backend was forced to CLI in both transport modes. Each
case starts a fresh process, loads independent repository clones, waits for secondary
panel jobs to settle, then reloads the same tabs once. There was no compiler or other test
suite running during timing. This is one observed cold/warm pair per count, not a repeated
statistical trial. Percentiles are completion times across the concurrently loaded tabs.

Each pair is **multiplexing disabled → managed multiplexing**. Authenticated TCP connections
are counted from the server's `Accepted publickey` log entries during the initial load.

| Concurrent repositories | Initial authenticated connections | Initial completion p95 (ms) | Warm completion p95 (ms) |
| ---: | ---: | ---: | ---: |
| 1 | 7 → 1 | 470.3 → 283.5 | 203.8 → 196.1 |
| 10 | 9 → 1 | 1353.7 → 1186.4 | 800.2 → 813.3 |
| 50 | 9 → 1 | 4978.0 → 4878.2 | 3195.3 → 3196.6 |

Warm loads opened **zero new authenticated connections in both modes** because the existing
shell pools already retain their sessions. Managed multiplexing reduced cold connection
count to one while retaining concurrent channels. Warm timing differences in this localhost
sample are small and do not demonstrate a general warm-latency improvement.

For 50 repositories, measured parent CPU was 2185.6 → 2155.2 ms on the initial load and
1980.0 → 1957.0 ms on the warm load. Peak parent RSS was 128.0 → 127.4 MiB, sampled OS
threads 42 → 42 and descendant processes 10 → 10. CPU/RSS exclude SSH server processes;
persistent masters can daemonize/reparent and escape descendant sampling. These figures
therefore describe client workload overhead, not total SSH CPU/memory savings. The server
authentication log is the precise connection-count evidence.

The real SSH reuse/concurrency/overflow/cancellation/reconnect regression passed against
the same saved binary before measurement. [Compact results](git-backend-results.json)
include all cold/warm wall times, p50/p95, CPU/resource counters, binary hash and fixture
revision. Raw samples/logs remain under ignored `target/process-bench/`; ephemeral keys,
server configuration and known-hosts files are destroyed by the harness afterward.
