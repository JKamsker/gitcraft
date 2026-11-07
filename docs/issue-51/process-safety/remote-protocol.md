# Remote endpoint foundation (WIP prerequisite)

The v1 endpoint is `(version, request nonce, fixed-root private directory, random capability)`, with strict parsing and redacted Debug. Its control operation is fixed stop/authenticate/read-receipt; it never accepts a PID or interpolated user operation. Every request write/flush error is potentially delivered, including zero-byte writer errors. No retry is authorized by those errors.

POSIX backend sources are embedded for Python 3 and core Perl. Capability selection occurs before repository code dispatch, with an explicit install-either support message when neither backend is available. Both own an unreaped live anchor in a new session; user code is held behind a gate until ownership/record establishment. Control/transport EOF and finite request deadlines enter the same signal-before-reap path. Private endpoints retain atomic terminal receipts. A detached supervisor survives loss of the local transport group. Deliberate session escapes and supervisor death still require stronger kernel containment and do not have an unconditional cleanup guarantee.

This checkpoint is **unintegrated WIP**. The old production remote paths remain reachable and unsafe test routes remain held. Native Windows bisect recovery needs an equivalent Job-owning endpoint. No real process, SSH, WSL or Windows gate has run. No independent review or full prerequisite acceptance is claimed.

Pure gates run:

* `CARGO_BUILD_JOBS=2 cargo +1.95.0 test --locked --lib remote::supervisor::recording_tests -- --nocapture`: 2 passed (strict identity/receipt, actual partial/flush writer failures); compilation has temporary unused WIP APIs in addition to baseline native warnings.
* `python3 -B src/remote/supervisor/recording_tests.py`: 3 passed. The actual finish routine receives a recording backend and invokes no OS signal, process discovery, fork, or command.
* `perl src/remote/supervisor/owner.pl recording`: 3 pure recording gates passed, likewise injecting operations into the actual finish routine.
* Python bytecode syntax and `perl -c` passed. Perl reports the expected warning after an exec fallback; no user command executed.

Further parser/delivery/capability/lifecycle and dispatcher gates, integration, immutable candidate review, and owned-child fixtures remain required before inherited tests can run.
