Vendored from JKamsker/winit 12fdcb03ef036f0fcb7fa26d19dd7395b1feb141 (winit 0.30.13, Apache-2.0).

The existing Windows per-monitor DPI patches and ApplicationDelegate remain intact. The local macOS patch adds applicationDockMenu: to that existing delegate and a main-thread setter retaining the supplied NSMenu. It does not replace or modify the runtime class of the delegate. The setter clears on null and returns a retained menu clone to AppKit.

PROVENANCE.json records every copied source file hash before the local patch. Cargo.toml also removes the explicit example target because examples/media are omitted. No binary evidence, builds, repository metadata or archives are vendored. Remove this dependency override after a reviewed upstream/fork hook replaces it.

The second local patch restores the captured primary-button release position before MouseInput on macOS, Windows, X11 and Wayland. This lets egui-winit finish a source-window-exit drag after CursorLeft cleared its position. Coordinates come only from the delivered native event, with the backend's existing scale conversion; no pointer polling or process/helper is added. macOS compiled locally; other backend source changes require their respective build/runtime gates.
