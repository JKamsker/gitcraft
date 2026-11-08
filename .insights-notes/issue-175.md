## Summary

A consolidated proposal for **12 interactive visualizations** that help developers *understand and act on* code changes, CI performance, parallel agent/worktree activity, review status, and release progress.

Related idea collections: **#46** (history minimap, activity calendar, churn treemap, animated growth), **#173** (staged tests, CI comparisons, worktree resource management, purpose/pattern-based PR review), plus **#18**, **#34**, **#35**, **#40**, and **#48**.

**Goal:** go beyond charts for decoration. Selecting a node, hotspot, edge, cell, or timeline segment should open the precise **commit, diff/hunk, symbol, worktree, PR, test, job, or operation** that explains it.

This is an **ideas backlog, not one implementation PR**. Split chosen items into focused issues later.

Effort estimates: **M** = moderate/new view and data processing; **L** = deeper analysis/integration across revisions or providers.

## P0 — standout visualizations

- [ ] ### 1. Change impact graph (L)
  Select a changed function/class/API and see a **revision-aware dependency graph**: affected callers, callees, modules, tests, and changes outside the PR diff.
  - Visually distinguish direct references, inferred impact, and unknown/unsupported relationships. Filter to public API changes, changed symbols, or one review group.
  - Click any node to open the definition/diff **at the exact base/head revision**, not the current working tree. Click an edge for supporting evidence.
  - Start with one language or configured language server; degrade gracefully when semantic analysis is unavailable.
  - **Why:** makes large/refactor/agent PRs reviewable in terms of consequences, not just changed lines.
  - Related: #35 (review freshness), #36 (symbol history), #173 (revision-scoped navigation).

- [ ] ### 2. CI critical-path and regression explorer (M–L)
  Visualize a workflow as a **DAG + job/step waterfall**, including parallelism, dependencies, waiting time, durations, failure points, and the **critical path**.
  - Overlay a previous successful/fast run to expose newly slow jobs/steps and lost parallelism; align by job identity and matrix dimensions.
  - Separate measured duration from queued/waiting time when data permits; clearly mark gaps where only coarse timestamps or no dependency edges are available.
  - Click a bar to open the step/log and associated commit; surface recorded cache/runner/toolchain differences without claiming causality.
  - **Why:** answers “What is slowing CI, and why did Windows double?”
  - Related: #18 (pipeline graph), #173 (compare CI runs). This is their **visual bottleneck/critical-path layer**.

- [ ] ### 3. Worktree & agent constellation (M–L)
  A navigable graph of **branches → worktrees → coding-agent tasks → PRs → CI runs**, across local, WSL, and SSH where supported.
  - Distinguish active, idle, dirty, unpublished, conflicting, merged, and failed states. Show meaningful edges such as same base, shared branch, conflicting file edits, or originating task.
  - Clicking nodes opens the existing worktree/PR/CI/terminal view; selecting two related branches can launch a diff or conflict preview.
  - Overlay resource pressure (disk usage, processes/ports only when reliably discoverable). Display provenance and last refresh; do not infer active agents just because a branch exists.
  - **Why:** many parallel coding agents quickly become hard to reason about.
  - Related: #34 (agent worktrees), #48 (agent checkpoints), #173 (resource lifecycle management).

## P1 — high-value analysis & review

- [ ] ### 4. Code evolution heatmap (M)
  **Commit or time on X**, **files/folders on Y**, colored by changed lines, additions/deletions, or change frequency. Group by directory, zoom, filter branch/range, and open a selected commit/file diff. Mark the loaded history boundary and avoid counting the same commit multiple times.

- [ ] ### 5. Change-coupling network (M–L)
  Connect files/modules frequently changed together, weighted by **co-change frequency**. Filter by time range, minimum sample size, and directory. When reviewing edits, highlight historically related files *not changed*. Show the commits behind each edge; **correlation is a hint, not a proven dependency**. Complements the churn treemap in #46.

- [ ] ### 6. Branch conflict radar (M–L)
  Graph potential conflicts between active branches/worktrees: edge thickness indicates **observed overlaps or dry-run merge conflicts**, not a guessed probability. Click an edge to inspect exact paths/hunks and possible merge ordering. Cache results per branch-tip pair and invalidate when tips move. Related: #32 (conflict previews), #48 (agent collision matrix).

