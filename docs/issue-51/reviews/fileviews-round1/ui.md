# Independent fileviews UI review, round 1

**Verdict: FAIL.** Two behavior/performance blockers and one exact requirement gap remain in this review's scope. This is a scoped review, not approval of staging transactions, standalone save safety, SOPS privacy, the full batch, or the final combined application.

## Immutable binding and scope

- Candidate: `c82112a5ab8ba461c487c3266462a81896c85d33`.
- Application: `5df74dbb3792a34ee0b1d13c301c9aa552aa6c69`.
- Base: `a850de26b0a0778ea96fe34cd1498e6e78928492`.
- Checkout: `/Users/jonas/.t3/worktrees/gitcraft/t3-e06bdc42`, branch `t3/diff-staging-views`. HEAD matched the candidate and the tracked tree was clean at review start. `git diff --stat application candidate` contains only the three validation documents; no application changes.
- Read original issue bodies #40/#44/#50 from `/tmp/gitcraft-issue51-issues.json`, the preserved Markdown/JSON validation record and source changes. Focus: migration map, branch matrix, cross-file moves/navigation, wrapping, DVC Pull. No tracked edits, CI, remote DVC transfers, native GUI launch or unsafe legacy process tests. No subagents.

## Blocking findings

### FV-UI-R1-1 — P2: changing matrix selection leaves an unrelated comparison visible or accepts its late result

**Source:** `src/repo/file_comparison/mod.rs:315-325`, together with `State::accept_pair` at lines 117-120 and `State::begin` at lines 97-108.

`comparison_cell` removes a selected cell and immediately returns. Starting a fresh selection after two cells also returns after storing its first cell. Both paths retain `state.pair`, `request`, `generation` and `loading`. The only clearing/cancellation occurs when another *complete pair* calls `load_comparison_pair`. Therefore the displayed highlighted selection and content comparison no longer describe the same two cells. A pending comparison still passes `accept_pair`'s unchanged request check after deselection.

**Concrete reproduction:** create branches release-a/release-b with distinct content, open File versions across branches, select their two cells and wait for the content diff. Deselect one selected cell: one highlight remains, while the old two-cell diff stays below. Alternatively select a third cell after a completed pair: only that third cell is highlighted while the prior pair remains. For the async variant, deselect either cell while its pair is loading; the old pair still arrives afterward. The existing two-cell UI test stops before any of these interactions.

**Required correction:** invalidate/cancel the pair request, clear its result and clear loading whenever the active cell pair changes, including transitions to zero/one cell. Keep the matrix's `data_request` ownership independent so clearing a pair does not invalidate displayed cells. Bind pair completion to the actual selected pair or a dedicated pair generation.

**Regression evidence needed:** operate actual matrix cells through 2→1, 2→new-1 and 1→0 transitions; deliver an older pair after deselection and after a new selection; assert both the visible comparison and spinner disappear and the late result is rejected.

### FV-UI-R1-2 — P2: the selected cross-file moved block reruns quadratic comparison on every render

**Source:** `src/repo/file_comparison/code_moves.rs:345-348`; `src/repo/diff/moved_view.rs:42-46`; `src/repo/diff/moved.rs:263-289`.

The new change-wide view calls `moved_view::Opened::new(&data.data, i)` in its paint function every frame. `Opened::new` calls `moved::compare`, which allocates and fills the full LCS table for edited moves up to 1,000,000 cells. The existing normal diff stores its opened comparison; this new path does not. Scrolling, hover, resize and other repaint causes repeat the analysis on the UI thread even though the accepted snapshot and selected block are unchanged.

**Concrete reproduction:** move a 1,000-line block between two tracked/staged files, retain long similar line prefixes (about 1,800 characters) and edit one middle line. This is about 1.81 MiB per side and 3.63 MiB of patch content, below the documented 2 MiB version / 6 MiB analysis limits; the unchanged pieces join around the edit. Select the resulting M-marker. Every render rebuilds the 1,000×1,000 comparison, before painting any visible rows.

**Measured evidence:** an optimized standalone microbenchmark compiles the exact production `moved.rs` algorithm with minimal host types, changing only inner module-doc comment spelling. Three calls on that fixture took approximately 58 ms each in the initial run. This is algorithm cost only, not an exercised native GUI frame rate. The source and rerun output are `/tmp/gitcraft-fileviews-ui-review-moved-bench.rs` and `/tmp/gitcraft-fileviews-ui-review-moved-bench.log`. The source path confirms this work repeats on paint.

**Required correction:** prepare/cache `Opened` per immutable accepted snapshot + selected block, preferably in the existing analysis worker or a cancellable selection worker. Painting should consume that result; it must not run LCS or reconstruct all comparison rows. Keep cancellation/generation ownership when selection changes.

**Regression evidence needed:** open an edited cross-file block, cause repeated UI frames and scrolling, and show that comparison is prepared once rather than once per repaint; cover stale selection results and one fixture near the supported comparison limit. A test asserting only the computed row list does not cover this regression.

### FV-UI-R1-3 — P2 requirement gap: line-history diffs have no Wrap switch

**Source:** `src/repo/diff/view.rs:488-500` and `view_options` at lines 738-744.

