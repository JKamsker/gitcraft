# Independent fileviews UI correction review R2

**Verdict: PASS within the assigned UI correction scope.** All three R1 UI findings are resolved on this exact candidate. No additional blocking regression was found in the adjacent paths inspected and exercised. This is not approval of SOPS privacy, staging/index publication, standalone save safety, Windows metadata, process containment, or the final combined application; those remain separate reviews/gates.

## Immutable binding

- Documentation candidate: `be013726aa51c270bde95cecff727191a603c785`.
- Application source: `10727c7e88725b4510350ec3416bd1e4ebfcfeda`.
- Original batch base: `a850de26b0a0778ea96fe34cd1498e6e78928492`.
- Writer worktree/branch: `/Users/jonas/.t3/worktrees/gitcraft/t3-e06bdc42`, `t3/diff-staging-views`.
- Reviewed immutable source extracted with `git archive` at the candidate into `/tmp/gitcraft-fileviews-ui-r2-source`. The candidate/application delta contains only four validation documents/artifacts under `docs/validation/`; no application files differ.
- Read the full prior parent `docs/issue-51/reviews/fileviews-round1/ui.md` at parent snapshot `4143becdd51f8c09002f10154ffce962b3ca193e`. Its bytes match `/tmp/gitcraft-51-fileviews-review-ui-r1.md`. Read the correction Markdown/JSON/log archive and the relevant exact-source diffs, alongside the original #40/#44/#50 requirements.

## R1 dispositions

### FV-UI-R1-1 — Stale matrix selection/result: resolved

`src/repo/file_comparison/mod.rs:316–319` revokes the prior pair generation and clears the pair before **every** selection transition, including removal and starting a new one-cell selection. `State::begin(false)` updates `request`/the generation and clears `pair` while keeping `data_request` stable (`:97–109`). The handler then clears loading for an incomplete pair; a complete two-cell selection starts its own asynchronous request. A completion can apply only if its request still equals the current request (`:118–122`). Thus neither an old success nor an old error can reappear after deselection.

Matrix-row ownership is separate from pair ownership. A refresh calls `begin(true)`, now setting `data_request=0` as well as dropping data and selected cells (`:103–107`); queued actions from old visible cells fail the `data_request` check before they can revoke the new refresh (`:309–315`). New snapshot acceptance establishes a new row epoch (`:111–115`). These checks address both the original stale-content case and the correction's adjacent refresh-race risk.

Independently reran `actual_matrix_cells_open_a_read_only_content_diff_from_pinned_branches`. It clicks actual cells and verifies the read-only content difference, then exercises 2→1, 1→0 and 2→new-1 transitions. It checks visible content removal, pair/spinner state and stable displayed-row epoch, explicitly delivers obsolete completed/pending pair results, and delivers a queued old cell while a new matrix read is outstanding. The old action cannot change that refresh's request/loading/data state. **PASS.** Late completions use error payloads in the fixture; source acceptance is payload-independent, so the same request rejection applies to successful stale pairs.

### FV-UI-R1-2 — Quadratic moved-block work on paint: resolved

`src/repo/file_comparison/code_moves.rs:15–16` stores prepared `Opened` values beside the immutable analyzed diff. `assemble` prepares the comparisons once (`:270–278`); production invokes `read`/`assemble` inside the existing analysis worker (`file_comparison/mod.rs:243–251`), rather than from the paint function. Painting borrows `data.opened[i]` (`code_moves.rs:359–362`). The only remaining `Opened::new` in this change-wide feature is the preparation loop, so hover/scroll/repaint do not run moved-block LCS or rebuild the complete comparison rows. Visible row rendering remains virtualized by `moved_view::content` (`diff/moved_view.rs:143–155`).

The prepared data shares one accepted request/snapshot. Refresh clears both accepted data and selected block; older results fail the request check. Selecting another block borrows the corresponding cached preparation, without combining rows from different snapshots. The existing source/destination navigation still opens the selected pinned/location patch and applies its logical line target after loading.

