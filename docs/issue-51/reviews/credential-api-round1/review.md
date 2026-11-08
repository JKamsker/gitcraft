# Independent shared credential/connection API review R1

**Verdict: FAIL — one P1 and two P2 boundary findings.** The captured connection-result API and local repository/profile normalization pass the scoped checks below. Broader setup F1–F6 UI fixes remain outside this review and are not cleared.

## Immutable binding and scope

Credentials candidate: `8796fc1db893693f43afdec9c0aaa2f255b184fe`, comprising implementation `a1b03673156d3f65d0a990c0deab0e7510088944` plus a fixture construction-error fix. Connection metadata API: `2c04e3bd52259e6fd70ae580fc2fe6824c37ae2e`. Moving writer checkout: `/Users/jonas/.t3/worktrees/gitcraft/t3-5b9a0fe3`. Source was extracted from immutable Git archives into separate `credentials` and `connection` trees under `/tmp/gitcraft-credential-api-r1-source`; mutable working files were not the review source. The credential module is byte-identical at the later connection commit, but that later commit also includes unrelated setup WIP, which was not accepted here.

Read the initial six setup findings and the three supplied API logs. This review addresses shared secret injection, Git credential URL/protocol agreement, helper precedence, transport/path containment, legacy refusal, captured result semantics and opt-in environment normalization. No implementation files were edited.

## Findings

### API-R1-1 — P1 — Helper path matching does not enforce redirect or LFS endpoint refusal

**Source:** credential candidate `src/git/command.rs:774–778`, `src/git/credential_scope.rs:127–140`; later connection tip retains the same injection at `command.rs:821–825`. Only the separate setup runner contains `http.followRedirects=false` (`src/git/command/setup.rs:159`).

The ordinary credential-bearing runner installs `credential.useHttpPath=true` and a helper checking the requested credential path. It neither refuses transport redirects nor contains secondary LFS requests. A helper checks a credential lookup, not every HTTP request that subsequently uses the returned secret.

