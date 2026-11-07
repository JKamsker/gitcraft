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
    let current = notes.namespace.clone().unwrap_or_default();
    let writing = notes.draft.is_some();
    let button = ui
        .add_enabled_ui(!writing, |ui| {
            crate::icons::small_button(ui, "tag", &format!("Notes namespace: {current}"))
        })
        .inner
        .on_disabled_hover_text("Save or cancel the note first");
    egui::Popup::menu(&button)
        .style(crate::theme::menu_style)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width((ui.ctx().content_rect().width() - 16.0).clamp(1.0, 240.0))
        .show(|ui| {
            ui.label("Notes namespace");
            // Reserve room for the creation controls, even in short scaled viewports.
            // Only the existing namespaces scroll; typing a new one stays in view.
            egui::ScrollArea::vertical()
                .id_salt("notes_namespaces")
                .max_height((ui.ctx().content_rect().height() - 160.0).clamp(1.0, 240.0))
                .show(ui, |ui| {
                    for name in &notes.namespaces {
                        if ui.selectable_label(*name == current, name).clicked() {
                            if *name != current {
                                cmds.push(ExtrasCmd::PickNamespace(name.clone()));
                            }
                            ui.close();
                        }
                    }
                });
            ui.separator();
            ui.label("New namespace");
            let field = ui.add(
                egui::TextEdit::singleline(&mut notes.new_namespace)
                    .hint_text("new namespace")
                    .desired_width(f32::INFINITY),
            );
            let typed = !notes.new_namespace.trim().is_empty();
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .add_enabled(typed, egui::Button::new("Use namespace"))
                .clicked()
                || (enter && typed)
            {
                cmds.push(ExtrasCmd::PickNamespace(notes.new_namespace.clone()));
                ui.close();
            }
        });
}

