//! Drag state, drop-target selection, and floating button animation.
use super::*;

/// Which way a place's buttons go from its outer end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    RightToLeft,
    LeftToRight,
    BottomUp,
}

impl Flow {
    /// Whether `a` comes before `b` along the flow.
    pub(super) fn before(self, a: Pos2, b: Pos2) -> bool {
        match self {
            Flow::RightToLeft => a.x > b.x,
            Flow::LeftToRight => a.x < b.x,
            Flow::BottomUp => a.y > b.y,
        }
    }

    /// One step along the flow, of `pitch`.
    pub(super) fn step(self, pitch: f32) -> Vec2 {
        match self {
            Flow::RightToLeft => vec2(-pitch, 0.0),
            Flow::LeftToRight => vec2(pitch, 0.0),
            Flow::BottomUp => vec2(0.0, -pitch),
        }
    }
}

/// A button being dragged: which, how it looked where it came from, and where the pointer
/// holds it from its corner.
#[derive(Clone, Copy, Debug)]
pub(super) struct Drag {
    pub(super) button: AppButton,
    pub(super) look: Look,
    pub(super) grab: Vec2,
}

/// A place as drawn: where a dragged button is caught by it, which way it goes, and the middle
/// of each of its buttons as it would be without an opening.
#[derive(Clone, Debug)]
pub(super) struct Zone {
    pub(super) place: Place,
    pub(super) catch: Rect,
    pub(super) flow: Flow,
    pub(super) slots: Vec<Pos2>,
}

/// The opening a dragged button is over: its place and its index there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Drop {
    pub(super) place: Place,
    pub(super) index: usize,
}

/// The dragging, kept from frame to frame.
#[derive(Clone, Debug, Default)]
pub(super) struct DragState {
    pub(super) drag: Option<Drag>,
    pub(super) drop: Option<Drop>,
    /// The places as drawn last frame, and so far this frame.
    pub(super) zones: Vec<Zone>,
    pub(super) zones_now: Vec<Zone>,
    /// Until when buttons slide rather than jump to their places (a window resizing doesn't
    /// make them trail behind).
    pub(super) settle_until: f64,
}

pub(super) fn state_id() -> Id {
    Id::new("gitcraft-app-button-drag")
}

pub(super) fn state(ctx: &egui::Context) -> DragState {
    ctx.data(|d| d.get_temp::<DragState>(state_id()))
        .unwrap_or_default()
}

pub(super) fn set_state(ctx: &egui::Context, s: DragState) {
    ctx.data_mut(|d| d.insert_temp(state_id(), s));
}

pub(super) fn button_id(button: AppButton) -> Id {
    Id::new(("app-button", button))
}

/// Before anything is drawn: ends a drag let go (moving the button where it was dropped, in
/// the settings, so it is drawn there at once), and finds the opening one is over.
pub fn begin_frame(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.zones = std::mem::take(&mut s.zones_now);
    let Some(drag) = s.drag else {
        set_state(ctx, s);
        return;
    };
    let now = ctx.input(|i| i.time);
    let (down, cancel, pointer) = ctx.input(|i| {
        (
            i.pointer.primary_down(),
            i.key_pressed(egui::Key::Escape),
            i.pointer.latest_pos(),
        )
    });
    if cancel || !down {
        if let Some(drop) = s.drop.filter(|_| !cancel) {
            let mut settings = Settings::get(ctx);
            settings.button_layout = layout(ctx).moved(drag.button, drop.place, drop.index);
            settings.set(ctx);
        }
        s.drag = None;
        s.drop = None;
        s.settle_until = now + f64::from(SLIDE) + 0.1;
    } else {
        s.drop = pointer.and_then(|p| drop_at(&s.zones, p));
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    set_state(ctx, s);
    ctx.request_repaint();
}

/// The opening for `pointer` among `zones`: in the place nearest to it, wherever it is, before
/// the first of that place's buttons the pointer isn't past.
pub(super) fn drop_at(zones: &[Zone], pointer: Pos2) -> Option<Drop> {
    let zone = zones.iter().min_by(|a, b| {
        let near = |z: &Zone| {
            (
                z.catch.distance_to_pos(pointer),
                z.catch.center().distance(pointer),
            )
        };
        near(a)
            .0
            .total_cmp(&near(b).0)
            .then(near(a).1.total_cmp(&near(b).1))
    })?;
    let index = zone
        .slots
        .iter()
        .filter(|&&slot| zone.flow.before(slot, pointer))
        .count();
    Some(Drop {
        place: zone.place,
        index,
    })
}

/// After everything is drawn: the dragged button over everything, where the pointer holds it,
/// or pulled toward the opening it is over.
pub fn end_frame(ctx: &egui::Context) {
    let s = state(ctx);
    let Some(drag) = s.drag else {
        if ctx.input(|i| i.time) < s.settle_until {
            ctx.request_repaint();
        }
        return;
    };
    let Some(pointer) = ctx.input(|i| i.pointer.latest_pos()) else {
        return;
    };
    // Held where the pointer took it. Kept as where it is shown, so that when let go it
    // slides from there into its place.
    let at = animated(ctx, drag.button, pointer - drag.grab, 0.0);
    floating(ctx, drag.button, drag.look, at, true);
}

/// Where `button` is shown on its way to `target`.
pub(super) fn animated(ctx: &egui::Context, button: AppButton, target: Pos2, time: f32) -> Pos2 {
    let id = button_id(button);
    pos2(
        ctx.animate_value_with_time(id.with("x"), target.x, time),
        ctx.animate_value_with_time(id.with("y"), target.y, time),
    )
}

/// `button` drawn at `at` over everything (dragged, or sliding across panels to its place).
pub(super) fn floating(ctx: &egui::Context, button: AppButton, look: Look, at: Pos2, lifted: bool) {
    egui::Area::new(button_id(button).with("floating"))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .interactable(false)
        .constrain(false)
        .show(ctx, |ui| {
            let rect = Rect::from_min_size(at, Vec2::splat(SIZE));
            if lifted {
                let t = Tokens::get(ctx);
                ui.painter().rect_filled(rect, t.radius, t.card);
                ui.painter().rect_stroke(
                    rect,
                    t.radius,
                    egui::Stroke::new(1.0, t.field_border),
                    egui::StrokeKind::Inside,
                );
            }
            paint(ui, rect, button, look, lifted, false);
        });
}
