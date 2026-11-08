# Issue #51 source/UI audit: issues #46–#50

Audited immutable checkout `a850de26b0a0778ea96fe34cd1498e6e78928492` on 2026-11-07. Read AGENTS.md and the complete issue bodies from `/tmp/gitcraft-issue51-issues.json`. This is a read-only source and UI-code audit: no application launch, tests, CI, process probes, or tracked-file writes were performed. “Implemented” means a substantive backend and reachable user-facing flow satisfy the issue's requirements by inspection; it is not a new runtime validation claim. “Partial” preserves concrete remaining requirements even when the existing module is substantial. All line references are for this commit.

## Counts

| Issue | Ideas | Implemented | Partial | Missing |
|---|---:|---:|---:|---:|
| #46 History visualization/delight | 8 | 0 | 0 | 8 |
| #47 App shell/OS | 15 | 8 | 3 | 4 |
| #48 Coding agents | 11 | 7 | 3 | 1 |
| #49 Remotes/config/internals | 14 | 2 | 1 | 11 |
| #50 Staging/conflicts/history | 5 | 2 | 1 | 2 |
| **Total** | **53** | **19** | **8** | **26** |

34 ideas still require work: 26 missing plus 8 partial. Count top-level ideas once, with all remaining subrequirements detailed below.

## #46 — visualising history, and delight

1. **Lanes by branching model — MISSING.** `src/graph.rs` has lane-name propagation and `src/repo/graph_view/{lanes,readability,order}.rs` draw/compact existing lanes. Current graph persistence/readability is useful infrastructure but has no branching-model picker, main/release/feature band allocation, or user pin-to-band setting. Deliver selectable simple models, stable colors/bands through reload, and per-branch band pins. Existing stable lane work must be retained.

2. **History minimap — MISSING.** `src/repo/graph_view/mod.rs` implements scrollable history and row virtualization; there is no minimap state, painter, viewport rectangle, or drag handler. Missing loaded-shape preview, current viewport, HEAD/tag/search marks, drag navigation, and explicit older-not-loaded boundary.

3. **Activity calendar — MISSING.** `src/repo/activity/{mod,view}.rs` is the in-app operation log (commands, outcomes and durations), not a commit calendar. Missing day/month browsing, click-day commit selection, drag-date range selection, author/committer-date choice, and deduplication of commits reachable through several refs.

4. **Churn and contributor atlas — MISSING.** Existing file history, blame and authorship statistics do not provide a folder treemap. Missing zoomable treemap, bytes/churn sizing, churn/most-frequent-contributor colors, selectable time window, click-to-file-history, and precise “most frequent contributor” labeling. Run expensive analysis off-thread and disclose history coverage.

5. **Watch repository grow — MISSING.** No file-tree replay view or timeline/playback state. Missing play/scrub animation, added/changed/deleted files, contributor highlights, and pause-to-open-commit. Reuse tree/file/history backends without blocking UI and state covered history.

6. **Contributor statistics — MISSING.** `src/authorship/branch.rs` reports agent-written lines, not per-author commits/files/active periods. Missing author statistics over selected time and `.mailmap` normalization. Existing `.mailmap` author-picking code alone does not satisfy this view.

7. **Quiet celebrations — MISSING.** No success-event celebration UI or sound/motion settings. Missing opt-in first-repository-commit, release-tag and completed-merge flourishes, independently switchable motion and soft sound, success-only triggering, and no commit-count/line-count rewards.

8. **History postcards — MISSING.** No release-graph PNG composition/export flow. Missing release/range selection, release name/date, optional contributor names, preview enumerating visible names/messages, anonymization option, and export. Graph screenshot facilities alone would not satisfy composed release postcards.

## #47 — app shell and OS

