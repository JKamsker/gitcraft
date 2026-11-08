# Credential direct API correction: independent R2

**Verdict: FAIL (static/pure/recording only).** Bound to `5716ad6ec158d433ce83499b384779ac397cbb38`. Four source-established boundary findings remain: redirect config context/selection, encoded slash-only paths, selected Git installation, and temporary artifact ownership. No runtime or cross-platform acceptance is granted. API-R1-1 remains OPEN; API-R1-2 is PARTIAL; the account/helper/storage correction in API-R1-3 passes this scoped static/recording review.

## Binding and review class

The initial trace examined preserved `8855beca4b8cbd1d9ac913bfe373c0a1c521bc3a`; the final review uses immutable `git show` objects at `5716ad6ec158d433ce83499b384779ac397cbb38`, including its HTTP wrapper. No verdict is issued for the moving checkout. Exact reviewed sources are extracted under `/tmp/gitcraft-credential-direct-r2-source-5716`. Prior inputs: `/tmp/gitcraft-51-credential-api-review-r1.md`, its manifest, and `/tmp/gitcraft-51-credential-policy-static-note.md`. This report does not accept broader setup F4/F6 UI work or downstream shared-runner migrations not contained in this snapshot.

Two standalone Rust adapters were compiled and run: one at8855 and one at5716, importing the exact credential scope and policy source with inert recording dependencies. The stubs return synthetic config-key names and no operation endpoints; they launch no Git, shell, transport, HTTP, or application subprocess. Thus these gates establish Rust normalization behavior and generated arguments/environment, not Git config precedence, PPID identity or native shell execution. No candidate application build, lint, full suite, CI, real credentials, actual Git-fill/config fixture, network, signal/timeout/cancellation/cleanup runtime or subagent occurred in this R2 review. Git commands used here only read immutable source/metadata. The earlier R1 report separately inventories its actual pre-hold executions; none were rerun here.

## Finding D-R2-1 — P1 — Redirect refusal does not cover the actual command's effective config

**Source at5716:** `src/git/command/credential_policy.rs:55` (normalized preflight), `:77` (generic false), `:83` (override only enumerated names), `src/git/command/connection.rs:37` (`normalize_repository=true`), `src/git/command.rs:205` (actual builder), `:218` (removed inherited variables), `:816` (credential extra builder leaves normalization false).

The policy reads config key names through `connection_read`, which removes inherited repository/config overrides. The actual ordinary/watched credential-bearing command retains those overrides: `extra_for` adds policy arguments/environment but never enables normalization. One concrete source-level input is:

```
GIT_CONFIG_COUNT=1
GIT_CONFIG_KEY_0=http.https://host.invalid/team/repo.followRedirects
GIT_CONFIG_VALUE_0=true
```

