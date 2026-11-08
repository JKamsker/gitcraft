# Full repository tearoff R1 — FAIL

Immutable candidate: `1b04721f524d9e8fdbcc5337366789295d675487`; tree `f16faca9ab1c70ab8c1f76c6b0fc2b7e93651603`; parent `5914916f1246986ae1e34de0ef6d8c03edaecd5b`. Reviewed a Git archive at `/tmp/gitcraft-tearoff-r1-1b04721f`, never the writer's moving checkout. All application line references below belong to this candidate. Manifest: `/tmp/gitcraft-51-tearoff-review-r1-manifest.json`.

Scope: original #38 full live repository tab tearoff and the interaction plan's direct ownership, window, input, task, close and persistence requirements. The previous checkpoint's editor type mismatch and immediate panel focus-flush defect remain separate unresolved findings: `src/keymap/editor.rs`, `src/repo/popout/window.rs` and `src/a11y/focus.rs` are byte-unchanged from 5914. They are not counted as introduced tearoff defects. Other desktop features are excluded.

The candidate does move the existing Box<RepoTab>, retains channels/drafts/operations, uses deferred callbacks, and gives full repository children their own shell/registry/a11y end-frame path. It is substantially more than panel popouts. Six integration findings prevent acceptance. Findings below distinguish source-established behavior from native scenarios that remain unexecuted.

## TEAR-R1-1 — Hidden/minimized-root close bypasses the keep-children policy (P1, static)

The new root-close policy exists only in `App::window_ui` (`src/app/mod.rs:821`, implementation `src/app/windows.rs:465–474`). `DesktopApp::logic` at windows.rs:239–244 only calls `tick_model`; `tick_model` does not call the close policy.

The pinned eframe 0.36.2 implementation explicitly processes root close events in its nonpainting path: `native/epi_integration.rs:324–345` calls App::logic and then accepts Close unless logic emitted CancelClose. `native/wgpu_integration.rs:700–713` selects this path when the root is not shown. The same logic-only integration exists for Glow.

Consequently, an ordinary close request delivered while the root is minimized/occluded/hidden is accepted without the new children check, even if live detached sessions remain. This exits the application rather than retaining children. Explicit Quit should continue to exit; it must be distinguished by `quit_requested` in this same logic-safe lifecycle reducer.

Repro for a future permitted integration gate: detach a live repository; minimize/occlude the root so eframe uses logic-only; deliver an ordinary root Close; verify CancelClose plus hiding and that the child session remains. A pure logic-only close test can inspect viewport commands without opening native windows. No native close fixture was executed here.

## TEAR-R1-2 — Modal ownership is still global across independent windows (P2, static)

`Window::swap` (`src/app/windows.rs:69–84`) swaps tabs, active index, palette/manual, drag/strip/switcher and frame state. Clone, remote, teleport, askpass, scan and My Work dialog state remains on the shared App. Every full shell calls the same dialogs (`src/app/mod.rs:878–888`). Its shortcut suppression at mod.rs:595–604 and repository `keys_blocked` at mod.rs:716–724 consult those shared flags without a dialog-owner viewport.

Concrete behavior: start Clone from detached window A. Its one `clone_form` is then rendered by root and detached window B too; B's otherwise unrelated repository shortcuts/pane/staging actions are blocked by A's form. Form focus/cancel/edit state is shared across these views. `clone::CloneForm.focus` is a single boolean, so whichever viewport renders its first frame consumes the initial focus request. The app mutex prevents simultaneous mutation but does not give dialogs a window identity.

This fails the plan's independent window/modal/focus state and usable detached-window interaction requirement. Global services may remain single-owner; their dialog presentation must be bound to one viewport (or an explicitly designed application-modal policy, consistently enforced). Keep repository editors on the one live RepoTab and route each global dialog to its owner rather than drawing shared mutable forms in every shell.

Required pure/headless gate: open a dummy Clone form in A; render B before A; assert only A displays/owns it and B's unrelated command remains eligible; cancel in A and verify no other window state is changed. No actual clone/dialog/network execution is needed.

## TEAR-R1-3 — Async open/clone/connection completion is routed by repaint order (P2, static)

The shared `Cloning`/`Connecting` records have no initiating WindowId. Every root logic tick and every child `window_ui` can drain them (`src/app/mod.rs:758–776`; `src/app/windows.rs:199–208`). `poll_clone` opens into the current swapped `app.tabs` (`src/app/clone.rs:203–215`), and `poll_connecting` does the same (`src/app/remote_open.rs:212–220`). A newly cloned/connected repository therefore opens in whichever window happens to poll the result first, commonly the still-ticking root, rather than the detached window that initiated it.

