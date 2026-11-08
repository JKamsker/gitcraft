# Issue 51 implementation ledger

Baseline: `a850de26b0a0778ea96fe34cd1498e6e78928492` (`dev`, 7 November 2026). This is a work-in-progress ledger, not a completion or validation claim.

Three independent source/UI-path audits reviewed the 191 top-level ideas in issues 32–50. They classify 98 as implemented, 28 as partial and 65 as missing. “Implemented” here means source-backed coverage, not freshly executed runtime validation. All 93 incomplete ideas remain in scope. The frozen issue bodies are in `source-issues.json`; per-item evidence and gaps are in `baseline-core.md`, `baseline-content.md` and `baseline-platform.md`.

## First implementation wave

Each group is assigned to an isolated worktree using GPT-6.1-Sol with high reasoning. A feature counts as delivered only after its commits, focused checks and independent review are recorded.

| Group | Ideas | Scope | State |
| --- | ---: | --- | --- |
| History/recovery | 10 | Rebase preview/weather, push preflight, undo hover, content recovery, file drafts; saved graph views, line dossier, symbol biography, cochange | In progress |
| Visualization | 10 | Eight issue 46 visualizations; audible graph; explanatory motion | In progress |
| Setup/connections | 8 | Three onboarding/portability ideas; profile accounts, SSH helper; network doctor, HTTPS accounts, diagnostics | In progress |
| File views | 10 | Migration map, encrypted/moved/prose/SVG/DVC diffs; standalone tools, keep-reset; staging lens, branch file matrix | In progress |
| Repository internals | 10 | Pull/push routes, submodule publishing/conflicts, refspecs, multi-host publishing, borrowed objects, refs, replacements, object storage, reftable | In progress |
| Review workflows | 10 | All incomplete issue 35 ideas (inbox already implemented) | In progress |

The remaining 35 incomplete ideas are retained in the baseline audits for later related batches. No idea is resolved solely because an older branch contains similarly named code.

## Gates

- Preserve each feature commit on its work branch, verify authorship and remote SHA.
- No implementation CI dispatches or PRs; work branches stay outside push triggers.
- All-target baseline test compilation passed with Rust 1.95.0 on macOS arm64; this is not a test run. The compiler reported six existing macOS library dead-code warnings.
- Current bisect/remote cleanup scripts derive descendant signal targets from host-wide process listings. Do not execute those cleanup paths in real tests until owned-process containment is established and synthetic/recording checks pass.
- Assemble final history from `dev`, run combined local checks, and obtain independent adversarial review on the immutable final application candidate before opening one consolidated PR.
- Record test/review evidence against immutable SHAs; document unavailable platform/runtime gates without claiming they passed.

## Review and second-wave checkpoint

Visualization candidate `39d8c4ce` is preserved in WIP integration `fa89c966`; three independent reviews rejected it. Their SHA-bound reports and archived writer evidence are under `reviews/visualization-round1` and `validation/visualization-39d8c4ce`. Keyboard entry and suspension corrections subsequently passed scoped independent reviews. Candidate `6a1cf232` passed 79 relevant local regressions after correcting a test fixture that treated an already displayed activity view as a deferred tab. Other findings remain open; the batch is not accepted and no full-suite result is claimed.

The machines/imports batch is active: seven remaining ideas (teleport drag, linked-machine facts, behaviour timeline, clone options and consent, contained bundle import, filter-repo cleanup, direct backlink diffs), plus the prerequisite owned-process supervision correction. Thus 65 incomplete ideas have implementation assignments and 28 await subsequent batches. Limits are concurrent: up to six coding subagents and ten total active subagents, with no lifetime limit. No CI or PR has been started.

## Fileviews review checkpoint

Fileviews candidate `c82112a5` (application `5df74dbb`) is preserved on `t3/diff-staging-views` and is not yet integrated. Independent review has reproduced index-lock replacement corruption and index permission changes, automatic textconv bypasses of encrypted-file consent, external local SVG image loading, and stale branch-matrix comparisons after selection changes. Reports are being finalized; corrections and independent re-review are required.

The orchestrator ran the writer's previously skipped real SOPS/age fixture with verified official binaries and generated local keys: one test passed without skipping. Evidence is in `validation/fileviews-c82112a5`; this narrow pass does not override the review findings. Six implementation/correction subagents remain active, with three independent reviewers. No CI or PR has been started.

