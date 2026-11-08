# Issue 51 batch handoff: learning, profile connections, diagnostics

Status: implemented and locally validated; independent adversarial review and combined application gates remain with parent. No PR or CI operation was created, dispatched, watched, or rerun. No additional agents were spawned.

Worktree: `/Users/jonas/.t3/worktrees/gitcraft/t3-5b9a0fe3`
Branch: `t3/practice-connections-diagnostics`
Base: `a850de26`
Final immutable commit: `b0698e9b1925f61810cc3a8823bdc6292525147c`
Verified origin: `https://github.com/JKamsker/gitcraft`
Verified remote branch SHA equals final commit. All 17 commits have the exact author `Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>`. Working tree is clean. Each completed unit was committed, pushed, and SHA-verified. No Cargo dependency or lockfile changes.

## Requirement checklist and feature commits

Original requirements: `/tmp/gitcraft-issue51-issues.json`, exact bodies #45, #43 and #47. Baseline audit references: `/tmp/gitcraft-51-audit-content.md`, `/tmp/gitcraft-51-audit-platform.md` and `/tmp/gitcraft-51-audit-core.md`. Parent's audit/docs commit was not cherry-picked.

### #45: Practice repositories and glossary

Primary commit: `5f67e91a2ac83a09e98adcf16cf9e89bbbf2b101`.

- [x] Home → **Practice git…**, four real disposable Git scenarios: merge conflict, diverged branches, detached HEAD, and revert an already-published commit.
- [x] Scenario goal, hint, persistent practice banner and explicit background **Check goal** result. Goal checks distinguish resolution from abort, joining the correct branch from merging elsewhere, rescuing the detached commit from an unrelated branch, and a pushed revert from rewriting the published history.
- [x] A global painter-based hover glossary explains Git words (including HEAD, upstream, fast-forward and rebase) wherever those words are painted in the app.
- [x] Exclusive temporary roots, local bare origin for the published scenario, explicit identities, no templates/hooks/signing, safe isolated Git environment across command paths; ambient native read backend refused for practice paths.

Sources: `src/app/home.rs`, `src/app/practice.rs`, `src/practice.rs`, `src/practice/tests.rs`, `src/repo/practice.rs`, `src/glossary.rs`, `src/git/command.rs`, `src/git/backend.rs`, `src/terminal/mod.rs`.
Validation: `practice::` 11 tests; `glossary::` 3 tests. A final fixture-only commit removes watcher startup as an unrelated source of goal-check invalidation; production stale-answer rejection is retained.

### #45: Tour of this repository

Primary commit: `007557de3183b4167f07c9039e7b63c98ea957d2`.