/// The note shown, with Edit and Delete; Add note when there is none.
fn note(ui: &mut Ui, notes: &mut Notes, idle: bool, cmds: &mut Vec<ExtrasCmd>) {
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
            ui.horizontal_wrapped(|ui| {
                if notes.confirm_delete {
                    ui.label(RichText::new("Delete this note?").color(t.danger));
                    if ui
                        .add_enabled(!busy && idle, egui::Button::new("Delete"))
                        .clicked()
                    {
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
    idle: bool,
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
    // Escape in the editor (egui has taken the focus away for it) cancels it, and goes no
    // further: the diff or search behind stays open.
    let cancel_keys = edit.lost_focus()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let mut keep = !cancel_keys;
    ui.horizontal_wrapped(|ui| {
        // Not while an operation runs: a notes fetch or push would hold the same ref.
        let can_save = !saving && idle && !draft.trim().is_empty();
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
                RichText::new(format!(
                    "{}+Enter saves · Esc cancels",
                    crate::manual::command()
                ))
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
        egui::ComboBox::from_id_salt("notes_remote")
            .selected_text(&remote)
            .width((ui.available_width() - 60.0).clamp(60.0, 140.0))
            .show_ui(ui, |ui| {
                for name in remotes {
                    if ui.selectable_label(*name == remote, name).clicked() {
                        notes.remote = Some(name.clone());
                    }
                }
            });
        let fetch = ui
            .add_enabled_ui(idle, |ui| crate::icons::small_button(ui, "cloud-download", "Fetch notes"))
            .inner
            .on_hover_text(format!(
                "Gets the notes in “{namespace}” from {remote}. If you have notes here that {remote} doesn't, nothing is changed and the fetch stops: your notes are kept."
            ))
            .on_disabled_hover_text("Wait for the operation running to end");
        if fetch.clicked() {
            cmds.push(ExtrasCmd::FetchNotes(remote.clone()));
        }
        let has_notes = notes.existing.contains(&namespace);
        let push = ui
            .add_enabled_ui(idle && has_notes, |ui| crate::icons::small_button(ui, "arrow-up-from-line", "Push notes"))
            .inner
            .on_hover_text(format!(
                "Sends your notes in “{namespace}” to {remote}. If {remote} has notes you don't, the push is refused."
            ))
            .on_disabled_hover_text(if has_notes {
                "Wait for the operation running to end".to_owned()
            } else {
                format!("No notes in “{namespace}” to push yet")
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

#[cfg(test)]
mod responsive_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn responsive_notes_keeps_namespace_and_draft_editing_out_of_the_form() {
        for width in [240.0, 420.0, 800.0] {
            for scale in [1.0, 1.25, 1.5] {
                let mut styled = false;
                let notes = Notes {
                    namespace: Some("commits".into()),
                    namespaces: vec!["commits".into(), "review".into()],
                    note: Some(Ok(Some("Review findings\nMore detail".into()))),
                    remote: Some("fork".into()),
                    existing: vec!["commits".into()],
                    ..Default::default()
                };
                let mut h = Harness::builder()
                    .with_size([width / scale, 550.0])
                    .with_pixels_per_point(scale)
                    .build_ui_state(
                        move |ui, (notes, cmds): &mut (Notes, Vec<ExtrasCmd>)| {
                            if !styled {
                                crate::theme::install_fonts(ui.ctx());
                                crate::theme::apply(
                                    ui.ctx(),
                                    &crate::theme::ThemeId::default(),
                                    &crate::theme::Tweaks::default(),
                                );
                                egui_extras::install_image_loaders(ui.ctx());
                                styled = true;
                                return;
                            }
                            egui::CentralPanel::default().show(ui, |ui| {
                                section(ui, notes, &["origin".into(), "fork".into()], true, cmds);
                            });
                        },
                        (notes, Vec::new()),
                    );
                for _ in 0..4 {
                    h.step();
                }
                assert!(h.query_by_label("Namespace").is_none());
                assert!(h.query_by_label("Use").is_none());
                h.get_by_label("Notes namespace: commits").click();
                h.step();
                assert!(h.query_by_label("review").is_some());
                h.key_press(egui::Key::Escape);
                h.step();
                h.get_by_label("Edit").click();
                h.step();
                assert_eq!(
                    h.state().0.draft.as_deref(),
                    Some("Review findings\nMore detail")
                );
                h.get_by_label("Notes namespace: commits").click();
                h.step();
                assert!(h.state().1.is_empty(), "writing changed namespace");
                crate::repo::detail::responsive_tests::evidence(
                    &mut h,
                    &format!("notes-{width}-{scale}"),
                );
                h.get_by_label("Save note").click();
                h.step();
                assert!(h.state().1.iter().any(
                    |c| matches!(c, ExtrasCmd::SaveNote(t) if t == "Review findings\nMore detail")
                ));
            }
        }
    }
}

#[cfg(test)]
mod followup_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn harness(
        width: f32,
        height: f32,
        scale: f32,
        many: bool,
    ) -> Harness<'static, (Notes, Vec<ExtrasCmd>)> {
        let notes = Notes {
            namespace: Some("commits".into()),
            namespaces: if many {
                (0..20)
                    .map(|i| format!("review/team-platform/preproduction/release-{i:02}"))
                    .collect()
            } else {
                vec!["commits".into(), "review".into()]
            },
            note: Some(Ok(Some("Review findings".into()))),
            ..Default::default()
        };
        let mut styled = false;
        Harness::builder()
            .with_size([width / scale, height / scale])
            .with_pixels_per_point(scale)
            .build_ui_state(
                move |ui, (notes, cmds): &mut (Notes, Vec<ExtrasCmd>)| {
                    if !styled {
                        crate::theme::install_fonts(ui.ctx());
                        crate::theme::apply(
                            ui.ctx(),
                            &crate::theme::ThemeId::default(),
                            &Default::default(),
                        );
                        egui_extras::install_image_loaders(ui.ctx());
                        styled = true;
                        return;
                    }
                    egui::CentralPanel::default()
                        .show(ui, |ui| section(ui, notes, &[], true, cmds));
                },
                (notes, Vec::new()),
            )
    }

    #[test]
    fn responsive_followup_namespace_field_keeps_popup_open_for_typing_and_submission() {
        let mut h = harness(420.0, 400.0, 1.0, false);
        for _ in 0..4 {
            h.step();
        }
        h.get_by_label("Notes namespace: commits").click();
        for _ in 0..4 {
            h.step();
        }
        h.get_by_role(egui::accesskit::Role::TextInput).click();
        for _ in 0..4 {
            h.step();
        }
        assert!(
            h.query_by_label("Use namespace").is_some(),
            "clicking the namespace field dismissed the popup"
        );
        h.get_by_role(egui::accesskit::Role::TextInput)
            .type_text("release-review");
        for _ in 0..3 {
            h.step();
        }
        assert_eq!(h.state().0.new_namespace, "release-review");
        crate::repo::detail::responsive_tests::evidence(&mut h, "followup-namespace-typing");
        h.key_press(egui::Key::Enter);
        for _ in 0..3 {
            h.step();
        }
        assert!(
            h.state().1.iter().any(
                |cmd| matches!(cmd, ExtrasCmd::PickNamespace(name) if name == "release-review")
            )
        );
        assert!(h.query_by_label("Use namespace").is_none());
        h.state_mut().1.clear();
        h.get_by_label("Notes namespace: commits").click();
        for _ in 0..4 {
            h.step();
        }
        h.get_by_label("review").click();
        for _ in 0..3 {
            h.step();
        }
        assert!(
            h.state()
                .1
                .iter()
                .any(|cmd| matches!(cmd, ExtrasCmd::PickNamespace(name) if name == "review"))
        );
        assert!(
            h.query_by_label("Use namespace").is_none(),
            "selection must dismiss the picker"
        );
    }

    #[test]
    fn responsive_followup_many_namespaces_scroll_and_keep_creation_reachable() {
        for width in [360.0, 420.0, 800.0] {
            for scale in [1.0, 1.25, 1.5] {
                let height = 400.0;
                let mut h = harness(width, height, scale, true);
                for _ in 0..4 {
                    h.step();
                }
                h.get_by_label("Notes namespace: commits").click();
                for _ in 0..4 {
                    h.step();
                }
                crate::repo::detail::responsive_tests::evidence(
                    &mut h,
                    &format!("followup-namespaces-{width}-{scale}"),
                );
                let use_button = h.get_by_label("Use namespace").rect();
                assert!(
                    use_button.bottom() <= height / scale && use_button.top() >= 0.0,
                    "creation controls offscreen: {use_button:?}"
                );
                let last = "review/team-platform/preproduction/release-19";
                h.get_by_label(last).scroll_to_me();
                for _ in 0..5 {
                    h.step();
                }
                let r = h.get_by_label(last).rect();
                assert!(
                    r.top() >= 0.0 && r.bottom() < use_button.top(),
                    "last namespace unreachable: {r:?}"
                );
                h.get_by_label(last).click();
                for _ in 0..3 {
                    h.step();
                }
                assert!(
                    h.state()
                        .1
                        .iter()
                        .any(|cmd| matches!(cmd, ExtrasCmd::PickNamespace(name) if name == last))
                );
                assert!(h.query_by_label("Use namespace").is_none());
            }
        }
    }
}
