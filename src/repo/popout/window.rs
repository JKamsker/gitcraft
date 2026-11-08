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
        // Focus records belong to this viewport. The main window's flush cannot
        // paint its ring or remember its focused row. Embedded panels above share
        // the parent's records and wait for the parent's end-of-frame flush.
        crate::a11y::end_frame(ui.ctx());
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

#[cfg(test)]
mod focus_tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    fn has_ring(shape: &egui::Shape, rect: egui::Rect, color: egui::Color32) -> bool {
        match shape {
            egui::Shape::Rect(shape) => {
                shape.rect == rect.shrink(1.0) && shape.stroke == egui::Stroke::new(2.0, color)
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| has_ring(shape, rect, color)),
            _ => false,
        }
    }

    #[test]
    fn native_panel_callback_flushes_child_ring_and_remembers_child_focus() {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let child = ViewportId::from_hash_of("synthetic-panel-focus");
        let rect = Rc::new(Cell::new(egui::Rect::NOTHING));
        let paints = Rc::new(RefCell::new(Vec::new()));
        let rect_in_renderer = rect.clone();
        let paints_in_renderer = paints.clone();
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            assert_eq!(viewport.ids.this, child);
            let mut input = egui::RawInput {
                viewport_id: child,
                ..Default::default()
            };
            input.viewports.insert(
                child,
                egui::ViewportInfo {
                    parent: Some(ViewportId::ROOT),
                    focused: Some(true),
                    ..Default::default()
                },
            );
            let output = ctx.run_ui(input, |ui| (viewport.viewport_ui_cb)(ui));
            let accent = Tokens::get(ctx).accent;
            paints_in_renderer.borrow_mut().push(
                output
                    .shapes
                    .iter()
                    .any(|shape| has_ring(&shape.shape, rect_in_renderer.get(), accent)),
            );
            output.drop_without_applying_deltas();
        });
        let row = egui::Id::new("child-panel-painted-row");
        let mut remembered = false;
        for frame in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                assert!(!show(ui.ctx(), child, "Synthetic panel", |ui, class| {
                    assert!(matches!(class, ViewportClass::Immediate));
                    if frame == 1 {
                        ui.memory_mut(|m| m.surrender_focus(row));
                    }
                    let response = ui.interact(
                        egui::Rect::from_min_size(ui.cursor().min, egui::vec2(180.0, 32.0)),
                        row,
                        egui::Sense::click(),
                    );
                    rect.set(response.rect);
                    if frame == 0 {
                        response.request_focus();
                    } else {
                        assert!(!response.has_focus());
                        remembered = crate::a11y::keyboard_on(&response);
                    }
                    crate::a11y::a11y(&response, egui::WidgetType::Button, "Inspect panel row");
                }));
                // This flush belongs to ROOT and happens after the renderer collected
                // the child's output. It cannot make a missing child ring pass.
                crate::a11y::end_frame(ui.ctx());
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(*paints.borrow(), vec![true, false]);
        assert!(
            remembered,
            "child callback must remember focus before the next child frame"
        );
    }

    #[test]
    fn embedded_panel_ring_is_flushed_by_the_parent() {
        let ctx = egui::Context::default();
        assert!(ctx.embed_viewports());
        let mut rect = egui::Rect::NOTHING;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(
                ui.ctx(),
                ViewportId::from_hash_of("embedded-panel-focus"),
                "Embedded panel",
                |ui, class| {
                    assert!(matches!(class, ViewportClass::EmbeddedWindow));
                    let response =
                        ui.allocate_response(egui::vec2(180.0, 32.0), egui::Sense::click());
                    rect = response.rect;
                    response.request_focus();
                    crate::a11y::a11y(&response, egui::WidgetType::Button, "Inspect embedded row");
                },
            );
            crate::a11y::end_frame(ui.ctx());
        });
        assert!(output.shapes.iter().any(|shape| has_ring(
            &shape.shape,
            rect,
            Tokens::get(&ctx).accent
        )));
        output.drop_without_applying_deltas();
    }
}
