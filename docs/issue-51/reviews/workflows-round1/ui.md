# Independent workflow UI review, round 1

**Verdict: FAIL — five blocking findings.** Scoped to continuous review, storyboard author/reviewer sharing and navigation, fix radar, and cross-repository release choreography. This does not approve the remaining workflow features or the final combined application.

## Immutable binding

- Candidate/application: `bf6089b684597d878d3c2f256b7c69d64c6785a0`.
- Base: `a850de26b0a0778ea96fe34cd1498e6e78928492`.
- Branch/worktree: `t3/implement-review-workflow-ideas`, `/Users/jonas/.t3/worktrees/gitcraft/t3-940761ce`.
- HEAD matched the candidate; tracked tree clean at start and after checks. Read AGENTS.md, `/tmp/gitcraft-51-review-implementation-handoff.md`, original issue #35 and implementation/integration source. No tracked edits, CI, live external service, full suite, native GUI launch, unsafe legacy process tests or subagents.

## Blocking findings

### RW-UI-R1-1 — P1: Reload preserves the old document membership and old PR range

**Source:** `src/repo/review_workflow/continuous.rs:25-28,46,103-137`; `src/repo/pr_view/mod.rs:182-249`.

The document stores only a captured `Vec<DiffKey>`. Reload clones those keys and reloads each patch; it never recomputes the source's current changed-file list. No current PR head/request or working-status membership is retained or checked. The generic header does not disclose the old pinned PR range. PR-file refresh updates the ordinary open diff and freshness, but neither updates nor invalidates this document.

**Reproduction:** open a working-tree document while only a.rs is modified, then change b.rs and press its Reload button. b.rs is still absent. Or open the PR continuous document at H1, refresh PR details/files after H2 changes/adds files, then Reload: the captured Range keys still load H1 and the old membership. A working hunk moved from unstaged to staged also remains represented by its old area after reload rather than the current list. This defeats “every changed file as one document,” especially on the page users rely on to finish a refactor review.

**Correction:** retain an explicit source descriptor (working/commit/comparison/PR identity), obtain its complete fresh membership on reload, and either follow the current PR head or clearly invalidate the old page until reloaded. Require consistent PR details/files before accepting a fresh document. Preserve immutable source heads for truly historical documents and display that identity.

**Related hunk coherency:** the mutation callback at lines 95-98 unconditionally replaces `review_document.data`, even when a newer document request has opened. It also displays “Hunk submitted” after `apply_picked` rejects a stale-source `Err`. Keep mutation completion/feedback bound to its originating request; say refused when validation fails. The fresh source-hash check itself is useful and remains necessary.

**Required regression:** actual UI working membership refresh; PR H1→H2 refresh with an added/deleted file; staged/unstaged transition; stale-source refusal; old hunk completion after opening a newer document. Assert the visible membership/head, actual index/worktree effect and truthful feedback, rather than only testing `partial_patch`.

### RW-UI-R1-2 — P2: metadata-only changes become empty, unreviewable file sections

**Source:** `src/repo/review_workflow/continuous.rs:178-196,213-214`; `src/repo/diff/parse.rs:8-43`.

The file header contains only its current path and optional staged suffix. The body renders only `DiffData.lines`; that parser omits Git's pre-hunk metadata. A mode-only change or identical-content rename has no hunk, no binary/image/LFS marker, no body, and no “Open file diff” button (that button is inside hunk rendering). The document therefore neither explains the change nor provides a route to inspect it. Renames also lose their original path in the header.

**Reproduction:** commit a plain script, commit `chmod +x`, and open Review all changes for that commit. The script's section is empty. Repeat with an unchanged `git mv old.txt new.txt`: the section shows new.txt alone. Preserved local-Git fixtures confirm these patches contain only old/new mode or rename-from/to lines, no `@@` body.

**Correction:** retain/render change status, old/new paths, modes and other metadata independently of textual hunks. Always provide a file comparison route, including no-hunk cases. Empty files/additions/deletions and unsupported/binary boundaries should have explicit explanations.

**Required regression:** actual continuous-page assertions for mode-only, pure rename/copy, empty add/delete and binary changes; verify both descriptive text and the comparison route.

### RW-UI-R1-3 — P2: storyboard line cards open wrong or empty code, and their jump can be lost asynchronously

**Source:** `src/repo/review_workflow/storyboard.rs:451-477`; `src/repo/state.rs:225-244`; `src/repo/navigate.rs:245-259`.

