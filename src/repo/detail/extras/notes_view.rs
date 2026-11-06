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
        Some(Err(_)) => Summary::Text("Could not be read".to_owned(), t.danger),
        Some(Ok(Some(text))) => Summary::Text(first_line(text), t.text_dim),
        Some(Ok(None)) => Summary::Text(format!("none in {namespace}"), t.text_faint),
    };
    let has_note = matches!(notes.note, Some(Ok(Some(_))));
    section::section(ui, "Notes", summary, has_note, None, |ui| {
        namespace_picker(ui, notes, cmds);
        if let Some(e) = &notes.error {
            section::error(ui, e);
        }
        match notes.draft.take() {
            Some(draft) => notes.draft = editor(ui, notes.saving, draft, cmds),
            None => note(ui, notes, cmds),
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
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Namespace").color(t.text_faint));
        egui::ComboBox::from_id_salt("notes_namespace")
            .selected_text(&current)
            .show_ui(ui, |ui| {
                for name in &notes.namespaces {
                    if ui.selectable_label(*name == current, name).clicked() && *name != current {
                        cmds.push(ExtrasCmd::PickNamespace(name.clone()));
                    }
                }
            })
            .response
            .on_hover_text(format!("Notes are kept in refs/notes/{current}"));
        let field = ui.add(
            egui::TextEdit::singleline(&mut notes.new_namespace)
                .hint_text("new namespace")
                .desired_width(110.0),
        );
        let typed = !notes.new_namespace.trim().is_empty();
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.add_enabled(typed, egui::Button::new("Use")).clicked() || (enter && typed) {
            cmds.push(ExtrasCmd::PickNamespace(notes.new_namespace.clone()));
        }
    });
}

/// The note shown, with Edit and Delete; Add note when there is none.
fn note(ui: &mut Ui, notes: &mut Notes, cmds: &mut Vec<ExtrasCmd>) {
    let t = Tokens::get(ui.ctx());
    let busy = notes.saving;
    match &notes.note {
        None => section::faint(ui, "Reading the note…"),
        Some(Err(e)) => section::error(ui, e),
        Some(Ok(None)) => {
            section::faint(ui, "No note on this commit in this namespace.");
            if ui
                .add_enabled(!busy, egui::Button::new("Add note"))
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
                    ui.label(RichText::new("Delete this note?").color(t.danger));
                    if ui.add_enabled(!busy, egui::Button::new("Delete")).clicked() {
                        cmds.push(ExtrasCmd::DeleteNote);
                    }
                    if ui.button("Keep").clicked() {
                        notes.confirm_delete = false;
                    }
                } else {
                    if ui.add_enabled(!busy, egui::Button::new("Edit")).clicked() {
                        notes.draft = Some(text.trim_end().to_owned());
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Delete…"))
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
    mut draft: String,
    cmds: &mut Vec<ExtrasCmd>,
) -> Option<String> {
    let t = Tokens::get(ui.ctx());
    let edit = ui.add_enabled(
        !saving,
        egui::TextEdit::multiline(&mut draft)
            .hint_text("Findings, review context, build links…")
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
    let save_keys =
        edit.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    let mut keep = true;
    ui.horizontal(|ui| {
        let can_save = !saving && !draft.trim().is_empty();
        if widgets::primary_button(ui, "Save note", can_save, 90.0).clicked()
            || (save_keys && can_save)
        {
            cmds.push(ExtrasCmd::SaveNote(draft.clone()));
        }
        if ui
            .add_enabled(!saving, egui::Button::new("Cancel"))
            .clicked()
        {
            keep = false;
        }
        if saving {
            ui.add(crate::spinner::Spinner::new().size(12.0));
        } else {
            ui.label(
                RichText::new(format!("{}+Enter saves", crate::manual::command()))
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
        section::faint(ui, "Add a remote to fetch or push notes.");
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
        ui.label(RichText::new("Remote").color(t.text_faint));
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
            .add_enabled(idle, egui::Button::new("Fetch"))
            .on_hover_text(format!(
                "Fetch refs/notes/{namespace} from {remote}. Notes made here that {remote} doesn't have stop it rather than being replaced."
            ));
        if fetch.clicked() {
            cmds.push(ExtrasCmd::FetchNotes(remote.clone()));
        }
        let push = ui
            .add_enabled(idle, egui::Button::new("Push"))
            .on_hover_text(format!("Push refs/notes/{namespace} to {remote}"));
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
