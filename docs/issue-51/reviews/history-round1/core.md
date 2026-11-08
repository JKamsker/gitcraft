Independent adversarial review ROUND1 — history core (#32)

**FAIL** for candidate `0de454f05f8fbdf932fc841b5d696bfc7d9b0cca`, base `a850de26`, branch `t3/history-investigation-views`, checkout `/Users/jonas/.t3/worktrees/gitcraft/t3-4a6c0b9e`. Six P1 findings and one P2 finding below require correction or an explicit narrowed contract. This is a read-only review: no tracked edits, fixes, commits, pushes, CI, forge requests, AI calls, external account changes, real credentials/keys, live SSH/WSL, or subagents. The checkout was clean and its HEAD exact at entry and exit.

Read `AGENTS.md`, the writer handoff `/tmp/gitcraft-51-implementation-history.md`, and original issue #32 in `/tmp/gitcraft-issue51-issues.json`. The four #36 features belong to another reviewer. Paths/lines below refer exclusively to this candidate, not unmerged dev/setup/workflow candidates.

1. **P1 / FAIL — Preflight scans replacement history while the approved push exports original objects.**

   Source: `src/push_preflight.rs:89` (`read`, only adds `--no-lazy-fetch`); `:670` (metadata), `:690` (raw commit), `:758` onward (outgoing objects and trees); `src/git/command.rs:1402` (forecast environment removal). Neither this read path nor the preflight commands use `--no-replace-objects`. Replacement refs also are not part of the approval stamp.

   A regular `git replace OLD SANITIZED` can make the commit IDs remain unchanged while `show`, `cat-file`, revision traversal and tree enumeration inspect sanitized replacement content. Pack transfer ignores replacements. The preflight can say all six categories are clear while the actual immutable source contains a secret, different authors/signatures/messages, or additional history. This is a physical-object identity problem, independent of heuristics finding every possible secret.

   Benign reproduction: `reproduce.py` constructs an owned repository with a synthetic `api_key` canary in an outgoing commit, replaces that commit with a safe tree, and calls the extracted candidate scanner. Result: **one outgoing commit, zero findings**, subject `sanitized replacement`, with `plan.head` still the original commit. An offline pack export of that immutable original commit, indexed in an owned bare repository, contains the canary. No push/network was executed. Installed Git's own `git-replace(1)` documents that pack transfer ignores replacements; a copy is preserved.

   Evidence: `replace-results.json`, `replace-scan.txt`, `scanner-extract.rs`, `scanner-harness.rs`, `reproduce.py`, `installed-git-replace.1`. Class: source-derived scanner runtime with minimal dependency stubs, plus real local Git object/pack operations; **not** a new full application/binary test. The scanner's `scan`/read/check logic is copied from the candidate; fix dispatch is omitted, profile metadata is a fixture stub, LFS parsing is a no-pointer stub for this fixture, and Git transport uses the candidate's removal list against owned configuration. Those substitutions do not decide replacement handling. Fix by scanning immutable physical objects consistently, and align replacement/namespace/shallow environment handling across preview, repair and transport.

2. **P1 / FAIL — Reviewed destinations share an unscoped cached credential.**

   Source: `src/repo/operations.rs:125` captures `self.credentials`; `:627` passes it to reviewed execution; `src/push_preflight.rs:1265` iterates all push URLs and `:1290` passes the same `creds` to each; `src/git/command.rs:1665` installs `ENV_HELPER`; `:693` defines a helper that answers every `get` without inspecting protocol, host, port, path or account. `Credentials` at `:683` contains only username/password. Authentication UI at `src/repo/operations.rs:289` derives its label from the default remote's fetch URL rather than the failing pinned endpoint.

   A password typed for one endpoint can be supplied to another configured push URL, a changed remote, or another LFS credential request inherited by the hook. Profile equality and a pinned destination URL do not scope this cached secret. The implementation expands this inherited helper behavior into the explicit multi-address preflight flow.

   Benign reproduction: extract the exact helper string and run `git credential fill` against `intended.example.invalid` and `unrelated.example.invalid`, with all ordinary helpers cleared and only fixture username/password environment variables. Both calls return the same fixture password. `credential fill` did not connect to either host. The propagation from the tab to every URL is source-confirmed; no authentication or server push was attempted.

   Evidence: `credential-results.json`, `reproduce.py`. Class: exact helper runtime plus source call-chain review. This shares a root cause with the parent's separate publication/internals finding; coordinate one credential-scope fix, while covering this history call chain too. Bind credentials to the actual protocol/host/port/path/account and only inject them into matching requests; show the endpoint that actually failed.

3. **P1 / FAIL — Interactive rebase approval does not retain the approved branch identity.**

   Source: `src/repo/rewrite.rs:236` reads a branch for preview, but `:266` stores only `ConfirmedRebase(editor.id)`; `src/repo/apply.rs:407` checks only that ID; `src/repo/rebase.rs:160` checks only HEAD hash in `moved`; `src/repo/rewrite.rs:333` creates `self.same_branch()` from the tab at command-ready time. `RebaseEditor` retains no original branch identity. Normal ref reload does not close this editor.

   Reproduce conceptually with branches A and B at the same commit: open A's interactive rebase confirmation, externally check out B, allow refs to reload, then approve the still-open confirmation. HEAD and editor ID pass. The command-ready path builds a guard for B and can rewrite B despite A being the branch shown for approval. This is especially consequential when A/B have different publication policies. The existing guard rejects a later checkout only after it has already rebound itself to the wrong branch.

   Benign recording: `rebase-harness.rs` executes copied `same_branch`, `run_rebase`, `rebase_ready`, and the actual `moved` predicate with dependency stubs. After switching modeled tab refs from `approved-A` to `unexpected-B` at the same HEAD/editor ID, the recorded executable steps start with `expect-symref HEAD refs/heads/unexpected-B`, followed by the original HEAD and rebase command. No Git or mutation runs. Only extracted visibility changes from `pub(super)` to `pub(crate)` are needed for compilation.

   Evidence: `rebase-results.txt`, `rebase-control-extract.rs`, `rebase-harness.rs`, `make-rebase-harness.py`. Class: exact control-flow recording, **not desktop UI execution**. The ordinary rebase target pinning test passed and is a distinct path. Retain and validate the approved branch/HEAD/plan through confirmation and asynchronous command preparation rather than rebuilding the guard from current tab refs.

4. **P1 / FAIL — Safety draft work runs after operation guards, which are never checked again.**

   Source: `src/repo/operations.rs:576` calls `check_expected`; `:588–590` subsequently reads draft settings and captures a snapshot; `:601–603` skips every expectation when executing commands. `src/git/drafts/mod.rs:115` allows capture up to two minutes. Capture validates coherence of its own starting state, not the operation's earlier approved state.

   A terminal commit/checkout after the guards pass and before capture reads its source can become a perfectly coherent successful safety draft. The original interactive todo, checkout, or rebase then runs against that later state without its intended guard. In the interactive-todo case a newly added commit can be dropped. Internal mutation serialization does not cover another Git client. Saving a draft does not authorize a stale operation or repair the undo journal's earlier HEAD binding.

   Benign recording: `draft-guard-harness.rs` executes the copied candidate fragment from the expectation check through the command loop. The stub capture coherently changes modeled HEAD after the first guard, then succeeds. Output records one expectation check and a successful `rebase -i pinned-base` dispatch against `later-head-not-approved`. No process or mutation is launched.

   Evidence: `draft-guard-results.txt`, `draft-guard-extract.rs`, `draft-guard-harness.rs`, `make-draft-guard-harness.py`. Class: source control-flow recording with an injected external-change window. Recheck the original guards after snapshot preparation and immediately before dependent mutation; preserve the approved context/undo identity throughout. This is introduced by the new safety work between the existing guard and execution, not a finding inferred from another branch.

5. **P1 / FAIL — Private transport pinning validates fetch URL expansion, not push URL expansion.**

   Source: `src/git/command.rs:1613–1621` configures a private alias with URL `gitcraft-preflight-address://OPAQUE` and an `insteadOf` mapping to approved A; `:1678–1687` validates using `ls-remote --get-url`; `:1692` executes `push`. A stable `url.B.pushInsteadOf=gitcraft-preflight-address://` rule does not affect the validation but rewrites the URL used by push. Pinning the unchanged configuration stamp does not remove this semantic difference.

   Benign recording-only reproduction: `transport-record.py` supplies the production alias/mapping and uses `git push --dry-run --no-verify` with a local `receive-pack` shim that records its argument and exits immediately. Neither receiver repository exists; no Git receiver, upload, ref change or network connection occurs. Validation returns intended A, but the shim records `uncontacted-B.gitgitcraft-preflight-fixture-0`. The same test with an ordinary stable A→B `pushInsteadOf` chain records A, confirming the placeholder **does protect against the parent's ordinary second-expansion example**. The failing placeholder-prefix push-only rule is a separate, source-confirmed case, not an assumed duplicate of that example.

   Evidence: `transport-results.json`, `transport-record.py`, `record-receive-pack.sh`, `receivepack-args.txt`. Class: exact mapping and Git dispatch recording with a failing dummy receiver. The private alias and original hook normally preserve the intended remote name, but their URL needs a push-semantic proof/freeze as well. Both ordinary and force pushes use this transport, and every URL in a multi-address plan repeats it.

6. **P2 / FAIL — Hovering an undo node can launch a lazy fetch.**

   Source: `src/repo/timeline/view.rs:367` offers a captured `GhostOp::Timeline`; `src/repo/ghost/plan.rs:228` calls ordinary `git::diff_files`; `src/git/diff.rs:200–205` builds `git diff -M --numstat -z --end-of-options OLD NEW --`; this goes through ordinary `git::run` without `--no-lazy-fetch` or `GIT_NO_LAZY_FETCH`. Unlike weather/recovery, the new timeline path does not enter a no-network forecast transport or reject partial history before the diff. Ordinary local execution (`src/git/command.rs:175`) also has no deadline unless one is supplied.

   In a partial clone, file-change counts need missing blobs and Git starts its promisor helper on a pointer hover. The request can reach a remote, run credentials, or hold a worker; an atomic hover-cancel flag is only checked between these ordinary calls. This contradicts the existing ghost hover contract of avoiding network/prompt work.

   Benign reproduction: an owned repository with two pinned commits and a removed promisor blob; `remote.origin.uploadpack` is an owned shell shim that writes a marker and exits. The exact timeline `diff --numstat` argv invokes it. A control with `--no-lazy-fetch` refuses without invoking it. No network or upload-pack receiver runs, and no timeouts/signals are required.

   Evidence: `ghost-lazyfetch-results.json`, `ghost-lazyfetch.py`, `record-upload-pack.sh`, `ghost-upload-helper-called`. Class: exact command runtime with local recording helper; **not a pointer-driven desktop test**. Use bounded, cancellable, no-lazy-fetch reads consistently for this new hover path, reporting incomplete local evidence instead.

7. **P1 / FAIL — Draft protection and its subsequent operation can resolve different repositories under inherited Git environment.**

   Source: `src/git/command.rs:200–216` ordinary `local_command` retains inherited `GIT_DIR`/worktree/index variables unless explicitly removed; `:1402–1424` `draft_bytes` removes them. The new safety path in `src/repo/operations.rs:589` checks opt-in and captures through the sanitized draft transport, then dispatches ordinary checkout/rebase through `run_step` using the original inherited environment.

   With the tab path pointing to A and inherited `GIT_DIR=B/.git`, draft settings/snapshots concern A while guards and the actual ordinary operation concern B. B can have drafts enabled while A does not, so B's guarded checkout/rebase gets no draft at all. A root-path normalization fix alone is insufficient unless the environment context is aligned too.

   Benign reproduction: two owned fixture repositories; read-only `git -C A rev-parse --absolute-git-dir` with inherited `GIT_DIR=B/.git` resolves B, while the same query with the draft removal resolves A. No checkout/rebase is executed. The exact inherited/removal behavior is independently checked in the candidate source.

   Evidence: `gitdir-results.json`. Class: real read-only Git context queries plus source call-chain review. This may share a correction with another batch's environment-context work, but it is present in this exact candidate; neither #169/#172 nor the separate profilecontext candidate has been credited as merged or assumed defective. Normalize and bind the context once for both the operation and its safety snapshot, with a cross-path regression.

Coverage of all six assigned features:

| #32 feature | Verdict in this round | Verified behavior and limits |
| --- | --- | --- |
| Rebase pushed-history preview | FAIL — finding 3, also 4/7 integration | Ordinary target is resolved before approval; plain preview request generation, HEAD and symref are retained; folded commands, leaving/surviving commits, upstream pushed warning, undo/autostash wording are wired. Interactive approval loses branch identity. Root rebase ranges filter an empty base correctly. Ordinary target move regression passed. |
| Rebase weather | PASS for reviewed scope | 350 ms debounce; sequential tree replay; cache keyed by parent tree/commit; terminal unknown rows after conflicts/budget; editor/generation cancellation; private bare context with parent-tree attributes and no source/global custom drivers/hooks. Clean tooltip explicitly discloses excluded source settings, so no unsupported driver equivalence is claimed. Exact conflict-position, driver nonexecution, generation/close cancellation and budget tests passed. No live WSL/SSH/Mergiraf binary validation in this review. |
| Push preflight + identity + repairs | FAIL — findings 1, 2, 5 | All six categories reachable through centralized push interception; explicit source/destination, checkout, profile policy, route/config stamp, companion refs and leases are represented. Multi-ref/mirror/recursive unsupported forms fail closed. `--no-follow-tags` prevents hidden tag payloads. Rejections/Sync feed the same dispatch. Author/secret/LFS repairs use outgoing-only commit reconstruction; transaction wrapper verifies branch/ref/input/index/working state before checkout and retains evidence on uncertainty. Five independent synthetic/recording repair tests passed; prior writer native transaction/LFS/signing/host-script runs are inherited evidence only. Actual URL/credentials/objects have the blockers above. |
| Undo hover ghost | FAIL — finding 6 | Hover captures an owned `RefState`, not merely a row label; IDs/branch moves and historical checkout identity feed the raw graph; lost commits and file changes are computed; dirty/keep restrictions disclosed; engine snapshot test passed. Separate content reviewer owns saved-view arrow/scroll behavior while timeline mode is open; do not duplicate its reported P2 as another finding here. |
| Lost content recovery | PASS for reviewed scope | Sidebar search reaches reflogs, unreachable/dangling commits/blobs and dropped stashes. Literal search and bounded batch reads; pinned blob/provenance; standalone blob path/time uncertainty disclosed. Create-only branches, exclusive new files and CAS stash restore preserve HEAD/index; metadata/link/ignored/tracked protections and root pins are present. Selected native branch/file and stale stash tests passed. No live remote/native Windows runtime claim. |
| Opt-in drafts + restore | FAIL — findings 4 and 7 | Earlier versions reachable from Changes and Recovery; default-off setting; 15-second pause and pre-checkout/rebase captures; protected per-worktree trees with separate raw index/work layers and retention CAS. Restore previews pin ref/HEAD/index/shared-index/bytes, back up before mutation, apply selected exact byte ranges, leave real index unchanged, replace the selected hardlink path, and restore existing native permissions via nofollow descriptor. Stale-state and hunk/index tests and racy-index timestamp regression passed. Integration fails to bind the protected operation to that snapshot's context/state. |

Evidence and execution record:

- Existing writer test binary `target/debug/deps/gitcraft-9ec15915cf80155c` was copied without building to `/tmp/gitcraft-51-history-core-r1-evidence/writer-tests`. SHA256 of both: `80161a9ca811a953d1a4de00a32b497fc632bde4565079c1d260e17cb6121fcd`. Writer gate logs name this binary; that is the provenance binding, not a claim that the binary self-attests its Git SHA.
- **17 independently selected tests passed**, each exact-filtered and run once: 12 focused weather/ghost/recovery/draft/plain-preview cases in `focused-results.json`, then 5 repair capability/receipt/recording-hook cases in `repair-recording-results.json`. Every record/log reports one test passed. These were selected for the concrete concerns above, not a repeated broad gate. Pure synthetic checks preceded command-recording checks. No new real cancellation/timeout/process-control tests were run.
- All new reproductions stayed inside `/tmp/gitcraft-51-history-core-r1-evidence`. Git was local Homebrew 2.56.0 on Darwin arm64. Small standalone extracted-source harnesses used rustc 1.95.0; no Cargo build/check/test compilation or broad suite was started. Marker fixtures used synthetic values and immediate-exit helper programs. No host-wide PID lookup/selectors/signals, watchdogs, cleanup signals or unrelated processes were involved.
- Writer's 156 distinct tests, 20 stability repeats, build/check/fmt/clippy/no-default gates remain **writer-supplied prior evidence** from the handoff and result files. This review does not relabel them as new independent runs. No final combined checks, CI or consolidated PR review is claimed.
- Native Windows, real WSL, live SSH sessions/remote cancellation, live LFS endpoints, actual signing with native tools, real credential services, remote hook races, full app pointer/keyboard flows, combined #169/#172 routing/root integration, profilecontext integration, workflow integration, and real rebase conflict/abort UI flows remain **unvalidated in this round**. The weather test uses real local tree conflicts; source records cancellation generations, not an actual user-cancelled rebase. Unsafe legacy parallel_bisect/remote cleanup/full-suite paths were not exercised. No claim that these inherited paths are repaired is made.

Immutable artifact binding:

`/tmp/gitcraft-51-history-core-r1-evidence/manifest.json` binds candidate Git blobs and SHA256s for 43 reviewed source files, requirement/handoff/gate inputs, copied executable, extracts, harnesses, scripts, logs and results. Its SHA256 is `cbd57eb2df3fea2ad63c04855ac480ead4b6acecdebdea1d7b64808b1260ef55`. Representative result hashes:

- `replace-results.json`: `f91afd902f59a76c261a07b993535e546a04e0fd590c0783bc525fcb73233f6e`
- `credential-results.json`: `611d08cc0c615c3a82b7cb179cba62e5e28aa9752aa91357b8877e5d9d864ce0`
- `transport-results.json`: `cc02b2373ee6cc6c3d7347ca0a4083f37906a0fdb5bbaab6bcc030fdf439694d`
- `ghost-lazyfetch-results.json`: `b9173cdde2afa208915e9bc68d4b53aa854077f7264c0d9c274c9b8e690f6a25`
- `gitdir-results.json`: `35a71426762ded7f6b2dfb205de10e83c4f1a5fb2663a79ee47d3220f920910b`

Report SHA256 is supplied alongside in `/tmp/gitcraft-51-history-core-r1.md.sha256`. Corrections require a new immutable candidate and another independent review round. No candidate fixes were made.