- [x] Repository toolbar **Tour** opens a generated page with committed README, contributing rules and pull request template excerpts.
- [x] Shows observed merge/squash-style history evidence, Conventional Commits, signature headers and DCO trailers; branch/tag names and observed release/* and v* schemes.
- [x] Shows frequent contributors per top folder over the last 90 UTC days and frequently changed files. It explicitly distinguishes sampled history and signature headers from complete history or verified trust.
- [x] Detects Git LFS, submodules, Husky/Lefthook and EditorConfig markers.
- [x] Buttons set repository identity, install local LFS hooks, review/initialize submodules, review hook/editor/signing requirements and copy a Signed-off-by line. Project scripts are not auto-executed.
- [x] Bounded metadata commands, generation/path checks, scoped cancellation and stale-answer rejection. Navigating elsewhere drops/cancels the tour.

Sources: `src/tour.rs`, `src/tour/tests.rs`, `src/repo/tour/mod.rs`, `src/repo/toolbar.rs`, `src/repo/navigation.rs`, `src/git/command.rs`.
Validation: `tour::` 3 tests, including repository tab stale results/navigation cancellation. History sampling is capped at 5000 commits and explicitly described in the UI.

### #45: New machine, same setup

Primary commit: `d337702ea3e3bbf07478963f26de6be3debf3e3a`.

- [x] Home **Export setup…** and **Import setup…**, versioned single-file JSON, explicit import preview and repository selection, current-machine destination/local-source pickers, progress/cancel and per-repository outcomes.
- [x] Profiles, safe typed settings/preferences/theme policy, saved author identities, recent repository remotes/push URLs, repository-local author, hidden refs, checkout branch/commit, linked worktrees and lock state, and allowlisted repository Git configuration round-trip.
- [x] Profiles are added with remapped IDs. Existing active profile remains active. Key/account/signing selections requiring machine-local setup are recorded as pending; **Finish setup** verifies exact account expectations and reselects local key material before activation. Direct activation of pending profiles is refused.
- [x] Selected repositories clone into fresh create-only destinations; linked worktrees use fresh paths. Secondary remotes are fetched through the isolated transport to restore secondary-only worktree branches. Explicitly chosen local sources do not contact imported network URLs and retain default origin branch restoration when no portable URL exists.
- [x] Create-only writes, credential-free validated HTTPS/SSH URLs, pinned source transport, redirect refusal, isolated HOME/config/credential handling, explicit selected profile helpers only, no ambient URL rewrites/.netrc/agent identity fallback/hooks/templates/custom filters/fsmonitor. Offline mode blocks network clones/fetches.
- [x] Scoped managed tasks/processes, bounded execution, cancellation, retained partial clones/worktrees and receipt-verified in-session retry. Missing branch/commit is an explicit incomplete result, never a substitute checkout.
- [x] Existing executable-adjacent portable mode behavior was preserved separately from setup export/import.

Sources: `src/portability.rs`, `src/portability/{repository,restore,url,config,tests}.rs`, `src/app/setup/{mod,files,pending,tests}.rs`, `src/app/profiles.rs`, `src/settings/profiles.rs`, `src/git/command/setup.rs`, `src/git/command/setup/tests.rs`, `src/app/home.rs`.
Validation: `portability::` 14 tests, `app::setup::` 6 tests (including egui interaction through export/import), `git::command::setup::` 3 tests. Hostile environment/global/profile/parent config tests use a recording Git transport and actual Git credential plumbing; no external clone is attempted.

Explicit portability boundaries shown in the UI: this is a portable setup description, not an object/working-file backup. Uncommitted files, stashes, local-only objects, LFS content and submodules need their own backup/setup. Config import is allowlisted, not arbitrary `.git/config`: author, tracking, line endings, safe fetch/pull/rebase/merge/push/status/diff preferences are restored; executable/include/trust/credential/path settings are excluded and require local setup. SSH aliases must be reviewed/changed to actual hostnames. SSH/signing keys, credential folders, tokens and executable tool commands are never exported. Retry receipts survive within the current app session; partial files remain on disk after exit. Network-unavailable or missing-ref restores are correctly reported incomplete.

### #43: Each profile's SSH key and forge account

Primary commit: `eec7be57937c4cc4178cb21bf6a6c042510b9c64`.

- [x] Settings → Profile connection controls select an SSH key, named gh/glab account and host (optional independent auth configuration directory), and a tea login alias/HTTPS host.
- [x] Profile Git config contains the fixed SSH wrapper/selected identity and host-scoped credential helpers. Per-repository overrides retain Git's normal precedence.
- [x] Selected account credentials are resolved with bounded managed commands; ambient token variables are removed; gh username is pinned, glab owner/token verified, tea login and HTTPS host matched. No global CLI active-account mutation.
- [x] Immutable leased config snapshots and task-submission Context capture keep queued/in-flight work bound to its initiating profile even across edits/deletion/activation. Nested workers inherit both Context and cancellation.
- [x] Fixed SSH helper validates options and expanded identities before transport, rejects additional identity fallback, and preserves normal Git I/O and SSH ports. Remote repository commands do not receive local keys/account secrets.
- [x] Editing connection identity invalidates credentials, sign-in dialogs and account-dependent PR/CI results on active or parked tabs; stale result generations are rejected.

Sources: `src/connections.rs`, `src/connections/tests.rs`, `src/settings/connection_page.rs`, `src/settings/profiles.rs`, `src/tasks.rs`, `src/git/command.rs`, `src/forge/{cli,gitea,gitlab,github}*`, `src/repo/load.rs`, `src/repo/ci.rs`, `src/repo/pr_view.rs`, `src/main.rs`.
Validation: `connections::tests::` 25 tests; `settings::profiles::` 8 tests; `repo::load::` 2 tests. Includes actual SSH wrapper/probe behavior against recording executables, synthetic option/identity checks, real OpenSSH -G fixture expansion and inherited-context/cancellation tests.

### #43: SSH key helper

Primary commit: `e4fa3b684f076f01078663bc92d6c5743bd8eb61`.

- [x] Repository external-link menu → **Set up SSH…** provides four concrete actions: create an ed25519 key, add it to the agent, register its verified public companion with GitHub/Forgejo/Gitea, and test SSH with ssh -T. Successful local key creation can select it for the current profile.
- [x] Passphrases are entered through PTY interaction, not Rust args/settings/log fields. Creation uses a new exclusive folder and refuses overwrites.
- [x] Repository-machine command plans cover local, SSH-remote and WSL execution. Remote actions generate/use keys and installed account helpers there; SSH transport disables agent forwarding. No private key is copied between machines.
- [x] Public key companion is validated/reduced to allowed OpenSSH fields before upload. Local API upload uses verified selected forge account. Remote gh is explicitly named/verified; remote tea prints alias/account metadata and requires an explicit UPLOAD response.
- [x] Scoped background work, deadlines, cancellation, PTY completion code, clear failure/result states and protection against abandoning unfinished setup. SSH auth tests preserve nonzero forge greeting conventions rather than falsely calling every exit 1 a failure.

Sources: `src/repo/ssh_setup/{mod,plan,tests}.rs`, `src/forge/ssh_keys.rs`, `src/forge/ssh_keys/public_key.rs`, `src/repo/toolbar.rs`, `src/terminal/mod.rs`.
Validation: `repo::ssh_setup::` 15 tests; `forge::ssh_keys::` 4 tests. Synthetic command plans ran before preservation/owned-child PTY/recording/loopback tests. No real user's key, agent, forge account or remote server was modified. Tea's named-login display and ssh-keys add syntax were checked against upstream source: https://gitea.com/gitea/tea/src/branch/main/cmd/login.go and https://gitea.com/gitea/tea/src/branch/main/cmd/sshkeys/add.go .

### #47: Connection doctor and proxy page

Primary commit shared with HTTPS page: `5d42fb2e915cf1135b8d7f4cc3bce02cbf30bd58`.

- [x] Settings → **Network**, active-profile repository selector, explicit inspect, remote selector, **Test connection** and cancellation.
- [x] Shows URL-matched Git HTTP proxy, remote-specific proxy override (including explicitly disabled proxy), environment proxy presence/redacted endpoints, forge REST client's actual environment route, CLI-environment caveat, SSH ProxyCommand/ProxyJump presence and command override markers. Commands/secrets are withheld.
- [x] Actual repository-machine ls-remote uses the normal repository/profile/remote command path and real custom-CA configuration. Unattended prompts are disabled, execution/output bounded, private redacted curl trace discarded after classification.
- [x] Reports DNS/TCP/TLS/auth stage evidence/failures and total ls-remote duration; specifically recognizes proxy 407, TLS trust errors and auth failures. It distinguishes unconfirmed stages and public-readable success from verified named authentication. Disabled TLS verification is explicitly reported NOT verified.
- [x] Bounded shared task/process execution, captured context, reset/cancel on context change, stale replies discarded; no background IO from painting the page.

Sources: `src/settings/network_page.rs`, `src/connection_doctor.rs`, `src/git/command/connection.rs`, `src/settings/mod.rs`, `src/app/mod.rs`, `src/forge/http.rs`.
Validation: `connection_doctor::` 6 tests; `git::command::connection::` 3 tests; `settings::network_page::` 1 test. Includes real safe fixture config inventory, effective empty proxy handling, secret withholding, TLS verification disabled markers, failure classification and recording-only command plans. Doctor supports HTTP(S), ordinary SSH and scp-style remotes; custom protocols, git:// and IPv6 SSH destinations are currently refused with an explicit unsupported-remote result. No live corporate proxy/custom-CA/remote-machine connectivity trial was performed.

### #47: HTTPS sign-ins page

Primary commit: `5d42fb2e915cf1135b8d7f4cc3bce02cbf30bd58`.

- [x] Settings → **HTTPS sign-ins** inspects the selected remote's URL-matched configured helper and records which helpers actually executed, with executable/secret details redacted.
- [x] Managed credential fill reports username/account when a helper supplies it; passwords/tokens are not retained in UI results. Shared-helper or host-wide erase behavior is explained before sign-out confirmation.
- [x] **Sign out** runs Git credential reject for that URL using Git's own useHttpPath behavior and clears matching in-memory repository credentials. Helpers may ignore erase; UI reports the request accurately instead of promising success.
- [x] A 401/403 immediately after helper sign-in offers stored-account recovery even when no user/password fields were typed. Switch-account rejection is followed by the existing sign-in dialog; profile-pinned forge account changes direct the user to Profile.

Sources: `src/settings/network_page.rs`, `src/git/command/connection.rs`, `src/repo/operations.rs`, `src/git/errors.rs`, `src/app/mod.rs`.
Validation: the command group exercises actual Git credential inspection/rejection against an owned recording helper and asserts the exact selected URL/path scope; command traces are scrubbed. No real credential-store entry was erased in testing.

### #47: Diagnostics completion

Primary commit: `c22716b1b4c9eff9048845ed28102c8ac8b76a28`.

- [x] Help → **Copy diagnostics** retains version/channel/OS/graphics adapter, Git/gh/glab/tea versions and sign-in state, and allowlisted config origins.
- [x] Early installed panic hook writes a bounded private last-panic log in the existing app/portable data location; paths/URLs and recognized secret forms are scrubbed. Unix log permissions are 0600. Prior panic reporting remains installed.
- [x] A bounded recent-operation ring records only Git operation name, exit status and elapsed time, with all arguments and paths omitted.
- [x] Background collection copies the full report to the clipboard and opens a bounded prefilled issue on JKamsker/gitcraft. It tells the user to review before submission; the full report remains on the clipboard if the form URL truncates.
- [x] Nested diagnostic workers preserve initiating context/cancellation bindings.

Sources: `src/diagnostics.rs`, `src/diagnostics/crash.rs`, `src/app/diagnostics.rs`, `src/app/panels.rs`, `src/main.rs`, `src/git/command.rs`.
Validation: `diagnostics::` 6 tests including live installed-tool collection, config allowlist/path redaction, private persistence and prefilled URL limits. Known-form redaction cannot promise removal of arbitrary secrets inside arbitrary freeform panic messages; the report is explicitly review-before-submit.

## Integration cautions and review limits

- Preserve the complete ordered commit range below, including the isolation, pending-profile, navigation, proxy/TLS, secondary-remote, local-source and test-fixture fixes. Feature commits alone omit necessary final behavior.
- Shared wiring touches App/Settings/Actions/tasks/Git/forge modules minimally per feature; resolve these with other writers' provider changes using the final behavior above.
- `src/parallel_bisect/runner.rs` has only the two-line closure wrapper change `scope.spawn(connections::inherit(...))`, keeping initiating context/cancellation. STOP_SCRIPT and signaling logic were not changed here. Preserve the inheritance when integrating the parent's process-safety repair.
- No full suite or parallel_bisect timeout/cancel process tests were run. Parent owns the containment repair, combined gates, and independent adversarial review. No host-wide process selection/signals were used.
- No Windows/Linux/actual WSL/SSH remote-machine application run or live forge key-registration/sign-out/proxy test was performed. Coverage uses actual owned Git repositories, fixed recording tools, owned PTYs, loopback API fixtures and egui interaction as documented above.
- Setup import intentionally rejects executable/private machine config. Existing arbitrary per-worktree config and private custom CA/key paths require local setup; the UI lists the boundary. Do not describe setup files as full repository backups.
- Work branch pushes only; no dev/master/tag push. No PR #169/#172 or other agent's branch/worktree was edited.

## Exact final validation

Final SHA, counts, commands and individual logs are in `/tmp/gitcraft-51-t3-5b9a0fe3-validation.json`.
Reproduction driver: `/tmp/gitcraft-51-t3-5b9a0fe3-validate.py`.

Environment for all gates:

```sh
export CARGO_BUILD_JOBS=2
export TMPDIR=/private/tmp
export PATH=/tmp/gitcraft-integration-tools:$PATH
export SHELL=/tmp/gitcraft-integration-tools/validation-shell
```

Each focused test command was `cargo +1.95.0 test --lib FILTER -- --test-threads=2`; exact filters and outcomes follow below. Binary build uses the assigned worktree's own APFS-cloned target. All gates below ran after the final commit was pushed and SHA-verified. Clippy allows only the existing macOS dead-code warnings; no lint allowance was added to production code.

| Command | Outcome | Log |
|---|---|---|
| `cargo +1.95.0 test --lib tour:: -- --test-threads=2` | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 3251 filtered out; finished in 0.18s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-tour.log` |
| `cargo +1.95.0 test --lib practice:: -- --test-threads=2` | test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 3243 filtered out; finished in 1.17s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-practice.log` |
| `cargo +1.95.0 test --lib glossary:: -- --test-threads=2` | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 3251 filtered out; finished in 0.00s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-glossary.log` |
| `cargo +1.95.0 test --lib settings::profiles:: -- --test-threads=2` | test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 3246 filtered out; finished in 0.46s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-settings-profiles.log` |
| `cargo +1.95.0 test --lib repo::ssh_setup:: -- --test-threads=2` | test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 3239 filtered out; finished in 0.41s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-repo-ssh_setup.log` |
| `cargo +1.95.0 test --lib forge::ssh_keys:: -- --test-threads=2` | test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 3250 filtered out; finished in 0.45s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-forge-ssh_keys.log` |
| `cargo +1.95.0 test --lib diagnostics:: -- --test-threads=2` | test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 3248 filtered out; finished in 0.21s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-diagnostics.log` |
| `cargo +1.95.0 test --lib connection_doctor:: -- --test-threads=2` | test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 3248 filtered out; finished in 0.26s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-connection_doctor.log` |
| `cargo +1.95.0 test --lib git::command::connection:: -- --test-threads=2` | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 3251 filtered out; finished in 0.15s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-git-command-connection.log` |
| `cargo +1.95.0 test --lib settings::network_page:: -- --test-threads=2` | test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3253 filtered out; finished in 0.00s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-settings-network_page.log` |
| `cargo +1.95.0 test --lib portability:: -- --test-threads=2` | test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 3240 filtered out; finished in 1.39s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-portability.log` |
| `cargo +1.95.0 test --lib app::setup:: -- --test-threads=2` | test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 3248 filtered out; finished in 0.47s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-app-setup.log` |
| `cargo +1.95.0 test --lib git::command::setup:: -- --test-threads=2` | test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 3251 filtered out; finished in 0.48s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-git-command-setup.log` |
| `cargo +1.95.0 test --lib connections::tests:: -- --test-threads=2` | test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 3229 filtered out; finished in 1.35s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-connections-tests.log` |
| `cargo +1.95.0 test --lib repo::load:: -- --test-threads=2` | test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 3252 filtered out; finished in 0.00s | `/tmp/gitcraft-51-t3-5b9a0fe3-final-repo-load.log` |
| `cargo +1.95.0 build --bin gitcraft` | passed | `/tmp/gitcraft-51-t3-5b9a0fe3-final-build.log` |
| `cargo +1.95.0 clippy --all-targets -- -D warnings -A dead_code` | passed | `/tmp/gitcraft-51-t3-5b9a0fe3-final-clippy.log` |
| `cargo +1.95.0 fmt --all --check` | passed | `/tmp/gitcraft-51-t3-5b9a0fe3-final-format.log` |
| `git diff --check` | passed | `/tmp/gitcraft-51-t3-5b9a0fe3-final-diff-check.log` |

Total focused tests: **110 passed**, 0 failed. Build, strict Clippy (`-D warnings -A dead_code`), formatting and diff checks passed on `b0698e9b1925f61810cc3a8823bdc6292525147c`.

## Complete ordered commit range

- `007557de3183b4167f07c9039e7b63c98ea957d2` — Add a bounded repository tour and actionable setup checklist
- `5f67e91a2ac83a09e98adcf16cf9e89bbbf2b101` — Add isolated practice repositories with checked goals and a global hover glossary
- `eec7be57937c4cc4178cb21bf6a6c042510b9c64` — Bind profile SSH keys and forge accounts to immutable task contexts
- `e4fa3b684f076f01078663bc92d6c5743bd8eb61` — Add explicit SSH key setup on the repository machine
- `c22716b1b4c9eff9048845ed28102c8ac8b76a28` — Complete diagnostics with private crash evidence and a prefilled issue form
- `5d42fb2e915cf1135b8d7f4cc3bce02cbf30bd58` — Add repository connection doctor and effective HTTPS sign-in controls
- `d337702ea3e3bbf07478963f26de6be3debf3e3a` — Export portable setup and safely restore selected profiles repositories and worktrees
- `97711cf75fe20c91c77e198b77eddc3663e065d6` — Require imported profile verification before activation
- `b2754051bf9fc599229571acbaaae9d2518d4d94` — Keep profile identity and cancellation bound through nested work and connection edits
- `3c1640a8ffa2a36cf052a1e57e6a11d2c02ff6fa` — Leave the repository tour when navigating to another page
- `f3156fa1cd620d743df3b1802a1daf7f0b47ba5f` — Report effective proxy and TLS verification accurately and offer stored-account recovery
- `fc7ee920b07744d256bb91fec8135b76f71a0b82` — Make connection UI and fixture checks satisfy repository lint rules
- `870b708da67e7c5ad466b5fa1a04a984f1a0b3d1` — Restore linked worktree branches from secondary remotes with isolated authentication
- `e0c0c33d5d7c7d746accfeef8779b99cd6080130` — Exercise secondary fetch authentication through a recording transport
- `4a52a930f7aba6f6f4f072d6b9812df63aa9609e` — Preserve the initiating context in parallel diagnostic workers
- `26fc95988314eead46f58fc760537b483173c108` — Keep default origin branch restoration for explicitly selected local sources
- `b0698e9b1925f61810cc3a8823bdc6292525147c` — Isolate goal-check fixtures from independent filesystem watcher startup