Issue #40 explicitly asks for “a wrap switch to every diff.” The new switch is bundled into `view_options`, whose call is gated by `view.lines.is_none()`. `git log -L` line-history patches set `view.lines`, so they use the wrapped patch renderer according to the global setting but expose no local Wrap switch. The adjacent comment only explains why whitespace/whole-file choices are inapplicable; wrapping remains applicable to those long text lines.

**Concrete reproduction:** open History of these lines for a long source/prose line and open a historical patch. Its header has change stepping but lacks Wrap; the user must leave that diff, toggle another ordinary diff and return to change its wrapping.

**Required correction:** expose Wrap independently from options that are inapplicable to `log -L`, for every visible ordinary text patch. Keep whitespace/whole-file/algorithm restrictions where needed.

**Regression evidence needed:** an actual line-history diff exposes and operates Wrap; its logical line target/selection remains intact after toggling. Existing helper-only wrap tests do not exercise the header gate.

## Requirement and behavior observations

| Area | Observed implemented behavior | Review qualification |
| --- | --- | --- |
| Migration map #40 | Staged/commit/range readers pin revisions; Git `--raw -z` recognizes renames and copies with `--find-renames=50% --find-copies=50% --find-copies-harder`. Virtualized old/new folder rows draw clickable ribbons and show similarity. Clicking reads the pinned raw blobs, separates movement from the content-only comparison, and visibly labels modes/missing/binary limitations. | No additional blocker found. Source and focused tests cover copies/renames, literal/raw paths and immutable content. List limit is explicit; unborn staged state is handled. |
| Branch versions #50 | Full branch refs resolve to pinned commit OIDs; literal folder filters and a union of paths create the matrix. Equal OIDs share markers, missing cells and differing modes are visible, and two cells open read-only raw content. Two-to-eight branches and 2,000 paths are explicit bounds. | Selection/result coherency fails as FV-UI-R1-1. Existing positive UI test exercises only the initial complete pair. |
| Cross-file moves #40 | Explicit staged/tracked-working/commit/range/commit-to-working analysis, raw literal path handling, change checks on mutable inputs, reported exclusions and bounded input. Shared M-markers and muted block colors identify paired source/destination; a selected block shows edits and numbered navigation waits for the destination patch. | Rendering fails as FV-UI-R1-2. No native runtime claim. Existing actual destination navigation test passed; it does not exercise repeated render analysis, all source-side variants or stale working-file navigation. |
| Ordinary wrapping #40 | Unified/split offsets keep logical patch indices, hit testing uses multirow galleys, selection/find rectangles span visual rows, logical navigation maps to physical offsets, map marks use physical offsets, and algorithm restoration retains a logical place. Settings persist the ordinary patch choice. | Header requirement gap FV-UI-R1-3. Existing wrap test validates helper geometry and Unicode rectangles, not actual drag/copy, keyboard navigation, search reveal or map clicks in both layouts. Source inspection found no further definite blocker in those paths. Existing 5,000-character ordinary-line display cap remains, with an explicit omitted-character note and whole-line copy handling; do not claim unlimited ordinary patch lines. |
| Private/standalone/snapshot wrapping | Private plaintext uses its own galley/offset path and Wrap control; generic standalone/migration/matrix snapshot comparisons have a Wrap control and full long text, virtualized by physical row offsets. | This review does not certify SOPS privacy or standalone save safety. Snapshot comparison text is a distinct read-only renderer; it has no ordinary diff text selection/find/change-map controls. Do not extrapolate ordinary diff interaction coverage to these panes. |
| DVC Pull #40 | Palette action and `.dvc` pointer button open confirmation describing current checkout, DVC installation, materialization and lack of automatic undo. The fixed Git alias invokes `dvc pull` on the repository machine through the existing operation runner. Network classification includes the alias in both runner offline refusal and command long/network handling. Busy/offline checks occur at execution; completion follows the shared refresh path. | No additional blocker found in the inspected wiring. No actual DVC remote, SSH/WSL machine or native Windows shell was exercised. The existing fixed-argv/network-classification test was rerun; the preserved writer evidence separately records an owned fake-DVC command test. |

## Checks and limits

Reused the test executable named by the archived application logs, copied it to `/tmp/gitcraft-fileviews-ui-review-c82112a5-tests`, and recorded SHA-256 `e3316b2d2b86f2667a519340fd7cbc563837a2abb9dc65f967239cf85769cff2`. No fresh Cargo compilation or independent cryptographic source-to-binary attestation was performed; this is a rerun of the preserved build artifact. The candidate/application difference is documentation only.

**11 exact focused tests passed**: migration immutable renames/copies; matrix equal/missing/modes/pinned refs; literal/non-UTF8 path distinction; link/binary reads; edited/reindented cross-file markers; staged/commit/range/working moved readers; actual destination navigation; actual initial two-cell diff; actual ribbon content opening; Unicode wrap geometry/rectangles; fixed DVC argv/network guards. Full names, durations and statuses: `/tmp/gitcraft-fileviews-ui-review-c82112a5-tests.json`; output: `/tmp/gitcraft-fileviews-ui-review-c82112a5-tests.log`.

Those successes do not cover the three findings above. Source inspection, positive tests and a pure production-algorithm microbenchmark were used; no full suite, CI, live remote transfer, native GUI, risky cancellation/process tests or modifications to the candidate were performed. New dependency and native-save portability review belongs to the other reviewer; this scoped result makes no cross-platform runtime claim.