Fileviews round-one reports and reproduction artifacts are now preserved under `reviews/fileviews-round1`: eleven findings across staging publication/access policy, Windows metadata, remote hashing, SOPS/default conversion, SVG resources and UI behavior. The original writer is correcting this batch; independent re-review remains required.

Review-workflow candidate `bf6089b6` is preserved on `t3/implement-review-workflow-ideas`, with remote SHA verified. The writer reports all ten assigned ideas implemented and 81 targeted tests, check, formatting and qualified clippy passing. Archived evidence is under `validation/review-workflows-bf6089b6`. Three independent reviewers are examining the exact candidate; it is not accepted or integrated. Real Docker reproducer execution, combined checks and native platform gates remain outstanding.

## Setup, internals and remaining assignments

Setup candidate `b0698e9b` and repository-internals candidate `3e5b685a` are preserved with correct author and matching remote SHA. Their writer gates report 110 and 53 focused tests respectively, application builds, formatting and qualified all-target clippy (baseline macOS dead-code allowance). Handoffs, exact commands and hashed logs are archived under `validation/setup-b0698e9b` and `validation/internals-3e5b685a`. Neither candidate is accepted; setup independent review is running and internals review is queued. SSH/WSL borrowed-object mutation and full native Windows validation remain explicit internals limitations to assess against requirements.

The freed implementation slots now cover interaction/desktop/accessibility (ten ideas) and developer/forge integrations (eight ideas). These agents use GPT-6.1-Sol high in isolated worktrees; integrations builds on the preserved profile/context implementation. Thus 83 of the 93 unfinished ideas have assignments; the ten working-state/agent ideas await the next slot. Seven machines/imports ideas remain assigned but not started while their ownership prerequisite is implemented. Six coding agents and four independent reviewers are active, respecting both concurrent limits. No CI or PR has been started.