There is also a concrete credential-assignment regression when a target is already in another window: `open_root`/`open_repo` now return after `focus_elsewhere`, but the completion still assigns credentials only through `self.tabs.get_mut(self.active)` and a path check (remote_open.rs:215–220; clone.rs:205–215). The target may be parked, so the credentials are dropped rather than reaching that repository. The path guard prevents attaching them to an unrelated active tab; this finding is loss of intended credentials, not leakage to another repository.

Preserve initiating window/request identity across asynchronous work, define fallback when that window closes, and locate an already-open target across all window memberships when applying completion data. The existing global launch-link path does search all windows and queues `Then` into the right RepoTab; that useful path does not fix new clone/connection ownership.

Required recording/headless gate: inject successful dummy completion into the pure result-routing layer, poll root first then child first in separate cases, and assert identical owner/target/credential assignment. No actual Git/auth/network fixture is needed.

## TEAR-R1-4 — Cross-window drops compare incompatible DPI coordinate spaces (P2, pure production-function evidence)

`window_strip` (`src/app/windows.rs:477–481`) records local strip points plus that viewport's `inner_rect.min`. `drop_window_target` (lines 493–503) constructs the source desktop point the same way, then compares it with every recorded target rectangle. It never converts either to one shared physical desktop coordinate system or records per-window pixels-per-point.

Pinned egui-winit 0.36.2 `src/lib.rs:1310–1319` constructs inner_rect by dividing physical desktop origin/size by **that window's** pixels_per_point; cursor positions likewise use the source window's scale at lines 844–856. Thus source 1x and target 2x rectangles are not comparable by direct addition/containment.

Independent pure probe includes the exact byte-copied production `drop_target` function: a physical target strip at (1600,100) sized (800,40), scale 2x, is recorded as (800,50) sized (400,20). A source 1x drop at physical (1700,120) validly hits the target but returns None with the current mixed-unit inputs; equivalent physical-coordinate inputs return the target ID. Current application behavior chooses a new window for None, so the intended existing-window transfer fails. This is an arithmetic/source result, not a native multi-monitor observation.

Use a supported common desktop coordinate system for both source pointer and targets, with explicit Wayland/compositor fallback where absolute positions are unavailable. Preserve position units when creating the new viewport. Existing synthetic tests use one scale and cannot establish this behavior. Native mixed-DPI checks remain held.

## TEAR-R1-5 — Reentrant fallback preserves full windows but prunes existing panel popouts (P2, source + headless lifecycle evidence)

The new coordinator is one Arc<Mutex<App>> held through `window_ui`, including synchronous native pickers (`src/app/windows.rs:187–227,247–260`; e.g. `src/app/home.rs:19–25`). On contention, a reentrant root/child callback skips normal UI and only calls `children` using cached **full repository window** specs (windows.rs:140–184,191–194,249–252). It does not redeclare the existing immediate Diff/Terminal/PR/CI panel viewports ordinarily drawn by mod.rs:873–875.

egui 0.36.2 `context.rs:2757–2786` removes a viewport when its parent pass completes without redeclaring it. There is no exception for an omitted panel when a full-shell sibling is retained. A small headless egui lifecycle probe confirmed that the cached full child survives while an omitted panel child ID is removed. The probe uses a deferred placeholder for the omitted ID; the cited pruning rule applies independently of class. It does not claim to have run a native picker or immediate renderer.

Source-derived scenario: an existing panel popout belongs to a viewport whose normal UI pass is skipped during native-event-loop reentrancy. The fallback retains registered full shells but that panel is pruned/closed and later recreated, losing native window continuity and interrupting panel use. The writer's reentrancy test asserts only full repository child registration and does not cover preserving existing panel popouts, an explicit tearoff requirement.

Correction must preserve every live viewport class or avoid committing an empty parent UI pass during contention. Add a headless viewport-tree gate containing full shells plus existing panel IDs, then keep native nested-picker/panel continuity as a later held integration gate. This is separate from the prior focus-flush finding.

## TEAR-R1-6 — Hidden model tick leaves dropped-patch continuations UI-dependent (P2 requirement gap, static)

The new `tick_model` drains repo messages and services (mod.rs:755–795), but the global PatchQueue is advanced only from `dropped_files`, called only by `window_ui` (mod.rs:892; `src/app/dropped.rs:201–216`). When a patch is queued behind a busy operation, or a first dropped patch finishes and another is waiting, hiding root and minimizing all detached shells leaves no painting call to advance that queue. Root logic still ticks and receives the operation result, but the next patch waits until a window paints again.

This does not claim every worker completion is lost: `r.poll` and completion-toasts do drain hidden. It is a concrete gap in the proposed nonpainting model/continuation owner. Extract queue advancement from input/drop-hint rendering and call it once through model tick, retaining destination path/session and normal operation safety. Also inventory other UI-only queued continuation hooks rather than generalizing the two synthetic finished-message assertions to all background flows.

