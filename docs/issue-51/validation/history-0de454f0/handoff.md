# Issue 51 implementation handoff: history and recovery batch

Status: all ten assigned feature implementations are committed and pushed; focused local validation passed at `0de454f05f8fbdf932fc841b5d696bfc7d9b0cca`. Independent adversarial review and the parent's combined candidate checks remain pending. No PR or CI run was created.

Branch: `t3/history-investigation-views`; remote: `https://github.com/JKamsker/gitcraft`; worktree: `/Users/jonas/.t3/worktrees/gitcraft/t3-4a6c0b9e`. Base: `a850de26`. This branch has not merged the newer `0830614f89de60936b7e6358060f99ce18f4f3c0` dev update; the parent owns that integration. All commits use Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>.

## Requirement coverage and feature commits

Coverage is checked against the original #32/#36 bodies in `/tmp/gitcraft-issue51-issues.json` and exact gaps in `/tmp/gitcraft-51-audit-core.md`. Existing features outside this batch are preserved rather than counted as new work.

| Idea | Complete production behavior and actual entry point | Main feature commit | Follow-up commits required |
|---|---|---|---|
| #32.1 rebase destructive preview | Ordinary and interactive rebase use concrete rewritten/leaving commit previews, surviving refs, explicit already-pushed/force-push warning, folded commands and correctly scoped undo/autostash notes. Ordinary preview resolves the target before displaying it and validates branch/HEAD; editor identity guards interactive approval. `src/git/preview.rs::PreviewAsk::rebase`, `src/repo/rewrite.rs`, `src/repo/dialogs/preview.rs`. | e2731ef787bd32e5d6347c9febd8b0d84b748e9c | 151a06edfcf8e09b65e57d465c2bd8962c14efee |
| #32.7 rebase weather | Interactive todo rows show clean/empty/conflict/unknown weather as reordered; 350ms debounce, sequential tree replay, (parent-tree, commit) cache, named conflict files and Mergiraf solver result on hover. Shared scoped tasks, generation/close cancellation, private repository context with tree attributes and no repository hooks/drivers. `src/repo/rebase/{rows,weather,weather_git}.rs`. | dccbed44da902ba1e69a0dcf2ee1a84bf5c9b107 | ee38b0de3d300eb1c237c93f464f32eb69689a47 |
| #32.8 push preflight and identity guard | Every supported actual push opens the six-category checklist (host/namespace profile identity, signing, WIP/fixup messages, secrets, large blobs/LFS, discarded remote authors). Fix buttons perform guarded author/sign/secret/LFS/remote-history repairs or prepare editable autosquash; rewrites enter existing undo. Profile settings edit author-per-host rules. Outgoing objects, checkout, routes, rewritten URLs, target refs/leases, all companion refs, hooks and LFS endpoints are pinned and revalidated; existing Sync/rejection/lease behavior is retained. `src/push_preflight.rs`, `src/push_preflight/fix*`, `src/repo/push_preflight.rs`, `src/repo/dialogs/mod.rs`, `src/git/command.rs::push_preflight_with`, `src/git/push_hooks.rs`, `src/settings/profiles.rs`. | b35d2c9acd40542551d7bfe1e9f20bccfb73d04d | 7752d63cd357d453f7e092177a2eb574fa7db7f9; 84eea3ea9b7a89199ed781a18cf62dd502ebd382; 1735c141dcd7d452eea5706139dfcde877f4a074 |
| #32.10 undo hover ghost | Hovering an undo-tree node offers that captured ref state to the existing ghost renderer; graph shows proposed lanes/ref moves and a strip of lost commits, authors, file changes and dirty-tree/keep restrictions. The raw graph is used while ghosting, preserving saved-view projection correctness. Existing click-to-restore confirmations remain. `src/repo/timeline/view.rs` and `src/repo/ghost/{op,plan}.rs`. | 2c51cd45a1196c4eb2b4f867bc3cdce03554b9e9 | None |
| #32.13 lost-and-found by content | Recovery sidebar opens literal search over reflogs, unreachable/dangling commits and blobs and dropped stashes. Bounded fsck/cat-file batch scanning supplies timeline/provenance and raw previews. Hits restore as a create-only branch, an exclusive new file, or a CAS-restored stash when provenance permits. Standalone blobs explicitly have unknown original path/time. Revalidation excludes tracked/ignored/linked targets and stale identity, without hooks/filters/network downloads. `src/git/recovery/{mod,batch,file,recover,tests}.rs`, `src/repo/content_recovery/{mod,view}.rs`. | 7bd86518b0ed8f2a5565fdf90180841a40081bd2 | d5951dd81d0ed79fe05396a5593946be1043e6e3; 5fe143465567daf2fa14f932460e18cf61240181 |
| #32.14 working-tree drafts | Changes → Earlier versions and Recovery sidebar expose opt-in drafts. After 15 quiet seconds and before guarded checkout/rebase, raw tracked/untracked nonignored contents and staged contents are separately protected under per-worktree tree refs, without visible WIP commits. Retention uses CAS (50 versions/14 days). Selected hunk recovery previews exact bytes, preserves staged state and permissions, takes a safety version, and supports undo. Mutating background/list/pruning/recovery work is serialized with repository operation safety. Real index remains untouched, including split/sparse indexes; copied cache preserves source mtime for racy edits. `src/git/drafts/{mod,context,restore,permissions,tests}.rs`, `src/repo/working_drafts/{mod,view}.rs`, `src/repo/operations.rs::run_steps_counted`. | da4b3bf703a2f5b3259c20b14f2e2b9faba485f2 | 1314c0a62011fd5b93e1f4bb8336640b5851a8b5; 9786920c1490be2491a43a228f1645597ff70af9; ee38b0de3d300eb1c237c93f464f32eb69689a47; 0de454f05f8fbdf932fc841b5d696bfc7d9b0cca |
| #36.4 saved graph views | Graph view selector has the four requested relation presets, save/delete/restart persistence, pinned revision queries and bounded older-context loading. Long omitted runs become expandable real-lane connectors. Raw IDs, selection/navigation, lane ancestry, WIP/stashes and ghost/timeline modes remain coherent. Latest-only shared readers cancel stale generations without detaching running owners. `src/git/investigations/views.rs`, `src/repo/saved_views/{mod,view,projection,reader,tests}.rs`, `src/repo/graph_view/{mod,row}.rs`. | f2d85db7ca55791fcc1704aeb1bb00965bead6ab | None |
| #36.6 line dossier | File/blame/diff line context menus expose Explain history; Recovery has manual entry. Repeated parent blame with -w -C -C -M follows moves/reformats with explicit ambiguous/merge alternatives. Version cards link pinned source/commit, associated PR, linked issue and review comments only after exact commit/path/line/blob proof. Optional own-key/local summary cites every evidence card; formatting-only ignore-revs append requires reviewed patch and checkbox and preserves index/root identity. GH/GitLab/Gitea/Forgejo adapters are bounded and use current transports/credentials. `src/git/investigations/{dossier,ignore}.rs`, `src/forge/history.rs`, `src/repo/line_dossier/{mod,view,summary}.rs`, `src/repo/diff/select.rs`, source-file/blame line menus. | b5229022e84dbf208baa36ce37f4f0829d245cf9 | Shared lint/source-display cleanup below |
| #36.7 symbol biography | Source line menus and Recovery entry find real tree-sitter symbols by name/line and structurally trace body similarity across rename-linked files. Timeline records creation/edit/rename/move/possible split/deletion; slider, previous/next and event buttons scrub each body, author, commit and selected-step PR. Ambiguity, truncation and budget stops are explicit; possible split helpers can be followed. Rust, Python, JS, TS/TSX, Go, C and C++ grammars are bundled. `src/git/investigations/symbols.rs`, `symbols/{syntax,disabled,tests}.rs`, `src/repo/symbol_biography.rs`. | de45a878b90e075eb78739a62127d2b5531021ca | 56160df1a375c8e26a0dafa12952722a505e0998; shared lint/source-display cleanup below |
| #36.8 cochange | While staging, the Changes pane shows missing companion-file hints from pinned recent cochange evidence. Bulk, generated/vendor/lock/minified/build-heavy commits/paths are excluded. Each hint has concrete supporting commits, click-to-select evidence and dismiss/restore. Already staged/changed/conflicted companions are omitted. Async HEAD cache reranks stage changes without another history query. `src/git/investigations/cochange.rs`, `src/repo/cochanges.rs`, `src/repo/detail/changes.rs`. | f6d6ab5d7bd6d2a1ceb8b15eed4c03b0e8c9ede3 | 6944d95afdb04f5b52fc1fa7737e7633c9560105 |

