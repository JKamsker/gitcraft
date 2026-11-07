# PR review workflow batch: implementation handoff

Worktree: /Users/jonas/.t3/worktrees/gitcraft/t3-940761ce
Branch: t3/implement-review-workflow-ideas
Base: a850de26b0a0778ea96fe34cd1498e6e78928492
Remote origin verified: https://github.com/JKamsker/gitcraft
Author verified for each published unit: Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>
No PR created; no CI invoked; no providers or shared account/profile schemas implemented.
Status: all ten ideas have implementation and UI entry points; final local gates below. Independent adversarial review and parent combined gate still required. Docker runtime validation is unavailable here.

## Exact requirement coverage from original #35

| Idea / preserved commit | Implemented behavior and source | Limits to retain in final audit |
| --- | --- | --- |
| Since last review comment relocation / 5855da5c647e99fc54d301b5ccfe5d723947604e | review_workflow/anchors.rs persists earlier thread anchors and maps unchanged old/new line spans with rename support; pr_review/send.rs renders relocated marks and pr_view/line_panel.rs navigates/replies through the original thread. Existing range-diff preserved. | Requires an earlier captured local snapshot. Edited/deleted/unknown anchors remain outdated rather than guessed. Pending reviews and forge anchors unchanged. |
| Continuous review / 81e47d8ab6624a2d42e50da5425cd898600e3605 | continuous.rs ReviewAct/Open builds one foldable document for changes, commit, compare, PR; per-hunk stage/unstage/discard/revert delegates partial_patch and apply_picked after fresh source hashing. PR comments use existing actions and line panel. UI buttons in changes/commit/compare/pr_view. | Explicit 500-file budget refuses larger documents. Editing hunks was explicitly deferred by original issue. |
| Freshness / 3f869333a70a89ccb32eec5bca73af887a7a010c plus follow-up below | freshness.rs per-file exact blob/patch receipts classify unchanged/direct/dependency change, preserve original base/head, and compare explicit watched dependency paths. Rust extension follows static direct/transitive calls and changed import targets automatically. UI lives inside PR page scroll. | Language analysis is conservative Rust syntax analysis, not type/LSP resolution; dynamic/external/macros/cfg/custom path/re-export ambiguities explicitly unchecked. Other languages can watch paths, without automatic semantic claims. |
| Storyboard / 60140c719e300634b493e029521ae4ac1f8e5848 | storyboard.rs author-only ordered failure/invariant/fix/result cards with revision-pinned code or existing test-lane verdict. Code opens original file/hunk; range-diff mapping plus blob/rename checks mark stale cards. Dedicated refs keep source commits reachable. | 6f33c176 adds explicit selected-remote author publication and reviewer fetch. The exported artifact retains pinned commits as parents, uses the push URL for publication leases, and keeps imported notes separate from local editing. Imported metadata is advisory; pinning a test-lane verdict does not rerun it. Its configured command is explicitly labeled as not bound to historical execution by existing test notes. |
| Reviewer echo / 757d8c65b8113785a6223cb48d171f49177d306e | echo.rs opt-in global personal book, imports only viewer-authored nonpending published comments across opened supported-forge PRs. Explicit selected AI CLI suggests cited regex rules disabled pending inspect/enable. Deterministic added-line checks cite own original comments. 67ee45ce moves preflight to the shared operation runner for normal Push, Sync, branch-context push, force/retry and Gerrit pushes. It resolves explicit/default/wildcard/config-mapped sources, handles first publication against the empty tree, and revalidates source refs, effective routing configuration and rulebook before resuming the original guarded operation. bf6089b6 handles header-like added text and quoted paths without misattribution. | Collection is explicit per open PR across supported forges, not an automatic account-wide historical crawl. Unknown push options or compound plans that change state before pushing require explicit human review/continue and never report an automatic successful check. |
| AI objection reproducer / 28df9b13b945d7c18e84a0c243487ff67c2e2bc1 | reproducer.rs explicit AI test generation then separate edited-code approval; Rust integration test runs against pinned base/head in owned scratch sources inside Docker isolation; immutable local image ID, no network/pull, read-only root/source mounts, nonroot, caps/resource limits, internal timeout, exact-owner cleanup. Same test/outputs attached next to captured hunk; no automatic wrong/right judgment. | Docker absent on this host: only recording containment/cleanup plan tested. Requires a preloaded Linux image with Rust/offline dependencies and timeout tooling. Real container success/failure/cancellation behavior needs environment validation; no host fallback used. |
| Fix radar / 883131be0ef72b3e1348a3745a1fd88828dc6a9c | radar.rs pins fixes, per-release same stable patch-id/adaptation/reverse patch status; regression command evidence independently keyed by fix/tree/command. Reverse ID uses canonical a/b prefixes. Original/revert/reapply test. | Scans up to latest 5000 nonmerge commits, explicitly states older history unchecked; patch evidence is distinct from behavior. |
| Contract backport / 97df273b3ab4716d76a98df5fdf4f84dcd440cd0 | contracts.rs drag graph commit into target form, resolve immutable fix/targets/adaptations, detached scratch cherry-pick/no-commit, trusted regression command per target, separate apply/test status, immutable tree/output, original/adapted side-by-side compare and cleanup errors. | Executes only explicit trusted user command, not model-generated command. Does not publish release branches. |
| Intent cards / 352b5d38e75f63515e60b13bdb60821563658bee | intents.rs short path/symbol notes with expiry, dedicated refs, selected-remote fetch/publish, only own namespace edits/end, stale/offline badges in file/diff; periodic refresh after explicit sync. | 09e4d428 binds caches and freshness to captured effective destinations, prunes only the dedicated fetched namespace, rejects changed routing, and resets freshness when selection changes. Advisory, no lock; sharing needs remote support for dedicated refs and teammate synchronization. |
| Cross-repo release / 6aca92eb8139a9d14075ed55cae677bc982a2e94 | choreography.rs board 2–6 repositories, 2–4 pinned stages each, threshold prerequisites, trusted compatibility contract against fresh scratch combinations, full allowed state matrix and BFS route through only passing one-component advances. Plan-only states labeled untested. Real backend/SDK fixture proves bridge-first route. | Explicit 256-combination budget; all source repositories must be on same local/remote host to expose paths to one test command. Publishing explicitly outside this idea's first phase. |

