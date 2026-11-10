//! Native edge hit testing and resize cursors.

use super::*;

/// Resizing from the window's edges and corners, which have no system frame to do it: over
/// everything else, and only while the window is neither maximized nor fullscreen. macOS
/// resizes borderless windows natively; winit's BeginResize is unsupported there.
pub(in crate::app) fn resize_edges(ctx: &egui::Context) {
    if cfg!(target_os = "macos") {
        return;
    }
    let state = window_state(ctx);
    if state.maximized || state.fullscreen {
        return;
    }
    for (i, (dir, rect)) in edges(ctx.viewport_rect()).into_iter().enumerate() {
        egui::Area::new(egui::Id::new(("window-edge", i)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .show(ctx, |ui| {
                let (_, resp) = ui.allocate_exact_size(rect.size(), Sense::click());
                let resp = resp.on_hover_cursor(cursor(dir));
                if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                }
            });
    }
}

/// The bands along the inside of `window` that resize it: the four corners, then the four
/// edges between them, none overlapping.
pub(super) fn edges(window: Rect) -> [(ResizeDirection, Rect); 8] {
    let (l, r, t, b) = (window.left(), window.right(), window.top(), window.bottom());
    let corner = |x: f32, y: f32| Rect::from_min_size(pos2(x, y), vec2(CORNER, CORNER));
    let span_x = |y: f32| Rect::from_min_max(pos2(l + CORNER, y), pos2(r - CORNER, y + EDGE));
    let span_y = |x: f32| Rect::from_min_max(pos2(x, t + CORNER), pos2(x + EDGE, b - CORNER));
    [
        (ResizeDirection::NorthWest, corner(l, t)),
        (ResizeDirection::NorthEast, corner(r - CORNER, t)),
        (ResizeDirection::SouthWest, corner(l, b - CORNER)),
        (ResizeDirection::SouthEast, corner(r - CORNER, b - CORNER)),
        (ResizeDirection::North, span_x(t)),
        (ResizeDirection::South, span_x(b - EDGE)),
        (ResizeDirection::West, span_y(l)),
        (ResizeDirection::East, span_y(r - EDGE)),
    ]
}

pub(super) fn cursor(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    }
}