Shared cleanup commits (retain both):
- 536ed1e065c684990a7f63ac28f2cbd9e1f36b69: new views use existing idle-aware spinners; oversized displayed symbol lines have explicit clipping and pinned-source access; clippy fixes confined to owned feature code.
- 1b530436e5d8d4c6b8312620e80ab56b502acc2d: byte slicing after validated quoted-path delimiters.

## Validation bound to `0de454f05f8fbdf932fc841b5d696bfc7d9b0cca`

Environment for focused integration tests:
```
TMPDIR=/private/tmp
PATH=/private/tmp/gitcraft-history-tools:/tmp/gitcraft-integration-tools:$PATH
SHELL=/tmp/gitcraft-integration-tools/validation-shell
CARGO_BUILD_JOBS=2
```

All 15 focused commands use `cargo +1.95.0 test --locked --lib FILTER -- --test-threads=2`. Exact argv, result summaries and per-command output are in `/private/tmp/gitcraft-history-gate-0de454f05f8f/results.json` and its referenced logs. They total 158 successful executions, 156 distinct tests; overlapping cochange filters account for the difference. No tests were ignored by these commands.

| FILTER | Passed |
|---|---:|
| repo::rebase::weather::tests | 13 |
| rebase_preview_names_rewritten_commits | 1 |
| saved_views | 7 |
| timeline_hover_restores | 1 |
| git::recovery | 18 |
| git::drafts | 20 |
| working_drafts | 1 |
| git::investigations | 19 |
| forge::history | 4 |
| line_dossier | 1 |
| cochange | 3 |
| symbol_biography | 1 |
| push_preflight | 37 |
| repo::push:: | 31 |
| plain_rebase_preview | 1 |