Follow-up hardening eda097febcd39caf39f7af09035916936e263c2f: missing evidence distinguished from read errors, ref CAS during individual writes, PR sections moved inside existing scroll, explicit web link handlers, clippy fixes.

## Integration safety

Backend work uses shared RepoTab then_or/tasks and git RunStop/Scratch mechanisms. Explicit model invocation uses existing selected trusted AI CLI. Imported comments never become commands. New metadata stays under private review_workflow modules and Git namespaces; no provider-specific ReviewFeatures additions. Original forge review submission, pending review state and stale-head checks remain in existing code paths.

No parallel_bisect real timeout/cancel tests or full suite were run, as requested. No host-wide process selectors/signals were used. Container process control first had recording-only owner/argument checks. Runtime scratch command tests were first run only after unit preservation.

## Evidence before final Rust extension

- 6aca92eb: normalized CARGO_BUILD_JOBS=2 cargo +1.95.0 test --locked --lib review_workflow: 15 passed, 3153 unrelated filtered. Log /tmp/gitcraft-51-review-combined-tests.log.
- eda097fe-equivalent source: CARGO_BUILD_JOBS=2 cargo +1.95.0 clippy --locked --lib -- -D warnings -A dead_code passed. Log /tmp/gitcraft-51-review-hardening-clippy.log. The allow covers existing macOS baseline dead_code warnings only.
- contracts preserved runtime suite: /tmp/gitcraft-51-contracts-preserved-tests.log (10 passed).
- reverse/reapply patch evidence: /tmp/gitcraft-51-radar-fixed-tests.log (focused 1 passed).
- Docker synthetic recording plan: /tmp/gitcraft-51-reproducer-recording-tests.log (1 passed).
- Each initial feature check/test log uses /tmp/gitcraft-51-{anchors,continuous,freshness,storyboard,echo,intents,contracts,radar,reproducer,choreography} names; final gates below supersede earlier source snapshots.

## Preserved follow-ups (all author and remote SHA verified)

