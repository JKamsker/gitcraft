# Interaction checkpoint R1 — FAIL

Reviewed immutable commit `5914916f1246986ae1e34de0ef6d8c03edaecd5b`, tree `3866f0f229b5e8bb847431aac28e7bf968d041bb`, parent `1c920c0937434bdc9fc13809e2b67e3b5daf70d6`. Source was extracted with `git archive` to `/tmp/gitcraft-interaction-r1-5914916f`; no moving writer source or tearoff WIP was reviewed. Paths/line numbers below refer to that exact archive. Companion manifest: `/tmp/gitcraft-51-interaction-checkpoint-review-r1-manifest.json`.

This is a checkpoint failure, not acceptance of the whole interaction batch. Two source correctness blockers were found, one independently reproduced in a pure/headless harness. The remaining registry and accessibility inventory gaps are material original-requirement omissions, even where the writer already acknowledges them. Excluded: unpreserved repository tearoff, macOS URLs/Dock, tray and Fluent; other issue #38/#39 ideas owned by separate batches.

## Source correctness blockers

### INT-R1-1 — Shortcut editor loses capture before a binding can be saved (P2, headless reproduced)

`src/keymap/editor.rs:27` reads `let mut editing: Option<Editing> = ...get_temp(edit_id)`, which requests a stored **Editing**. The Change button sets `Some(Editing)` at lines 108–114, and line 125 stores **Option<Editing>**. `capturing()` at lines 11–15 explicitly reads `Option<Editing>`, so it initially returns true while `page()` cannot restore that same state on the next frame. The next page frame overwrites the stored option with None.

Repro: Settings → Keyboard shortcuts → Change on New tab → press F8. Change initially starts capture; on the following frame the capture panel/Save binding is absent, capture is false, and overrides remain unchanged. This blocks creating or changing single-key bindings and chords through the delivered editor. Reset/Unbind use separate mutations and are not evidence that Change works.

Independent evidence: `/tmp/gitcraft-interaction-r1-evidence/keymap-probe.rs` runs the exact production keymap modules with cached egui/serde dependencies, driving actual Change-button pointer events over multiple egui frames and then an F8 event. Only Settings/theme/manual dependencies are minimal in-memory stand-ins; the editor/config/resolver modules are byte-identical copies. The final assertions passed and confirmed the failure. No Git, PTY, window or network fixture was run.

Correction: use one consistent stored type on read/write, then verify Change → capture → Save → persisted override → actual dispatch/label update across frames. Also address keyboard operation of capture: the surviving capture path currently requests a synthetic focus ID every frame and consumes every Key/Text event before drawing Save/Clear/Cancel (lines 32, 35–54, 70–84). Tab/Enter are captured rather than offered to those controls. That secondary issue is established statically; the current earlier state-loss bug prevents exercising it end-to-end. A keyboard-only edit/save/cancel gate is required rather than testing only programmatically supplied overrides.

### INT-R1-2 — Existing native panel viewports never flush custom focus rings (P2, static)

`src/repo/popout/window.rs:37–55` invokes the child viewport body and returns without `a11y::end_frame`. `src/repo/popout/views.rs:38–40` only calls `popped_view`. The only application flush is the root viewport's `src/app/mod.rs:859`, after invoking all popouts at lines 827–830.

The new focus system deliberately stores ring and remembered keyboard records under `ctx.viewport_id()` (`src/a11y/focus.rs:17–28, 35–44, 58–65, 70–74`). A root flush cannot consume a native child's ring or remember that child's focus for `keyboard_on`. Thus already-shipped popped Diff/Terminal/PR/CI custom controls cannot receive the promised painted ring from this wiring, and child rows' keyboard-dependent affordance history is not recorded. Embedded fallback shares the root viewport and is not affected by this specific omission.

Source-derived repro for the later native gate: pop a diff out, Tab to a painted focusable control, compare its accent ring/keyboard affordance with the docked version. No native window test was executed. Correction: flush once inside each native viewport after its widgets, and add a synthetic multi-viewport/headless gate proving both child paint and remembered focus are consumed by the correct viewport. An ID-inequality test alone does not verify the real child wiring.