1. **Second launch goes to existing window — IMPLEMENTED.** `src/main.rs:37` calls `app::single_instance`; `src/instance/{discovery,lock,server,client,message}.rs` implement authenticated loopback handoff; `src/app/handoff.rs` opens requests and requests focus/attention. `src/app/tests.rs` and `src/instance/tests.rs` contain end-to-end/source-level regressions. No missing issue subrequirement identified by inspection.

2. **CLI and Explorer/Finder/Nautilus file actions — IMPLEMENTED.** `src/launch/args.rs:1-13,21-35` exposes folder, log, blame+line, show and compare; `src/launch/resolve.rs` resolves subfolders to repository roots; `src/app/launch.rs` applies view requests with single-instance handoff. Windows file verbs are in `packaging/windows/gitcraft.iss:68-73`; Finder Quick Actions in `src/finder_actions.rs`; GNOME scripts in `src/nautilus_scripts.rs`; Dolphin service actions in `packaging/linux/gitcraft-servicemenu.desktop`. `src/settings/file_menus.rs:20-49` makes Finder/Files setup reachable. macOS/GNOME actions are installed from Settings instead of a package installer, but user-visible actions exist.

3. **Browser deep links — PARTIAL.** Substantial implementation: `src/launch/{link,open_link}.rs` parse forge URLs and resolve known clones by remote; `src/app/links.rs` adds start-page and palette paste behavior and clone offers; Windows registry and Linux scheme entries exist (`gitcraft.iss`, `src/desktop_entry.rs:229-244`, `packaging/linux/gitcraft-link.desktop`). Remaining: `packaging/macos/Info.plist` has no `CFBundleURLTypes`, and no macOS URL-open event hookup exists, so browser-to-app scheme on macOS is absent. `src/launch/open_link.rs:15-109` also accepts PRs only for origin/first hosting remote, rejecting a PR whose repository is another existing remote; requirement says find clone by its remote URL, so selected-remote PR routing needs completing rather than asking the user to clone an already present repository. Preserve commit/file lookup, line jumps, palette/home entry, and no-clone offer.

4. **Explain Git error with fix button — IMPLEMENTED.** `src/git/errors.rs:18-60` classifies all listed families: dubious ownership, locks, CRLF, long names, TLS, transfer/RPC/413, unrelated histories, shallow push and SSH keys. `src/error_card/{mod,view,text,fix}.rs` render explanations and safe config/lock fixes; `src/git/errors/lock.rs` shows age and Git process observations and checks file identity/mtime before deletion. Trust adds the exact folder, not `*`. `src/repo/error_help.rs` and app opening flows expose cards; discovery preserves ownership diagnostics. For explanatory families, guidance deliberately requires terminal choices instead of silently joining histories or weakening TLS. SSH errors include helper guidance. Existing tests cover representative real-version error fixtures. No new process tests were run for this audit.

5. **Typed drafts survive restart/crash — IMPLEMENTED.** `src/drafts/mod.rs:35-125` persists commit, PR comments/reviews/line replies and edited rebase lists per repository. `src/repo/drafts.rs` serializes/restores them; `src/app/draft_saving.rs` periodically saves and flushes on close; `src/drafts/session.rs` marks abnormal exit and the app uses the explicit restored-drafts/tabs notice. The lifecycle avoids unopened lazy tabs overwriting disk drafts.

6. **Diagnostics for bug reports — PARTIAL.** Help/palette/manual actions exist (`src/app/panels.rs`, `src/app/palette/items.rs:113`, `src/app/diagnostics.rs`); background collector in `src/diagnostics.rs:28-136` includes version/channel, OS, wgpu adapter/backend, Git/gh/glab/tea versions/login state, allowlisted config origin/scope, and redaction. Missing last-panic persistent hook/log, redacted recent operation entries, and opening a prefilled issue. Current completion (`src/app/diagnostics.rs:64-78`) only copies clipboard text and tells user to paste it themselves. Existing process/task/read metrics are valuable additions but do not replace the missing diagnostic contents.

