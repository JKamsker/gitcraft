# Interaction editor and panel-focus corrections R2 — scoped PASS

Immutable candidate `586d0894fe72e3a621672bbe9e0371beaaa9d8a0`, tree `ef7c151fe25ce2532be6386fdd11cbc091568375`; parent/editor correction `08b5bdedb9aacd79298e1f3da91b9694b5231e42`. Reviewed archive `/private/tmp/gitcraft-interaction-r2-586d0894`, with 15 relevant files independently byte-checked against exact Git objects. Line references below belong to this archive. Companion hash manifest: `/tmp/gitcraft-51-interaction-corrections-review-r2-manifest.json`.

**PASS for INT-R1-1 and INT-R1-2 only.** Both prior source defects are corrected, with independent execution of nine narrow pure/headless assertion groups. No new blocker was established within these two corrections. The broader action-registry/menu/AccessKit inventory and all six tearoff R1 findings remain open. This does not accept the interaction batch, native platforms, process runtime or the final combined candidate.

## INT-R1-1 — PASS: editor state, keyboard capture, Save/Cancel and settings wiring

The original `Editing` versus `Option<Editing>` storage mismatch is gone. `src/keymap/editor.rs:14–17,29,172–181` consistently reads/stores/removes **Editing**. `Option<Editing>` is only the local optional value, as required by egui's `get_temp` return type. Capture ownership is keyed by viewport and expires when the page is no longer drawn; a displayed editor refreshes its pass at the end of every page invocation.

The capture control is now an actual focusable button. Lines 35–38 record only while it owns focus or awaits initial focus. Lines 58–67 leave ordinary Tab and Shift+Tab navigation events available to egui; lines 86–89 request capture focus initially or on an explicit click, rather than forcibly taking it every frame. Save/Clear/Cancel therefore receive their keyboard activation without recording Enter or Space into the shortcut. Escape is consumed and cancels from capture or the action controls. Modified Tab can still be recorded, and explicit Add Tab/Add Shift+Tab controls support binding the plain navigation keys without trapping keyboard navigation.

The transaction is still local until Save: a fresh clone of current overrides receives the captured command, `config::validate` checks it, and only the enabled Save action assigns it back (`editor.rs:70–72,95–106`). Cancel/Escape discard the pending state. Existing unrelated overrides are retained in the candidate. Input recording excludes repeat events and stops at four strokes; the explicit Add controls obey the same bound. Clear permits the intentional empty/unbound binding. This review does not reinterpret an empty binding as a missing/default binding.

Independent harness `/private/tmp/gitcraft-interaction-r2-evidence/editor-probe.rs` includes byte-identical copies of the exact production keymap/editor/config/resolver modules. It drives raw keyboard and pointer events through egui, rather than manually inserting an `Editing` object or a predetermined saved override. Seven assertion groups pass:

1. Keyboard Tab → first Change → Enter → F8, several idle frames, Tab → Save → Enter. The resulting NewTab override is exactly one F8 stroke; navigation/activation keys do not contaminate it, and capture closes.
2. Tab through the action controls to Cancel, activate with Space: saved bindings remain unchanged and capture closes.
3. Escape while Save owns focus cancels without mutation.
4. Captured Command-W conflicts with CloseTab: clicking disabled Save cannot commit. In a separate fresh conflict fixture, Tab reaches Clear and Enter clears the candidate; explicit Save stores an intentional empty binding.
5. A fifth captured stroke is refused; the saved chord contains the first four only.
6. Add Shift+Tab stores that literal navigation stroke.
7. Two page-absent frames release shortcut ownership.

Persistence/dispatch evidence is deliberately separated. The independent probe serializes the **actual editor output**, deserializes it using production `Overrides`/`Stroke` serde implementations, puts it in a minimal in-memory Settings stand-in, and verifies production `keymap::label` is F8 and production `pressed(Window)` emits NewTab for F8 (`keymap/resolver.rs:186–211`). It does not launch App or write eframe storage.

The real application path is present and source-traced: Settings derives Serialize/Deserialize/PartialEq and owns `key_bindings` (`settings/mod.rs:44–48`); Shortcuts passes that field to this editor (1076–1078); `Settings::page` compares the complete value before/after (`settings/page.rs:57–58,140–141`); `App::settings_page` calls `self.settings.set(&ctx)` on change (`app/panels.rs:46–74`); set publishes it to context (`settings/mod.rs:308–313`). App save includes `self.settings.clone()` in Saved and calls `eframe::set_value` (`app/mod.rs:918–944`); startup restores `saved.settings` and calls set (249–252). The window coordinator suppresses registry dispatch while capture is live (`app/mod.rs:606–633`), otherwise consumes the production resolver output and maps NewTab to `self.new_tab()` (681). Actual persisted disk restoration and App side effects were not executed; their source wiring and the exact editor-to-resolver boundary support this scoped correction disposition.

