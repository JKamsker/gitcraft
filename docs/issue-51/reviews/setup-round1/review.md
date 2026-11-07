# Independent adversarial setup review — ROUND1

**Verdict: FAIL for the whole assigned candidate.** Two P1 and four P2 findings remain. This verdict covers all eight assigned ideas; it does not approve a combined application candidate or any other agent's changes.

Candidate: `b0698e9b1925f61810cc3a8823bdc6292525147c`  
Base: `a850de26b0a0778ea96fe34cd1498e6e78928492`  
Branch/worktree: `t3/practice-connections-diagnostics`, `/Users/jonas/.t3/worktrees/gitcraft/t3-5b9a0fe3`  
Review platform: native macOS, Darwin arm64. Read-only review; no tracked files changed, no agents spawned, no CI/PR/forge writes, no account or external key mutation.

## Candidate and evidence integrity

- Read the applicable `AGENTS.md`, original complete issue bodies #45/#43/#47 from `/tmp/gitcraft-issue51-issues.json`, the writer handoff and validation JSON. Assessed the eight assigned ideas against those issue bodies, rather than taking the handoff checklist as acceptance.
- Verified HEAD and branch exactly match the candidate, clean `git status --porcelain=v1 --untracked-files=all`, and `git diff --check` succeeds. Tree: `ffc42379f5779f208e302e3bb07baeac74aa84ab`.
- Author and committer are `Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>`. All 17 commits in the assigned range have that author. Configured `origin` is the fork, `https://github.com/JKamsker/gitcraft`; its local remote-tracking branch equals the candidate. The writer separately reports fresh push/remote SHA verification; I did not make an additional network request to reverify the live remote.
- Copied the writer-built binary identified by its archived build log; original and copy SHA256 match: `6202f364e537253c5968c6442068bac2225d8aaa57a10a9b9a84d3b8a237fd27`. No candidate compilation occurred during this review. Also preserved the exact library test executable named by the archived focused-test logs, SHA256 `8fa263e4d210a4db3133d5cbd9194297179d514b465bd75e9cb790e123375193`; it was not broadly rerun.
- Evidence directory: `/tmp/gitcraft-51-setup-review-r1-evidence`. `verification.json` records repository/platform checks. `SHA256SUMS.json` binds the copied binaries, original input documents, archived writer logs, standalone reproducer source and outputs. `source-function-hashes.json` additionally hashes byte-extracted candidate functions used in pure checks.

## Findings

### F1 — P1: profile SSH key isolation is bypassed by an existing multiplexed master

**Source:** `src/connections.rs:721` (`ssh_command`), `:804` (`run_ssh`), `:618` (`validate_ssh_identities`); generated profile integration in `src/settings/profiles.rs:190`.

The wrapper forces `IdentitiesOnly=yes` and `-i <selected key>`, checks the effective `IdentityFile` list, then invokes ordinary OpenSSH with the user's `ControlPath` policy intact. OpenSSH can attach to an already-authenticated master for the same user/host/port. It then sends the Git command over that session without authenticating with the selected profile key. A conventional shared `ControlPath` based on `%r/%h/%p` or `%C` is independent of the profile's key. Switching from personal to work therefore does not establish the advertised push identity.

**Benign exact-binary proof:**

```sh
python3 /tmp/gitcraft-51-setup-review-r1-evidence/mux_repro.py
```

The owned fixture config has `ControlMaster auto`, an owned Unix socket `ControlPath`, `HostName fixture.invalid`, and `ProxyCommand /usr/bin/false` to prevent external transport. The selected work-key file is deliberately an invalid placeholder. A recording-only fake master replies to the OpenSSH mux protocol. The copied candidate wrapper exits **0**, with no stderr, after the master receives HELLO, alive-check, and NEW_SESSION carrying `git-receive-pack 'fixture-repo'`. No TCP connection, real SSH server, real authentication, account write or private credential is involved. See `mux-repro.json`; its packet payload records the command.