`OpenCode` opens a normal commit diff, then separately reloads that patch with `DiffOptions::default()` to compute a raw row index. The actual diff uses current whole-file, whitespace, algorithm and textconv settings. The independent row index is therefore not necessarily a row of the displayed patch. If this second read finishes before the actual `Msg::Diff`, `DiffView::arrived` clears the assigned `jump_line`. An accepted line card on a file untouched by its pinned commit has an empty commit patch; the lookup silently falls back to row zero. Cards validate against the complete pinned file, so this is a supported input.

**Reproduction A:** enable Whole file, pin line 90 of a 100-line file in a commit changing only that line, and open its card. The exact production parser gives row 5 for the default-context patch and row 91 for the whole-file patch. The callback applies row 5 to the latter. **B:** pin line 90 of a file present at a commit that changes a different file: its commit patch is empty and the code does not open the pinned file. **C:** delay the actual displayed diff until after the index callback; its arrival discards the jump.

**Correction:** store a logical source location with the actual diff request; resolve it only against accepted displayed data. Use the pinned full-file view or explicitly expanded context when the line is outside the commit's hunks. Remove the independently prepared row-index read. Bind to opening/request identity, not merely an equal DiffKey that could have been closed/reopened.

**Required regression:** actual card navigation for default/whole-file/custom-algorithm layouts, unchanged-file/out-of-context lines, both callback orders and close/reopen of the same key. Assert the final visible target line after the real patch arrives.

### RW-UI-R1-4 — P2: loaded storyboard cards keep their old fresh status after PR pushes

**Source:** `src/repo/review_workflow/storyboard.rs:184-239,672-688`; `src/repo/pr_view/mod.rs:133-179,182-249`.

Staleness is calculated only when Load/Fetch/edit/publish explicitly calls `show_card`. The cached `Shown` rows retain their old target and `stale=None` when PR details and files are refreshed. PR refresh calls comment relocation and freshness, but never rechecks or invalidates storyboard rows. The UI simply paints those cached flags. This misses #35's explicit behavior that new pushes changing a card's target mark it stale.

**Reproduction:** load/fetch a fresh code card at H1, push H2 changing its file, and reload the PR until its files/head show H2. The existing card still has its earlier target and no stale badge. Manually Load/Fetch recalculates it, which demonstrates that the matcher works but lifecycle wiring is missing. Test-result cards have the same cached-tree issue.

**Correction:** invalidate or recompute loaded local/imported cards on accepted PR head/base/files changes, with guarded async ownership. Do not allow local Add/Move/Remove to capture mixed `details.head_oid` and `files.to` while a PR refresh is between those arrivals; Fetch/Publish already check that consistency. Keep imported/local provenance when re-evaluating the cards.

**Required regression:** actual H1→H2 PR message integration for code and test cards, unchanged/rebased/renamed/changed targets, both imported and local boards, plus an in-flight old storyboard load completing after the PR changed. No provider calls are necessary: supply captured details/files locally.

### RW-UI-R1-5 — P1: reverted adaptations still appear linked as surviving fixes

**Source:** `src/repo/review_workflow/radar.rs:145-168,175-194`.

`status` detects only the original fix's stable patch ID and its reverse. If neither occurs, any linked adaptation that is an ancestor of the release tip produces `Patch::Adapted`, even if a later commit exactly reverts that adaptation. The original issue requires a later revert to turn the branch amber. The positive original-fix/revert/reapply test does not cover adapted patches.

**Reproduction:** pin original F, link release=A as its explicit different-patch adaptation, then `git revert A` on release and Refresh branch observations. A remains an ancestor; the original F and reverse(F) IDs are absent; the function returns Adapted(A), despite the exact later revert of A. A local-Git fixture plus the exact extracted production `status` function reproduced that output.

**Correction:** inspect each linked adaptation's own patch and reverse evidence and order, applying the same conservative reverted/reapplied handling. Preserve the separation between patch witnesses and independently keyed behavior results; missing/truncated evidence must remain unchecked.

**Required regression:** adaptation present→exact revert→reapply, original and adaptation both reachable, truncated-history uncertainty, and behavior results remaining independent. Assert the visible amber status after the adaptation's revert.

## Coverage, entry points and qualifications

