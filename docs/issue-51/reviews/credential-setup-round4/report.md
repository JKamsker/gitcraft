# Credential/setup R4 — bounded two-fix correction review

**PASS within static/pure/recording scope on exact `8aaadb4fb1af291a12368c3ff0160892126885c1`.** R3-1 mixed foreign endpoint authorization and R3-2 optional-netrc account precedence are corrected in the reviewed source. The prior narrow 154 cleanup STATIC PASS is preserved. No blocking finding remains in this frozen correction scope. This is not native authentication, platform, supervisor or final combined-candidate acceptance.

## Binding and scope

Tree `9dab16ec9bee85b6aae3a01f9789d8716b1bdfb7`; immutable archive `/tmp/gitcraft-credential-setup-r4-8aaa-source`. The only commits after accepted cleanup baseline `154c34b09123be9c97e3fd0945f3ab6dbc02c1f7` are route correction `c7421c9d6192457450b87bdf478ea22faf27aee2` and netrc correction `8aaadb4fb1af291a12368c3ff0160892126885c1`. Exact complete diffs are retained in the evidence directory. Source references below refer to that archive, never the moving writer checkout.

Read the R3 report/manifest, finalized netrc design note/manifest, owner 154 handoff, exact two commits and final two-fix handoff/manifest. Previous API and setup findings retain their prior scoped dispositions; this review does not reopen unrelated authentication or older-Git implementation work. The manifest binds source blobs, inputs, supplied receipts/logs and independently executed adapter.

## R3-1: corrected

`src/git/command/endpoints.rs:178–180` centralizes authoritative membership using the existing canonical endpoint identity. `needs_separate_account:75–96` sends wholly foreign or explicit LFS failures to independent inspection while allowing mixed ordinary choices to survive. This deliberately does not treat an unconfirmed parser result as route authority.

`src/repo/https_recovery.rs:86–113` partitions mixed candidates against the captured binding: every ordinary bound choice remains in the credential dialog; foreign entries are staged in standalone HTTPS Settings. `Ticket::authorizes_endpoint:15–20` requires both candidate membership and authoritative bound-route membership. Merely injecting a foreign entry into ticket.endpoints no longer authorizes it.

Both production action barriers use that predicate before effects:

- `src/repo/operations.rs:189–205`: Switch/Forget requires a current ticket with proof and selected endpoint membership before clearing cached credentials, publishing a rejection event or scheduling the rejection worker. The captured proof is required in the worker, checked before and after rejection (`:216–228`).
- `operations.rs:235–267`: Sign in first matches the credential scope to an offered endpoint, then requires that selected endpoint's authoritative membership before setting checking state or scheduling work. A foreign selection invalidates recovery and opens independent inspection. The existing off-thread proof check, callback currency check, serialized route marker and retained-retry check remain (`:275–305,662–673`).

Rejection completion additionally checks ticket endpoint authority (`https_recovery.rs:115–127`). Existing context/generation/path/remotes checks remain intact. This is a captured-proof/freshness workflow; it is not atomic exclusion of independent external configuration writers.

Independent inert reproduction imports the exact scope implementation, extracts the exact AuthFailure parser, RouteBinding/Ticket authorization functions and actual bound/foreign partition. It verifies unconfirmed normal+mirror+foreign errors preserve both normal choices; old/new foreign routes remain unauthorized; an injected foreign ticket entry, absent proof and a route absent from ticket candidates cannot authorize. The full production action methods and worker ordering were statically traced; their worker functions were not executed by this adapter. Writer headless entry tests additionally report that foreign Sign in and Switch leave the existing Alice credential intact and schedule no worker/rejection/retry.

## R3-2: corrected in the supported source contract

`src/git/command/credential_policy.rs:57–68` retains pure construction and refuses non-HTTPS transient mode. `src/git/credential_scope.rs:131–133` adds only the protocol getter. The generated actual-transport wrapper also rejects a non-HTTPS actual `$2` before native launch (`credential_policy.rs:135`). The credential dialog limits transient input and explains the Git floor (`src/repo/dialogs/remote.rs:132–145`); ordinary profile-manager paths remain available.

The wrapper installs **both** late `http.<actual-url/>.proactiveAuth=basic` and `.emptyAuth=false` at `credential_policy.rs:139–141`, after deriving its exact actual URL and trailing slash. These accompany existing selected helper reset/account/path, redirect refusal and extraHeader reset. They are neither generic-only settings nor a stale preflight enumeration. Under the already reviewed URL-match contract, exact full actual-URL values have maximal matching specificity and later equal matches win over earlier command/repository/includeIf configuration. No new effective-config lookup or race-dependent key list is introduced.

