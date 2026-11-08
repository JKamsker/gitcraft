//! The focus ring of painted widgets, and long lists that are a single Tab stop.

use egui::{Context, EventFilter, Id, LayerId, Rect, Response, Sense, Stroke, StrokeKind};

use crate::theme::Tokens;

/// The painted widget with the keyboard this frame, to ring once everything is drawn.
#[derive(Clone, Copy)]
struct Ring {
    id: Id,
    layer: LayerId,
    rect: Rect,
    /// The part of `rect` in view (a row half scrolled out is ringed only where it shows).
    clip: Rect,
}

fn ring_key(viewport: egui::ViewportId) -> Id {
    Id::new(("a11y-focus-ring", viewport))
}

/// The last painted widget that had the keyboard: while it still has it, the keys are free for
/// the window's shortcuts (it takes none but Enter and Space).
fn painted_key(viewport: egui::ViewportId) -> Id {
    Id::new(("a11y-painted-focus", viewport))
}

fn before_key(viewport: egui::ViewportId) -> Id {
    Id::new(("a11y-focus-before", viewport))
}

/// Whether `resp` had the keyboard as this pass began, or has it now. A widget that gives the
/// keyboard away to the next one with Tab has already let go when it is drawn, but what shows
/// only while it has the keyboard (a row's hover buttons) must still be drawn this pass, or
/// Tab skips over it.
pub fn keyboard_on(resp: &Response) -> bool {
    let key = before_key(resp.ctx.viewport_id());
    resp.has_focus() || resp.ctx.data(|d| d.get_temp::<Option<Id>>(key)).flatten() == Some(resp.id)
}

/// Remembers which widget has the keyboard as the pass ends, for [`keyboard_on`].
pub(super) fn remember(ctx: &Context) {
    let key = before_key(ctx.viewport_id());
    let focused = ctx.memory(|m| m.focused());
    ctx.data_mut(|d| d.insert_temp(key, focused));
}

/// Notes `resp` for the ring when it has the keyboard.
pub(super) fn note(resp: &Response, owns_keys: bool) {
    if !resp.has_focus() {
        return;
    }
    let ring = Ring {
        id: resp.id,
        layer: resp.layer_id,
        rect: resp.rect,
        clip: resp.interact_rect,
    };
    let viewport = resp.ctx.viewport_id();
    resp.ctx.data_mut(|d| {
        d.insert_temp(ring_key(viewport), Some(ring));
        if owns_keys {
            d.remove::<Id>(painted_key(viewport));
        } else {
            d.insert_temp(painted_key(viewport), resp.id);
        }
    });
}

/// Draws the ring around the painted widget that has the keyboard, on top of its layer.
pub(super) fn paint_ring(ctx: &Context) {
    let key = ring_key(ctx.viewport_id());
    let Some(ring) = ctx
        .data_mut(|d| d.remove_temp::<Option<Ring>>(key))
        .flatten()
    else {
        return;
    };
    if !ctx.memory(|m| m.has_focus(ring.id)) || !ring.clip.is_positive() {
        return;
    }
    let t = Tokens::get(ctx);
    ctx.layer_painter(ring.layer)
        .with_clip_rect(ring.clip)
        .rect_stroke(
            ring.rect.shrink(1.0),
            t.radius_sm,
            Stroke::new(2.0, t.accent),
            StrokeKind::Inside,
        );
}

/// Whether a widget that reads keys (a text field, a slider, egui's own buttons) has the
/// keyboard, so the window's shortcuts must leave them alone. A painted row or pill with the
/// focus ring doesn't count.
pub fn keyboard_taken(ctx: &Context) -> bool {
    let key = painted_key(ctx.viewport_id());
    let painted: Option<Id> = ctx.data(|d| d.get_temp(key));
    ctx.memory(|m| m.focused())
        .is_some_and(|focused| Some(focused) != painted)
}

/// Where the keyboard was in a long list as a pass ended: its stop row, and that pass.
#[derive(Clone, Copy)]
struct ListFocus {
    pass: u64,
    row: Id,
}

fn list_key(list: Id) -> Id {
    list.with("a11y-list-focus")
}

/// How a row of a long list (the graph's commits) senses: it clicks either way, but only the
/// list's one stop (the selected row, else the first in view) is reached with Tab, so Tab
/// steps over the list rather than through thousands of rows; ↑ / ↓ move inside it.
pub fn list_row_sense(stop: bool) -> Sense {
    if stop { Sense::click() } else { Sense::CLICK }
}

/// For the stop row of `list`, after its interaction. While it has the keyboard, the arrows are
/// the list's own (↑ / ↓ move its selection, not egui's focus); when the selection moves to
/// another row, the keyboard follows it there. `selected`: the stop is the selected row, not
/// the first in view standing in for one scrolled away (scrolling doesn't move the keyboard).
pub fn list_stop(resp: &Response, list: Id, selected: bool) {
    let ctx = &resp.ctx;
    let key = list_key(list).with(ctx.viewport_id());
    let pass = ctx.cumulative_pass_nr();
    let last: Option<ListFocus> = ctx.data(|d| d.get_temp(key));
    if resp.has_focus() {
        hold(resp, key, pass);
        return;
    }
    // The stop moved this pass, with the keyboard on the old one in the pass before (it lets
    // go as it stops being focusable, or does a moment later in this pass).
    let followed = selected
        && last.is_some_and(|l| {
            l.pass + 1 == pass
                && l.row != resp.id
                && ctx.memory(|m| m.focused().is_none_or(|f| f == l.row))
        });
    if followed {
        resp.request_focus();
        hold(resp, key, pass);
    }
}

/// Keeps ↑ / ↓ for the list while `resp`, its stop, has the keyboard (set again every pass, as
/// a new focus starts without it), and remembers where the keyboard is.
fn hold(resp: &Response, key: Id, pass: u64) {
    // ← / → too: egui would otherwise move the keyboard to whatever lies beside the row.
    let arrows = EventFilter {
        vertical_arrows: true,
        horizontal_arrows: true,
        ..EventFilter::default()
    };
    resp.ctx
        .memory_mut(|m| m.set_focus_lock_filter(resp.id, arrows));
    resp.ctx
        .data_mut(|d| d.insert_temp(key, ListFocus { pass, row: resp.id }));
}

/// Whether the keyboard is on `list`'s stop row (as of the last pass), whose ↑ / ↓ the list
/// takes.
pub fn list_has_focus(ctx: &Context, list: Id) -> bool {
    let key = list_key(list).with(ctx.viewport_id());
    let last: Option<ListFocus> = ctx.data(|d| d.get_temp(key));
    last.is_some_and(|l| ctx.memory(|m| m.has_focus(l.row)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_focus_records_are_scoped_to_the_window() {
        let a = egui::ViewportId::ROOT;
        let b = egui::ViewportId::from_hash_of("detached-repository");
        assert_ne!(ring_key(a), ring_key(b));
        assert_ne!(painted_key(a), painted_key(b));
        assert_ne!(before_key(a), before_key(b));
        assert_ne!(
            list_key(Id::new("graph")).with(a),
            list_key(Id::new("graph")).with(b)
        );
    }
}