7. **Connection doctor/proxy page — MISSING.** Settings has offline/metered controls, not a network diagnostics page. No remote DNS→TCP→TLS/custom-CA→authentication→timed-ls-remote flow; no Git/environment/forge/SSH proxy inventory or step-specific 407 errors. Existing SSH opening/sign-in and general error cards provide infrastructure but not this feature. Keep proxy/credential secrets out of diagnostics.

8. **Offline/metered mode — IMPLEMENTED.** `src/settings/mod.rs:106-108,823-829` persists controls; `src/network/{mod,detect}.rs` detect Windows connection/metering and centralize pause/refusal policy. `src/repo/offline.rs` exposes last-fetch toolbar chip, explicit refusals and resume behavior. Network policy is used for background fetch, forge/CI polling, avatars and updater. Metered pauses background work while allowing explicit user requests; offline refuses them with reasons. SSH repositories correctly distinguish local versus repository-machine network costs.

9. **Taskbar progress/jump list/dock menu — PARTIAL.** `src/os_shell/{taskbar,menu,windows,linux}.rs` and `src/app/os_shell.rs` supply clone/fetch/push/pull progress, failures, recents, Clone/Open actions for Windows/Linux. Missing macOS Dock progress/failure indication and dynamic recents/Clone/Open menu: `src/os_shell/mod.rs:43-46` and `src/app/os_shell.rs` compile to no-op implementations outside Windows/Linux. Native macOS support must be implemented, not claimed from portable state logic.

10. **Opt-in tray watcher — MISSING.** No tray icon/runtime setting, close-to-tray behavior or background notification state. Missing continued recent-repo fetch/watch, moved-main/review-requested/check-failed tray messages, recent-repo menu, and GNOME notification fallback. Current normal close ends the app and clears session marker.

11. **Portable mode — IMPLEMENTED.** `src/portable.rs` detects marker/data directory and redirects persistent storage/cache/profile config; `src/settings/portable_page.rs` exposes optional host credential store; `src/settings/profiles.rs` uses portable data for profile `.gitconfig`; `src/update/install.rs:102-114` parks updates in portable directory. Tests cover portable detection/config behavior. No missing requirement identified by inspection.

12. **HTTPS sign-ins page — MISSING.** Existing `src/git/command.rs:974` credential lookup and per-tab sign-in dialog (`src/repo/dialogs/remote.rs`) are not a host accounts page. Missing effective-helper/account inventory per host, credential reject sign-out, post-login 401/403 wrong-account diagnosis and switch-account action. SSH “Forget saved sign-ins” is separate and does not satisfy this feature.

13. **Lazy/suspended tabs — IMPLEMENTED.** `src/repo/lifecycle.rs` explicitly models unread/live/suspended phases, lazy first-show load, 30-minute idle suspension, watcher/cache release, safe busy/draft exclusions and selection/diff restoration; `src/app/mod.rs:721` drives it. Existing lifecycle tests cover lazy loads, wakeups and watcher disposal.

14. **Undo update/stay on version — IMPLEMENTED.** `src/update/previous.rs` keeps previous binary, swaps back safely and settles a persistent pin; Settings `rollback_rows` (`src/settings/mod.rs:1083`) and `src/app/update_banner.rs` expose version-back/pin/unpin. This is more than skip-one-release.

15. **Localization — MISSING.** Strings remain inline English in UI and `src/manual.rs`; no translation catalogs, Fluent loader or locale setting. Missing extraction/catalog architecture and translated UI/manual support; issue explicitly leaves RTL for later, so do not turn RTL into an initial completion gate.

## #48 — coding agents