The preflight strips these variables and therefore does not enumerate that URL-specific key. The actual command inherits it and receives only generic `http.followRedirects=false` for this setting. Git HTTP URL matching prefers the matching URL-specific key over the generic setting, irrespective of the latter being a later generic command option. Therefore an explicitly scoped command can still follow redirects. This conclusion comes from the actual builder plus [Git http_init](https://raw.githubusercontent.com/git/git/master/http.c) and [URL match selection](https://raw.githubusercontent.com/git/git/master/urlmatch.c); no HTTP or Git fixture was executed.

Even after aligning normalization, separate mutable name enumeration and command launch leave a race: a newly introduced matching URL-specific key is never overridden. Clone preflight at `dir=None` also reads `.` before the destination gitdir exists; conditional configuration can differ in the transport's actual repository context. The wrapper at5716 simply execs the HTTP transport and does not install transport-time exact-URL overrides, so it does not close these cases.

**Required correction:** establish redirect=false in the actual transport context with precedence over every matching setting, and align metadata/operation normalization. Avoid treating a mutable list of earlier key names as an immutable effective config. Exact full actual-URL transport-time overriding is a plausible direction under Git's URL-match contract, but the parent's newer proposal is unpublished and is not accepted by this report. Preserve selected profile and clone/conditional-context behavior. The ordinary matched-key fixture passes, but it assumes the missing key is already in the supplied list and cannot prove this property.

## Finding D-R2-2 — P2 — Encoded slash-only paths collide with host root

**Source:** `src/git/credential_scope.rs:114` trims every trailing decoded slash; helper equality is at `:146`.

For `https://host.invalid/%2F`, production scope construction yields an empty path and `matches_url("https://host.invalid/") == true`. The pure exact-module probe reproduced this on both8855 and5716. Git's credential parser removes raw leading slashes, decodes the remainder and trims trailing slashes while retaining its first decoded character. Consequently this encoded-only-slash URL has credential path `/`, distinct from root's absent path. See [Git credential_from_url_1](https://raw.githubusercontent.com/git/git/master/credential.c). The new helper would refuse its own `path=/` request and can answer a root lookup instead, provided the designated consumer condition also holds. This is a false refusal and a scope collision, not a network-leak runtime claim.

Ordinary paths with a trailing raw slash, multiple raw leading slashes, encoded trailing slash after a nonempty name, embedded encoded space and embedded encoded slash all pass the exact-module pure checks. Those original failures are corrected; the broad normalization claim is incomplete.

**Required correction:** retain Git's exact decoded-path trimming semantics, or reject unsupported slash-only encodings at construction. Add pure cases for `/%2F`, `/%2F%2F`, `/%2F/` and raw root, alongside the passing ordinary cases; do not broaden equality to a prefix match.

## Finding D-R2-3 — P2 — The designated transport is not bound to the selected Git installation

**Source:** `src/git/command/credential_policy.rs:138` and `:149`; initial executable resolution is `src/git/command.rs:98`.

The app chooses an executable, but the alias clears `GIT_EXEC_PATH` and invokes bare `git --exec-path`, then invokes bare `git` again for the operation. Clearing the variable does not remove the exec-path that the outer Git already prepended to PATH. For example, selected Git A with an inherited Git B exec-path containing a `git` executable can enter A and then resolve B inside the alias; PATH changes/installations can likewise change the second lookup. The HTTP originals are pinned only after that new lookup. This does not establish the promised selected Git/HTTP pair. Git explicitly prepends exec-path in [exec-cmd.c, setup_path](https://raw.githubusercontent.com/git/git/master/exec-cmd.c).

**Required correction:** carry a verified absolute selected Git executable and its corresponding absolute native HTTP transport paths into the repository-machine wrapper, and invoke those exact paths. Fail explicitly when that binding is unavailable. A generic PATH-based `git` lookup with only an absolute returned exec-path check is insufficient. This finding concerns installation/consumer binding, not a sandbox against a user deliberately configuring malicious trusted programs.

## Finding D-R2-4 — P2 — Cleanup checks pathname shape, not retained artifact identity

**Source:** `src/git/command/credential_policy.rs:143` through `:149`, especially cleanup at `:144`.

Exclusive mktemp, umask077, create-only writes and fixed basenames are useful, but no directory identity is retained. If the owned directory is renamed and an ordinary replacement directory is put at the original pathname before cleanup, both checks pass and cleanup removes the replacement's `git-remote-http`/`git-remote-https`, while leaking the original artifacts. A directory-to-symlink replacement between the checks and removal can also redirect pathname traversal. An intermediate TMPDIR component can change independently. Fixed basenames do not prove those files still belong to this invocation.

This is a static replacement counterexample under the requested owned-artifact contract, not a performed race or a claim of isolation from arbitrary same-user hooks. Standard sticky `/tmp` prevents a different user renaming this user's entry; it does not establish the stronger directory-identity property, and arbitrary inherited TMPDIR is not verified here.

**Required correction:** retain and use the owned directory identity for operations/cleanup (for example an appropriate directory handle and relative no-follow operations), refuse replacement, and keep deletion limited to verified owned artifacts. Do not equate `test -d`/`test ! -L` with an atomic identity check. Cancellation/deadline retirement remains additionally unvalidated by this static review; a shell EXIT trap is not independent proof of all contained-child cleanup paths.

## Prior finding dispositions and passing scoped observations

| Prior subfinding | Disposition at5716 | Evidence and limit |
| --- | --- | --- |
| API-R1-1 redirect boundary | OPEN | D-R2-1; known enumerated settings are overridden, but effective context/races are not contained. |
| API-R1-1 LFS/helper consumer boundary | PARTIAL, no runtime acceptance | Helper checks owner presence and immediate PPID before reading/returning credentials (`credential_scope.rs:136`); alias clears inherited owner; HTTP wrapper sets $$ then execs native transport. Git directly spawns its shell helper from its credential consumer. This is structurally plausible on Unix, but D-R2-3/4 and native gates remain. |
| API-R1-2 URL normalization | PARTIAL | Ordinary requested cases pass; D-R2-2 remains. |
| API-R1-3 earlier helpers/configured account | RESOLVED within static/recording scope | Generic helper reset, selected username/useHttpPath, matching URL-specific overrides, final scoped helper; embedded mismatched username/password rejected. No actual helper-chain rerun. |
| Explicit storage behavior | RESOLVED within scope | Policy docs: transient authoritative sign-in; helper returns immediately for store/erase; UI says not automatically saved. Separate credential-manager persistence is deliberate. |
| Clone let-chain lint correction | STATIC PASS | `src/app/clone.rs:224` combines the final constructor guard with the existing short-circuit chain; password moves only at the final guard, credentials attach only to matching opened root. No candidate clippy/build rerun claimed. |

The account construction probe rejects Bob in the URL when Alice is selected, rejects embedded password, and accepts a percent-encoded matching username. Production helper requires exact path/account/protocol/authority and refuses legacy scope=None; secrets remain in environment rather than command argv. Account precedence follows Git's credential config callback/order: empty helper clears prior entries; credential matching processes all applicable entries rather than HTTP's best-match-only rule; late selected username wins unless protocol already supplied one, which the constructor now checks. [Git credential config implementation](https://raw.githubusercontent.com/git/git/master/credential.c).

The HTTP-owner design intentionally permits nested native HTTP transports for the same endpoint/account: inherited wrappers mint a fresh owner. Ordinary LFS `git credential fill` has a different immediate parent and should refuse without bypassing hooks or omitting LFS objects. This is a trusted-consumer policy, not top-level-only authorization or a sandbox for inherited token environment. [Git shell launch](https://raw.githubusercontent.com/git/git/master/run-command.c) supports the Unix shape; [Git shell alias handling](https://raw.githubusercontent.com/git/git/master/git.c) supports foreground alias waiting. These are source inferences, not exercised PPID/wait proofs. Windows local use explicitly errors at policy:18; SSH/WSL/remote machine shell compatibility is not validated. No Windows-completion claim is accepted.

## Gates, regressions and next prerequisites

The pure adapter at5716 confirms normal slash/space/embedded-slash construction, URL-account rejection/acceptance, the residual encoded slash collision, and generated reset/account/known redirect arguments, alias suffix and dummy-secret environment placement. It does not execute generated shell text. The earlier8855 adapter provides the same normalization trace and prior policy shape, not acceptance of the newer wrapper. Logs, adapters, binaries and hashes are in the manifest.

Existing test `credential_scope.rs:245` is still an actual Git credential-fill fixture despite its `recording_` name. It supplies helper_environment alone, expects two successful credentials, and has no owner marker; that expectation now contradicts the missing-owner refusal. `command.rs:1381` has the same helper-only success expectation. Neither test was executed. Update these to match the new contract; source-string presence assertions in policy:155 are not consumer identity or artifact-ownership evidence.

Before any runtime permission: close the four findings, retain exact source/recording proofs for metadata-versus-command config, actual transport URL specificity, selected executable, owner absence/mismatch/ordinary-LFS/nested-HTTP shapes, and directory replacement/retirement. Broader callers must append original args after policy.arguments and install policy.environment; helper_environment alone is not the new supported transport API. Runtime helper ancestry, failure/cleanup/stdin/stdout/exit preservation, actual redirects, remote Unix and platform-specific behavior all remain held. This FAIL does not authorize full suite, native fixtures or process-control tests.