Fork `origin/dev` advanced to `0830614f` with per-repository worktree roots (#169) and build-status routing by push destination (#172). WIP integration `ed92fde2` incorporates both by an automatic conflict-free merge and passes all-target `cargo check --locked` on macOS. This is compilation only; evidence is under `validation/dev-0830614f-integration`. The local `dev` checkout is stale/divergent, so final assembly must use the verified fork `origin/dev`.

## History and round-one review checkpoint

History candidate `0de454f0` is preserved with matching remote SHA. Its writer reports 156 distinct focused tests (158 executions across overlapping filters), twenty repeated draft regressions, build/check/format and clippy with existing warnings. Full immutable handoff and logs are archived under `validation/history-0de454f0`. Two independent reviewers are covering the four investigation ideas and six history/recovery/preflight ideas; acceptance remains pending.

All 93 unfinished baseline ideas now have assignments. The final ten working-state/agent/outbox/subtree ideas are running in a separate worktree. This is assignment coverage, not completion: the seven machines/imports features still follow their substantial process-ownership prerequisite.

Workflow round one rejected `bf6089b6` with fifteen correction groups across three reports; setup round one rejected `b0698e9b` with six findings. Reports and reproduction artifacts are archived under `reviews/workflows-round1` and `reviews/setup-round1`. Corrections await coding capacity. Internals content review found three issues; remaining internals scopes are still under review. No batch is accepted solely from its writer's focused test results. No CI or PR has been started.

## Process-ownership prerequisite checkpoint

Preserved WIP `6a289fd8fe90b50750d639958f70f1c48337b8c4` on `t3/safe-process-supervision` has the required author and matching remote SHA, independently verified by the orchestrator. The writer reports all listed production callers, including bisect, migrated to a live local anchor/Windows Job and authenticated endpoint; legacy STOP_SCRIPT and raw remote PID-stop APIs are removed in that candidate. Reported recording gates are eight application and three OS tests, three Python and three Perl recording cases, and a Windows OS cross-check. These are writer reports, not new orchestrator test executions or runtime validation.

An independent static-only review is assigned to `audit_core_ideas` against that immutable commit before any narrow owned-child fixtures. The writer continues static endpoint-retirement, proof-handoff and Windows-ordering work on its own branch; those later changes need their own review binding. Runtime and full-suite gates remain held. The seven machines/imports feature ideas have not started, and no completion claim is made for the prerequisite or that batch.

History round one rejected `0de454f0` with seven core and five investigation findings; internals round one rejected `3e5b685a` with ten findings across three reports. Complete reports and benign reproductions are archived under `reviews/history-round1` and `reviews/internals-round1`. Shared credential-scope, Git-environment and push-URL semantics require coordinated corrections across the affected batches. These findings remain queued while all six coding slots are occupied.

The process prerequisite advanced to preserved `bc2521c6`, including `b9ec1294` (Windows launch ordering), `103a6b4b` (nonblocking forwarding and stdin proofs), and endpoint retirement/shutdown changes. Required author and remote SHA were verified. Original static review of `6a289fd8` failed and is archived under `reviews/process-safety-round1`. Three reviewers now examine the exact follow-up candidate separately for remote transport, native ownership, and bisect/caller cleanup; runtime remains held. No recording result is treated as runtime validation.

The integrations writer has preserved dependency license/changelog metadata (`dde8f355`) and rendered-rich search (`270a8a3b`); those two units are under independent review while the writer continues its other assigned integrations. Defaulted selected credential references and provider-native revision/account bindings will extend the existing profile/context interfaces. No external writes, hosted CI or PR have been started.

## Corrected visualization and review follow-up

Visualization correction candidate `cfc96d8f` (application `bc201e42`) is preserved and independently under review across content, UI and lifecycle. The writer records 176 unique passing tests / 180 executions, build, formatting and qualified lint; strict macOS lint retains the unchanged baseline warnings, and two process tests remain explicitly ignored. Its complete evidence is in `reviews/visualization-corrections`. WIP merge `a20ebf4a` incorporates the correction branch while retaining both the new worktree-root assignment and shared branch-band synchronization in the one resolved `load.rs` conflict. This is not acceptance of the batch.

The freed coding slot is correcting setup findings and producing a shared endpoint-scoped cached-credential/helper API for the affected history/internals/integrations paths. All three static reviews of process candidate `bc2521c6` failed; reports are preserved under `reviews/process-safety-round2` and sent to its writer. No runtime process gate is cleared.

Rich-content review rejected `270a8a3b` with eight P2 findings, now dispatched to its active writer; report and bounded reproductions are under `reviews/rich-round1`. Local `act` feature candidate `052721ed` is preserved with reported focused/recording/fake-tool gates and is under a separate independent review. No actual workflow, Docker execution, hosted CI or PR was started.

The exact WIP merge `a20ebf4a` passed all-target locked compilation and formatting. No tests were run for this merge gate; evidence is under `validation/visualization-dev-merge-a20ebf4a`.

## Latest correction checkpoint

Visual R2 cleared its prior scoped findings but identified three followups: agent-filter minimap invalidation, stale before-HEAD celebration, and postcard parent-edge work bounds. Reports are archived under `reviews/visualization-round2`. Parent fixes `4143becd`, `26061b7a` and `4a9a22c8` are preserved; 21 focused tests across their exact commits and the final all-target check passed. Evidence is under `validation/visual-followups-4a9a22c8`. Independent review of the three fixes is running; this is not final combined acceptance.

Fileviews corrected candidate `be013726` (application `10727c7e`) passed scoped UI R2, but core/content R2 still reject it: standalone lock-release replacement race, Linux remote xattr allocation before budget checks, and SVG nested-marker expansion. Complete reports and bounded evidence are under `reviews/fileviews-round2`. These three corrections are queued. The freed coding slot is now addressing the twelve history findings using the shared scoped credential API.

Local Actions R1 rejected `052721ed` with four P2 findings; `reviews/act-round1` contains the report and pure/recording evidence. Its writer is fixing them. Rich-content corrections are preserved at `53832430` and undergoing independent R2. Setup's shared credential API is preserved at `a1b03673` plus `8796fc1d`; its scoped independent review is running while broader setup corrections continue.

The process prerequisite has advanced to writer-preserved `fb569885`, with additional recording and cross-compilation checks only. Blocked protocol writes, pinned retirement and exclusive bisect claims/checked cleanup remain open. Static FAIL and the runtime/full-suite hold remain in force. The seven machines/imports features have not started. No CI or PR has been started.
