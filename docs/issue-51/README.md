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
