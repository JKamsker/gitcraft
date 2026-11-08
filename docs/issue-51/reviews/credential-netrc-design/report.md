# Selected-account/netrc design note — exact 154c34b0

**Recommendation:** for the existing Unix designated-HTTP policy, require Git 2.46 or newer on the repository machine and inject late, exact actual-URL `http.proactiveAuth=basic` plus `http.emptyAuth=false`. Keep the scoped helper supplying the full selected username/password. Refuse transient mode when the actual pinned Git cannot establish this capability; leave ordinary profile/credential-manager use available. This is an actionable source-supported design, not acceptance of unpublished code or native authentication.

Reviewed immutable candidate `154c34b09123be9c97e3fd0945f3ab6dbc02c1f7`, tree `2ead8b1615d66199c7911d6b1523b0b3fdf8a1df`, parent `73331f2d20809903300bd34d3b472d84035cd152`, exported to `/tmp/gitcraft-credential-netrc-154c-source`. Read full fresh credential/setup R3 report and parsed its complete manifest; full 154 handoff and complete owner manifest; API R1/R2, policy feasibility and setup F4 reports. No moving source was used. Companion manifest binds inputs, cited production objects and primary-source downloads.

## 1. Cleanup correction: narrow STATIC PASS

At `src/git/command/credential_policy.rs:146–156`, the alias enters its exclusive private directory and keeps that directory as its CWD while launching the foreground operation with Git -C. Cleanup now removes only `./git-remote-http` and `./git-remote-https` relative to that retained directory. The final pathname identity check and rmdir are gone. Exact diff from 0319 shows this behavior change; intervening 733 changes to this module are documentation and the fixed inert-builder test seam.

This resolves the R3/D-R2-4 **residual empty replacement-directory deletion**: cleanup no longer resolves the mutable $owned pathname to remove a directory. An unrelated empty replacement at that former pathname is left alone. The original ordinary replacement-directory file cleanup was already improved by retained CWD; do not restate its old consequence as still unchanged.

Limit: successful/error cleanup deliberately leaves an empty private 0700 directory per invocation. Abrupt termination may leave wrappers as well. No bounded artifact reclamation or cancellation/lifetime execution is proved. Do not add a pathname/glob-based directory sweeper to hide that tradeoff. This narrow pass is not a complete filesystem-ownership/native supervisor acceptance.

## 2. Why R3-2 remains valid at 154

Production policy still injects helper reset, selected username/useHttpPath, redirect refusal and actual-URL extra-header reset, but no proactive auth setting (`credential_policy.rs:68–72,124–134`). Normal home/profile/CA state is retained. Therefore the native transport can reach libcurl without first consulting that scoped helper.

Git v2.47's get_curl_handle enables optional netrc. Its init_curl_http_auth returns early for a credential-free URL when proactive mode is not enabled; credential.username is applied only in the subsequent credential-fill path. A complete ambient account can therefore authenticate first. This is a source inference, not a server/netrc experiment. [Git v2.47 HTTP implementation](https://raw.githubusercontent.com/git/git/v2.47.0/http.c)

libcurl's optional netrc contract permits host credentials when no explicit account is installed. It gives explicit username/password options priority. The corrective invariant is therefore **full selected credential installed in libcurl before the first request**, rather than only a preferred username or a later helper. [libcurl netrc contract](https://curl.se/libcurl/c/CURLOPT_NETRC.html)

## 3. Supported minimal transport correction

Add these two nonsecret settings to the existing late native-transport invocation, using the same normalized full actual `$2` URL and trailing slash already used for redirect/helper policy:

```text
http.<full-actual-url/>.proactiveAuth=basic
http.<full-actual-url/>.emptyAuth=false
```

These are additional exact-URL overrides at `credential_policy.rs:134`, not generic-only preflight values and not a mutable enumeration of matching config keys. Keep the existing helper-chain resets, selected username, exact path, URL-account refusal, no-owner refusal, extraHeader reset and redirects=false. Pass no token or encoded Authorization value in Git argv, config arguments, URLs, wrapper file contents or version diagnostics. Continue carrying the secret only through the existing transient environment/helper channel; the trusted-subprocess limitation remains explicit.