1. **Agent turns as review steps — PARTIAL.** `src/agent/checkpoint.rs:77-128,145-225` saves trees/turn records under the requested refs and enumerates sessions; `src/agent/hooks.rs` installs optional Claude/Codex hooks. `src/repo/agents/turns.rs:83-259`, called by Changes (`src/repo/detail/changes.rs:45`), shows prompt-labelled sessions/turns and compares one turn to previous or a turn to now. Missing durable “last looked/reviewed” baseline, per-hunk reviewed marks that hide unchanged reviewed hunks and reappear when changed, and **Revert this turn only preserving later turns**. Current menus only compare or delete saved session refs; deleting refs is not reverting a turn. Preserve prompts, optional hook installation and worktree isolation.

2. **Hand it to agent — IMPLEMENTED.** `src/repo/agents/handoff.rs` provides local line-note model, persisted per-worktree tray, terminal picker and clearing only after successful paste; `src/repo/diff/rows.rs:505-559` adds local gutter note UI. `src/agent/handoff.rs` formats messages. Conflict three-side payload comes from `src/repo/conflict/mod.rs:104`; CI log handoff from `src/repo/ci_view/page.rs:155`; PR thread file/line/message handoff from `src/repo/pr_review/line_panel.rs:189`. Terminal handoff is an allowed alternative to MCP, so no MCP send endpoint is required for completion.

3. **AI authorship lens — IMPLEMENTED.** `src/authorship/{gitai,trace,agents,mod,load}.rs` read explicit git-ai/Agent Trace/trailers rather than guessing code. `src/repo/authorship_view.rs:70-115,156-234` supplies stripes, robot badge, agent-written filter and branch report. Blame stripe is wired at `src/repo/file_view/blame.rs:129`; diff gutters use attribution in `src/repo/diff/rows.rs`; graph uses `robot`. `src/authorship/branch.rs:16-70,137-245` computes branch percentage and subsequent human edits with coverage disclosure. Distinct graph search matching rather than topology hiding is sufficient for this issue's filter wording.

4. **Worktree setup recipe — IMPLEMENTED.** `src/agent/recipe.rs` handles copy/link ignored paths, setup command and per-worktree variables/index; `src/repo/agents/recipe_editor.rs` exposes preview/edit/run controls. Creation/start flows execute recipes for non-agent and agent worktrees via repository-machine terminals (local/WSL/SSH). Do not replace this with a local-only process when extending it.

5. **Bake-off between attempts — PARTIAL.** `src/repo/bakeoff/{picker,mod,view}.rs` plus `src/agent/bakeoff/{mod,deps,build,hunks,assemble}.rs` implement 2–4 attempts, common base, touched-file/size/dependency/build grid, side-by-side files, whole-file/hunk selection and new assembled branch. Remaining: branch-only attempts explicitly show “a branch: no worktree to build in” (`src/repo/bakeoff/view.rs:240`), so the requested build comparison works only for worktree attempts. Add disposable, safely owned build workspaces for branch attempts or another concrete branch build flow, with complete failure/output results. Keep existing common-base and hunk-clash protections.

6. **gitcraft MCP server — PARTIAL.** `src/mcp/{server,tools}.rs` provides graph/status/blame/conflict/CI reads; `src/mcp/tools/write.rs` routes undoable operations to running UI; `src/repo/agent/{mod,plan}.rs` plans, revalidates and labels agent operations in shared undo history; `src/app/agent_card.rs` exposes timed Approve/Deny for destructive requests. Missing explicit attribution read tool/fields (`history.rs` log/show/blame JSON contain only standard Git author data, not lens attribution). Missing ghost-run preview integration on applicable destructive requests: approval cards expose textual impact and command list only; no use of existing dry-run/ghost preview flows. Preserve shared undo badges, exact-candidate revalidation and approval requirements. Existing tests are evidence of implementation, not new execution.

7. **Prompt behind commit — MISSING.** Checkpoint turn labels and AI prompt generation are separate features. No selected-commit Prompt & session tab, Entire checkpoint-branch source reader, GitButler message prompt parser, local Claude/Codex transcript matching by worktree/time, tool-call-to-hunk links, or click-hunk-to-writing-step. No push warning specifically for checkpoints/transcript branches. Keep transcripts local and warn before checkpoint branch publication as issue requests.

