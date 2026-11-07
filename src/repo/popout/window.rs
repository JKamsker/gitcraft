//! One pop-out window: a native window of its own where the platform has several, else an
//! egui window inside the main one.
//!
//! It is an *immediate* viewport (`Context::show_viewport_immediate`), not a deferred one: a
//! deferred viewport's drawing is a `Send + Sync + 'static` callback egui may run on its own,
//! while the main window sleeps, so everything it draws would have to live behind an
//! `Arc<Mutex<…>>`. The views popped out (the diff, the terminal, a pull request, the CI view)
//! draw from and act on the repository tab itself (`&mut RepoTab`: its channels, its terminals,
//! its staged lines), which would mean taking the whole tab apart. An immediate viewport is
//! drawn right where it is asked for, borrowing the tab like the main window does; the cost is
//! that the main window and the pop-outs repaint together, each pop-out adding its drawing to
//! every frame, and that a pop-out is only drawn in the main window's passes: where a minimized
//! or hidden main window gets none (Wayland), its pop-outs wait for it too. A minimized pop-out
//! isn't painted.

use std::sync::{Arc, OnceLock};

use egui::{Ui, ViewportBuilder, ViewportClass, ViewportId};

use crate::theme::Tokens;

/// The size a pop-out opens at.
const SIZE: [f32; 2] = [960.0, 720.0];
/// The smallest it can be made.
const MIN_SIZE: [f32; 2] = [360.0, 240.0];

/// Shows the window `id`, titled `title`, with `body` drawn in it (told whether it is a
/// native window or one inside the main window). Returns whether its close button was
/// clicked: the caller then stops showing it, which closes it. A window inside the main one
/// has no close button of its own (`body` offers Bring back instead).
pub(super) fn show(
    ctx: &egui::Context,
    id: ViewportId,
    title: &str,
    mut body: impl FnMut(&mut Ui, ViewportClass),
) -> bool {
    ctx.show_viewport_immediate(id, builder(title), |ui, class| {
        if class == ViewportClass::EmbeddedWindow {
            // The main window's input: its close button isn't this window's.
            ui.set_min_size(egui::vec2(SIZE[0] * 0.75, SIZE[1] * 0.6));
            body(ui, class);
            return false;
        }
        let (closing, minimized) =
            ui.input(|i| (i.viewport().close_requested(), i.viewport().minimized));
        if minimized == Some(true) {
            // Nothing to see; and on macOS painting a hidden window can wait on the system.
            return closing;
        }
        let t = Tokens::get(ui.ctx());
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.dock))
            .show(ui, |ui| body(ui, class));
        closing
    })
}

/// The window as it is asked for every frame (a changed title is applied to it).
fn builder(title: &str) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(title)
        .with_app_id("gitcraft")
        .with_icon(icon())
        .with_inner_size(SIZE)
        .with_min_inner_size(MIN_SIZE)
}

/// The window icon, decoded once: the same `Arc` every frame, which egui takes as unchanged.
fn icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| Arc::new(crate::app::window_icon()))
        .clone()
}