In Git v2.46, proactive Basic takes the credential-fill path even when the URL contains no username, before request execution, then sets libcurl username/password and Basic authentication from the returned credential. Its explicit empty-auth=true branch otherwise short-circuits this path, so **both overrides are required for the supported version range**. The selected scoped helper already returns a nonempty full credential. [Git v2.46 HTTP source](https://raw.githubusercontent.com/git/git/v2.46.0/http.c)

Git v2.54 retains early credential fill and explicit libcurl credential installation. Its empty-auth conditional has changed, but false remains the portable explicit policy. [Git v2.54 HTTP source](https://raw.githubusercontent.com/git/git/v2.54.0/http.c)

The libcurl source contract is concrete: override_login in 7.88.1 and 8.10.1 only enters netrc parsing when STRING_USERNAME is absent. Git's full explicit credential setters prevent that branch. No libcurl 8.16-specific environment feature is required. [libcurl 7.88.1 source](https://raw.githubusercontent.com/curl/curl/curl-7_88_1/lib/url.c), [libcurl 8.10.1 source](https://raw.githubusercontent.com/curl/curl/curl-8_10_1/lib/url.c)

The helper must remain the sole selected provider. Git's credential_fill iterates configured helpers, accepts a complete credential, and treats quit as fatal when incomplete; it does not itself obtain the origin account from netrc. [Git v2.54 credential source](https://raw.githubusercontent.com/git/git/v2.54.0/credential.c)

This design deliberately chooses username/password Basic for explicit transient sign-in. It does not claim to preserve ambient Negotiate/NTLM account selection within that mode. Normal profile-managed authentication remains separate. Restrict this proactive-secret mode to an actual HTTPS transport; otherwise refuse with an explanatory capability error rather than silently making a new preemptive plaintext credential path. Preserve selected profile and TLS/CA configuration; do not replace HOME or globally disable the user's profile/trust settings. This note addresses origin netrc precedence, not a guarantee about which account a server chooses when independent client certificates/cookies or proxy authentication are configured.

## 4. Version capability must be checked at the actual pinned launch

Git v2.45 lacks the public proactiveAuth setting; v2.46's config and implementation supply it. An unknown -c key on old Git is not a successful capability probe. Gate this feature explicitly rather than assuming the option worked. Git's 2.46 documentation also records emptyAuth interaction and the Basic scheme semantics. [Git v2.45 source](https://raw.githubusercontent.com/git/git/v2.45.0/http.c), [Git v2.46 HTTP configuration](https://raw.githubusercontent.com/git/git/v2.46.0/Documentation/config/http.txt)

Concrete placement: inside TRANSPORT_ALIAS, immediately after the absolute GITCRAFT_GIT_ORIGINAL validation at line140 and before exec-path discovery/artifact creation at lines141–146. Query **that quoted absolute executable** with --version on the repository machine, check a bounded well-formed version result against the 2.46 capability floor, and fail before HTTP/helper/operation launch if unsupported or unrecognized. Use the same executable again for exec-path, inner Git and native transport selection. No bare git lookup, local version substitution for SSH/WSL, constructor discovery, separate metadata config read, or helper execution belongs in policy construction.

Keep credential_command_policy pure. It should still produce arguments/environment without starting anything; the generated repository-machine alias performs the eventual version check as part of an otherwise authorized operation. A local startup version string is insufficient for a remote selected installation. A checked capability applies to the pinned installation contract, not a sandbox against replacement of trusted executables. Parse ordinary vendor suffixes deliberately and test recognized/unknown forms; do not claim arbitrary vendor backports or future alternate implementations were inspected. This review inspected immutable upstream2.43/2.45/2.46/2.47/2.54 sources, not every supported vendor binary.

No production --version command was executed in this review. The proposed query is a future production step; current gates must feed inert supplied version strings to a pure parser and inspect generated command construction with a fixed recorded absolute executable. Version read failure must produce a clear transient-sign-in refusal, never fall through to default native authentication/netrc. UI may pre-explain the capability floor using already-owned metadata, but the launch guard remains authoritative.

## 5. Alternatives rejected or deferred

| Proposal | Disposition |
| --- | --- |
| Only credential.username / helper reset | Insufficient: does not force native credential fill before the first exchange. |
| Only proactiveAuth=basic | Incomplete across the inspected supported range because emptyAuth=true bypasses it in 2.46/2.47. |
| Generic http.proactiveAuth / emptyAuth only | Matching URL-specific settings can win; preserve the actual transport-time full-URL approach. |
| Invent http.netrc=false or CURLOPT_NETRC config | No such supported mapping was found in inspected Git HTTP config; a libcurl C option is not a Git configuration key. |
| NETRC=/dev/null as universal solution | Not portable to older linked libcurl: that environment override was added in8.16. It is not the proposed compatibility floor. |
| CURLOPT_NETRC_FILE without native integration | A real libcurl C API, but Git does not expose it through this policy's command configuration. [libcurl netrc-file option](https://curl.se/libcurl/c/CURLOPT_NETRC_FILE.html) |
| Replace HOME with empty directory | Violates preservation of profile/home/trust behavior and adds a new artifact/config migration. |
| Put password in URL / -c Authorization header | Exposes the selected secret in command/config surfaces; unnecessary when full credential fill is supported. |
| Older-Git username-in-transport-URL workaround | Deferred, not accepted. Old Git can have different early-fill behavior for parsed URL usernames, but changing internal URLs needs its own scope/config/auth proof. The requested minimal correction can honestly refuse unsupported transient mode. |

## 6. R3 route authorization remains separately OPEN

The154 delta does not change endpoints/recovery/dialog/operation authorization. `endpoints.rs:88–94` still tests foreign routes only when confirmed=true; mixed conflicting structured records remain the R3-1 path. Do not clear it through the netrc correction.

Required separate correction: the **selected endpoint**, at each operation-bound Switch/Forget and Sign in authorization and worker/retained retry barrier, must belong to the authoritative captured route set under the existing canonical endpoint identity. A selected entry appearing only in the reported error endpoint list is not proof. Apply this regardless of confirmed/ambiguous parser status, preserve ordinary bound choices from mixed lists, and send unsupported foreign choices to explicit standalone inspection. Existing route freshness/context/generation checks remain necessary. No full F4/F6 re-review or new route-fix acceptance is claimed here.

## 7. Required inert gates before future native evidence

1. Pure version parser cases below 2.46, 2.46, current recorded Apple/vendor forms, malformed/unknown output, command failure and remote/local pin separation. Recording must prove querying exactly the pinned repository-machine executable and refusing before wrapper/operation on unsupported results.
2. Recorded transport generation includes both late actual-URL auth keys, existing route/helper/redirect/header controls and unchanged selected profile/home/CA; token absent from argv/files/diagnostics. Test command-only URL-specific proactiveAuth=none and emptyAuth=true are superseded by later equal/maximal actual-URL policy.
3. Source-derived inert auth sequence: initial credential-free URL → proactive helper fill → full selected dummy username/password → explicit curl options → no optional netrc branch. EmptyAuth override omitted must remain a negative vector. Helper refusal/missing owner/wrong endpoint must stop before native request rather than enable ambient fallback.
4. Retain owner/LFS/redirect normalization, no-secret-output and per-selected-endpoint route membership recording gates. New config must not bypass ordinary LFS refusal or reset transport owner before proactive credential fill.
5. After separate hold clearance, exact preserved-candidate native tests must still establish positive HTTP authentication, conflicting dummy netrc account, unsupported Git refusal, redirects/config precedence, helper ancestry, SSH/WSL repository-machine pin, stdin/stdout/exit/lifetime/cleanup and native platform limitations. These tests are **not authorized or executed by this note**.

## Evidence class and final disposition

Only Git object reads, static source/document retrieval, input parsing/hashing and note/manifest creation occurred. No candidate build, application constructor/test executable, pure compiled adapter, generated shell, Git/helper/netrc read/auth/HTTP/SSH/PTY/native/process-control/cancellation fixture, full suite, CI, PR, account/store mutation or subagent occurred. Downloaded official source text is evidence retrieval, not an authentication fixture. The owner R8 receipt reports 19 selected passes plus check/all-target qualified lint/fmt/diff; inspected, not rerun or independently attested here. Prior historical actual Git/OpenSSH executions retain their original classifications.

**Disposition:**154 cleanup residual resolved narrowly in static review. R3-2 netrc and R3-1 mixed-route authorization remain OPEN in the preserved candidate. The above version-gated proactive/full-credential design is supported for implementation review; no unpublished correction, final shared API, platform runtime, supervisor containment or combined candidate is accepted.
