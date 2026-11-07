# Issue #171: diff, detail and composition acceptance

Scope: items 2, 3, 6 and 7. Visual reference: original issue #170 S2–S11, all opened and inspected. Read #167, #171 and docs/development.md. No forge/settings/history/CI changes are included.

The focused `responsive_` headless regressions can export screenshots with:

```sh
CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=/Users/jonas/Documents/gitcraft/target \
  GITCRAFT_UI_EVIDENCE=/tmp/gitcraft-c-final-evidence \
  cargo +1.95.0 test --lib responsive_ -- --test-threads=1
```

Widths are physical pixels; tests divide the layout width by 1, 1.25 or 1.5 and render at that pixels-per-point setting. This exercises both available logical space and raster scale. Diff matrices: 360/800/1280. Detail/composer matrices: 360/520/900. Extras/notes: 240/420/800 (including deliberately smaller-than-supported standalone panels). Full-app collapse additionally exercises 1100 → 360 → explicit reopen → 1100 and a divider compressed to 180 logical points.

## Checkbox evidence

| Issue checkbox | Status | Implementation / regression evidence |
| --- | --- | --- |
| 2.1 Reproduce narrow diff in unified/split and zoom | Complete | `responsive_diff_header_matrix…`, `responsive_diff_body_matrix…`; long-path File, unified, split and horizontally scrolled screenshots at all nine width/scale pairs. |
| 2.2 Header/title/origin/chips/options/actions do not collide | Complete | Path and origin truncate separately with full tooltips. Close/popout/Diff/File reserve space at the right before the title is allocated. Secondary controls have their own wrapping row, moving into a menu below 620 logical points. Tests assert title/action geometry and stable switch position across selection/File mode. |
| 2.3 Long paths, line actions, comments, Git/PR and split scrolling | Complete | Selected-line revert dispatches the exact chosen indices; busy dispatch is blocked. Full-body matrix checks text hitboxes and horizontal scroll. PR comment matrix checks long title versus close action and retains the draft while dispatching the comment action. Comment controls wrap at narrow widths. |
| 2.4 Preserve horizontal diff scrolling | Complete | Existing `Rows` horizontal-scroll and gutter/change-map code is unchanged. Body matrix scrolls long lines in unified/split modes and checks text moves. |
| 3.1 Remove minimum AND automatically collapse/reopen | Complete | Detail panel range is 0–900, with a 240-point usability threshold and window-space/height collapse. Window-driven collapse retains the visibility preference and prior width; an explicit footer reopen gives detail priority in a small window. Divider compression collapses completely and restores the last usable width. |
| 3.2 Independent horizontal file lists | Complete | Changed, staged, unstaged and all-files lists each have their own horizontal scroll ID; the editor/metadata remain outside them. Regression scrolls the first of two lists, asserts the other offset remains zero and the editor stays bounded. |
| 3.3 Compact adaptive selected metadata/extras/notes | Complete | Hash-copy, parent navigation and build actions use an inline wrapping metadata row; author/date truncate with tooltips. Notes namespace selection and creation move into one compact icon popup; fetch/push become icons beside the remote selector. Existing note edit/save/delete and asynchronous operations remain intact. Selected-commit/all-files and notes screenshot matrices cover this. |
| 3.4 Contained In refresh collision | Complete | Refresh is allocated first at the right, then title/summary in remaining space. Summary truncates with full hover text. Long ref chips also truncate with tooltips. Tests verify refresh does not toggle expansion and the summary cannot reach it. |
| 3.5 Composer heading and identity footer overlap | Complete | One wrapping icon toolbar shares the heading's layout; identity action is allocated first, with truncated text in remaining width and the full identity/source on hover. Geometry matrix checks all tool rectangles and the identity text/icon boundary. |
| 3.6 Editability/focus/scroll/staged state through collapse | Complete | Collapse hides presentation only. It leaves RepoTab drafts, selected detail, amend, work data, note editors and egui scroll state in place. Focus in the detail panel is saved and restored on reopen. The layout regression verifies draft/amend/width retention and automatic/manual reopen behavior. |
| 6.1 Anchored icon Diff/File choices | Complete | Accessible selected-state `file-diff` and `book-open` toggles are allocated before title/secondary controls. Geometry checked in every header matrix case. |
| 6.2 Icon revert with distinct selected-line semantics | Complete | One undo icon retains `RevertFile` versus `PickLines(Revert)` routing, Clear and busy guards; tooltip/accessibility names identify the scope and selected count. Regression checks both dispatches and busy refusal. |
| 6.3 Compact algorithm menu, persisted choices | Complete | One code icon shows the current algorithm and filetype in tooltip. Menu retains Default/Myers/Minimal/Patience/Histogram. Existing per-filetype storage/restore-place logic is unchanged; overflow regression exercises keyboard opening and selection. |
| 6.4 Adaptive consolidated view controls | Complete | Split/unified/whitespace/whole-file/algorithm/change navigation/rich/rendered/layout/file actions share a wrapping secondary control group, reachable in an overflow popup at narrow widths. Existing view-option action semantics remain intact. |
| 7.1 Merge composition controls, retain lower tools | Complete | Heading row includes Amend, Sign off, Fold, AI, Skip hooks, templates/prepare hook, type/scope, coauthor and message options. Normal widths have one row; exceptionally narrow widths wrap without overlap. |
| 7.2 Amend/Sign off toggles and guards | Complete | Persistent accent, selected accessibility state, tooltip and keyboard support. Existing pushed-amend warning and DCO reason text remain. Amend blocks while busy/unborn/continuing. Regression exercises keyboard Amend, Sign off dispatch and continuation guards. |
| 7.3 Fold icon/picker/refusal reasons | Complete | One git-merge icon retains its popup, eligibility checks, amending refusal, disabled explanations and operation guard. |
| 7.4 Distribute icon in STAGED header | Complete | Header icon retains staged/attached/idle/no-in-progress eligibility and explains each refusal. No extra row. Regression verifies detached and continuation cases refuse dispatch. |
| 7.5 View all files inline header toggle | Complete | Folder-tree toggle is alongside changed-file summary, with selected state and tooltips distinguishing complete tree/changed files. Regression checks same-row geometry, dispatch and unchanged-tree files. |
| 7.6 Consider Skip hooks toggle | Complete | Implemented as a third stateful icon, retaining one-commit reset and continuation reset. Regression verifies toggle and continuation reset. |
| 7.7 Preserve commit/staging/continuation/templates/coauthors/AI/hooks/DCO/amend | Complete | Existing action/operation paths are reused. Focused UI regressions exercise toolbar presence and action safety; existing compose, DCO, fold, algorithm and remerge tests are run as focused compatibility checks. |


