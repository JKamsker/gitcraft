//! Accessibility of the hand-painted widgets: a focus ring while one has the keyboard, a name
//! for screen readers (AccessKit), and the system's reduced-motion setting.
//!
//! Painted rows, pills and cards sense clicks, so egui already lets Tab reach them and Enter /
//! Space click them. [`a11y`] names each one and marks it for the ring, which [`end_frame`]
//! draws over everything else of its layer once the frame is painted.

mod focus;
pub mod labels;
pub mod motion;
pub mod value;

use egui::{Response, WidgetInfo, WidgetType};

pub use focus::{keyboard_on, keyboard_taken, list_has_focus, list_row_sense, list_stop};

/// A single-line field may submit after Enter surrenders its focus. Focus transfers
/// to another widget, including Tab and Enter in one frame, must not submit it.
pub(crate) fn singleline_enter(response: &Response) -> bool {
    (response.has_focus()
        || (response.lost_focus() && response.ctx.memory(|m| m.focused().is_none())))
        && response
            .ctx
            .input(|i| i.key_pressed(egui::Key::Enter) && !i.key_pressed(egui::Key::Tab))
}

/// What a widget is called, built only when a screen reader or an event asks: a graph row's
/// name joins several fields, and rows are drawn every frame.
pub trait Label {
    fn text(&self) -> String;
}

impl Label for &str {
    fn text(&self) -> String {
        (*self).to_owned()
    }
}

impl Label for String {
    fn text(&self) -> String {
        self.clone()
    }
}

impl Label for &String {
    fn text(&self) -> String {
        (*self).clone()
    }
}

impl<F: Fn() -> String> Label for F {
    fn text(&self) -> String {
        self()
    }
}

/// Names a hand-painted widget for screen readers (as `typ`, read as `label`) and gives it the
/// focus ring while it has the keyboard. Call once per frame, after the widget's interaction.
pub fn a11y(resp: &Response, typ: WidgetType, label: impl Label) {
    let enabled = resp.enabled();
    resp.widget_info(|| WidgetInfo::labeled(typ, enabled, labels::name(&label.text())));
    focus::note(
        resp,
        matches!(
            typ,
            WidgetType::TextEdit | WidgetType::Slider | WidgetType::DragValue
        ),
    );
}

/// [`a11y`] for a row of a list that is selected or not (a commit, a palette entry), so a
/// screen reader says which one is.
pub fn a11y_selected(resp: &Response, label: impl Label, selected: bool) {
    let enabled = resp.enabled();
    resp.widget_info(|| WidgetInfo {
        selected: Some(selected),
        ..WidgetInfo::labeled(
            WidgetType::SelectableLabel,
            enabled,
            labels::name(&label.text()),
        )
    });
    focus::note(resp, false);
}

/// A disclosure's semantics include its expanded state, not just its painted chevron.
pub fn expanded(resp: &Response, label: impl Label, open: bool) {
    a11y(resp, WidgetType::Button, label);
    resp.ctx
        .accesskit_node_builder(resp.id, |node| node.set_expanded(open));
}

pub fn pane(resp: &Response, label: impl Label) {
    a11y(resp, WidgetType::Other, label);
    resp.ctx
        .accesskit_node_builder(resp.id, |node| node.set_role(egui::accesskit::Role::Pane));
}

/// [`a11y`] in a chain of a response's calls, for a button that only shows an icon: its name
/// replaces the one the shared button gave it.
pub trait Named {
    fn named(self, label: impl Label) -> Self;
}

impl Named for Response {
    fn named(self, label: impl Label) -> Self {
        a11y(&self, WidgetType::Button, label);
        self
    }
}

/// Once the frame's widgets are drawn: the focus ring, over them, and egui's own animations
/// stopped when the system asks for less motion.
pub fn end_frame(ctx: &egui::Context) {
    focus::paint_ring(ctx);
    focus::remember(ctx);
    motion::apply(ctx);
}

#[cfg(test)]
mod tests;
