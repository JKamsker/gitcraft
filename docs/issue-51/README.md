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