## Actual validation

- `cargo +1.95.0 check --lib`: passed.
- `cargo +1.95.0 fmt --all -- --check` and `git diff --check`: passed.
- `cargo +1.95.0 test --lib responsive_ -- --test-threads=1`: **12 passed** on final source (log `/tmp/gitcraft-c-tests-final.log`).
- Focused existing workflow regressions, run sequentially from the successful test binary: compose 39, DCO 16, Fold 30, algorithms 6, extras 5 and remerge 5: **101 passed**.
- `cargo +1.95.0 clippy --lib --tests`: **passed** (log `/tmp/gitcraft-c-clippy-verified.log`). Only existing platform dead-code warnings in `desktop_entry.rs` and `diagnostics.rs` remain. New test fixtures use fallible returns and explicit render-result assertions; no lint exceptions were added.

Screenshot output: **146 PNGs**, `/tmp/gitcraft-c-final-evidence/`. Representative review paths:

| Surface | Narrow / 150% | Normal / 125% | Wide / 100% |
| --- | --- | --- | --- |
| Composer | `/tmp/gitcraft-c-final-evidence/compose-360-1.5.png` | `/tmp/gitcraft-c-final-evidence/compose-520-1.25.png` | `/tmp/gitcraft-c-final-evidence/compose-900-1.png` |
| Selected commit | `/tmp/gitcraft-c-final-evidence/selected-commit-360-1.5.png` | `/tmp/gitcraft-c-final-evidence/selected-commit-520-1.25.png` | `/tmp/gitcraft-c-final-evidence/selected-commit-900-1.png` |
| Diff unified | `/tmp/gitcraft-c-final-evidence/diff-body-360-1.5-unified.png` | `/tmp/gitcraft-c-final-evidence/diff-body-800-1.25-unified.png` | `/tmp/gitcraft-c-final-evidence/diff-body-1280-1-unified.png` |
| Diff split | `/tmp/gitcraft-c-final-evidence/diff-body-360-1.5-split.png` | `/tmp/gitcraft-c-final-evidence/diff-body-800-1.25-split.png` | `/tmp/gitcraft-c-final-evidence/diff-body-1280-1-split.png` |
| Notes | `/tmp/gitcraft-c-final-evidence/notes-240-1.5.png` | `/tmp/gitcraft-c-final-evidence/notes-420-1.25.png` | `/tmp/gitcraft-c-final-evidence/notes-800-1.png` |

Also inspect `diff-overflow-algorithm.png`, `detail-auto-collapsed.png`, `extras-240-1.5.png`, `all-files-360-1.5.png`, and `pr-comment-360-1.5.png` in that directory. Header fixtures, File mode, horizontal-scroll variants, and every other width/scale combination are exported there too. Screenshots were rendered with the real egui widgets and bundled fonts; key narrow and normal outputs were opened for visual review.

The headless harness does not exercise a physical display, a running forge, external AI processes or actual hook execution from the UI. Existing operation tests cover the underlying workflows; no live PR comment, history rewrite, or commit operation is performed by these visual fixtures.
