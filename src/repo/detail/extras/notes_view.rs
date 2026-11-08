//! The Notes section: the note in the namespace picked, to add, edit or delete, and the
//! namespace fetched from or pushed to a remote.

use egui::{RichText, Ui};

use super::section::{self, Summary};
use super::{ExtrasCmd, Notes};
use crate::repo::widgets;
use crate::theme::Tokens;

pub(super) fn section(
    ui: &mut Ui,
    notes: &mut Notes,
    remotes: &[String],
    idle: bool,
    cmds: &mut Vec<ExtrasCmd>,
) {
    let t = Tokens::get(ui.ctx());
    let namespace = notes.namespace.clone().unwrap_or_default();
    let summary = match &notes.note {
        None => Summary::Loading,
        Some(Err(_)) => Summary::Text(crate::i18n::text("repo-core-1081").to_owned(), t.danger),
        Some(Ok(Some(text))) => Summary::Text(first_line(text), t.text_dim),
        Some(Ok(None)) => Summary::Text(crate::i18n::message("repo-core-1103", &[("namespace", format!("{}", namespace).into())]), t.text_faint),
    };
    let has_note = matches!(notes.note, Some(Ok(Some(_))));
    section::section(ui, crate::i18n::text("repo-core-1104"), summary, has_note, None, |ui| {
        namespace_picker(ui, notes, cmds);
        if let Some(e) = &notes.error {
            section::error(ui, e);
        }
        match notes.draft.take() {
            Some(draft) => notes.draft = editor(ui, notes.saving, idle, draft, cmds),
            None => note(ui, notes, idle, cmds),
        }
        ui.add_space(4.0);
        share(ui, notes, remotes, idle, cmds);
    });
}

/// The note's first line, for the header.
fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// The namespace shown, a list of the others, and a field to type a new one in.
fn namespace_picker(ui: &mut Ui, notes: &mut Notes, cmds: &mut Vec<ExtrasCmd>) {
    let t = Tokens::get(ui.ctx());
    let current = notes.namespace.clone().unwrap_or_default();
    // Not while a note is being written in this one.
    let writing = notes.draft.is_some();
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(!writing, |ui| namespace_choices(ui, notes, &current, cmds))
            .response
            .on_disabled_hover_text(crate::i18n::text("repo-core-1105"));
    });
    let _ = t;
}

/// The namespace list and the field for a new one.
fn namespace_choices(ui: &mut Ui, notes: &mut Notes, current: &str, cmds: &mut Vec<ExtrasCmd>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(crate::i18n::text("repo-core-1106")).color(t.text_faint));
        egui::ComboBox::from_id_salt("notes_namespace")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for name in &notes.namespaces {
                    if ui.selectable_label(name == current, name).clicked() && name != current {
                        cmds.push(ExtrasCmd::PickNamespace(name.clone()));
                    }
                }
            })
            .response
            .on_hover_text(crate::i18n::message("repo-core-1108", &[("current", format!("{}", current).into())]));
        let field = ui.add(
            egui::TextEdit::singleline(&mut notes.new_namespace)
                .hint_text(crate::i18n::text("repo-core-1109"))
                .desired_width(110.0),
        );
        let typed = !notes.new_namespace.trim().is_empty();
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.add_enabled(typed, egui::Button::new(crate::i18n::text("repo-core-1110"))).clicked() || (enter && typed) {
            cmds.push(ExtrasCmd::PickNamespace(notes.new_namespace.clone()));
        }
    });
}

/// The note shown, with Edit and Delete; Add note when there is none.
fn note(ui: &mut Ui, notes: &mut Notes, idle: bool, cmds: &mut Vec<ExtrasCmd>) {
    let t = Tokens::get(ui.ctx());
    let busy = notes.saving;
    match &notes.note {
        None => section::faint(ui, crate::i18n::text("repo-core-1111")),
        Some(Err(e)) => section::error(ui, e),
        Some(Ok(None)) => {
            section::faint(ui, crate::i18n::text("repo-core-1112"));
            if ui
                .add_enabled(!busy, egui::Button::new(crate::i18n::text("repo-core-1113")))
                .clicked()
            {
                notes.draft = Some(String::new());
            }
        }
        Some(Ok(Some(text))) => {
            let text = text.clone();
            egui::Frame::new()
                .fill(t.field)
                .corner_radius(t.radius_sm)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(text.trim_end()).wrap().selectable(true));
                });
            ui.horizontal(|ui| {
                if notes.confirm_delete {
                    ui.label(RichText::new(crate::i18n::text("repo-core-1114")).color(t.danger));
                    if ui
                        .add_enabled(!busy && idle, egui::Button::new(crate::i18n::text("repo-core-1115")))
                        .clicked()
                    {
                        cmds.push(ExtrasCmd::DeleteNote);
                    }
                    if ui.button(crate::i18n::text("repo-core-0380")).clicked() {
                        notes.confirm_delete = false;
                    }
                } else {
                    if ui.add_enabled(!busy, egui::Button::new(crate::i18n::text("repo-core-1116"))).clicked() {
                        notes.draft = Some(text.trim_end().to_owned());
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(crate::i18n::text("repo-core-1117")))
                        .clicked()
                    {
                        notes.confirm_delete = true;
                    }
                }
                if busy {
                    ui.add(crate::spinner::Spinner::new().size(12.0));
                }
            });
        }
    }
}