8. **Weakened-check flags — IMPLEMENTED.** `src/weakened/{lines,patch,paths,tolerance,ci,load}.rs` deterministically detects removed assertions/tests, ignore/skip/assert-true, tolerance loosening, broad snapshots, permissive CI/removed jobs/hooks/no-verify. `src/repo/weakened_marks.rs` adds evidence-bearing file badges; `src/repo/diff/weakened.rs` adds hunk badges/evidence. No LLM inference required. Tests cover evidence patterns and repository integration.

9. **Agent fence — IMPLEMENTED.** `src/fence/{store,pattern,check,configs}.rs` persists patterns, writes Claude deny/Cursor ignore/AGENTS.md instructions (the latter for Codex and other AGENTS readers), and examines actual tracked/untracked changes. `src/repo/fence_view/mod.rs:100-166,173-279` checks linked worktrees, makes violations red and gates merges with explicit confirmation; editor exposes patterns and config targets. Codex enforcement uses AGENTS guidance rather than a claimed sandbox, and UI explains rules can be bypassed. This matches requirement to check actual results instead of trusting config alone.

10. **Scope drift meter — IMPLEMENTED.** `src/drift/{mod,task,deps,signatures,llm}.rs` measures files/folders/dependencies/public signatures/formatting ratio against task or linked issue, with optional LLM judgment. `src/repo/drift_view/{mod,items}.rs` loads task/issue, places exact figures first and provides file-opening rows. No missing issue subrequirement identified.

11. **Collision matrix — IMPLEMENTED.** `src/collision/{matrix,merge_tree,order}.rs` calculates pairwise merge-tree collisions and suggested landing order; `src/repo/collision_view/{mod,grid}.rs` presents N×N results and explanation of order. Worktree menu entry `src/repo/sidebar/worktrees.rs:232-238` makes it reachable. Existing tests cover matrix and order behavior.

## #49 — remotes, submodules, configuration, internals

1. **Separate pull/push routes — PARTIAL.** Push remote selection already honors `branch.<name>.pushRemote`, `remote.pushDefault`, branch remote then origin (`src/repo/push/check.rs:168-181`); `src/across/my_work.rs:255` understands it and fork setup sets pushRemote (`src/repo/forks/plan.rs:311`). Missing two explicit per-branch route rows (pull remote/branch and push remote/branch), **independent ahead/behind counts per route**, and both pickers. Sidebar currently says “Tracks …” with one upstream and Set upstream (`src/repo/sidebar/branches.rs:132-135,370-386`). Do not classify backend honoring existing Git configuration as the requested route UI.

2. **Which config wins/from which file — IMPLEMENTED.** `src/git/doctor/config.rs:2,330-343` uses origin/scope/included config with allowlisted settings, profile awareness and ordered origins; `src/repo/doctor/config_section.rs` shows winner and overridden layers, removes override to inherit, explains scope/profile/include ordering and compares other worktrees. Credentials are explicitly excluded (tests assert `credential.helper` cannot be selected/edited). No missing issue requirement identified.

3. **Tracked files missing from Changes — IMPLEMENTED.** `src/git/doctor/flags.rs` parses ls-files flags, separates manual/sparse-managed flags and builds clear operations; `src/repo/doctor/flags_section.rs` explains both, lists separate groups and confirms selected/all clearing. Sparse-checkout managed entries cannot be indiscriminately cleared. No missing requirement identified.

4. **Publish submodule dependencies before parent — MISSING.** `src/git/submodule.rs` reports state; `src/repo/sidebar/submodules.rs` opens/updates modules; `src/repo/keep_submodules.rs` updates checkouts after operations. None inspects whether referenced submodule commits are published to a named remote, provides the dependency list, or executes dependency→parent push with per-repository outcomes. No `push --recurse-submodules=check|on-demand` flow in source. Existing keep-updated behavior is not dependency publication.