Additional gates:
- `cargo +1.95.0 fmt --all -- --check`: passed.
- `CARGO_BUILD_JOBS=2 cargo +1.95.0 check --locked --tests`: passed; `/private/tmp/gitcraft-history-gate-check-tests-0de454f0.log`.
- `CARGO_BUILD_JOBS=2 cargo +1.95.0 clippy --locked --lib`: passed; only the six existing macOS dead-code warnings in desktop_entry/diagnostics; `/private/tmp/gitcraft-history-gate-clippy-0de454f0.log`.
- `CARGO_BUILD_JOBS=2 cargo +1.95.0 build --locked`: passed; `/private/tmp/gitcraft-history-gate-build-0de454f0.log`.
- `CARGO_BUILD_JOBS=2 cargo +1.95.0 check --locked --no-default-features --lib`: passed; `/private/tmp/gitcraft-history-gate-no-default-0de454f0.log`.
- Hostile-filter draft fixture repeat: all 20 runs passed. Command: `cargo +1.95.0 test --locked --lib configured_filters_hooks_fsmonitor_and_signing_never_run_for_drafts_or_recovery -- --nocapture` repeated 20 times with the same test environment; logs `/private/tmp/gitcraft-draft-fixed-repeat-N.log`.

The previous immutable candidate 1735c141dcd7d452eea5706139dfcde877f4a074 failed one draft consistency test; its failed gate remains in `/private/tmp/gitcraft-history-gate-1735c141dcd7/results.json`. Reproduction identified an isolated-index mtime change that disabled Git's racy-stat check. [Git's racy-stat rules](https://git-scm.com/docs/racy-git) explain why advancing the copied index timestamp can hide a same-size edit. 0de454f0 fixes source timestamp preservation; a deterministic same-size edit regression now passes without weakening content/index revalidation.

Process safety evidence: synthetic transaction/protocol cases and recording-only routing/signal/command checks passed before owned-child cases were run on preserved commits. The preflight gate now includes the local native transaction case matrix (files/reftable, SHA1/SHA256, hook veto/phases, checkout/ref races, timeout, dirty-index/worktree and missing-ack failures), plus execution of the exact SSH host script in a local owned repository for success and stale-retry terminal receipts. No host-wide PID selectors/signals are added. The known unsafe parallel_bisect real timeout/cancel tests and full suite were not run.

Git LFS fixture dependency: host initially lacked git-lfs. Official v3.8.1 Darwin arm64 binary is scoped to `/private/tmp/gitcraft-history-tools/git-lfs`; archive SHA256 0ba10e897e0496412cd86eacaed4ed554c97ddc29bc5c709d28faa5eaaa2222c matched the official GitHub asset digest. No host-global installation/config changes. Signing tests create an owned fixture key; no user's signing key is used.

## Integration notes and genuine limits

- Parent must preserve new dev #169 worktree-root and #172 actual-push-remote routing while merging. Our shared touch points are `src/repo/{operations,actions,apply,mod,rewrite}.rs`, `src/git/{command,mod}.rs`, `src/settings/profiles.rs`, profile UI, graph/diff/sidebar wiring and Cargo manifest/lock. In particular, keep #172 target selection and feed its actual final remote/ref/URL into preflight; retain our pinned URL, immutable approved transport, lease/hook/LFS checks. This branch does not overwrite or change another agent's branch/worktree.
- New dependencies are optional tree-sitter core + seven language grammars under default `symbol-history`. They are required for structural named-symbol parsing; disabling default features compiles and explicitly reports symbol tracing unavailable. No new dependency is used for drafts/recovery/preflight.
- Read paths never download missing Git objects. Size/history/time budgets and unsupported content are shown as explicit incomplete/refusal results, not false clean/success. Recovery scan limits: 500 commits, 10k objects, 15k entries, 2 MiB/blob, 64 MiB scanned, 300 hits, 120s. Draft limits: 8 MiB/file, 32 MiB changed data, 2000 changed/20k index paths; changed links/conflicts/special files require manual backup. Mirror/all-ref exports can include protected draft refs; UI warns.
- Symbol tracking is a bounded first-parent structural/similarity investigation (16–2048 selected revisions, 1500 body versions/64 MiB total/120s); possible splits are identified as such, and ambiguous identity stops with explanation. It does not claim infallible semantic tracking. Cochange uses up to 600 nonmerge commits, excludes >20-file commits and requires >=3 shared commits / >=60% evidence confidence.
- Preflight is heuristic for secret/message/large-blob findings and a signature-presence guard when profile signing is configured. It does not prove secrets absent or cryptographically validate every outgoing signature. Secret values are withheld. LFS migration requires Git LFS; missing/promisor objects refuse rather than fetch. Guarded repairs require Git >=2.50 with an actual per-repository capability proof and fail closed on unsupported backends. Secret repair removes the reviewed flagged files/messages across outgoing history rather than guessing redaction. Native reference hooks and original pre-push hooks retain veto behavior.
- SSH/WSL adapters are implemented via existing shared infrastructure, but there was no live SSH/WSL endpoint test here. The exact host script was exercised locally; that is not a claim of live SSH validation. Remote draft permission preservation requires Python 3 on the repository machine and private index timestamp preservation uses standard touch -r. BSD/GNU recovery identity/exclusive-file paths are implemented. Windows runtime was not tested.
- Forge dossier adapters, review-line attribution and summary prompt/citation rules have synthetic/fixture tests; no live GitHub/GitLab/Gitea/Forgejo account or optional paid/local LLM service was exercised. UI pointer tests cover saved view persistence/expansion, cochange dismissal/evidence, undo hover and symbol scrubbing; no claim of a full manual desktop session.
- Parent still owns full combined validation, independent adversarial review and final PR/CI on the exact integrated candidate. This implementation handoff is focused-validated, not independently reviewed.

## Complete ordered commit manifest

```
dccbed44da902ba1e69a0dcf2ee1a84bf5c9b107 feat: forecast interactive rebase positions with isolated tree replay
e2731ef787bd32e5d6347c9febd8b0d84b748e9c feat: preview rebase rewrites and warn about already pushed commits
f2d85db7ca55791fcc1704aeb1bb00965bead6ab feat: save relation based graph views with expandable history connectors
2c51cd45a1196c4eb2b4f867bc3cdce03554b9e9 feat: preview hovered undo tree states with the ghost graph renderer
7bd86518b0ed8f2a5565fdf90180841a40081bd2 feat: recover remembered content from lost commits stashes and staged blobs
d5951dd81d0ed79fe05396a5593946be1043e6e3 fix: support BSD remote filesystem identity and exclusive recovery
5fe143465567daf2fa14f932460e18cf61240181 fix: use command substitution for remote recovery identity
da4b3bf703a2f5b3259c20b14f2e2b9faba485f2 feat: preserve working tree drafts and recover selected earlier hunks
b5229022e84dbf208baa36ce37f4f0829d245cf9 feat: explain line evolution with pinned review evidence and cited summaries
1314c0a62011fd5b93e1f4bb8336640b5851a8b5 fix: retain draft recovery permissions and fail closed on ignored rule warnings
9786920c1490be2491a43a228f1645597ff70af9 fix: expose guarded root device identity for permission preservation
ee38b0de3d300eb1c237c93f464f32eb69689a47 fix: initialize isolated forecast contexts without advisory diagnostics
f6d6ab5d7bd6d2a1ceb8b15eed4c03b0e8c9ede3 feat: suggest unstaged companion files from historical cochange evidence
6944d95afdb04f5b52fc1fa7737e7633c9560105 test: consume texture updates in cochange pointer interaction fixture
de45a878b90e075eb78739a62127d2b5531021ca Add structural symbol biography with scrubbable pinned history
56160df1a375c8e26a0dafa12952722a505e0998 Recognize extracted helpers sharing a source line with their caller
b35d2c9acd40542551d7bfe1e9f20bccfb73d04d Add pinned push preflight, host identity policies and guarded undoable repairs
7752d63cd357d453f7e092177a2eb574fa7db7f9 Use a native hook event for repair capability proofs on newer Git
84eea3ea9b7a89199ed781a18cf62dd502ebd382 Relay pre-lock transaction hooks and read optional attributes from pinned trees
151a06edfcf8e09b65e57d465c2bd8962c14efee Pin ordinary rebase previews to the reviewed target commit
536ed1e065c684990a7f63ac28f2cbd9e1f36b69 Keep history loading spinners idle and make source clipping explicit
1b530436e5d8d4c6b8312620e80ab56b502acc2d Slice quoted Git paths as bytes after delimiter validation
1735c141dcd7d452eea5706139dfcde877f4a074 Exercise remote repair host protocol in an owned repository fixture
0de454f05f8fbdf932fc841b5d696bfc7d9b0cca Preserve source index timestamps in isolated draft stat caches
```

Final preservation verified at 2026-11-07 23:17 UTC: remote branch SHA equals 0de454f05f8fbdf932fc841b5d696bfc7d9b0cca; worktree clean; all 24 commit authors match the required identity. All 15 gate records reference this same immutable SHA; all 20 repeat logs report success.