Existing popouts also remain an unfinished registry context: `src/repo/popout/views.rs:153–198` dispatches hardcoded reload/Undo/Redo/change-step keys, does not set the child's active registry scopes, and does not paint a child which-key overlay. New tearoff work is excluded, but these existing viewport paths must be inventoried when claiming unified registry/pane behavior.

## Original-requirement coverage

| Preserved idea | Checkpoint assessment | Evidence and remaining work |
| --- | --- | --- |
| One action registry, editable bindings, chords and which-key (#38) | **Partial; editor blocked** | Registry/config/resolver and per-viewport chord state exist; settings persist overrides; window/repository/file/hunk commands consume them. INT-R1-1 blocks UI editing. The registry still excludes many existing handlers/actions, so manual/palette/tooltips/menus/handlers are not driven by one source. |
| Keyboard staging and pane focus (#38) | **Substantial source wiring; bounded evidence, no independent mutation acceptance** | `src/repo/keyboard.rs` routes j/k, file Space/Enter and hunk S/U/D through normal actions, checks modal/popup/text ownership, and carries work epoch/read/path/view request guards. `src/repo/apply.rs:566–650` rechecks prepared hunk context; `src/repo/load.rs:61,172` invalidates work epochs when reload starts. The writer's owned-Git/PTY history was inspected only. Existing child-viewport registry context remains open as above. Pane defaults are Cmd/Ctrl+Alt+1/2/3 to avoid numbered-tab conflicts; this differs from the issue's literal Ctrl+1/2/3 but is visible/editable in the intended registry. |
| Context-menu icons, actual shortcut labels, inline reasons and keyboard access (#38 + plan) | **Partial** | `src/menu_item.rs` supplies icons, registry-derived keys, disabled reason text and Shift+F10 opening; graph/sidebar/tab/diff menus use it. Actual shortcut coverage is narrow, and actions outside the registry cannot display authoritative editable bindings. See examples below. |
| Visible focus on hand-painted controls (#39) | **Partial** | Shared clipped accent-ring helper, selected list behavior and extensive callsite conversion exist. INT-R1-2 and the concrete missed controls below prevent a source-wide claim. Native inspection is unperformed. |
| AccessKit names/roles/states (#39 + plan) | **Partial** | Shared naming, selection, disclosure and pane helpers exist. Graph, file/branch/palette/home controls and the previously identified inbox/workspace/config/recovery/suggestion controls have meaningful wiring. Remaining custom controls and slider state below need inventory and source-bound semantic gates. No screen-reader/native acceptance. |

### COV-R1-1 — Registry still omits existing action families

`src/keymap.rs`'s Command/BINDINGS and `src/keymap/config.rs::commands()` define only the delivered subset; the editor iterates only that subset. `src/repo/layout.rs:251–258` still hardcodes Undo/Redo/undo tree/search, and `change_step` at lines 448 onward still handles F7/Shift+F7/Alt+arrows directly. `src/manual.rs:397–521` combines static SHORTCUTS with registry rows: graph arrows/search, rebase pick/reword/edit/squash/fixup/drop/reordering, back/forward, change navigation, find and settings keys remain static. `src/repo/palette/mod.rs` retains literal Undo/Redo/search shortcut strings; these actions have no corresponding editable Command. Existing panel popouts have another hardcoded path above.

This is an acknowledged scope gap, not a claim that every direct key handler is a new regression. It nevertheless leaves the original “one registry drives the manual, palette, tooltips, menus and handlers” requirement incomplete. Inventory action identity, scopes, eligibility, bindings and displayed keys together, rather than closing this idea based on the new registry module alone.

### COV-R1-2 — Context menus still omit real shortcut hints

Production `.keys`/`.keys_when` use is limited to Close/Reopen tab, New branch for HEAD, Stage all and Stash. Concrete remaining example: `src/repo/diff/select.rs:153–157` implements native Copy events, while its Copy menu item at lines 176–183 has an icon/reason but no shortcut label. Copy is not an editable registry action. History and numerous selection/edit actions likewise are not registry entries. Inline disabled reasons and icons are substantial improvements; they do not establish actual-shortcut completeness.

### COV-R1-3 — Remaining painted control/focus/semantic inventory

Concrete reachable examples lacking the shared focus annotation/action semantics:

- `src/repo/diff/rich/deps_view.rs:80–85,118–123`: clickable package label jumps to the changed lockfile line; no shared a11y/focus helper.
- `src/repo/diff/rich/dvc_view.rs:51–67`: clickable tracked path jumps to its text line; no shared helper.
- `src/repo/popout/hint.rs:42–49`: clickable label raises an existing child window; no shared helper (the adjacent Bring back button is separately annotated).
- `src/repo/diff/rich/table_view.rs:190`: discarded `Sense::click()` header interaction is focusable but has no action/name/ring. This hit shield should not create an unexplained Tab stop.
- `src/repo/diff/rows.rs:154–158`: the text-drag interaction is another focusable unannotated surface, distinct from its annotated line controls. Audit whether it should be a separate focus stop and supply semantics or make it nonfocusable.

Clickable egui Labels can retain their built-in text; this review does **not** claim all of these are wholly unnamed. They lack the promised shared accent ring and explicit action semantics appropriate to their behavior. These source omissions are sufficient to leave the source-wide claim open; this is a targeted inventory, not an exhaustive widget census.

State coverage is also incomplete on newly labelled custom controls. `src/repo/graph_view/minimap.rs:116–120` and `src/repo/diff/rows.rs:148–152` expose Slider with only a static instruction label through `a11y()`, without a current numeric viewport position/range/orientation or AccessKit increment/decrement wiring. `src/repo/diff/image_compare/view.rs:80–85` exposes a two-dimensional pan/zoom canvas as Slider without its zoom/pan value. Keyboard handling exists, but keyboard events and static names alone are not complete role/state/action semantics. Define suitable roles and values and assert the actual AccessKit node properties, then retain native reader checks as a separate gate.

## Evidence inspected versus executed

Inputs inspected: the full interaction plan; original #38/#39 bodies from `/tmp/gitcraft-issue51-issues.json`; archived baseline core/content/platform audits; exact source; the four writer validation manifests and their named logs. The 5914 manifest declares 65 passing tests (menu 4, keymap 16, keyboard 9, keys 8, palette 8, a11y 20). Those are **historical writer results, not this review's executions** and not independent source-to-built-binary attestations.

They are not a pure/recording-only inventory. All nine `repo::keyboard::` cases use the owned Git fixture, all eight `repo::keys::` cases use owned Git, and `a11y::tests::repo_view::the_graph_is_reached_with_tab_and_read_aloud` constructs a Git repository. Thus at least **18/65 test cases involve actual owned Git execution**; `repo::keyboard::tests::text_terminal_palette_modal_and_context_menu_own_their_keys` also starts an actual shell/PTY. Earlier keyboard manifests include that same class. These historical executions predate the current hold as described in the handoff; inspecting them does not clear the hold. No such test was rerun.

Executed here: a narrow Rust/egui pure/headless harness only. Three compiler invocations: initial direct-path module setup failed child-module lookup; byte-exact copied-module setup compiled; revised assertion harness compiled. Two harness executions: the first failed its expected capture-panel assertion (exposing INT-R1-1); the revised reproducer passed and confirmed the failure. No application subprocess/Git fixture, PTY, signals, cancellation, network, native window/audio/screen reader, full suite, CI or PR was exercised. Shell/Git-object reads and compiler execution are review tooling, not application process-control fixtures.

Final reproducer output:

```text
HEADLESS: clicking first Change sets capturing=true.
HEADLESS: next F8 frame has no Save binding/capture panel; capturing=false and overrides remain empty.
```

## Required next gates and limits

First fix INT-R1-1 and INT-R1-2, then run multi-frame pure/headless editor tests (including keyboard-only Save/Cancel, chord, conflicts and persistence), synthetic child-viewport ring/focus tests, and actual context menu label/disabled-reason checks tied to the selected action and configured keys. Complete the action/widget inventory and inspect AccessKit roles, names, selection/expanded/value/action properties in headless output. Existing programmatic override/helper tests do not cover the broken settings flow or absent child callback flush.

Repository mutation, terminal ownership, native per-platform focus and screen-reader behavior remain outside this review's allowed execution gates. No runtime/native acceptance or full-batch acceptance follows from this checkpoint, and no full-suite/CI authorization is given.
