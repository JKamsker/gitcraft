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
    icon: Option<&'static str>,
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
            icon: None,
            enabled: true,
            keys: None,
            refused: None,
            explained: true,
        }
    }

    pub(crate) fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
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
        let mut button = match self.icon {
            Some(icon) => egui::Button::image_and_text(
                crate::icons::image(icon, 16.0, Tokens::get(ui.ctx()).icon),
                self.label,
            ),
            None => egui::Button::new(self.label),
        };
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

/// The in-viewport fallback for platforms without native transient menus. egui's Popup selects
/// the best alignment and constrains its Area to the viewport; bound every nested menu too.
/// Stable salts preserve scroll/focus state while repository rows move underneath the popup.
pub(crate) fn bounded_menu<R>(
    ui: &mut Ui,
    salt: impl egui::AsIdSalt,
    content: impl FnOnce(&mut Ui) -> R,
) -> R {
    let room = ui.ctx().content_rect().size();
    // Leave a second column for hover submenus: a full-window parent would force the
    // child over its siblings, making sibling hover targets impossible to reach.
    let width = (room.x * 0.5 - 40.0).clamp(1.0, 360.0);
    ui.set_max_width(width);
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    egui::ScrollArea::both()
        .id_salt(salt)
        .max_width(width)
        .max_height((room.y - 32.0).max(1.0))
        .auto_shrink([true, true])
        .show(ui, |ui| {
            ui.set_max_width(width);
            content(ui)
        })
        .inner
}

/// Keep the toolkit's submenu hover, keyboard and close behavior, with reachable scrolling.
pub(crate) fn group(ui: &mut Ui, label: &str, content: impl FnOnce(&mut Ui)) {
    ui.menu_button(label, |ui| {
        bounded_menu(ui, label, content);
    });
}

#[cfg(test)]
mod area_d_tests {
    use egui_kittest::kittest::Queryable;

    #[test]
    fn area_d_tall_menu_scrolls_to_last_action_and_preserves_identity() {
        let mut h = egui_kittest::Harness::builder()
            .with_size([420.0, 240.0])
            .build_ui_state(
                |ui, picked: &mut Option<usize>| {
                    let r = ui.button("Long menu");
                    egui::Popup::menu(&r)
                        .style(crate::theme::menu_style)
                        .show(|ui| {
                            super::bounded_menu(ui, "many-branches", |ui| {
                                for i in 0..100 {
                                    if ui.button(format!("Branch action {i}")).clicked() {
                                        *picked = Some(i);
                                    }
                                }
                            });
                        });
                },
                None,
            );
        h.run();
        h.get_by_label("Long menu").click();
        h.run();
        let first = h.get_by_label("Branch action 0").rect();
        assert!(h.ctx.content_rect().contains_rect(first));
        h.get_by_label("Branch action 99").scroll_to_me();
        h.run();
        let last = h.get_by_label("Branch action 99").rect();
        assert!(
            h.ctx.content_rect().contains_rect(last),
            "last action is unreachable: {last:?}"
        );
        h.get_by_label("Branch action 99").click();
        h.run();
        assert_eq!(*h.state(), Some(99));
    }
}
