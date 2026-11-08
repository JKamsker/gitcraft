# Independent visual follow-up review R3 — scoped PASS

Candidate: `4a9a22c806126a813d9d5633a699dd7cafa135da`, branch `t3/implement-issue-51-ideas`. Source base: `baf5126118232b2f6da2f8411bef2e884ff6c474`. Reviewed changes are exactly `4143becd`, `26061b7a`, and `4a9a22c8`, affecting four application files. **All three assigned R2 blockers PASS; no new blocking finding found in these paths.** This is not whole-task, whole-feature, final combined-candidate, process-safety, or PR acceptance.

## Binding and method

Read the original full `/tmp/gitcraft-51-visual-{content,ui,lifecycle}-r2.md` reports, the correction README, manifest and receipt, and the full relevant production/test paths. Independently checked all 28 historical receipt source hashes at `bc201e42d3480c4cc9525c948a69fb06afd3ac73`, all 30 compressed/raw log hash pairs, the manifest hash, and the gate-script hash. That is historical evidence integrity, not current-candidate execution.

Initial parent checkout was clean and HEAD equalled the requested candidate. Exported `git archive` at the full immutable SHA before executing anything; built it independently with Rust/Cargo `+1.95.0`, `--locked --offline`, `CARGO_BUILD_JOBS=2`, `TMPDIR=/private/tmp`, validation-shell and the requested PATH prefix, using a private APFS-cloned target. The parent checkout/target was never written by this reviewer. Copied the resulting exact test executable before any probe build. All 1,516 archived source files were independently compared byte-for-byte after the build and remained unchanged.

The parent later advanced to documentation-only `2e4b87ffe431ac2660a42c71da8be7ebaa5a28c7`; its 87 changed paths were all under `docs/`, with no application changes and clean status. Execution/review remains bound to the original archive and exact requested SHA, not the moving checkout. Source line references below are relative to that candidate.

Three additional review-only tests were appended in a separate probe copy. All original production and existing test bytes remain exact prefixes of those three files; all other files remain identical. `source-integrity.json` records this distinction. These probes are independently authored tests against unchanged application logic, not new committed candidate tests.

## Per-finding dispositions

| R2 finding | Verdict | Source and independent evidence |
| --- | --- | --- |
| Lifecycle P2: agent filter leaves completed/in-flight minimap search stale | **PASS** | `src/repo/authorship_view.rs:158-202` calls `cancel_minimap()` after installing refreshed matches and when clearing agent-filter matches. `graph_view/minimap.rs:106-109` cancels the Reader and dirties the model. `history_reader.rs:29-41,46-68` retires the generation, drops pending captures, and rejects late results. Next preparation captures the current search Arc and clears old bins before polling (`minimap.rs:89-104`). |
| UI-R2-1 P2: stale UI before-HEAD lets a no-op merge-mode Pull celebrate an earlier merge | **PASS** | `operations.rs:109-134` observes HEAD before execution and after successful execution on the same mutation worker, then applies those observations before handling completion/starting queued work. `celebration.rs:85-95` replaces the Merge baseline with the actual worker-before value; a missing Merge baseline or nonempty FirstCommit baseline clears that candidate. `:140-149` requires an actual before/after change, and Pull additionally requires the observed merge commit's first parent to equal worker-before. Pending successes and pinned after-target confirmation remain intact (`:98-117`). |
| Content P2: one postcard row can cause unbounded parent/lane scans before cancellation | **PASS** | `postcards/render.rs:9,222-247` checks the 200-row limit and cumulative parent count against 1,024 using checked subtraction, before `graph::layout()`. It counts excluded and repeated parents on every row. Oversized input returns an actionable error without dropping edges or nodes. Cancellation is checked per preflight row and immediately after bounded layout, before font/image/PNG composition. `render.rs:25-33` routes actual render through this gate; errors reach visible postcard status (`postcards/mod.rs:203-227,310-317`). |

### Minimap semantics and regression quality

The committed lens test drives the actual AgentFilter action and refresh/clear paths and checks dirtying, but does not by itself verify stale Reader completion. The independent probe fills an already completed minimap, starts a controlled old worker that deliberately returns an obsolete model despite retirement, then drives actual apply, refresh and clear transitions. Each old result carries sentinel row count 777. The next accepted model must have exactly two rows and the new expected hit bitmap, and the sentinel must never appear. All three transitions passed, including the real Reader queue/poll path and real search replacement.

Existing four minimap tests and the actual Git-backed lens test also passed on the unmodified candidate. Reviewed refresh callback wiring in `authorship_view.rs:137-144` and Action dispatch in `apply.rs:123`. Clearing while a different ordinary search owns the result set correctly leaves that search untouched. The withdrawn ghost concern is not a finding: production minimap entry remains guarded by `!ghost` in `graph_view/mod.rs:335-337`.

### Celebration semantics and regression quality

For actual worker-before=worker-after=M and stale UI-before=A, the new baseline is M, so the transition predicate is false even when M has parents `[A, topic]`. The observation callback and `OpDone` execute in that order before `after_op` starts the next waiting action (`operations.rs:130-134,198-215,308-315`; `running.rs:142-155`). An unrelated later read/HEAD movement cannot substitute for the pinned after-target. Empty/invalid Merge-before is refused; a UI-eligible first commit is refused when the worker observed an already existing HEAD.

