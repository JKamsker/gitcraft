//! The find bar under a diff's or a file's header: the text to find, its toggles, which match
//! is the current one, and the buttons to step and to close.

use egui::{Key, RichText, Stroke, Ui};

use super::Find;
use super::scan::MAX_HITS;
use crate::icons;
use crate::theme::{self, Tokens};

/// The bar's text field, so that tests can find it.
pub(crate) const FIELD_ID: &str = "find_in_view_field";

impl Find {
    /// The bar, when open. `what`: what is searched, for the hint ("the diff").
    pub(crate) fn bar(&mut self, ui: &mut Ui, what: &str) {
        if !self.open {
            return;
        }
        let t = Tokens::get(ui.ctx());
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 4))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "search", 14.0, t.text_faint);
                    self.field(ui, what);
                    self.toggles(ui);
                    if let Some((text, error)) = self.summary() {
                        let color = if error { t.danger } else { t.text_dim };
                        let label = ui.label(RichText::new(text).color(color).small());
                        if self.capped() {
                            label.on_hover_text(format!(
                                "Only the first {MAX_HITS} matches are found; type more to \
                                 narrow it down"
                            ));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                            self.close();
                        }
                        if icons::small_button(ui, "chevron-down", "Next match (Enter, F3)")
                            .clicked()
                        {
                            self.step(1);
                        }
                        if icons::small_button(
                            ui,
                            "chevron-up",
                            "Previous match (Shift+Enter, Shift+F3)",
                        )
                        .clicked()
                        {
                            self.step(-1);
                        }
                    });
                });
            });
    }

    /// The text field; Enter / Shift+Enter in it step, Esc closes the bar.
    fn field(&mut self, ui: &mut Ui, what: &str) {
        let edit = egui::TextEdit::singleline(&mut self.query.text)
            .id(egui::Id::new(FIELD_ID))
            .hint_text(format!("Find in {what}"))
            .font(theme::mono(12.0))
            .desired_width((ui.available_width() - 330.0).clamp(140.0, 420.0))
            .margin(egui::Margin::symmetric(8, 3));
        let mut out = edit.show(ui);
        if std::mem::take(&mut self.focus) {
            out.response.request_focus();
            // All of it selected, so typing replaces it.
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(self.query.text.chars().count()),
            );
            out.state.cursor.set_char_range(Some(all));
            out.state.store(ui.ctx(), out.response.id);
        }
        let edit = out.response;
        let (enter, shift, esc) = ui.input(|i| {
            (
                i.key_pressed(Key::Enter),
                i.modifiers.shift,
                i.key_pressed(Key::Escape),
            )
        });
        if edit.lost_focus() && enter {
            self.step(if shift { -1 } else { 1 });
            edit.request_focus();
        }
        if (edit.has_focus() || edit.lost_focus()) && esc {
            self.close();
        }
    }

    /// Match case, whole word and regular expression.
    fn toggles(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let q = &mut self.query;
        for (label, on, tip) in [
            ("Aa", &mut q.case_sensitive, "Match case"),
            ("ab", &mut q.whole_word, "Match whole words only"),
            (".*", &mut q.regex, "Use a regular expression"),
        ] {
            let text = RichText::new(label).font(theme::mono(11.5)).color(if *on {
                t.text
            } else {
                t.text_dim
            });
            let r = ui.selectable_label(*on, text).on_hover_text(tip);
            if *on {
                crate::repo::widgets::active_underline(ui, r.rect, t.accent);
            }
            if r.clicked() {
                *on = !*on;
            }
        }
    }
}