5. **Submodule conflicts as graphs — MISSING.** Conflicted gitlinks are identified by `SubmoduleState::Conflicted`, but sidebar only offers update/open actions. Missing graph around base/ours/theirs inside submodule, finding/selecting an existing commit containing both, opening it to merge there, and recording selected gitlink in parent. Text/binary conflict editors do not provide this topology flow.

6. **Remote refspec editor — MISSING.** `src/repo/dialogs/remote.rs:16-87` still exposes a name and single URL and calls remote add/rename/set-url. Missing positive/negative fetch branch/prefix refspecs, distinct fetch/push URLs, mirror choices, mapping preview and prune-deletion preview. The presence of command-level refspecs for existing actions is not an editor.

7. **Publish several hosts — MISSING.** Existing push plan targets one selected/default destination. Missing persisted named publication groups, one action for each destination, independent success/failure results, failed-only retry and explicit non-atomic explanation. Capability-test Git remote-group support rather than assuming recent Git everywhere; sequential multiple pushes can provide older-Git behavior.

8. **Push outbox — MISSING.** Offline policy currently refuses network operations (`src/repo/offline.rs`); it does not store pushes. Missing exact-commit/remote/destination-branch persistent entries, later-commit isolation, selected send/optional plain auto-send, destination/history-change/rejection pause-and-review, and resumable entry UI. Reuse push safety checks without silently converting queue entry to current HEAD.

9. **Subtrees — MISSING.** No subtree source/folder registry, pull upstream, view local vendored changes, split-and-publish flow, squash-versus-full-history explanation, tool/capability discovery or signing option. Submodules and tree file views are unrelated infrastructure.

10. **Shared/borrowed objects — MISSING.** Worktree handling knows common dirs but there is no repository-sharing/alternates topology UI, missing-donor diagnosis, or make-independent copying+verification+unlink flow. Do not remove an alternate before proving required objects are local.

11. **All refs — MISSING.** `src/git/refs.rs` loads ordinary branch/tag decorations; notes/reflog each expose their own narrow namespace. Missing generic root/bisect/original/application/namespaces browser, symbolic-ref following, target object detail, and namespace history. Existing `--all` history traversal does not satisfy browsing refs.

12. **Replace refs and grafts — MISSING.** No replace-affected commit indication, original/effective history switch via `--no-replace-objects`, temporary imported-history graft flow, visible/removable join. Ordinary branch graph connections are not replacement metadata.

13. **How object stored — MISSING.** Diff binary/file metadata is content information, not object storage. Missing OID/file-version entry, type/size/packed-size/packfile/delta-chain inspector, tree-entry navigation and delta-base navigation from cat-file/verify-pack. Repository-health totals are a separate feature.

14. **Reftable inspect/migrate — MISSING.** `src/repo/watch.rs:75-76,144` watches reftable changes and tests accept such directories, but there is no backend-format display, compatibility guidance or migration wizard. Missing installed-Git/tool capability checks, linked-worktree refusal, explicit other-writers-stopped precondition, `git refs migrate`, post-migration verification and actionable failure reporting. Watching an existing format is not migration support.

## #50 — staging, conflicts, history

1. **Three-state staging lens — MISSING.** `src/repo/detail/changes.rs` and `src/repo/diff` support index-vs-HEAD and worktree-vs-index separately. No aligned HEAD→staged→working three-column lens, boundary-specific stage/unstage-line operations, or “working edit undoes staged change” warning. Existing three-pane conflict UI is a distinct feature. Add reachable partly-staged-file entry and preserve index/worktree versions when applying either boundary.

2. **Original inside conflict — IMPLEMENTED.** `src/repo/conflict/original.rs` gets base from diff3 or index and aligns per-card original segments; `original_view.rs:45-154` renders expandable Original with both sides' additions/deletions and explicit missing/unknown-base messages. Conflict card hookup is `src/repo/conflict/view.rs:324`. Existing real-conflict tests cover added/no-base and changed content.

