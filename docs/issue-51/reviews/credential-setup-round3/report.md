# Independent shared credential API and route-binding review R3

**FAIL on exact `0319dfa27e0fff740a954ac0abb2dd398890bd20`.** Two P2 selection/route defects and a bounded cleanup ownership residual remain. Several prior defects are corrected within static/pure/recording scope. This is not native HTTP, Windows, process-supervisor or final combined-candidate acceptance.

## Binding and execution class

Candidate tree: `c9e5543a1e00edcea5d14e94413c1c562b4db756`; author: Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>. Source came from `git archive` into `/tmp/gitcraft-credential-setup-r3-0319-source`; the moving setup checkout was not used as source. Every candidate reference below names that snapshot. No source edits, commits, pushes, external writes, PR/CI activity or subagents occurred.

Read the full API R1, direct API R2, policy feasibility note, setup R2/R3 reports and their manifests. Also inspected parent-preserved setup R1 material. Preserved equivalents were read from immutable parent commit `131cd833ea9504daec87400ec8efcacd583cfe7a`; all four requested report/manifest pairs equal the `/tmp` originals byte for byte. The 74 preserved inputs, including archive/status manifests and logs, are retained under the evidence directory and hash-bound by `prior-inputs.json`. Historical actual Git/SSH experiments were read, never rerun or reclassified as pure.

Independent execution: two standalone Rust inert adapters, compiled successfully with `rustc +1.95.0`, plus one Python pure namespace recording. An initial adapter compilation failed because of a harness-only import; corrected before execution. Three compiler invocations total, two successful binaries, each run once. The adapters import exact scope/policy modules or extract exact production function bodies with inert dependency types. No application test binary or build was run. No generated shell, Git/credential fill/reject, SSH, LFS, HTTP/transport, PTY, native fixture, timeout/cancel/kill/selector or full-suite fixture ran. Tooling Git commands only read status/objects/metadata/diffs. Primary-source HTTP retrieval below was document retrieval, not an HTTP authentication fixture.

## Findings

### R3-1 — P2: conflicting structured endpoints bypass foreign-endpoint recovery refusal

Source: `src/git/command/endpoints.rs:75–94,126–143`; `src/repo/https_recovery.rs:57–92`; `src/repo/dialogs/remote.rs:94–105,116–119`; `src/repo/operations.rs:189–219,232–293`.

`needs_separate_account` checks foreign endpoints only when `self.confirmed` is true. Conflicting canonical records deliberately set confirmed=false and keep all reported endpoints. That preserves F6's useful explicit-choice behavior, but also removes the new foreign-route refusal. There is no equivalent endpoint-versus-route check after the user chooses an endpoint in the dialog.

Minimal exact-function pure reproduction (`parser.rs`, exit0, defect assertions passed):

```text
binding.args   = ["push", "origin"]
binding.routes = ["https://git.invalid/repo"]
error = fatal: Authentication failed for 'https://git.invalid/repo'
        fatal: Authentication failed for 'https://lfs.invalid/old'
=> endpoints contains both; confirmed=false; lfs=false
=> needs_separate_account() == false
```

Both records are already accepted parser input; no forged help-link confirmation or live LFS format is assumed. This can also be a secondary native Git endpoint rather than LFS. `offer_failed_https_signin` therefore offers an operation-bound dialog with the ordinary Git proof. Selecting the foreign URL passes sign-in's membership test because that test checks `ticket.endpoints`, not `binding.routes`. Switch/Forget verifies only the ordinary Git snapshot before calling `https_reject` on the foreign selected URL. Changing only an independent secondary/LFS route remains invisible to that snapshot. The UI can thus authorize rejection or retry for a route that the retained proof never covered.

Single confirmed foreign failures, explicitly recognized LFS errors and explicit `git lfs` operations do refuse operation-bound recovery correctly. Those fixes are preserved. The gap is the ambiguous/multiple-endpoint branch, not the accepted nested same-endpoint HTTP design. This is a concrete workflow authorization defect; no transient-token leak or actual rejection is claimed.