- **Continuous review:** buttons are wired in Changes, commit, comparison and PR file panels. Foldable file headers and in-place Stage/Unstage/Discard/Revert route through existing patch builders and the operation runner. PR line actions route through existing drafts/posting. The source-hash recheck rejects changed patches; it does not solve stale document membership, request feedback or missing metadata above. Editing hunks was explicitly deferred in #35 and is not a gap.
- **Storyboard:** PR page exposes author-only arrangement, local load, selected-remote publication and reviewer fetch. Exported commit parents retain pinned history across rewritten branches; leases are obtained from the push destination; imported cards stay separate/advisory. The safe local-Git sharing test passed, including split fetch/push routing and unchanged branch heads. This exercises backend cross-clone transport, not the complete author/reviewer UI. Navigation and fresh-status lifecycle remain blocked above.
- **Radar:** toolbar → “Review evidence and release coordination…” → Fix survival radar exposes pin/load/check/test controls. Exact original/reverse/reapply evidence is real; behavior evidence is separately keyed by fix/tree/command. The UI discloses 5,000 inspected nonmerge commits and older unchecked history. The adaptation revert branch is incomplete.
- **Choreography:** same toolbar window → Release choreography exposes 2–6 repositories, 2–4 pinned stages, threshold prerequisites, command trust, plan-only and rehearsal actions. The route requires allowed states and, for tested reports, exit-0/nonstopped observations. BFS advances one component exactly one stage at a time; missing/error/failing states cannot establish a tested route. Source records pinned commits/trees, command, combinations and cleanup errors. Plan-only output is explicitly labeled untested. Publishing is intentionally outside #35's first phase. No further definite route-algorithm blocker found in this scoped review.

## Work, cancellation and evidence limits

The continuous renderer loops through every logical line of every default-open file on each frame (`continuous.rs:175-258`) and its worker loads all captured files serially. The 500-file limit does not bound total rows/bytes: each file can reach the existing 50,000-line cap. Close only sets `open=false`; no document cancellation token is checked between loads. This is a material scale/cancellation concern requiring virtualized rendering and a bounded/cancellable loader before claiming large-refactor responsiveness. I did not run a native performance benchmark and have not assigned a separate measured performance blocker.

Radar's Stop flag is checked between target regression runs and connected to the running command; patch/history checking and setup Git calls are not cancellable through that flag. Choreography likewise checks Stop between combinations and during commands, with setup Git calls outside that stopper. Window close preserves the tool state and does not itself request Stop. These are source observations, not real cancellation validation.

The release form remains editable while a rehearsal is busy and old reports remain visible after edits. Reports contain their own pinned source data, so they must be treated as historical evidence rather than proof of the edited form. A form/report identity or stale-evidence indication would make that distinction clearer. I found no automatic publishing or automatic compatibility-pass claim for untested combinations.

## Independent checks performed

Copied the executable named in `/tmp/gitcraft-51-review-final-tests.log` to `/tmp/gitcraft-workflows-ui-bf6089b6-tests`; SHA-256 `c4dadb23e9102a4b4a69e8f5d3d4988764b603391afaca452637e753e25f7b0f`. This reruns the preserved artifact; no independent fresh source-to-binary build attestation was performed.

**8 exact focused tests passed:** continuous backend single-file hunk staging and capability rules; storyboard range-diff/stale matcher and local-Git cross-clone sharing; original radar revert/reapply; sharing destination replacement guard; choreography route filtering and environment-name validation. Names/results/durations: `/tmp/gitcraft-workflows-ui-bf6089b6-tests.json`; output: `/tmp/gitcraft-workflows-ui-bf6089b6-tests.log`. The continuous test applies a generated patch directly; it is not an actual UI hunk-click/stale callback test. No actual continuous-page or storyboard-navigation UI test was rerun because this candidate has none for the reported paths.

Additional safe local evidence:

- `/tmp/gitcraft-workflows-ui-bf6089b6-adversarial-fixtures.json`: original/adaptation/revert patch IDs and ancestry; pinned unchanged-file/line existence with an empty commit patch; mode-only and pure-rename patches; line90-change commit.
- `/tmp/gitcraft-workflows-ui-radar-status-probe.rs` and `.log`: exact production `status` extracted with minimal host types/Git ancestry adapter; returns Adapted for the reverted-adaptation fixture.
- `/tmp/gitcraft-workflows-ui-story-index-probe.rs` and `.log`: exact production `diff/parse.rs` with minimal host types/module placement (module doc comments adjusted for embedding), parsing the same commit's default and whole-file patches; reports target row 5 versus row 91. The default/whole `.patch` inputs are preserved alongside it.

No reproduction above is presented as native GUI runtime evidence. The rehearsal runtime test was intentionally not rerun because it enters the existing scratch/legacy command process path; the preserved writer evidence remains the only runtime gate for that path. No real cancellation, Docker, external AI, forge, SSH/WSL or cross-platform runtime validation was performed. Dependencies were not upgraded according to the candidate diff; no CI compatibility claim is made here.