- [ ] ### 7. PR review heatmap (M–L)
  An overview by file/hunk of **reviewed vs. unreviewed**, unresolved comments, changed-since-last-review, uncovered changed lines (when revision-matched coverage exists), and other verified signals.
  - Distinct encoding for each signal; **unknown ≠ failing**, “unreviewed” ≠ “unsafe”.
  - Click a hotspot to navigate to the exact unreviewed or disputed hunk. Tie reviewer state to PR head SHA and mark stale after force-push.
  - Related: #35 (review freshness), #173 (purpose grouping, coverage).

- [ ] ### 8. Repeated-edit transformation matrix (M–L)
  **Patterns on one axis, files/occurrences on the other**. Spot 57 matching API migrations and 3 deviations immediately. Clicking a cell opens its before/after hunk and unchanged context. Begin with normalized token/patch matching, add AST support later; never silently collapse an outlier. Related: #40 (structural diffs), #173 (repeated-edit grouping).

- [ ] ### 9. Deployment provenance flow (M per provider)
  An evidence-backed **commit → CI run → artifact digest → staging → production** flow. Select a fix and see which environments are *known* to include it, how fresh that evidence is, and what is queued for deployment. Click through to deployment event, artifact, or SHA. **Successful CI or a tag must never be presented as proof of production deployment.** Related: #173 (deployment markers).

## P2 — specialized, still compelling

- [ ] ### 10. Rebase before/after ribbon map (M–L)
  Animate or render ribbons between original and rewritten commits using **range-diff / patch identity**. Highlight unchanged, modified, split/squashed, reordered, and dropped commits; distinguish inferred match from exact match. Click to inspect paired patches. Include reduced-motion/static mode. Related: #33 (compare branch versions), #35 (review after force-push).

- [ ] ### 11. Test coverage relationship map (M–L)
  Visualize **changed symbols/hunks ↔ tests** and gaps in evidence, based on coverage reports for the **same tested revision**. Links to test results, failures, and the changed code. Only claim per-test relationships if the source format actually records them; otherwise display aggregate coverage. Unknown or stale coverage remains explicitly unknown. Related: #34 (test lane), #173 (coverage in diffs).

- [ ] ### 12. Code complexity / churn timeline (M–L)
  Track file/module **size, churn, complexity (where a reliable language analyzer exists), and test evidence** over chosen commits/time. Highlight hotspots whose change frequency and complexity are both increasing. Clicking an inflection point opens the responsible commits. Specify which metrics are factual vs analyzer-defined; avoid simplistic developer quality scores. Complements #46 (churn/contributor atlas).

## UX / technical constraints applying to all visualizations

- **Actionable:** every selectable visual element navigates to existing GitCraft views or opens a relevant operation/inspection; include text/table fallback.
- **Provenance & freshness:** show branch/tip SHAs, provider, collection time, analyzed history/range, and whether the data is incomplete or stale.
- **Scalable:** aggregate intelligently; progressive loading, zoom/search/filters, async computation off the UI thread, caches keyed by relevant revisions.
- **Readable & accessible:** graph legend, hover details, keyboard navigation, sufficient contrast, reduced motion and no color-only meaning.
- **Honest semantics:** distinguish observed Git/provider data from inferred static-analysis results; do not manufacture confidence, coverage, dependencies, or deployment evidence.
- **No destructive side effects:** analysis/preview is read-only; any subsequent mutation requires explicit review/confirmation.
- **Architecture:** aim to reuse GitCraft's graph/diff, Git/forge/CI integration and egui rendering. Rendering complexity can evolve independently from data collection.

## Suggested implementation order

1. **CI critical-path waterfall** — readily actionable once workflow/job/step timings are available (#18 / #173).
2. **Worktree/agent constellation (read-only v1)** — build from existing branches/worktrees/PR relationships; add observed process/resources and conflicts later.
3. **PR review heatmap + repeated-edit matrix** — leverage #35/#173's review data and gradually improve analysis.
4. **Change impact graph** — strongest long-term differentiator, but start with revision-correct navigation and one supported language before claiming complete call graphs.

### Minimum acceptance criteria for any first slice
- Works on a real repository with a clearly described data source.
- Can navigate from visualization to the underlying supporting evidence.
- Does not block UI interactions while calculating.
- Handles missing/stale/partial information visibly.
- Includes a test for revision changes and a fallback for unavailable integrations.

**Not included here because #46 already covers them:** general activity calendar, history minimap, repository churn treemap, and animated repository growth.