This is proof of transport reuse bypass, not a claim that a real push was made as another user. A real forge/account trial is unnecessary and deliberately omitted.

**Required correction:** disable shared-master reuse for selected-profile transport, such as forcing `ControlPath=none` together with the relevant master policy, before user configuration can select another session. Apply compatible handling to every probe/launch. Add a regression against an owned recording master. Separately audit effective `CertificateFile`/agent identities: the current guard inspects only `IdentityFile`; no certificate-authentication guarantee was independently established in this round.

### F2 — P1: raw configured command secrets are sent in the automatically opened diagnostics URL

**Source:** `src/diagnostics.rs:170` (`settings_rows`), `:150` (final report redaction), `:425` (`issue_url`); new transmission at `src/app/diagnostics.rs:83`.

The config allowlist includes arbitrary executable command values such as `core.editor` and signing programs. `settings_rows` withholds a value only if it contains `/` or `\`. A valid setting such as `core.editor = env API_TOKEN=FIXTURE_SECRET nvim` has neither separator and is copied verbatim. The complete report only receives home/user-folder redaction, not secret-value filtering. The new completion path then opens a GitHub URL containing that report in its query string **before** the user can review the issue contents. Review before clicking Submit does not prevent the URL from reaching the browser/host.

**Benign proof:** `source_repros.py` copies the exact candidate `settings_rows`, final user redactors, `issue_url` and encoder into a standalone pure harness, with the actual config parser/allowlist. No editor is executed and no browser or network is opened. `source-repros.log` contains:

```text
DIAGNOSTIC_ROWS=core.editor = env API_TOKEN=FIXTURE_SECRET nvim · Repository · config
PREFILLED_URL=https://github.com/JKamsker/gitcraft/issues/new?...API_TOKEN%3DFIXTURE_SECRET...
```

The unsafe value formatting predates this range, but its newly added automatic transmission is a candidate regression and violates the assigned “never tokens” diagnostics requirement. This is independent of the handoff's acknowledged arbitrary-freeform-panic limitation.

**Required correction:** report only presence/origin for executable and other free-text sensitive settings; validate displayed values of bounded enum/boolean settings. Sanitize the entire outgoing report and provide a local review/redaction opportunity before transmitting private panic/config content to an external issue form. Preserve the clipboard/full-report behavior without leaking secrets into a URL. Add a no-browser recording assertion for this command-value fixture.

### F3 — P2: HTTPS sign-out leaves parked worktree credentials active

**Source:** `src/repo/operations.rs:154` (`forget_https_memory`); dispatch in `src/app/panels.rs:89`; parked-view restoration in `src/repo/worktree_tab/mod.rs:305` (`take_parked`).

The sign-out event visits top-level tabs. `forget_https_memory` clears only `self.credentials` and never visits `self.parked`. Parked worktree views retain the credentials typed into those tabs. Resuming a parked worktree does not change the profile context, so `notice_connections` does not clear them either. A later network operation can therefore reuse the account the user just asked to forget/switch.

**Benign proof:** the pure harness executes the byte-extracted production method on a current view and a parked same-host view, both with a dummy credential marker. Output:

```text
HTTPS_SIGNOUT main_cleared=true parked_still_authenticated=true
```

This tests the actual clearing method with modeled view storage; it is not represented as an end-to-end UI/keychain test. The production dispatch and `take_parked` paths establish reachability. No credential helper/store was touched.

**Required correction:** recursively clear matching credentials throughout all parked views, including the path used by operation-level account switching; test reopening a parked view after sign-out and verify a recording transport never receives the old dummy account.

### F4 — P2: a stale HTTPS-switch completion recreates a sign-in dialog after connection invalidation

**Source:** `src/repo/operations.rs:178` (`switch_https` completion), `src/repo/load.rs:291` (`notice_connections`), `src/repo/messages.rs:626` (unguarded `Msg::Then`).

The new switch-account operation captures a URL and asynchronously rejects its stored credential. The completion unconditionally sets a fresh credentials dialog. Editing the active profile's connections while rejection is pending causes `notice_connections` to clear the old credentials/dialog, but it does not invalidate this continuation. When the old rejection completes, the callback recreates the old dialog under the edited account context. Its subsequent Sign in retries `last_op` under the current context, not a verified binding to the original account/URL. Keeping the original already-submitted operation pinned is appropriate; resurrecting its stale sign-in workflow after invalidation is not.

**Benign proof:** `stale_switch_repro.py` byte-extracts `switch_https` into a deterministic queue model. Helper rejection is an inert success stub; completion is delivered after the connection-edit clearing step. Output in `stale-switch.log`:

```text
STALE_HTTPS_COMPLETION_REOPENED_DIALOG=https://fixture.invalid/old-account
```

The model proves the callback's missing guard; the actual Settings/edit UI sequence was source-traced, not automated against a real account. No private credential or external service is needed.

**Required correction:** capture and compare connection identity plus the relevant tab/remote/operation generation before applying completion or permitting retry. Discard stale UI results, while retaining an accurate outcome for the already-submitted rejection. Cover an edit between rejection submission and completion.

### F5 — P2: successful TLS trace lines mask a later authentication failure

**Source:** `src/connection_doctor.rs:255`, with trace enabled at `src/git/command/connection.rs:15`.

`classify` treats any occurrence of `certificate` or `ssl connect` as a TLS failure, before checking authentication errors. Normal verbose Git/curl output after a successful TLS handshake contains `Server certificate` and `SSL certificate verify ok`. If the server then returns 401/403 or credentials are refused, the doctor diagnoses the certificate chain instead of authentication. It directs the user to fix CA/hostname settings that already worked.

**Benign proof:** `source_repros.py` compiles the exact candidate classifier and `git::is_auth_error` with a synthetic successful-TLS/HTTP-401 trace. Output:

```text
TLS chain (Git's configured CA trust): failed
Authentication: not confirmed
TLS: certificate chain or handshake failed; check the configured CA and hostname
```

**Required correction:** classify explicit failure evidence rather than incidental words anywhere in the complete trace; account for chronological completed stages and later failures. Add traces for TLS-success followed by 401/403, a completed handshake followed by TCP failure, and a genuine certificate failure. Keep credentials and raw traces out of displayed results.

### F6 — P2: HTTPS account controls omit separate push endpoints and recovery can reject the fetch account

**Source:** `src/connection_doctor.rs:63` (inventory derives only `Remote.url`); `src/settings/network_page.rs:150` (endpoint choices); `src/git/refs.rs:228` (`parse_remotes` keeps the first URL per remote name); recovery selection in `src/repo/operations.rs:278`.

For `origin` with fetch `https://fetch.fixture.invalid/team/repo` and push `https://push.fixture.invalid/team/repo`, inventory contains only the fetch endpoint. There is no control to inspect or sign out the push host. The new post-401/403 recovery obtains `r.url` in the same way, so a failed push can offer rejection/sign-in for the fetch URL. This is a concrete requirement gap in “which helper/account is in effect for each host” and “switch the wrongly stored account”; it is not merely a limitation of the doctor's read-only `ls-remote` probe.

**Benign proof:** `push-endpoint-repro.rs` runs the exact candidate `parse_remotes` on realistic `remote -v` fetch/push rows. `push-endpoint.log` records only `fetch.fixture.invalid`. Source tracing shows both new sign-in controls and the rejection target consume that fetch-only field. No Git remote, credential store, or network was mutated.

**Required correction:** enumerate and identify effective fetch and all push URLs in the sign-ins inventory. Carry the actual attempted operation/remote endpoint into auth failure recovery and reject only the explicitly selected intended URL. Test different fetch/push hosts and multiple push URLs with recording credential helpers.

## Coverage against the original eight ideas

1. **Practice repositories plus glossary (#45):** start-page entry, all four real disposable scenarios, persistent goal/hint banner, explicit background goal check, and global painter-based hover handling are present. Goal logic distinguishes abort/unrelated branch and preserves published history. Setup and Git command/stream/read paths isolate inherited repository/config, hooks/templates, signing, and native read-backend use. No additional blocking finding from this source review. Writer's 11 practice + 3 glossary tests are archived, not rerun here. Hover discovery and all scenario UI completion paths were not independently exercised on-screen in this round.
2. **Repository tour (#45):** committed README/contribution/PR-template excerpts; merge/conventional/signature/DCO observations; branches/tags; recent folder contributors/hot files; LFS/submodule/Husky/Lefthook/EditorConfig detection; identity/LFS/submodule/review actions are implemented. Blob reads use object IDs and `cat-file`, not checkout/textconv execution. Request generations/path checks and dropping the tour scope cover navigation. Sampling limits and unverified signature/history caveats are shown. The identity action is repository-scoped, rather than a reusable host identity policy; review that product choice when assessing the original “identity for this host” wording. No extra blocker claimed for it.
3. **Portable setup (#45):** versioned file; selectable repository previews; profile ID remapping, saved identities, hidden branches, remotes including push URLs, current-machine destinations/local sources, branches/worktrees/locks and allowlisted config restore; pending local credential/key reselection; create-only destinations and retained partial results are implemented. Restore uses pinned explicit transports, isolated HOME/global config/credentials, no-checkout clone, disabled hooks/templates/fsmonitor, redirects refused and allowlisted config before checkout. Receipt checks bind definition/root/config/marker; worktree retries also verify their `.git` descriptor. Malformed imported fields/refs/URLs and path names are validated; imported old paths are display-only. No new bypass was established in those paths. This is not full fidelity setup/config backup: executable/private/path/trust settings, custom theme files (fall back to a built-in), arbitrary per-worktree config, local-only objects, dirty files, stashes, LFS and submodule contents are not restored. The UI explains most exclusions; custom-theme fallback is silent in `PublicProfile::capture`. These are explicit completeness boundaries to carry into final acceptance, not a claim that all settings/config round-trip.
4. **Per-profile SSH/accounts (#43):** explicit key and named gh/glab/tea selections, host-scoped Git helpers, local forge-account use for remote repositories, immutable leased profile config and nested worker context capture exist. Token-variable cleanup, glab owner checks and tea host/alias checks materially improve separation. **Blocked by F1**; stale UI invalidation also needs F4. Effective SSH certificate identities remain a specific unverified boundary, rather than being covered by the IdentityFile tests.
5. **SSH setup helper (#43):** explicit generation, agent addition, public-key upload, and `ssh -T`; local/SSH/WSL command plans; exclusive key-generation folder; interactive PTY passphrases; public-only companion parsing; captured local upload token and account verification; remote-local key/account separation and disabled agent forwarding are present. Local and remote paths are distinguished before commands. Remote tea requires visible account metadata and an explicit UPLOAD response. Source review found no additional blocker in these actions. No live agent add, forge upload, actual remote SSH/WSL machine or non-macOS validation was performed here. The direct identity test intentionally ignores SSH aliases/proxies, which the UI explains.
6. **Connection doctor/proxy page (#47):** repository-machine Git proxy/CA/env and SSH proxy inventory, forge REST route and CLI caveat, explicit unattended `ls-remote`, timeout/cancel, stage evidence and elapsed duration are present. UI context reset drops/cancels stale jobs. **Blocked by F5.** Raw trace is classified then discarded; no trace content is rendered by the report. Ordinary HTTP(S), SSH and scp-style are handled; custom protocols/git:// and IPv6 SSH are refused. No real corporate proxy/custom CA test was run.
7. **HTTPS sign-ins (#47):** explicit inspection, configured/executed helper names with command withholding, account result, shared-store warning, URL-scoped `credential reject`, and stored-account 401/403 recovery exist. **Blocked by F3, F4, F6.** The helper itself may ignore erase; the UI appropriately says it requested rejection. Test coverage of a selected URL does not establish parked-view clearing, current context on completion, or correct push-host selection.
8. **Diagnostics (#47):** version/channel/platform/graphics/tools/sign-in/config origins; early panic hook with bounded private persistence; operation name/status/time ring omitting arguments; background collection and bounded prefilled issue URL are implemented. **Blocked by F2.** The recent-operation design avoids raw argument/path output, and panic logs use create-new temporary files/0600 and bounded safe reads. Known-form scrubbing cannot guarantee removal of arbitrary freeform panic secrets. Sending a report before local review makes that residual boundary material. Do not claim universal private-content redaction.

## Commands, outcomes and limits

New independent checks used only owned local fixtures and standalone pure/recording harnesses:

```sh
python3 /tmp/gitcraft-51-setup-review-r1-evidence/mux_repro.py
python3 /tmp/gitcraft-51-setup-review-r1-evidence/source_repros.py
python3 /tmp/gitcraft-51-setup-review-r1-evidence/stale_switch_repro.py
rustc +1.95.0 --edition=2024 /tmp/gitcraft-51-setup-review-r1-evidence/push-endpoint-repro.rs -o /tmp/gitcraft-51-setup-review-r1-evidence/push-endpoint-repro
/tmp/gitcraft-51-setup-review-r1-evidence/push-endpoint-repro
```

The standalone source harnesses compiled and their defect-observation assertions passed; that means the candidate's defects were reproduced, not that application validation passed. The initial source-harness compilation had an extraction-boundary error (a dangling doc comment); the evidence script fixes the extraction and the preserved final build/output are successful. No writer source was changed.

The mux harness uses only an exclusively owned private temporary directory/socket, its own listener thread and copied candidate child process. It makes no PID search, sends no process signal, creates no watchdog, and cannot target any unrelated process. All process payloads and credential strings are synthetic. No account/key registration, sign-out of a real helper, browser transmission, live SSH authentication, remote cleanup or legacy parallel-bisect process test was performed.

The archived writer gates report **110 focused tests passed**, build passed, formatting passed and diff checks passed. Clippy is **qualified**, exactly `cargo +1.95.0 clippy --all-targets -- -D warnings -A dead_code`, not unqualified strict lint. Exact filters, commands, counts, environment and paths remain in the copied validation JSON and `writer-logs/`; I verified every listed exit status is 0 and retained hashes. These are prior writer executions on the named candidate, not independent reruns or combined application gates.

Key evidence/log SHA256 values (remaining logs are in `SHA256SUMS.json`):

- `mux-repro.json`: `2abd01056f347e5d005f103c0941c1641574cdd55e57dc51dc1050a62ffb72b4`.
- `source-repros.log`: `51dd70b09d7c1b517a964047d876bdceb7cb1b1ea7db1707ecec1585e1c6df4d`.
- `stale-switch.log`: `0a8de83ef240833695d770ed35c6ede3825418d7048cd3960237d47bed5b66ee`.
- `push-endpoint.log`: `59d849142c82b0874aa01fbf812af3e2d39e93c91d61261495f527d8c82018de`.
- `writer-logs/gitcraft-51-t3-5b9a0fe3-final-build.log`: `1c4a0a9671aec92efb924d238d5335352b73212cdbe3b5bbe96eae71fdf49211`.
- `writer-logs/gitcraft-51-t3-5b9a0fe3-final-clippy.log`: `82f835997f77e01b696a27d8a81cd34ef67654838c8cb9331dae49bebb1531b8`.

Input hashes:

- Issues JSON: `411e76639a880c52a8f92ef5ec543e4e76befaa1f95b2be271629df23a88edfc`.
- Writer handoff: `ff7f1286271fee46c0211e887475747065850e122d4dedd33484cde0254cc4f2`.
- Writer validation JSON: `1fd6b4b9c2ab6fba6c89818b51fe18483bbe8dac687975746eadfba9a544b1fe`.

**Final recommendation:** route F1–F6 to the writer, preserve this candidate and evidence, and obtain an independent re-review of the corrected immutable candidate. No fixes were made by this reviewer. This ROUND1 does not satisfy the final independent review gate.