Required correction: enforce the selected endpoint's membership in the authoritative bound routes at both account-switch and sign-in authorization, or conservatively route foreign candidates to standalone Settings inspection even when confirmation is ambiguous. Keep normal bound candidates selectable. Add a pure/headless case for this mixed record and an independent-route-only edit. An explicit user choice identifies a desired URL; it does not supply missing route-freshness proof.

### R3-2 — P2: ambient `.netrc` can authenticate before the explicitly selected helper

Source: `src/git/command/credential_policy.rs:68–72,124–134`; `src/git/command.rs:237–261`; `src/repo/dialogs/remote.rs:132–135`.

The new policy resets credential helpers, selects a username and clears actual-URL HTTP extra headers. It retains the normal home/netrc environment and does not force the native transport to obtain selected credentials before its first authenticated HTTP exchange. Git's native transport enables libcurl's optional netrc lookup. For a credential-free URL and default nonproactive authentication, its initial auth setup can return before `credential_fill`; `credential.username` is applied inside that later credential path, not by parsing the credential-free URL. See immutable [Git v2.47 HTTP setup](https://raw.githubusercontent.com/git/git/v2.47.0/http.c), specifically `init_curl_http_auth` and `get_curl_handle`.

Minimal source-derived input, **not executed**: select Alice/DUMMY_ALICE for `https://host.invalid/team/repo`; the repository-machine home has a conventional `machine host.invalid login bob password DUMMY_BOB` netrc entry; a server accepts Bob. With no initial libcurl username/password, optional netrc can supply Bob and complete authentication before the scoped helper is consulted. This inference follows Git's setup and [libcurl's optional netrc contract](https://curl.se/libcurl/c/CURLOPT_NETRC.html). No real or synthetic netrc file, account or server was opened or created by this review.

Therefore resetting the helper chain fixes the original earlier-helper defect, but does not establish the UI's broader promise that an explicit sign-in bypasses stored accounts. A retry/clone may succeed as the stored account. The selected token need not leak for this to violate account selection.

Required correction: make the selected endpoint/account authoritative before native HTTP can authenticate, or explicitly refuse unsupported ambient-auth configurations. Preserve normal profile/home/CA behavior deliberately; merely changing the username config or checking helper reset is insufficient. Include static/recording evidence for the native auth initialization path first; any later positive transport/netrc fixture remains a separately held runtime prerequisite.

### D-R2-4 residual — P3: final pathname `rmdir` can delete an unrelated empty directory

Source: `src/git/command/credential_policy.rs:146–152`.

Retaining the wrapper directory as the alias shell's CWD materially fixes the prior ordinary directory-replacement example for file cleanup. `rm -f -- ./git-remote-http ./git-remote-https` now resolves inside that retained directory, even if its former pathname moves. The earlier claim that a replacement directory's wrapper files would be deleted does **not** carry unchanged.

However, `test "$owned" -ef .` followed by `rmdir -- "$owned"` is still two pathname operations. Pure recording in `cleanup-recording.json` models this interleaving without filesystem mutation:

1. CWD holds owned D0; relative cleanup removes the two wrapper entries from D0.
2. `$owned` resolves to D0 at the identity check, which passes.
3. Another actor renames D0 and installs empty D1 at `$owned` before `rmdir` resolves it.
4. `rmdir "$owned"` removes unrelated empty D1; renamed D0 remains.

The residual consequence is deletion of an unrelated **empty directory** and retention of the renamed owned directory, not recursive deletion, a transient-secret leak or the old arbitrary-file deletion. Severity is reduced accordingly. The stronger retained-artifact ownership requirement is still unsatisfied. Keep directory removal identity-safe or leave the empty owned directory; a preceding `-ef` test is not atomic removal proof. No race, actual cleanup command, signal or cancellation fixture ran.

## Per-prior-finding disposition at0319

| Item | Disposition | Evidence / practical limit |
| --- | --- | --- |
| API-R1-1 redirect containment | **PASS scoped static** | Actual transport `$2` URL gets remote-curl's trailing slash and a late exact-URL `followRedirects=false` at policy:128–134. No mutable key enumeration is required. Native behavior not exercised. |
| API-R1-1 ordinary LFS lookup boundary | **PASS scoped static consumer design** | Alias unsets owner; native alias sets owner immediately before exec; helper refuses missing/mismatched PPID before reading credentials. Ordinary LFS → `git credential fill` cannot acquire the repo token through this helper. No hooks/LFS skipping added. No native PPID proof. |
| API-R1-2 intended-URL normalization | **PASS pure** | Exact scope handles raw trailing/leading slashes, encoded trailing slash and preserves slash-only decoded first character. The original ordinary cases and D-R2-2 cases passed. |
| API-R1-3 earlier helper/account precedence | **PASS for original helper defect; overall authoritative-account claim FAIL** | Generic reset and selected username precede final scoped helper; late actual-URL settings reinforce transport policy. URL account mismatch/password refused. Ambient netrc remains R3-2. No actual helper-chain rerun. |
| D-R2-1 effective config / race / includeIf | **PASS scoped static** | Pure API no longer enumerates or resolves caller-private aliases. Normalized production extras remove foreign inherited config before reinstalling profile and intentional command config. Actual transport injects late full-URL redirect policy in its own context. |
| D-R2-2 encoded slash-only/root collision | **PASS pure** | `/%2F`, `/%2F%2F`, `/%2F/` retain helper path `/` and do not match raw root. |
| D-R2-3 selected Git installation | **PASS static/recording contract** | Shared builder pins actual fixed selected program; custom `bind_git` rejects relative path. Alias uses that absolute Git for exec-path and operation, pins native HTTP paths, traced remote script uses pin. Repository-machine remote pin precedes outer Git. No executable/runtime compatibility certification. |
| D-R2-4 temporary artifact ownership | **FAIL residual** | Relative file cleanup improved; final empty-directory pathname removal retains the race above. |
| R3-F4a missing proof | **PASS scoped static** | Network capture error returns before command dispatch (operations:511–519); absent proof refuses dialog, switch and sign-in before workers/account mutation. Retained serialized proof errors stop retry at operations:650–659. No automatic None-proof authorization remains on traced production UI path. |
| R3-F4b LFS/secondary proof omission | **FAIL partially corrected** | Explicit LFS and single confirmed foreign routes use standalone inspection; mixed unconfirmed foreign routes bypass refusal (R3-1). No complete LFS fingerprint is claimed. |
| F6 structured failure parser | **PASS preserved** | Pure docs-before-fatal and single-foreign cases passed. Incidental links remain unconfirmed; conflicting records remain explicit choices. R3-1 concerns authorization after that choice, not regression to arbitrary first-URL confirmation. |

The actual-URL redirect conclusion is an inference from the wrapper plus [Git remote-curl URL initialization](https://raw.githubusercontent.com/git/git/master/remote-curl.c) and [URL-match precedence](https://raw.githubusercontent.com/git/git/master/urlmatch.c): no matching config path can be longer than the actual normalized base, and equal matches allow the later value. New includeIf keys do not need enumeration. This does not establish native execution or general filesystem/config atomic exclusion.

Helper/account semantics follow [Git credential config and URL parsing](https://raw.githubusercontent.com/git/git/master/credential.c). The adapter checked wrong URL account and embedded-password rejection, percent-encoded matching account acceptance, scoped env placement and exact path normalization. The selected native HTTP consumer may authorize nested Git HTTP at the same endpoint/account; no blanket nested/submodule refusal is required or claimed. Trusted hooks can read inherited token env; no general sandbox is claimed.

Windows local transient mode explicitly returns an unsupported native/MSYS identity error (policy:36–37). This is an availability limitation, not Windows completion. Remote Unix/SSH/WSL execution remains unvalidated.

## Shared-runner migration and carried setup observations

Repository-wide search at0319 found production `helper_environment` use only through `credential_command_policy`; remaining direct uses are the module implementation and pure tests. There is no production helper-only compatibility consumer in this tree. Ordinary/quiet/recorded runners flow through `extra_for`; watched/recorded uses the same extras; `run_extra` now retains normalization; local `apply_command_extras` removes foreign env before owned env and pins the program; remote requests unset before owned env at `remote/session.rs:559–564`. Credential-bearing traced remote scripts preserve the selected pin (`git/hooks.rs:222–241`). Helper-only success fixtures were replaced with no-owner-authority recording assertions.

Custom consumers must append original args after the policy prefix, install its env and normalize before reinstalling owned alias/count config. Local custom consumers must call `bind_git` on the actual absolute selected executable. Remote consumers must apply `CREDENTIAL_REMOTE_PIN` on the repository machine. `bind_git` is unused by a custom production consumer at0319; this review does not certify the separately reported history WIP `68ca8a66` or its integration, even if its copied policy/scope are identical.

The secondary Settings route is reachable: `inspect_secondary` stages actual reported endpoint entries, app/mod.rs:813–817 opens HTTPS sign-ins, and Settings offers selected-endpoint Inspect/Forget with no retry continuation. It does not perform account rejection simply by opening. This path passes static review for supported HTTPS entries; no Settings/helper execution was performed.

Prior F1/F2 scoped passes carry after exact source identity checks on connections, connections/tests, diagnostics and app/diagnostics. Parked recursive clearing and app rejection dispatch are retained. F5's only intervening doctor change is the equivalent nonminimal-boolean lint correction; its prior scoped classifier pass carries by inspected diff, not whole-file identity. App changes add the inspection-page opening described above. No new SSH/mux, diagnostics transmission, sign-out or doctor runtime proof is claimed.

## Supplied gates and remaining holds

Read and preserved exact0319 R6 JSON and all listed logs: author reports **18 selected test passes across 14 filters**, qualified all-target Clippy `-D warnings -A dead_code`, fmt and diff-check exit0. These are author executions, not reviewer reruns or independently attested application binaries. The private-alias test at policy:189 still calls `local_command` → `repository_command` → `git_cmd` → lazy shell PATH discovery. Consequently **R6 is not wholly strictly pure**, despite filenames. Owner reports an inert macOS login-shell probe was possible and no Git/helper/transport/network fixture occurred. The normalized-builder test already uses fixed `Command::new`; our independent extras adapter also uses only fixed `/recorded/git` and no discovery.

Later `73331f2d20809903300bd34d3b472d84035cd152` R7 reports the same 18 selected passes and qualified lint/fmt/diff pass. Exact diff shows only policy documentation/test changes and two alias-auth test expectation changes in repo/tests.rs; no production behavior changed. The alias test now uses fixed `apply_command_extras`, supporting the author's no-discovery classification for that later test only. R7's corrected legacy alias tests were not among the selected filters and are not claimed executed. R7 does not replace this review's target, its exact-source gates, or cure any finding here. `later-r7.diff` preserves the complete relevant diff. No separate all-target `cargo check` result was supplied in R6/R7 JSON; qualified Clippy is the observed compile/lint evidence.

Positive actual native HTTP authentication/retry, redirect/LFS ancestry, wrapper failure/stdin/stdout/exit/lifetime behavior and platform compatibility remain future runtime prerequisites. This review clears neither process supervisor/global runtime containment nor final combined checks/review. Owner subsequently reported `154c34b09123be9c97e3fd0945f3ab6dbc02c1f7` removes the final pathname rmdir and retains the empty private directory, with R8 strict-pure/recording evidence. That later delta/handoff was not reviewed in this round and receives no disposition here; a separate final-delta review is required. No permission for held fixtures is implied. Report and manifest: `/tmp/gitcraft-51-credential-setup-review-r3.md` and `/tmp/gitcraft-51-credential-setup-review-r3-manifest.json`; full evidence under `/tmp/gitcraft-credential-setup-r3-0319-evidence`.