Required pure/recording gate: a fake ready/busy/result sequence with no painting must consume one outcome, advance once, and route the next patch to the same live session after a window transfer. The authored stale-patch real-Git test is NOT RUN and provides no execution acceptance here.

## Coverage and positive source findings

- **Single live session ownership:** `move_to_window` removes the actual Tab/Box, validates a stale existing-window target before removal, preserves other selected tabs, appends into the target and cancels keyboard ownership by incrementing the mutation epoch (`windows.rs:376–432`; `repo/keyboard.rs:94–98`). No duplicate reopen path is used for a normal move. The compile-time Send assertion and deferred Send+Sync callback are present; no unsafe Send implementation was introduced by this diff.
- **Shell isolation:** tabs/active index, palette/manual, tab drag/scroll/switcher and frame decoration state are swapped. Child `window_ui` runs registry owner/scopes, normal repository UI, which-key and a11y flush. This supports real child callbacks, unlike merely drawing a second screenshot. Modal/async gaps above remain.
- **Close/return:** a visible child close moves sessions to root without closing their RepoTab, reassigns full-shell descendants to root and shows root (`windows.rs:437–462`). Per-tab Move to Main/other existing window and Move to New Window are reachable context-menu equivalents. Quit gitcraft sends root Close with quit_requested; draft shutdown iterates current and parked tabs. Hidden-root ordinary close remains TEAR-R1-1.
- **Global services:** draft collection, agent request routing, taskbar progress, completion toasts, launch-link known repos/Then actions, inbox and parked-worktree lookup now include detached memberships. These are source-established improvements; no actual service/repository tests were run by this reviewer.
- **Persistence:** save writes root and detached path lists separately; restore filters duplicate/missing repositories and preserves active selection through filtering. Position validation only rejects nonfinite values (`windows.rs:300`) and size validation only rejects nonfinite/small values (304–307). Monitor-safe position/size clamping is absent: a finite position on a disconnected monitor is reused verbatim by builder lines 153–161. This leaves the plan's sensible monitor-safe restoration requirement open, without claiming any OS necessarily honors an offscreen request unchanged.
- **Drag coverage limit:** tests drive a release outside the tab strip but still inside a synthetic source window, or provide source-local coordinates directly. No source-window-exit/capture protocol was added. A pure egui probe shows PointerGone plus another pass leaves primary_down true but pointer_interact_pos None, which bypasses `tabs_row`'s dragged branch and `drop_window_target`. Pinned egui-winit clears its pointer position on CursorLeft and only emits mouse-button events when a position exists (lib.rs:359–361,799–817). Native implicit capture differs by platform; do not infer that all platforms fail, but do not claim dragging/releasing outside the native source window is validated. Required later platform gates include this exact sequence, mixed DPI, Wayland positioning and window focus after transfer.

## Executed versus inspected evidence

Inspected: exact candidate and parent diff; full interaction plan and original #38 requirement; writer `/tmp/gitcraft-interaction-windows-validation.json` and its two logs. The writer manifest declares nine passing headless window cases and all-target Clippy with `-D warnings -A dead_code`. The logs match that declaration; these are historical writer gates, not reruns or independent binary attestations. The lint qualification is explicit. The harness supplies a recorded Git version and deferred synthetic repositories; constructor/service lazy shell-path behavior was not independently executed/audited as a process-free guarantee. The stale-patch real-Git test is authored but unexecuted as stated in the handoff.

Executed by this reviewer: one rustc +1.95.0 compile of `/tmp/gitcraft-tearoff-r1-evidence/probe.rs`, then one successful pure/headless executable run with three bounded assertions (production drop_target arithmetic, egui viewport pruning, egui PointerGone state). It uses cached egui rlib, no App/RepoTab constructor, no real repository or model services. The one compile emitted only an unused_mut warning in the harness. No actual Git, shell/PTY, native dialog/window/drag/screen-reader, HTTP, process cancellation/signal, full suite, CI, PR or tracked-source mutation was performed.

Observed probe output:

```text
PURE production drop_target: equivalent physical hit misses when target rect uses its 2x point scale.
HEADLESS egui lifecycle: redeclaring cached full shell alone prunes omitted panel child ID.
HEADLESS input limitation: after PointerGone plus a pass, primary remains down while source has no interact position.
```

## Disposition

FAIL for full tearoff acceptance at this SHA. Address six findings and the stated restoration/drag coverage gaps; add source-bound pure/headless/recording gates around ownership/routing/lifecycle before later native testing. Preserve prior checkpoint R1 findings as separately open. Native cross-platform usability and shutdown behavior remain unvalidated; this review neither clears the runtime hold nor authorizes CI/full-suite execution.
