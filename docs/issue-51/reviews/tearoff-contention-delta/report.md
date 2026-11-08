# TEAR-R1-1 contention delta — scoped PASS

Exact candidate `e12121f9dbb52a71976b0a12dc5473041f31fa76`, tree `d98ad3dedd6f8c70bc789c69d2bbc1b8fc609f1d`, parent `cd6562b008462a371439fbd6c042c7c434d4c63f`. Compared the complete `f48a28ad41f7e2d29119311fa0c77affdbdef124..e12121f9` delta using immutable Git objects/archive `/private/tmp/gitcraft-tearoff-contention-e12121f9`. Only `src/app/windows.rs` changes. Manifest: `/tmp/gitcraft-51-tearoff-contention-delta-review-manifest.json`.

**PASS: close the residual TEAR-R1-1 source finding.** Both previously reproduced ROOT coordinator-contention branches now cancel and retain Close, then replay through the existing owner policy after release. No remaining defect was established in this narrow delta. The other five TEAR dispositions and prior editor/panel closures are preserved, not re-reviewed or reopened. Original native source-window-exit PointerGone/capture remains **OPEN**; this is not whole-feature/native/final-combined acceptance.

`DesktopApp::logic` and `ui` now delegate to `logic_tick`/`ui_tick` (`windows.rs:355–361`). On failed `try_lock`, both call the same `defer_close` (324,339). It recognizes ROOT Close, emits CancelClose immediately and sets `deferred_close` (329–333), so eframe can no longer accept the reproduced request merely because App is borrowed.

When the coordinator becomes available, `logic_tick` takes the pending bit once and calls `root_close_request(ctx,true)` before ordinary model work (315–322). The extracted existing reducer retains its keep-children/dialog/explicit-Quit conditions (640–654). If the reducer handles the request it cancels/hides as appropriate; otherwise replay emits Close, preserving explicit Quit and ordinary last-window closing. The pending flag is consumed once. Normal visible eframe updates invoke App::logic before App::ui, while hidden updates invoke logic alone; pinned eframe `native/epi_integration.rs:291–300,324–345` therefore supplies the replay entry on either kind of subsequent root update.

Independent proof: `/private/tmp/gitcraft-tearoff-contention-evidence/probe.rs` embeds seven byte-identical extracted production methods: both eframe wrappers, logic_tick, ui_tick, defer_close, close_root_with_children and root_close_request. Minimal recorded state/Frame and no-op UI/children adapters avoid constructing actual App or RepoTab. For **both** logic-only and ROOT UI entries, it holds the owned mutex during a ROOT Close and asserts CancelClose plus pending=true, releases it, then invokes the exact logic wrapper with no new Close event. Six combinations pass: each entry with ordinary live-child close, explicit Quit with children, and ordinary last-window close. Replay keeps the child in the ordinary case; the other cases emit Close without CancelClose. Pending clears and the next empty tick emits no duplicate Close. The locked UI branch bypasses the no-op normal UI/children stand-ins; no native callback or process is launched.

One independent rustc invocation and one pure/headless execution passed, producing:

```text
PASS exact logic entry: held-owner Close cancelled/remembered; replay keeps children, permits explicit Quit and last-window close; no duplicate replay.
PASS exact ROOT UI entry: held-owner Close cancelled/remembered; replay keeps children, permits explicit Quit and last-window close; no duplicate replay.
```

Inspected separately, not rerun: `/tmp/gitcraft-interaction-both-contention-validation.json` binds exact e121 and its historical log reports two passing actual-Desktop-adapter headless cases; qualified all-target Clippy is `-D warnings -A dead_code`. Those test results are not this review's executions. The prior full R2 report/manifest were read; application files other than windows.rs are byte-unchanged from f48, and its delta is confined to deferred-close state/entry wiring/reducer extraction and the two focused tests.

No actual App/native/Git/helper/PTY/network/process-control/cancel/runtime fixture, full suite, CI, PR, tracked-source edit or new agent was used. Source reads, compiler and the bounded recorded adapter are the only new review work. Native close/picker behavior remains unexecuted, the original PointerGone/source-exit requirement remains open, and global runtime/final combined/full-suite acceptance is not cleared.
