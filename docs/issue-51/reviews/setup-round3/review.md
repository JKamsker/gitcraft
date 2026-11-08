# Independent setup F4/F6 R3 — static review

**FAIL on exact `753040fe942c9ef96cc496b61eb0174486fda366`.** F6's R2 incidental-URL confirmation defect is corrected in static review. Normal F4 route binding is substantially improved, but route-capture failure is accepted without a proof and LFS endpoint configuration is absent from that proof. Both leave real sign-in/reject/retry paths incompletely bound. No program under test or runtime fixture was executed.

## Immutable evidence and disposition

Source exported from the exact Git object to `/private/tmp/gitcraft-setup-review-r3-753040fe`; no mutable writer source was reviewed. Read the complete R1 and R2 reports. Manifest `/tmp/gitcraft-51-setup-review-r3-manifest.json` (SHA256 `9100e7c624427e7776efcd6667b395624fe4297ea9a74b8faf4485fb13218362`) binds 16 inspected source files, unchanged-source carry checks, preserved reports/historical logs and their real commit binding. Historical artifacts are copied under the snapshot's `review-evidence` directory.

| Original item | R3 scoped result |
| --- | --- |
| F1 SSH multiplexed identity bypass | Prior **STATIC PASS carried**: exact `connections.rs` and its tests unchanged from reviewed adca. No new SSH/mux proof. |
| F2 diagnostics auto-transmission/secrets | Prior **STATIC PASS carried**: diagnostics and app diagnostics production unchanged; only test texture-delta disposal changed. Later-SHA writer test passes are described below, not claimed as independently executed on this candidate. |
| F3 parked credentials | Prior **STATIC PASS carried**: recursive clearing method and app dispatch unchanged. Scope matching still depends on separately reviewed shared API. |
| F4 stale completion/retry | **FAIL**, two route-proof gaps below. Original profile-context/generation callback protection and normal-route verification are retained. |
| F5 TLS success masking auth failure | Prior **STATIC PASS carried**: full doctor source unchanged. |
| F6 wrong endpoint inventory/recovery | **STATIC PASS for the R2 parser finding**: help/banner URLs no longer confirm an endpoint; structured conflicts require explicit choice. Inventory all-fetch/all-push correction retained. Live Git/LFS format and shared dispatch behavior remain unvalidated. |

## R3-F4a — P2: absent route proof silently permits reject and retry

Source: `repo/operations.rs:492-511`, `:189-220`, `:238-274`; `repo/https_recovery.rs:42-68`; `git/command/connection.rs:34-65`.

The new route capture returns `Result`, but `run_step` converts any capture error to `None` with `.ok()` and still executes the operation. A canonical authentication error can independently supply a confirmed URL. `offer_failed_https_signin` accepts that failure and carries `binding=None` into the dialog.

That absent proof is then treated as success by every new barrier:

- Sign-in worker uses `binding.as_ref().map_or(Ok(()), …)` at operations:245.
- No retained worker-preflight marker is inserted when binding is absent (:259-274).
- Switch-account rejection skips its before-check for None (:205) and treats None as route-current after rejection (:211-213).

Source-derived path, not an executed fixture: route inventory/config snapshot fails while the actual network operation remains runnable; actual Git returns a canonical authentication failure with a URL; the dialog is created with None proof; change only a push endpoint under the unchanged fetch/profile context; signing in or switching accounts proceeds with no effective-route verification. This is reachable through genuine metadata-read errors, including the bounded connection reader refusing oversized routing output or one failing metadata query. No particular Git exit code for a malformed unused remote was experimentally established here. A route-proof error must not be treated as proof that the route is unchanged.

Required correction: retain the capture failure and refuse operation-bound account mutation/retry until an authoritative proof can be captured. The original operation may still report its actual failure, and an explicit Settings sign-out for a user-selected URL can remain a separate unbound action; do not silently turn failed operation binding into that action. A failure with a confirmed URL but no proof must not get automatic retry authorization. Add pure/recording checks for capture error → canonical auth failure → attempted Sign in/Forget: neither reject nor retry should be scheduled, and no success continuation should be installed. Verify error propagation on the retained-worker path too.

This is stale workflow authorization, not a claimed token leak. The separately reviewed exact credential helper must still enforce its own endpoint boundary.

## R3-F4b — P2: LFS-only endpoint changes are invisible to the route fingerprint

Source: `git/command/endpoints.rs:120-150,156-175`, `repo/operations.rs:492-511,620-633` and its recognized LFS network operations at `:571-594`.

`RouteBinding` captures operation args, normal Git routes and config matching remote URL/pushurl/push, branch remote/pushremote/merge, remote.pushdefault, URL rewrites and push.default. It omits LFS endpoint settings such as `lfs.url`, `lfs.pushurl`, `remote.<name>.lfsurl` and `remote.<name>.lfspushurl`; no `.lfsconfig` identity/bytes are retained either. `operation_endpoints` returns empty routes for explicit `git lfs fetch/pull/push`, while the operation runner explicitly recognizes those as network operations. A push with an LFS hook similarly has a normal Git route snapshot without the independent LFS URL inputs.

