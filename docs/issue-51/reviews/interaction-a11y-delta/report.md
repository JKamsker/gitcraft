# Interaction COV-R1-3 bounded correction review

**FAIL on exact `435648afec0431e19641282207508ae883cb44f6` for one remaining minimap naming defect.** The named painted-button, dead-focus-stop, diff-map numeric/action and image zoom corrections are source-backed and have meaningful supplied headless gates. This verdict is restricted to the prior COV-R1-3 inventory; it does not reopen the closed editor/panel/tearoff fixes or decide the separately active registry/menu inventory.

Tree `2235e4e042760ac0dd0cafbe06dcfa8228371afa`; immutable archive `/tmp/gitcraft-interaction-a11y-r1-4356-source`. Read the relevant original checkpoint report and complete manifest, the exact correction diff and current coverage receipt/logs. Source was read from Git archive/objects, not mutable writer files. Report/manifest and bounded evidence are under `/tmp/gitcraft-51-interaction-a11y-delta-review*` and `/tmp/gitcraft-interaction-a11y-r1-4356-evidence`.

## COV3-D1 — P2: populated minimap publishes its footer as the control's name

`src/repo/graph_view/minimap.rs:116` defines the useful accessible instruction beginning "Loaded history minimap. Drag to scroll…". The new empty-map helper uses it correctly at:131. However, the same method shadows label at:175–179 with the painted footer text. The new scalar adapter at:236–243 receives that later variable, so every populated map is exposed as a Slider named only "History ends" or "Older unloaded ↓". These describe loading boundaries rather than identifying the viewport-position control or its keyboard behavior. This regresses the prior descriptive name on the exact control whose role/state inventory was being corrected.

Independent source-bound headless proof compiles the **exact history_minimap method**, exact offset_at and exact a11y/value.rs against cached egui. A small inert struct supplies already-prepared bins/rows/graph width; prepare_minimap is a no-op. It does not instantiate production RepoTab, invoke a history reader, create a native window or access a repository. A minimal naming shim preserves the supplied labels. AccessKit tree output, exit0:

```text
has_more=false: Slider label="History ends", value=40, range=0..1900
has_more=true:  Slider label="Older unloaded ↓", value=40, range=0..1900
```

The numeric adapter works in this supplied state; it is the actual callsite's name argument that is wrong. The shared scalar helper test cannot detect this lexical shadowing, and the writer explicitly did not run an actual minimap callback gate.

Minimal correction: keep footer text and accessible control label under distinct bindings, then inspect the populated map's actual AccessKit Slider label for both has_more states. This can remain a pure/inert callback gate; no production RepoTab constructor, reader/Git fixture or native accessibility service is needed.

## Disposition of the original named inventory

| Original COV-R1-3 item | Scoped disposition at 435648af |
| --- | --- |
| Clickable dependency/package name, deps_view.rs:80–85/118–123 | **Corrected.** New a11y Button annotation at:118–120 gives package/action text and shared focus note. Supplied test calls the real row, Tabs to the Button, checks the accent ring and Enter → DiffGoToLine. It does not merely call the helper in isolation. |
| Clickable DVC tracked path | **Corrected.** dvc_view.rs:62–67 annotates the actual path response as Button with line/path/state. The real pair test checks role, Tab focus, ring and Enter action. No DVC execution is involved. |
| Raise existing child-window label | **Corrected.** popout/hint.rs:48–50 annotates the real label as Show the … window Button. The chip test checks Tab/ring/Enter → Focus(Panel::Diff) action, without creating/raising an OS window. |
| Table header's unexplained focus stop | **Corrected.** table_view.rs:190 uses Sense::CLICK rather than click(), preserving the pointer shield without FOCUSABLE. The actual header gate Tabs from Before table directly to After table. This is a meaningful focus-order check; it is not native pointer/window acceptance. |
| Diff text-drag hit shield | **Corrected.** rows.rs:149–153 uses CLICK|DRAG without FOCUSABLE; existing drag/selection path retains its pointer response. The actual Rows.show gate Tabs to the map instead of this shield. |
| Diff change map numeric state/actions | **Corrected within headless source contract.** rows.rs:222–233 publishes actual scroll offset, range, line step and vertical orientation. Returned assistive offset updates/stores actual ScrollArea state and requests repaint. The real Rows.show gate verifies SetValue=100 and retention across later frames. |
| Image canvas static Slider with no value | **Corrected for the declared zoom scalar.** image_compare/view.rs:173–207 publishes zoom percent/range/step/horizontal orientation, names the control as image comparison zoom and includes current pan position/instructions. SetValue flows through ordinary ViewState::zoom_at and the per-image stored state. The real compare_view test verifies 100→200% and stored zoom=2. Its history/difference lazies are precompleted (lazy.rs test-only ready seam), so that gate cannot start Git/history/difference work. It does not prove every zoom/pan/mode combination or native reader behavior. |
| History minimap numeric state/actions | **Source adapter implemented; accessible name remains FAIL COV3-D1.** minimap.rs:130–132 handles empty range; :236–245 publishes loaded offset/range/row step/vertical orientation and writes pending offset. graph_view/mod.rs:240–243 consumes that offset through normal scroll handling. Shared numeric actions are headlessly tested; actual production history/reader integration was compiled, not executed. The independent inert callback above verifies emitted state/name with prepared rows only. |

The new shared scalar helper (a11y/value.rs:9–56) publishes bounded numeric value/min/max/step and orientation; handles Increment, Decrement and finite NumericValue SetValue; applies clamp and disabled refusal; and registers the shared ring/naming helper. Supplied scalar tests inspect the AccessKit node and drive actual headless action-request events, including clamping and disabled state. Those are useful helper-level gates, but do not substitute for the actual callsite checks listed above.

The prior COV-R1-3 list explicitly was a targeted inventory, not an exhaustive source-wide widget census. After the naming correction, these named omissions can close within source/headless scope. The broader final source-inventory exit and native/platform screen-reader checks remain pending as already assigned; this report does not invent additional widgets or call all accessibility complete. COV-R1-1 registry and COV-R1-2 menu hints remain separately owned. Previously closed editor, panel viewport and tearoff findings receive no new disposition here. Four OS integrations/localization and native source exit are outside this correction.

## Evidence classification

Inspected writer receipt `/tmp/gitcraft-interaction-a11y-coverage-validation.json` and all seven named logs: seven passing pure/headless tests across five filters (1 scalar + 3 painted actions + 1 header + 1 image + 1 diff map), fmt exit0 and qualified all-target Clippy -D warnings -A dead_code exit0. This is qualified lint, not unqualified strict lint or a separate all-target check claim. Tests were not rerun as an application test binary by this reviewer. Their bodies reach the actual production widgets as described; they instantiate no App/RepoTab/discovery constructor. The image gate precompletes lazies, and the raise test records an Action rather than opening a child window. Minimap production integration was compiled only in supplied evidence.

Independent execution: one rustc +1.95.0 compiler invocation and one inert egui/AccessKit callback adapter run, both exit0. The method and helper are exact source; prepared metadata/theme/naming and history preparation dependencies are inert stand-ins. No application constructor, production RepoTab/reader, Git/helper/CLI, keyring, HTTP/network, process/cancellation/native-screen-reader/window/PTY fixture, full suite, CI, PR, source edit or subagent ran. Git object reads and compiler/document tooling are not application runtime fixtures.

Historical pre-hold actual Git/PTY checks described in the original checkpoint retain their original classification; none were rerun or reclassified as pure. The strict runtime hold is unchanged. This report requests only the concrete minimap-label correction and its bounded semantic gate.