**Redirect route, static source/documentation proof:** Git defaults to following the initial redirect chain. Its HTTP implementation enables libcurl following for the initial request. libcurl's Authorization protection is host-based; a same-host path redirect is allowed to retain authorization. An allowed repository endpoint can challenge for authentication, receive the scoped credential, then redirect that authenticated retry to a different path on the same host. That hop can carry authorization before Git can issue any new helper lookup. The helper's rejection of a *separate* lookup for the new path does not prevent this route. See [Git redirect configuration](https://git-scm.com/docs/git-config#Documentation/git-config.txt-httpfollowRedirects), [Git HTTP implementation](https://raw.githubusercontent.com/git/git/master/http.c), and [libcurl redirect/header behavior](https://curl.se/libcurl/c/CURLOPT_FOLLOWLOCATION.html). This is an inference from production wiring and primary transport contracts; no HTTP server/network experiment was performed.

**LFS route, static source proof:** current upstream Git LFS `getCredURLForAPI` deliberately returns the **Git remote URL** when its scheme and host match the LFS API endpoint, irrespective of the API path. `getCreds` fills credentials for that returned URL and then attaches them to the LFS request. Therefore a normal repository URL `https://host/team/repo.git` plus a configured same-host LFS API at `https://host/other-service` can request the allowed repository credential and apply it to that other path. A synthetic helper request directly naming `/team/repo.git/info/lfs` is refused, but that is not the lookup used in this production LFS route. See [Git LFS auth implementation, getCredURLForAPI/getCreds](https://raw.githubusercontent.com/git-lfs/git-lfs/main/lfsapi/auth.go). No installed Git LFS version, LFS process or remote was exercised; this is a source-established integration gap, not a runtime claim.

**Required correction:** establish an explicit credential-bearing transport policy before reusing this API as an exact-endpoint boundary. Refuse redirects in the actual runner with precedence that cannot silently be overridden by matching URL-specific configuration, or validate/rebind each destination through a transport capable of doing so. Refuse or explicitly authorize LFS/secondary transport use when its credential lookup can name the original Git URL for another destination; helper-path checks alone cannot distinguish those requests. Apply the policy consistently to ordinary/watched and downstream shared runners. Preserve normal uncredentialed operation behavior intentionally. Add recording policy tests first; any later real transport fixture remains subject to the parent's hold/containment gate.

### API-R1-2 — P2 — Valid trailing-slash URLs refuse their own credentials

**Source:** `src/git/credential_scope.rs:59`, `:109`, `:138` at the credentials candidate.

`for_url` separates at the first slash and percent-decodes the remainder, but retains extra leading/trailing slashes. Git's URL-to-credential parser trims leading path slashes and trailing slashes (including decoded trailing slashes). The resulting protocol path therefore differs from the scope produced for the very same URL. The exact helper comparison then refuses the intended sign-in.

**Actual synthetic Git-fill executions, completed before the subsequent hold update:** a standalone Rust adapter imports the exact immutable credential module and exports its helper/environment. Git `credential fill` then runs with only that helper, no global/system helpers, disabled prompts and dummy values. Results:

| URL supplied both to construction and fill | Scoped path | Outcome |
| --- | --- | --- |
| `https://host-a.invalid/team/repo.git` | `team/repo.git` | Accepted dummy selected credential |
| `https://host-a.invalid/team/repo.git/` | `team/repo.git/` | Exit128; refused |
| `https://host-a.invalid//team/repo.git` | `/team/repo.git` | Exit128; refused |
| `https://host-a.invalid/team/a%20b` | `team/a b` | Accepted |
| `https://host-a.invalid/team/a%2Fb` | `team/a/b` | Accepted |
| `https://host-a.invalid/team/a%2F` | `team/a/` | Exit128; refused |

These were real local Git child executions, not recording-only tests and not network authentication. Full results are `credential-fill-probes.json`. Git's primary [credential parser](https://raw.githubusercontent.com/git/git/master/credential.c) independently confirms trimming order.

**Required correction:** make constructor normalization agree with the supported Git credential parser (leading raw slashes, percent decoding and trailing decoded slash trimming), or reject unsupported forms with an actionable construction error rather than creating an unusable credential. Keep path isolation; do not repair this with prefix matching. Cover trailing slash, multiple leading slashes, encoded trailing slash and ordinary encoded spaces/embedded slashes against a recording model first; any new actual Git-fill rerun remains held.

### API-R1-3 — P2 — An earlier configured helper can override the explicitly selected account

**Source:** credential candidate `src/git/command.rs:774–778`; public ordinary/watched runners share `extra_for` (`command/watched.rs:195–198`).

`extra_for` appends the scoped helper to the existing helper chain without resetting it or constraining an earlier helper's account. Git stops once an earlier helper supplies a complete credential. In that case the selected transient account's helper is never consulted. The implementation's isolated recording fixture explicitly clears `credential.helper` before adding its helper, so it does not test this actual runner composition.

**Actual synthetic Git-fill execution before the hold update:** configure an earlier inert helper returning dummy Bob/prior-token; then install exactly the runner's `useHttpPath` plus scoped helper for dummy Alice/selected-token and fill the matching repository URL. Git exits0 and returns **Bob/prior-token**; Alice/selected-token is absent. This is an ordinary owned local Git execution with synthetic credentials, no external auth. The probe reset unrelated inherited helpers before installing these two, so no real store was read.

This can leave an explicit sign-in/retry using the same wrongly stored account, or make clone succeed under a different stored account than the form selected. The transient token itself was not leaked in this proof; the bug is failure to enforce intended authentication identity. Configured `credential.username` and URL-embedded account mismatches also need deliberate handling rather than accidental fallback.

**Required correction:** define a runner policy where supplying an explicit scoped account makes that account authoritative for the attempted endpoint, while refusing mismatched endpoints/accounts. Reset/order/bind helpers appropriately and preserve any intended post-success storage through an explicit, reviewed path. Do not broaden the scoped helper to answer mismatched usernames. Add recording coverage with an earlier complete wrong-account helper and an earlier username-only helper, configured usernames, matching/foreign endpoints and URL account mismatch. The actual Git rerun is held until separately authorized.

## Passing observations within scope

- `Credentials::helper_environment` refuses legacy `scope=None`, validates bounded single-line username/password, and refuses mutation of the username away from the scope account (`credential_scope.rs:157–165`). The constructor rejects decoded controls and unsupported protocols. A pure Rust check importing the immutable module passes protocol/host/default-port/path distinctions, separate account identity, decoded-control refusal, legacy refusal and account-mutation refusal. Exact LFS-path lookup refusal works as a lookup property; API-R1-1 explains its integration limit.
- The helper compares protocol, authority/default-port alias, exact path and any supplied username before printing credentials; duplicate recognized fields return without broadening. The runner propagates `helper_environment` errors rather than silently falling back to unbound injection, including watched/recorded routes. Secrets stay in environment/helper output rather than Git argv. This does not make that environment inaccessible to trusted subprocesses/helpers or establish a general sandbox.
- `connection_captured` at `2c04e3bd`, `src/git/command/connection.rs:38–60`, preserves nonzero `code`, raw stdout bytes, display stdout and separate stderr. A pure adapter using the exact body and a recording execute result verifies exit1, non-UTF8 stdout/raw preservation, stderr, stdin, normalization opt-in and 128KiB/10s options. Display strings intentionally use lossy UTF-8; callers needing exact stdout retain `raw`. Configured limits were inspected, not tested via timeout/cancellation/resource-exhaustion runtime.
- Local normalization (`command.rs:199–240`) removes inherited repository/object/index/config redirection and enumerated `GIT_CONFIG_KEY_/VALUE_` variables, then reinstalls the selected immutable profile configuration. Before the hold update, an exact-function builder harness with a stub selected-profile context executed three ordinary Git config reads in exclusively owned temporary repositories: conflicting inherited `GIT_DIR`, `GIT_WORK_TREE`, counted config and config parameters could not replace selected-repository data; the selected profile value was returned; a missing key retained exit1. This checks the builder and Git environment interaction, not the full profile lease/UI lifecycle.
- The remote request records the equivalent fixed unset list and preserves stdin/options; the existing transport consumes unsets before request env. Remote SSH/WSL execution, selected remote account behavior and native platform execution were not exercised or approved. This report does not treat the local normalization harness as proof of those paths.

## Execution inventory, evidence and held gates

Before the parent's classification update, this review executed **7 actual synthetic Git credential-fill calls**, **3 actual Git config reads** through the normalization adapter, and **5 ordinary owned fixture setup Git commands** (two init/config pairs and one profile-file config). Owned temporary repositories were removed by their fixture scope. No existing account/store, real secret, network authentication or external state was used. These are explicitly process executions, not pure recording. The earlier task permitted synthetic actual Git-fill; after the update, no further actual Git-fill/process fixture was run.

Four standalone Rust adapters were compiled with `rustc +1.95.0`: exact-module credential-plan, exact-function local normalization, exact-body captured-result recording, and pure scope assertions. The last pure scope check occurred after the update and starts no Git/transport process. The adapters use narrow host stubs and are not candidate application builds, full UI runs or cryptographic source-to-binary attestation.

Inspected supplied writer logs report one pure scope test, one actual dummy Git-fill helper test and a successful all-target check. The log called “recording” includes a real Git child execution; its filename is not its execution class. These writer logs were not rerun and do not contain an independent artifact hash/source attestation sufficient to claim new exact-candidate application gates here.

Evidence: `/tmp/gitcraft-credential-api-r1-evidence/`; manifest: `/tmp/gitcraft-51-credential-api-review-r1-manifest.json`. Source hashes bind both immutable API snapshots and extracted adapters/results. No tracked files changed, no CI/PR, full suite, process signal/selector/timeout/cancellation runtime, live external auth, source edits or subagents. **The process/full-runtime hold remains in force; this FAIL does not authorize further fixtures.**
