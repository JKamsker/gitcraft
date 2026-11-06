//! A context-menu entry: the key that does the same, small on its right, and, when it can't be
//! used, why in small text under it rather than only in a tooltip.

use egui::{Response, RichText, Ui, WidgetText};

use crate::keymap::{self, Command};
use crate::theme::Tokens;

/// The widest a reason under an entry gets before it wraps.
const REASON_WIDTH: f32 = 280.0;

/// A menu entry, built up and then shown with [`MenuItem::show`].
pub(crate) struct MenuItem<'a> {
    label: WidgetText,
    enabled: bool,
    keys: Option<Command>,
    refused: Option<&'a str>,
    /// Whether the reason is written under the entry (else the entry below says the same).
    explained: bool,
}

impl<'a> MenuItem<'a> {
    pub(crate) fn new(label: impl Into<WidgetText>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            keys: None,
            refused: None,
            explained: true,
        }
    }

    /// Greyed out when false, with no reason given (an operation is running).
    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The key of `command`, shown on the right.
    pub(crate) fn keys(mut self, command: Command) -> Self {
        self.keys = Some(command);
        self
    }

    /// Why the entry can't be used, if it can't: it is greyed out and says so under it.
    pub(crate) fn refused(mut self, why: Option<&'a str>) -> Self {
        self.refused = why;
        self
    }

    /// Greyed out for its reason, but with the reason written only under the entry below, which
    /// has the same one.
    pub(crate) fn explained_below(mut self, below: bool) -> Self {
        self.explained = !below;
        self
    }

    pub(crate) fn show(self, ui: &mut Ui) -> Response {
        let mut button = egui::Button::new(self.label);
        if let Some(command) = self.keys {
            button = button.shortcut_text(keymap::label(command));
        }
        let response = ui.add_enabled(self.enabled && self.refused.is_none(), button);
        if let Some(why) = self.refused.filter(|why| self.explained && !why.is_empty()) {
            reason(ui, why);
        }
        response
    }
}

/// Why the entry above can't be used, small and faint, lined up with its label.
fn reason(ui: &mut Ui, why: &str) {
    let t = Tokens::get(ui.ctx());
    let pad = ui.spacing().button_padding.x as i8;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: pad,
            right: pad,
            top: -2,
            bottom: 5,
        })
        .show(ui, |ui| {
            ui.set_max_width(REASON_WIDTH);
            ui.add(egui::Label::new(RichText::new(why).size(11.0).color(t.text_faint)).wrap());
        });
}

/// For entries one under the other, each with why it can't be used: whether the next entry
/// gives the same reason, which is then written once, under the last of them.
pub(crate) fn same_reason_below<T: PartialEq>(reasons: &[Option<T>]) -> Vec<bool> {
    (0..reasons.len())
        .map(|i| reasons[i].is_some() && reasons.get(i + 1) == Some(&reasons[i]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::same_reason_below;

    #[test]
    fn a_reason_shared_by_entries_in_a_row_is_written_once() {
        let r = |s: &'static str| Some(s);
        assert_eq!(
            same_reason_below(&[r("a"), r("a"), r("a"), None]),
            [true, true, false, false]
        );
        assert_eq!(
            same_reason_below(&[r("a"), r("b"), r("b"), r("a")]),
            [false, true, false, false]
        );
        assert_eq!(same_reason_below::<&str>(&[None, None]), [false, false]);
        assert_eq!(same_reason_below::<&str>(&[]), Vec::<bool>::new());
    }
}