- Rust static call freshness: eb5e44f44d40fed077670e88ddcbc2245be0aad4 (19 workflow tests passed).
- Shared push-path reviewer echo: 67ee45cef6962caa8edcc3db91e6e22893aac575 (79 workflow/PR/push tests passed).
- Cross-clone storyboard sharing: 6f33c1765d313afe189c64d6ce81375c7c210889 (22 workflow tests passed, including actual local-Git sharing and split fetch/push routing).
- Intent source/routing freshness: 09e4d428f84ebd32f9d0b97be51c87c00074328d (22 workflow tests passed; final gate covers subsequent guard additions).
- Callback request ownership and robust echo parsing: bf6089b684597d878d3c2f256b7c69d64c6785a0. This is final branch HEAD.

Follow-up clippy caught an unused captured storyboard request token while adding the guard; the missing callback check was added, not silenced. Initial echo parser spoof coverage passed before final commit. Final gates below were then rerun on the exact committed source.

## Final candidate gates

Commit: bf6089b684597d878d3c2f256b7c69d64c6785a0

- `TMPDIR=/private/tmp PATH=/tmp/gitcraft-integration-tools:$PATH SHELL=/tmp/gitcraft-integration-tools/validation-shell CARGO_BUILD_JOBS=2 cargo +1.95.0 test --locked --lib -- review_workflow repo::pr_review repo::pr_view repo::push` — PASS: 81 passed, 0 failed, 3094 filtered (plus gitcraft-os 0 selected / 2 filtered); log `/tmp/gitcraft-51-review-final-tests.log`.
- `CARGO_BUILD_JOBS=2 cargo +1.95.0 clippy --locked --lib -- -D warnings -A dead_code` — PASS; log `/tmp/gitcraft-51-review-final-clippy.log`. The allow is for pre-existing macOS dead-code warnings.
- `cargo +1.95.0 fmt --all -- --check` — PASS; log `/tmp/gitcraft-51-review-final-fmt.log`.
- No real Docker execution: `command -v docker` found no executable. No external AI invocation was made; generation remains an explicit user action with existing trusted CLI configuration.
- No full suite, unsafe parallel-bisect process tests, provider changes, PR or CI action.

`Cargo.toml` adds direct syn 2 full/visit and quote dependencies for Rust syntax traversal and token comparison; these versions already existed transitively in Cargo.lock. No dependency versions were upgraded.


## Immutable source entry points

- [src/repo/review_workflow/anchors.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/anchors.rs)
- [src/repo/review_workflow/continuous.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/continuous.rs)
- [src/repo/review_workflow/freshness.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/freshness.rs)
- [src/repo/review_workflow/storyboard.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/storyboard.rs)
- [src/repo/review_workflow/echo.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/echo.rs)
- [src/repo/review_workflow/echo_push.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/echo_push.rs)
- [src/repo/review_workflow/reproducer.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/reproducer.rs)
- [src/repo/review_workflow/radar.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/radar.rs)
- [src/repo/review_workflow/contracts.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/contracts.rs)
- [src/repo/review_workflow/intents.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/intents.rs)
- [src/repo/review_workflow/choreography.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/choreography.rs)
- [src/repo/review_workflow/rust_dependencies.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/rust_dependencies.rs)
- [src/repo/review_workflow/sharing.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/sharing.rs)
- [src/repo/review_workflow/store.rs](https://github.com/JKamsker/gitcraft/blob/bf6089b684597d878d3c2f256b7c69d64c6785a0/src/repo/review_workflow/store.rs)

## Delivery verification

- `CARGO_BUILD_JOBS=2 cargo +1.95.0 check --locked` — PASS on bf6089b684597d878d3c2f256b7c69d64c6785a0; six pre-existing macOS dead-code warnings. Log `/tmp/gitcraft-51-review-final-check.log`.
- Working tree clean after gates (`git status --porcelain` empty).
- Local HEAD and `git ls-remote origin refs/heads/t3/implement-review-workflow-ideas` both bf6089b684597d878d3c2f256b7c69d64c6785a0.
- All 16 commits after a850de26 have the exact requested Jonas Kamsker author identity; verified with unique `git log --format` output.
- No uncommitted implementation files remain. Parent owns combined history assembly/ledger, full suite and independent adversarial reviews. No claim of those gates passing is made here.
