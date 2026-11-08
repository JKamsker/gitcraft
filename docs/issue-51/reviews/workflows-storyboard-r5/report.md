# Storyboard navigation correction R5

**SCOPED PASS — MERGE-R4-1 closed** at exact candidate `00b8539e50d60d97bcba24621ccb14a7f67b2a3f`, tree `db1779d003fa8f3d2178b9f6b81bef0487cfd207`.

Read the complete prior R4 report and its `-manifest.json`, and adapted its exact-branch inert storyboard reproduction. The entire immutable delta from `f04ecabccd1da063528495aac53d316a3a428913` changes only `src/repo/review_workflow/storyboard.rs`, adding three statements. No mutable checkout source was reviewed or edited.

## Named correction

`storyboard.rs:517–530` now closes the review document, calls the existing dossier and symbol close methods, and sets `file_comparison = None` **before** the existing `open_file_view(path, Some(commit), FileTab::File, ctx)` call. Its original delayed logical `goto_line` and `marked_line` assignments remain unchanged. Thus all four page masks relevant to this finding are absent when the pinned file is installed.

These are the real existing page-reset methods, rather than flag-only substitutes in production: `line_dossier/mod.rs:55–69` closes the page, cancels its three Readers and clears retained result/evidence/summary/ignore state; `symbol_biography.rs:41–49` closes its page, cancels its two Readers and clears retained result/evidence/error state. The inert adapter extracts both complete close bodies unchanged and replaces only Readers with recording counters. No actual Reader or cancellation runtime was constructed/executed.

This matches ordinary FileView competing-page resets in `apply.rs:20–31,48–58,60–74`. `layout.rs:127–143` gives comparison/symbol/dossier/review-document pages priority; the corrected branch clears each before the pinned view becomes eligible. The existing `navigate.rs:425–469` opener still receives the exact revision/path and handles normal file data loading. That loader was inspected only and never run.

## Independent inert regression

Compiled only the isolated exact-function recording adapter outside the repository; no application/library build or application test binary was invoked. Final execution: **2 tests passed, 0 failed, 0 ignored**.

- Corrected exact branch: six initial-state schedules — review document alone, dossier alone, symbols alone, comparison alone, all four open and none open. In every case the recording opener observes all masks already closed; exact 40-character pinned revision, `nested/pinned file.rs`, File tab, logical goto90 and mark90 survive. Both extracted close bodies run their expected recording Reader cancellations and clear stale results.
- Discriminating old-source control: the exact `f04ecabc` branch still reproduces dossier, symbol and comparison masks while retaining the same pinned file/line. Its passing test confirms the old bad state only; it is not counted as corrected production behavior.

The current OpenCode branch and both close methods are byte-preserved extractions. The scaffold uses minimal inert state and a recording file opener, so it proves branch ordering, reset semantics and pinned argument/line preservation, not native rendering, asynchronous file loading or pop-out interaction.

## Preserved scope and evidence

Git blob comparisons confirm all four accepted bridge-check sources are unchanged from R4: `src/repo/operations.rs`, `src/repo/review_workflow/echo_push.rs`, `src/git/command/source.rs`, `src/push_preflight.rs`. Their earlier acceptance stands; these checks were not rerun or reopened. This correction has no other file delta.

Plain-text source snapshots, extraction hashes, exact diff, adapter, compile log and test log are under `/tmp/gitcraft-51-storyboard-delta-r5-evidence`; report/hash manifest is `/tmp/gitcraft-51-storyboard-delta-review-r5.manifest.json`. The adapter's generated executable stays external in /tmp and is not committed evidence. No tracked/source edit, app constructor/discovery, production process/Git/helper/native/window/HTTP/SSH fixture, actual Reader cancellation, application build/test, broad suite, CI, PR or subagent was used. Final combined and native/runtime acceptance remain separate and held. This report closes only MERGE-R4-1.