/// The note being written, with Save and Cancel: what is left of it afterwards.
fn editor(
    ui: &mut Ui,
    saving: bool,
    idle: bool,
    mut draft: String,
    cmds: &mut Vec<ExtrasCmd>,
) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let edit = ui.add_enabled(
        !saving,
        egui::TextEdit::multiline(&mut draft)
            .hint_text(crate::i18n::text("repo-core-1118"))
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
    let save_keys =
        edit.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    // Escape in the editor (egui has taken the focus away for it) cancels it, and goes no
    // further: the diff or search behind stays open.
    let cancel_keys = edit.lost_focus()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let mut keep = !cancel_keys;
    ui.horizontal(|ui| {
        // Not while an operation runs: a notes fetch or push would hold the same ref.
        let can_save = !saving && idle && !draft.trim().is_empty();
        if widgets::primary_button(ui, crate::i18n::text("repo-core-1119"), can_save, 90.0).clicked()
            || (save_keys && can_save)
        {
            cmds.push(ExtrasCmd::SaveNote(draft.clone()));
        }
        if ui
            .add_enabled(!saving, egui::Button::new(crate::i18n::text("repo-core-0443")))
            .clicked()
        {
            keep = false;
        }
        if saving {
            ui.add(crate::spinner::Spinner::new().size(12.0));
        } else {
            ui.label(
                RichText::new(crate::i18n::message("repo-core-1120", &[("command", format!("{}", crate::manual::command()).into())]))
                .small()
                .color(t.text_faint),
            );
        }
    });
    keep.then_some(draft)
}

/// The remote picked, with Fetch and Push for the namespace shown.
fn share(
    ui: &mut Ui,
    notes: &mut Notes,
    remotes: &[String],
    idle: bool,
    cmds: &mut Vec<ExtrasCmd>,
) {
    let t = Tokens::get(ui.ctx());
    if remotes.is_empty() {
        section::faint(ui, crate::i18n::text("repo-core-1121"));
        return;
    }
    let remote = notes
        .remote
        .clone()
        .filter(|r| remotes.contains(r))
        .or_else(|| remotes.iter().find(|r| *r == "origin").cloned())
        .unwrap_or_else(|| remotes[0].clone());
    let namespace = notes.namespace.clone().unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(crate::i18n::text("repo-core-0707")).color(t.text_faint));
        egui::ComboBox::from_id_salt("notes_remote")
            .selected_text(&remote)
            .show_ui(ui, |ui| {
                for name in remotes {
                    if ui.selectable_label(*name == remote, name).clicked() {
                        notes.remote = Some(name.clone());
                    }
                }
            });
        let fetch = ui
            .add_enabled(idle, egui::Button::new(crate::i18n::text("repo-core-1123")))
            .on_hover_text(crate::i18n::message("repo-core-1124", &[("namespace", format!("{}", namespace).into()), ("remote", format!("{}", remote).into())]))
            .on_disabled_hover_text(crate::i18n::text("repo-core-1125"));
        if fetch.clicked() {
            cmds.push(ExtrasCmd::FetchNotes(remote.clone()));
        }
        let has_notes = notes.existing.contains(&namespace);
        let push = ui
            .add_enabled(idle && has_notes, egui::Button::new(crate::i18n::text("repo-core-1126")))
            .on_hover_text(crate::i18n::message("repo-core-1127", &[("namespace", format!("{}", namespace).into()), ("remote", format!("{}", remote).into())]))
            .on_disabled_hover_text(if has_notes {
                crate::i18n::text("repo-core-1125").to_owned()
            } else {
                crate::i18n::message("repo-core-1128", &[("namespace", format!("{}", namespace).into())])
            });
        if push.clicked() {
            cmds.push(ExtrasCmd::PushNotes(remote.clone()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_shows_the_first_line_with_text() {
        assert_eq!(first_line("\n  \n  Reviewed ✓ \nmore"), "Reviewed ✓");
        assert_eq!(first_line(""), "");
    }
}
