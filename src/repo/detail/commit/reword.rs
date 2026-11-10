//! Reword editor keyboard focus and save/cancel handling.

use super::*;

impl Reword {
    /// The summary, then the description after a blank line.
    pub(super) fn message(&self) -> String {
        let (subject, body) = (self.subject.trim(), self.body.trim());
        if body.is_empty() {
            subject.to_owned()
        } else {
            format!("{subject}\n\n{body}")
        }
    }
}

/// The summary and description being edited, with Save and Cancel: `Some(true)` to save,
/// `Some(false)` to cancel.
pub(super) fn reword_editor(ui: &mut Ui, reword: &mut Reword) -> Option<bool> {
    let t = Tokens::get(ui.ctx());
    let summary = ui.add(
        egui::TextEdit::singleline(&mut reword.subject)
            .font(theme::semibold(14.0))
            .hint_text(crate::i18n::text("repo-core-0886"))
            .desired_width(f32::INFINITY),
    );
    if reword.focus {
        summary.request_focus();
        reword.focus = false;
    }
    ui.add_space(4.0);
    let body = ui.add(
        egui::TextEdit::multiline(&mut reword.body)
            .hint_text(crate::i18n::text("repo-core-0887"))
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);
    let can_save = !reword.subject.trim().is_empty();
    // The keys are the editor's from its own fields, or when nothing else has them: Escape in
    // the terminal or another field, or one closing a dialog, doesn't throw the message away.
    // Escape in a text field takes the focus away before anything is drawn, so what counts is
    // which widget had it as the pass began.
    let ctx = ui.ctx().clone();
    ctx.add_plugin(FocusBefore);
    let before = ctx.data(|d| d.get_temp::<Option<egui::Id>>(egui::Id::new(FocusBefore::KEY)));
    let keys_here = summary.has_focus()
        || body.has_focus()
        || match before {
            Some(Some(id)) => id == summary.id || id == body.id,
            Some(None) => ctx.memory(|m| m.focused().is_none() && m.top_modal_layer().is_none()),
            // Not known yet (the editor's first pass): only its own fields.
            None => false,
        };
    let (save_keys, cancel_keys) = ui.input(|i| {
        (
            keys_here && i.modifiers.command && i.key_pressed(Key::Enter),
            keys_here && i.key_pressed(Key::Escape),
        )
    });
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        if widgets::primary_button(ui, crate::i18n::text("repo-core-0888"), can_save, 110.0)
            .clicked()
        {
            out = Some(true);
        }
        if ui.button(crate::i18n::text("repo-core-0443")).clicked() {
            out = Some(false);
        }
        ui.label(
            RichText::new(crate::i18n::text("repo-core-0889"))
                .small()
                .color(t.text_faint),
        );
    });
    if save_keys && can_save {
        out = Some(true);
    } else if cancel_keys {
        out = Some(false);
    }
    out
}

/// Keeps in temp memory (under [`FocusBefore::KEY`]) which widget had the keyboard when the
/// last pass ended, before the next one's input is taken in: egui then takes the focus away
/// from a text field for an Escape pressed in it, so that afterwards an Escape typed in the
/// commit box or a search field looks like one pressed with nothing focused.
pub(super) struct FocusBefore;

impl FocusBefore {
    const KEY: &'static str = "gitcraft-focus-before-pass";
}

impl egui::Plugin for FocusBefore {
    fn debug_name(&self) -> &'static str {
        "focus before the pass"
    }

    fn input_hook(&mut self, ctx: &egui::Context, _input: &mut egui::RawInput) {
        let focused = ctx.memory(|m| m.focused());
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(Self::KEY), focused));
    }
}
