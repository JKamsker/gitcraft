This is one ignored Unix native Git/libcurl/credential-helper integration fixture,
separate from the setup clone/fetch tests. It uses the production
`credential_command_policy` and `bind_git`, plus an owned `processes::Service`.
It does not change production policy or add a server dependency.

Runtime is pending parent review and containment clearance. Prerequisites are
**unchecked**: absolute trusted native Git supporting the production Git >=2.46
version guard and its native HTTPS helper, Python 3 with stdlib TLS, and OpenSSL
supporting `req -addext`. Missing prerequisites fail explicitly; there is no skip
or fallback. Supply them through `GITCRAFT_FIXTURE_GIT`,
`GITCRAFT_FIXTURE_PYTHON`, and `GITCRAFT_FIXTURE_OPENSSL`. Do not point at wrappers
or a different worktree's application binary.

Prospective single-test selection (not executed during implementation):

```text
cargo +1.95.0 test --offline --locked -p gitcraft --lib git::command::native_https_fixture::native_https_selected_credential_contract -- --exact --ignored --test-threads=1 --nocapture
```

Expect exactly one selected ignored test. The ordinary recording-plan test only
constructs commands with `/recording/...` paths; it starts no tool or service.
The fixture module explicitly denies warnings, dead code, all standard Clippy
lints and the repository's unwrap/expect/panic restriction lints. This permits
strict scoped linting with unrelated baseline lints allowed at the command line;
it is not a claim that the entire application passes unqualified strict lint.

The six related cases use synthetic credentials exclusively:

1. Selected Basic Authorization wins over a wrong private profile helper and a
   wrong private `.netrc`; command-only remote alias/count configuration resolves,
   and exact-URL proactiveAuth=none/emptyAuth=true overrides lose to production
   policy. A real dumb-Git response reaches `ls-remote`; the private profile marker
   remains readable and TLS trusts the explicit fixture CA.
2. A same-origin path redirect is refused, with no sink request.
3. A redirect to a second loopback HTTPS origin is refused, with no sink request.
4. A wrong repository path refuses before any request.
5. A different embedded URL account explicitly refuses before any request.
6. A real secondary `git credential fill` without a designated HTTP owner refuses,
   emits no credentials, and reaches no endpoint. This is the secondary-consumer
   ancestry boundary; it does not install or execute Git LFS.

Every child has a cleared environment, private HOME/XDG/profile/system config,
private TMPDIR and explicit CA, disabled interactive prompts, and a safe system
tool PATH. The parent environment is never mutated. Production code still sees
and preserves the supplied private HOME/profile/CA; no global HTTP/netrc/trust
settings or desktop accounts/keychains are read or edited.

OpenSSL creates a fresh synthetic CA and 127.0.0.1-only leaf at runtime (RSA2048,
SHA256, two days). Three owned commands each have a 12-second timeout. Git
commands each have a 12-second timeout. All commands have fresh cancellation
tokens, 32 KiB stdout/stderr caps and a one-second pipe-drain bound. The Python
server has a five-second startup receipt deadline, 100-second internal lifetime,
105-second Service deadline, two loopback-only listeners, at most 64 requests,
8192-byte headers, one-second TLS/write timeouts and a two-second absolute header
read deadline. Its select loop polls an owned stop file every 100 ms. There are
no threads per request, subprocesses in Python, host PID selectors, signals,
watchdogs, external hosts or global client modifications.

The exact server ticket is collected on both success and failure; its normal
shutdown must exit successfully and return `CLOSED`. Stop/cleanup uncertainty is
retained alongside the primary failure. Service handles perform any necessary
owned-child cancellation. No new process-control implementation is introduced.

The private 0700 runtime root is intentionally retained and printed, including on
success: HOME/.netrc, profile, empty config, CA/key, leaf/key/CSR/extensions,
server.py, requests.jsonl, stop receipt file, and production-created TMPDIR wrapper
directories. Normal production cleanup intentionally leaves empty 0700 wrapper
directories; abrupt termination may leave wrapper files. No glob sweeper or
pathname cleanup is added. Failed/unknown cleanup must be inspected by the parent;
source presence/compilation is not native runtime acceptance.