Independently reran all five change-wide moved-code tests, including the original near-limit 1,000-line, long-prefix edited block. The new regression paints five frames with wheel events and asserts that production `Opened` construction does not increase during paint. Its actual debug preparation took approximately **322 ms once** in this reviewer execution; that is preparation time, not a native frame-rate measurement. It also covers stale refreshed snapshot rejection and pure inactive-generation refusal. Actual destination navigation and migration ribbon/content separation tests pass. **PASS.**

Preparation still calls bounded LCS without a cancellation poll inside each individual comparison; activity is checked between blocks and afterward. Moving it off paint resolves this finding, but neither these tests nor this report establish a hard cancellation-latency or overall process-resource guarantee. The 40-second eligibility check is not a preemptive deadline inside every CPU loop.

### FV-UI-R1-3 — Line-history Wrap control: resolved

`src/repo/diff/view.rs:490–506` now exposes `wrap_control` for visible ordinary text patches independently of the `view.lines.is_none()` gate. Only whole-file/whitespace/layout/algorithm options remain under that gate. The same control is retained for ordinary non-line-history patches, with no duplicate Wrap button introduced. Folded, non-patch and explicitly private views keep their appropriate separate behavior.

`Action::DiffWrap` updates the persisted setting, maps the old physical scroll offset back to a logical offset, drops old wrap geometry, and does not reload the diff or clear logical chosen lines (`src/repo/apply.rs:304–316`). The action does clear transient character selection; this existing behavior is distinct from the chosen logical line set and was not changed by this correction.

Independently reran the actual line-history header fixture on a real `log -L` patch. It clicks Wrap both ways, observes the setting change, and retains selected history ranges, chosen logical line and loaded diff data. The adjacent Unicode logical-index/selection-rectangle wrap test and full long Unicode snapshot text test pass. All ten line-history parser/mapping tests also pass. **PASS.** This does not imply new native drag-selection/search/map interaction coverage in every unified/split/private renderer; the R1 qualifications on those broader coverage claims remain.

## Independent checks and evidence

Copied the existing exact writer test artifact to `/tmp/gitcraft-fileviews-ui-r2-evidence/exact-tests`. SHA-256 `21ea31281ac07bd05a85eebbcd437b8be9172a55274e8639f19d18cd3d6c3af2` matches the candidate's archived JSON. This is a rerun of the preserved artifact, not a fresh compilation or independent cryptographic source-to-binary attestation.

**24 focused tests passed, zero failed, zero ignored**, across 11 narrow invocations:

- Actual matrix selection/stale request transitions: 1.
- Change-wide moved-code preparation/readers/markers: 5.
- Actual line-history Wrap interaction: 1.
- Actual moved destination navigation, migration ribbon/content opening, stale line-action rejection and snapshot-view lifetime reset: 4.
- Snapshot full Unicode text and ordinary Unicode wrap geometry: 2.
- Line-history parsing/mapping: 10.
- DVC fixed-argv/network classification: 1.

Environment followed the writer normalization: `TMPDIR=/private/tmp`, validation shell, `/tmp/gitcraft-integration-tools` PATH prefix, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`; test invocations used one thread. Writer artifacts were built using Cargo/Rust `+1.95.0`; no Cargo build was needed or rerun by this reviewer. Tests used pure/headless fixtures and ordinary owned local-Git repositories; no live DVC/SSH/WSL service was exercised.

Independently verified the archive SHA-256 and all 19 archived command-section log hashes. The writer's 236 passing test executions and build/check/qualified-lint gates are inspected evidence, not reviewer reruns and not a unique-test count. Strict unqualified `-D warnings` is not claimed; the qualified gate allows baseline `dead_code` as documented. Native Windows label/audit validation and held/excluded transport/process gates remain outstanding in their owning scopes.

Evidence and exact commands/results/hashes: `/tmp/gitcraft-fileviews-ui-r2-evidence/`, with standalone manifest `/tmp/gitcraft-51-fileviews-ui-r2-manifest.json`. No tracked changes, CI, PR, full suite, process runtime/cancellation/signaling tests, source edits, native GUI launch or subagents occurred. This scoped PASS clears these three prior UI findings only and does not authorize the held gates.