All eight committed celebration tests passed, including the ordinary local-Git worker fixture that leaves UI HEAD stale, externally installs a merge, runs the real successful no-op merge command, and confirms zero new cues. The new committed stale-Pull regression covers the precise before=after=M state through production celebration methods. The independent probe additionally queues a genuine A→M merge candidate followed by a stale-UI M→M Pull candidate, delays confirmation until both are pending, and verifies exactly one cue total and no replay. It also verifies invalid first-commit and missing Merge-before rejection. The queued pair in this extra probe is a deterministic state/callback simulation, not two real network operations. Existing tests retain successful merge-mode Pull, fast-forward/failure exclusion, delayed unrelated-HEAD confirmation, queued-success retention, and reduced-motion behavior.

HEAD observations are samples around the operation, not a repository transaction against unrelated external Git writers. No stronger external-concurrency guarantee is inferred.

### Postcard bounds and regression quality

With N≤200 rows and cumulative E≤1,024 parent entries, layout can open at most N+E≤1,224 lanes. Every linear lane search and per-row lane traversal in `graph.rs:55-105,129-136` therefore has a finite bound. A conservative count for those scan iterations is at most `(4N + 2E) * (N + E)` (about 3.49 million at both ceilings), rather than the prior arbitrary-parent quadratic work. Actual postcard hashes/parents come from validated 40/64-byte IDs, and Snapshot strips refs/other commit metadata before rendering. Repeated parents do not evade the cumulative charge; excluded parents are charged equally.

The committed injected-budget test checks the exact threshold across repeated/excluded parents in multiple rows, a one-row 1,025-parent overflow, and cancellation during preflight and after layout. Independent boundary tests additionally accept all 1,024 outgoing edges without truncation, accept 200 rows with 1,000 repeated parent entries, refuse those rows after expansion to 1,025 entries, and refuse 201 rows. The actual `render()` entry rejects a one-row overflow after only the initial and preflight active checks. It also returns cancellation at the post-layout check before composition. Existing dense decoded-PNG, preview/anonymization, text and export tests all passed.

**Limit:** there is no callback inside `graph::layout()`. The correction bounds the whole non-interruptible section and rejects cancellation immediately afterward. It does not provide mid-layout preemption or a measured wall-time guarantee. The review accepts the R2 correction's explicit pre-layout work-budget alternative; it does not claim a new cancellable graph algorithm. No giant fixture, OOM experiment, or benchmark was run. Range parsing and Snapshot copying still have their existing capped-input costs; the new gate specifically closes unbounded lane/layout amplification before composition.

## Independent execution

Exact unmodified archived-candidate build: **PASS**. Copied executable SHA-256: `7ef2699dd2abc4151fda8d798e58ca7d699478514a13b6782f75c2fead5b409d`.

**34 exact-candidate tests passed, zero failed, zero ignored**, selected as:

- actual authorship lens: 1;
- minimap: 4;
- celebrations: 8;
- postcards UI/render/export: 12;
- postcard Git metadata parser: 1;
- graph layout filter: 8 (five commit-layout tests and three harmless pure CI-view layout tests matched the substring filter; no CI operation occurred).

**3 additional independent append-only probes passed, zero failed, zero ignored**, exercising the specific stale-result, combined-cue and boundary cases described above. Those executions are recorded separately from the 34 unchanged-candidate tests.

Writer logs were copied and hashed for provenance: minimap 1 pass at `4143becd`, celebrations 8 passes at `26061b7a`, postcards 12 passes at `4a9a22c8`. They are writer execution, separate from the independent executions. The parent all-target check was not used as an independent reviewer gate. Historical fmt/lint/production build evidence was inspected, not rerun; this report claims no fresh fmt, lint, or production build gate.

No subagents, parent/shared-checkout edits or commits, CI/PR/API/external writes, OS-process cancellation, kill or process-runtime test, host PID selector, signal command, full suite, native speaker action, native save-dialog test, screen-reader application, or Windows/Linux runtime test was performed. Ordinary local-Git fixture tests and pure/headless thread/Reader and local-file tests were the only runtime scope. In particular, a pure Reader token/generation exercise is not OS-process containment evidence. The separately held ownership-supervisor/full-runtime gates remain held; these narrow passes do not clear them or the consolidated final PR gates.

## Deliverables

- Report: `/tmp/gitcraft-51-visual-followups-r3.md`.
- Evidence manifest: `/tmp/gitcraft-51-visual-followups-r3-evidence-manifest.json`.
- Immutable source archive, source-integrity check, copied executable, exact build outputs, per-group logs, append-only probe source/script/build/executable/results, copied R2 reports and writer logs: `/tmp/gitcraft-51-visual-followups-r3-evidence/`.

Final scoped disposition: **PASS / PASS / PASS for the three parent visual R2 follow-ups on `4a9a22c806126a813d9d5633a699dd7cafa135da`.** Other reviewers' scopes, the original broader R2 disposition, process safety, and whole-task acceptance are not superseded.