Consequently, an LFS auth record may correctly identify an LFS URL and receive a Some binding, yet changing only the LFS endpoint config leaves the captured normal routes/config/args identical. Sign-in precheck, switch rejection checks and the serialized retained worker proof all accept that unchanged incomplete snapshot. Moving the check to another worker does not add the omitted inputs.

Source-derived example, not executed: failure identifies `https://old-lfs.fixture.invalid/repo/info/lfs`; `lfs.url` is then edited to a different LFS host/path while ordinary origin fetch/push routes and profile remain unchanged. `RouteBinding::capture` produces equal fields before/after, so verification succeeds for the old challenge. The lower-level helper may correctly withhold its old-scoped token from the new LFS endpoint; that independent defense does not make the challenge/rejection/retry current.

Required correction: bind the effective LFS route and all supported route-selection inputs when offering LFS recovery, including local `.lfsconfig` where applicable, or explicitly refuse operation-bound LFS retry/reject when authoritative LFS binding is unsupported. Preserve the user-visible ability to select the actual failed endpoint; do not silently substitute the Git remote URL. Add pure/recording cases that change only LFS URL/config inputs and prove rejection/retry is not authorized. This finding concerns high-level route freshness, not the platform review's separate actual HTTP/LFS credential dispatch implementation.

## Corrections that survive source review

- `RouteBinding::capture` now includes all ordinary effective fetch/push URLs plus matching branch/default/rewrite inputs. `verify` recaptures off the UI thread. The original R2 pushurl-only mutation is detected on the normal Some-proof path.
- `sign_in` first checks tab/profile/operation Ticket generation, then schedules the route verification worker and checks Ticket currency again when its callback arrives. The preserved serialized proof is inserted into the pending operation, retained with expectations when earlier completed steps are removed, and verified by `run_steps_counted` before executing the retried batch (`operations.rs:134-139,259-274,620-633,671-676`). No Git work was added to `https_ticket_current` or painting. New rejection work checks the Some proof before/after helper mutation and suppresses stale completion UI. These are useful barriers; neither establishes atomic exclusion against an external config writer after the final preflight read, and no such atomic guarantee is inferred.
- `AuthFailure::from_error` now parses canonical `fatal: Authentication failed for '…'`, authenticated `unable to access` errors and its explicit `LFS: <URL> … authentication required` record form (`endpoints.rs:68-111`). Ordinary help/banner URLs do not enter `reported`. One distinct reported URL confirms; multiple reported URLs remain unconfirmed and combine with attempted candidates for explicit selection. The exact R2 docs-before-fatal input therefore selects the actual fatal URL. Duplicate URL strings are coalesced. No arbitrary first URL is promoted to confirmed evidence anymore. Live Git/LFS variants and redirects are not certified by inspecting these pattern matches.
- Separate `--repo <name>` operands are now recognized rather than skipped. Literal URLs resolve through Git's command-only temporary remote/get-url metadata mechanism, preserving insteadOf/pushInsteadOf semantics without feeding expanded URLs back into mutation. These paths were inspected only; no actual Git command was run to validate their behavior in R3.

## Evidence limits and gates

**Independent execution: zero tests, zero programs under test.** Only Git object/archive/diff reads, static text inspection, historical log reads and report artifact creation occurred. No actual Git operation, helper, SSH, HTTP, live service, browser, cancellation/signal/selector, process fixture, full suite, CI/PR, tracked edit or agent was used. No native platform transport behavior is certified.

The supplied `/tmp/gitcraft-51-setup-r3-pure-validation.json` is actually bound to **later commit `5716ad6ec158d433ce83499b384779ac397cbb38`**, not this review's 753040fe. It reports 12 passing test executions across 11 selected filters and a **failed qualified Clippy command (exit101)**. Logs were preserved/read, not rerun. The diff from this review commit to 5716 changes `git/command.rs`, `git/command/credential_policy.rs`, `git/command/watched.rs`, and `git/credential_scope.rs`; the high-level route/parser/diagnostics/parked/doctor files inspected here are unchanged, but the whole later application is different. Those author tests are supporting historical source-coverage evidence, not exact-candidate runtime gates or fresh independent passes.

The later diagnostics fixture reports success after its texture-delta disposal correction; R2's historical failure remains correctly preserved rather than rewritten. The later Clippy log reports candidate-relevant issues in endpoints.rs (collapsible if), operations.rs (expect_used), and doctor classification (nonminimal boolean), plus separate shared API/test diagnostics. No final lint pass is claimed.

The shared Credentials HTTP wrapper at 5716 and its normalization/helper precedence/redirect/LFS behavior belong to the independent platform review. F3 matching and F4/F6 token dispatch depend on it, so the scoped static carries here do not clear the whole API. Final exact-source evidence, fixes/re-review of both F4 gaps, shared API clearance, global process containment and final combined-candidate validation remain required.