## INT-R1-2 — PASS: shared native-class callback flushes child focus; embedded waits

`src/repo/popout/window.rs:37–58` now flushes `a11y::end_frame(ui.ctx())` after the native-class CentralPanel/body. The embedded branch runs its body and returns before that call (38–42), so its records remain with the parent. Minimized native-class callbacks return before drawing a body, as before. The flush uses the callback's viewport context, matching the viewport-scoped keys in `a11y/focus.rs:17–28,35–44,70–74`. It paints the ring and records focus after the child's widgets, before the renderer collects child output.

Reachability is shared production wiring, not a test-only helper: `repo/popout/views.rs:33–41` routes each selected panel through `window::show`; the correction does not create an alternate callback for tests. Existing Diff/Terminal/PR/CI panels therefore use the corrected callback. Their separate hardcoded shortcut/registry omissions remain outside this correction and open under the prior inventory.

Independent `/private/tmp/gitcraft-interaction-r2-evidence/panel-probe.rs` directly includes the exact window module and exact a11y/focus/labels/motion modules. It supplies an egui immediate renderer implemented entirely in memory; there are **no native windows or OS renderer calls**. Two assertion groups pass:

- Two distinct synthetic native-class children each paint their own ring in the child output **before ROOT flush**, independently remember their row into the following child frame, and stop painting after focus surrender. ROOT's separate ring survives both child flushes. This proves actual callback execution, not merely unequal IDs or a direct test call to the missing flush.
- Embedded rendering produces no ring shape before the parent flush (graphics inspected immediately after `window::show`), then produces its ring after parent `a11y::end_frame`. This explicitly checks the callback does not flush the shared parent early.

Theme tokens and icon decoding are minimal pure stand-ins. `GITCRAFT_REDUCE_MOTION=1` is asserted before the probe and selects the exact motion module's environment override, so its OS preference-discovery branch never runs. No App constructor or discovery services are invoked.

## Executed evidence, historical gates and exclusions

Independent final logs: `/private/tmp/gitcraft-interaction-r2-evidence/editor-probe.log` (seven PASS groups), `/private/tmp/gitcraft-interaction-r2-evidence/panel-probe.log` (two PASS groups). Cached egui/serde/serde_json rlibs, exact source hashes, harnesses and final binary hashes are bound in the manifest. These are focused module probes, not an independently rebuilt full application artifact.

Execution inventory: five compiler invocations, including one harness-only compile failure because `ViewportClass` has no Debug implementation; three headless executable runs, including one initial editor harness failure. That initial harness clicked disabled Save and then incorrectly assumed focus was still on capture before Tab-to-Clear. The passing probe splits disabled-click refusal from a fresh keyboard-Clear fixture. Initial harness source/log are preserved; neither failure is counted as an application defect or a passing execution.

Historical writer evidence was inspected, not rerun: `/tmp/gitcraft-interaction-editor-validation.json` binds 08b5 and its log shows 23 passing keymap cases; `/tmp/gitcraft-interaction-panel-focus-validation.json` binds 586 and shows two passing callback cases. Their qualified Clippy gates are explicitly `-D warnings -A dead_code`. The exact 08b5 editor bytes are retained in 586; the two correction commits change only `src/keymap/editor.rs` and `src/repo/popout/window.rs`. Writer results remain separately attributed rather than added to this review's execution count.

The full prior checkpoint report/manifest and full tearoff R1 report were read. COV-R1-1/2/3 (registry coverage, authoritative menu hints, complete painted-control/focus/AccessKit role/state inventory) and TEAR-R1-1 through TEAR-R1-6 retain their prior open dispositions; this scoped task did not re-review or clear them. The archive includes broader tearoff changes between checkpoint5914 and this candidate, so no broad unchanged-source or whole-batch acceptance is claimed.

No actual application Git/helper/credential/HTTP/PTY operation, process-control/cancellation/signal/timeout fixture, OS/native window/screen-reader fixture, App/RepoTab constructor, full suite, CI, PR or tracked-source edit was executed. Git object reads and rustc are review tooling. Actual platform accessibility behavior, settings-storage I/O, complete App dispatch, combined integration and full-suite gates remain unvalidated. Global runtime containment hold remains in force.
