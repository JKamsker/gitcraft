# Captured source-window release

The pinned local winit patch emits the captured primary-button release position before its button-up event. macOS uses the delivered NSEvent's view coordinates even after all buttons are up; Windows uses signed WM_LBUTTONUP client coordinates; X11 uses XI_ButtonRelease event coordinates; Wayland uses the captured surface event position. No host pointer query, helper process, timer or global input hook is introduced.

This ordering matters because egui-winit clears its pointer position on CursorLeft and otherwise drops the subsequent button-up. The tab-strip tests now send PointerGone, three idle frames and the recorded position/release pair outside the source window. They assert that the same repository Box remains owned during exit and moves exactly once, either into a new window or an existing peer strip. Repository tabs are deferred; Git and native windows are not started.

Bounded validation commands (macOS host, Cargo +1.95.0, CARGO_BUILD_JOBS=2):

- `cargo +1.95.0 test --offline --lib app::windows::tests::captured_release_after_source_exit -- --nocapture` — two headless tests passed.
- `cargo +1.95.0 clippy --offline --all-targets -- -D warnings -A dead_code` — passed, including native macOS source compilation.

Immutable execution logs and source hashes remain outside the repository in `/tmp/gitcraft-interaction-source-exit-validation.json`. Native captured mouse delivery and desktop placement have not been exercised under the runtime hold. Windows/X11/Wayland source adapters have not been compiled or executed here. Wayland compositors that withhold global window origins cannot support desktop-coordinate inter-window targeting through the existing window adapter; native acceptance remains WIP.
