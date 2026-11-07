# Native asynchronous network experiment (#163)

`examples/async_git_transport.rs` is a loopback-only smart-HTTP protocol v2 discovery
experiment. It runs TCP connection establishment, request writes, bounded response reads
and deadlines through Tokio on one event-loop thread. It parses the server's packet-line
capability advertisement. It does not run a Git client subprocess. A test server uses a
real `git upload-pack --stateless-rpc --advertise-refs` advertisement as fixture data;
fixture creation is synchronous and outside the asynchronous transport.

Run it with Rust 1.95+:

```sh
cargo test --locked --example async_git_transport
cargo run --locked --example async_git_transport -- 127.0.0.1:8080
```

The example requests `/repo.git/info/refs?service=git-upload-pack`, sends
`Git-Protocol: version=2`, and requires an exact, length-delimited Git advertisement.
Tests exercise a fragmented real advertisement and server delay on the same single-thread
runtime, malformed packets, and deadline cancellation that closes the connection. Reads
are capped at 1 MiB. No production operation is routed through this example.

This establishes that discovery can use native asynchronous I/O; it does **not** establish
clone/fetch/push parity. Missing work includes `ls-refs` requests, negotiation, pack transfer,
sidebands, updates, partial/shallow clones, cancellation during pack/index writes, progress,
HTTP chunking, redirects, credentials/helpers, configured proxies, TLS and custom CA trust.
The loopback restriction prevents this deliberately incomplete client from handling real
credentials or replacing Git's security policy. Desktop network operations continue through
the bounded Tokio subprocess service and installed Git.

The [Git smart HTTP specification](https://git-scm.com/docs/http-protocol) and
[protocol v2 specification](https://git-scm.com/docs/protocol-v2) define discovery and
packet framing. The currently documented
[gix transport feature flags](https://docs.rs/gix-transport/latest/gix_transport/) distinguish
blocking HTTP/SSH clients from the async client, whose built-in transport is `git://`.
The reqwest feature there selects **blocking** reqwest; enabling it does not create
asynchronous HTTP Git operations. The application enables no gix networking features.

A russh integration would require an adapter plus independent validation of OpenSSH
configuration/aliases, jump hosts, agent/key/askpass authentication, known-host policy,
session identity, channel limits and platform support. It is not a plug-in replacement
for the existing OpenSSH command. The production SSH reuse in #158 therefore keeps
OpenSSH responsible for these policies and multiplexes its authenticated transport.