The pinned repository-machine Git performs its own `--version` check at `:147–161` before `--exec-path`, wrapper creation or original operation. Query failure, old/malformed/unrecognized/prerelease/native-MSYS version output returns126 with a clear profile-manager alternative. Recognized upstream numeric X.Y.Z and numeric Apple forms require >=2.46. The same absolute executable supplies exec-path and launches the operation/native alias. No constructor discovery, bare local version substitute for a remote installation, unknown-key capability test or older-Git workaround was added.

The design note's pinned upstream Git 2.46/2.47/2.54 and libcurl 7.88.1/8.10.1 sources support the corrected auth sequence: proactive Basic with emptyAuth=false calls credential_fill before native request setup, the sole scoped helper supplies full selected username/password, and Git installs explicit curl username/password. The inspected optional-netrc branch requires an absent explicit username. Thus the demonstrated account-precedence defect is removed under this source contract. Helper refusal stops credential fill; it does not authorize ambient fallback. This conclusion is static source reasoning, not a live netrc/server result. [Git 2.46 HTTP source](https://raw.githubusercontent.com/git/git/v2.46.0/http.c), [libcurl 8.10.1 source](https://raw.githubusercontent.com/curl/curl/curl-8_10_1/lib/url.c)

Independent exact-policy/builder recording uses only fixed `/recorded/repository-machine/git`; it confirms the actual auth pair, HTTPS refusal, pin/guard ordering, original argv and retained HOME/profile/CA. The dummy selected token is absent from argv and generated wrapper contents; the preexisting transient helper/environment channel remains. The shared builder/profile functions are byte-identical to the accepted baseline. Native shell parsing/version execution is not proved: the writer's supplied-version Rust parser is an inert interpretation, not execution of the generated POSIX guard.

## Cleanup preservation

The cleanup/lifetime suffix is identical to 154. `credential_policy.rs:167–177` retains the private directory as CWD and retires only fixed relative wrapper files. No final pathname rmdir/-ef was restored. The narrow residual-empty-directory deletion finding stays resolved. Empty 0700 directories intentionally remain; abrupt termination can leave wrappers. No sweeper, cancellation or lifetime claim is added.

## Gates, classification and remaining limits

Independent review compiled one standalone exact-source Rust adapter and ran it once, exit0. An initial compilation failed on a harness-only missing crate-root import; correction produced the successful binary, with one harness-only visibility warning. One earlier Python adapter generator failed on a source delimiter before compilation. No production source was modified. Source/seam record, adapter and execution inventory are preserved under `/tmp/gitcraft-credential-setup-r4-8aaa-evidence`.

Supplied final R10 receipt matches all listed logs: **23 passes across 14 selected filters**, all-target check, qualified all-target Clippy `-D warnings -A dead_code`, fmt and diff-check all exit0. These are inspected writer executions, not reviewer application reruns. Earlier c742 R9 has seven selected passes and its compile/lint/format gates; R10 binds final source. Owner manifest verification matched all six source hashes, 27 artifacts and four input hashes.

The supplied C adapter embeds the exact upstream Git 2.46 init_curl_http_auth function (function hash `d1b2dc0f62a2b7ad6213e6eba17ad14826a2611d76eb45f275eecba8f584ef29` verified), replacing credential/curl operations with inert recording stubs. Its inspected log reports the positive full-credential sequence, negative emptyAuth=true/proactive-omitted vectors and refusal-without-continuation. It is pure source evidence, not Git/libcurl/helper execution; this reviewer did not rerun it.

No application constructor/test binary, generated shell, actual Git/helper/credential-fill/reject, netrc file, HTTP/SSH/PTY/native window, process-control/cancellation fixture, full suite, CI, PR or subagent ran in this review. Historical pre-hold actual Git/OpenSSH evidence remains historical, not relabeled pure.

The existing runtime hold remains unchanged. Later explicitly authorized contained native acceptance must cover actual HTTPS against conflicting dummy netrc, unsupported-version refusal, real config/redirect/helper/LFS ancestry, repository-machine pin and argv/stdin/stdout/status/lifetime/cleanup. Local Windows transient mode remains unavailable; remote Unix/SSH/WSL behavior remains unvalidated. This supported mode is HTTPS username/password Basic on recognized Git>=2.46, preserving home/profile/CA; independent client-certificate/cookie/proxy identity and trusted inherited-environment code remain the already disclosed limits. These future gates do not reopen the two demonstrated source defects, which are corrected here.