3. **Keep both binary versions — IMPLEMENTED.** `src/repo/conflict/keep_both.rs` plans collision-free ordinary filenames, reads origins/sizes and writes both without staging; `keep_both_view.rs:181-322` previews names/origins/sizes, warns renamed references need edits and requires review before separate Stage both action. Post-write window allows reviewing new files; parent conflict identity is rechecked before staging. Existing real-conflict tests cover binary versions, filters, ignored names, name collisions and rebase orientation.

4. **Commit backlinks — PARTIAL.** `src/git/backlinks.rs` finds subsequent explicit reverts/trailers/cherry-picks and bare hash mentions; `src/repo/detail/extras/backlinks_view.rs` displays separate Links/Mentions, matching text and click rows. Missing exact requested click behavior: row emits `ExtrasCmd::Open`, handled by `src/repo/detail/extras/mod.rs:415` as **SelectHash only**, opening commit selection/details, not that commit's diff. Implement direct commit changes/diff navigation, retaining grouped evidence and lazy/background search. If product intentionally accepts selected details as the meaning of “opens diff,” document the changed acceptance explicitly rather than silently claiming exact behavior.

5. **File versions across branches — MISSING.** `src/repo/versions/mod.rs` is commit-series/range-diff comparison (old/new rewrite or PR), not a files×branches matrix. Missing multi-release-branch and folder picker, file rows/branch columns, identical-content markers, missing-file/mode-change cells, two-cell selection and their diff. Existing bake-off comparison only covers task attempts against common base and is not the release-folder matrix.

## Recovery scope and deferred features

Read `docs/codex-recovery-2026-11-07.md` including current review/integration sections. It explicitly defers **bundle import** (index-pack memory containment unresolved) and **remote Syntax** (remote cleanup/disconnect containment unresolved); local Syntax and bundle export/header inspection remain. These are not any of the 53 top-level ideas in #46–#50, so they are not double-counted here. Parent's other issue audits must retain them if those issues require them; old source branches/recovery descriptions are not evidence that current user-visible import/remote Syntax exists. This audit ran no risky process checks and creates no renewed validation claims from prior logs.

## Suggested implementation batches (1–10 related remaining ideas each)

These are cohesive scope groups, not permission to omit listed subrequirements. Keep one immutable commit per completed feature and preserve prior reviewed code.

- **History navigation/analytics (7):** #46 lanes, minimap, calendar, atlas, growth, contributor stats; #50 branch-file matrix. Share history coverage/date/mailmap/tree backends. Consider split into navigation (2) and analytics (5) if dispatch limits permit.
- **Visual exports/success feedback (2):** #46 celebrations + postcards. Shared appearance settings, motion preference and export preview infrastructure.
- **Shell completion/diagnostics (5):** #47 deep links, diagnostics, macOS shell, tray, localization. Each needs real OS integration, not only portable state tests.
- **Network and publication (7):** #47 connection doctor and HTTPS sign-ins; #49 routes, refspecs, publication groups, outbox, submodule dependency publication. Shared remote destination model and redacted credential/proxy handling; outbox immutable snapshots are important.
- **Submodule/subtree/topology (3):** #49 submodule conflict graph, subtrees, shared/borrowed objects.
- **Git internals browser (4):** #49 all refs, replace/grafts, object inspector, reftable inspect/migrate. Shared read-only object/ref explorer before guarded mutations.
- **Agent review/provenance (4):** #48 turns, bake-off branch builds, MCP attribution/preview, prompt/session. Existing full features must be extended rather than replaced by skeletal stand-ins.
- **Index/history detail completion (2):** #50 staging lens and backlink direct-diff navigation.

Total grouped remaining ideas: **34**. The locale extraction group may need its own implementation unit because cross-cutting string edits conflict with concurrent UI work; recommend defer integration of that commit until UI writers finish.
