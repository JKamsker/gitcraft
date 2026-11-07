use super::Outcome;
use crate::git::drafts::{Draft, RestoreHunk, RestorePreview};
use crate::repo::Action;
use crate::theme::Tokens;
use egui::Ui;
use std::collections::BTreeSet;

pub(super) fn history(
    ui: &mut Ui,
    on: &mut bool,
    versions: &[Draft],
    error: Option<&str>,
    index: &mut bool,
    idle: bool,
    undo: bool,
) -> Outcome {
    ui.set_width(700.0);
    ui.heading("Earlier versions of working changes");
    let t = Tokens::get(ui.ctx());
    let mut outcome = Outcome::Open;
    if ui
        .add_enabled(
            idle && (*on || error.is_none()),
            egui::Checkbox::new(on, "Automatically save working drafts (opt in)"),
        )
        .changed()
    {
        return Outcome::Done(Action::DraftEnable(*on));
    }
    ui.label("Save after 15 seconds of quiet and before checkout/rebase. Includes tracked changes and nonignored untracked regular files; ignored files and submodule contents stay out. Changed links/conflicts/special files require a manual backup.");
    ui.label(egui::RichText::new("Index and working contents are saved separately, locally under protected tree refs. Normal pushes skip them; mirror pushes or manual all-ref exports can include drafts. Keep up to 50 versions for 14 days. Limits: 8 MiB/file, 32 MiB changed content, 2000 changed/20000 index paths. Git must support --no-lazy-fetch.").small().color(t.text_dim));
    if let Some(error) = error {
        ui.colored_label(t.warning, error.chars().take(500).collect::<String>())
            .on_hover_text(error);
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(idle && *on, egui::Button::new("Save version now"))
            .clicked()
        {
            outcome = Outcome::Done(Action::DraftSnapshot);
        }
        if ui.button("Refresh").clicked() {
            outcome = Outcome::Done(Action::EarlierVersions);
        }
        if ui
            .add_enabled(idle && undo, egui::Button::new("Undo last recovery…"))
            .clicked()
        {
            outcome = Outcome::Done(Action::DraftUndo);
        }
    });
    ui.separator();
    ui.checkbox(
        index,
        "Preview the saved staged/index contents (otherwise the working version)",
    );
    ui.label("Staged versions contain raw Git blobs (including LFS pointers). Recovery does not run filters or line-ending conversions; working versions contain the saved on-disk bytes.");
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .show(ui, |ui| {
            if versions.is_empty() {
                ui.label("No saved versions. Enabling drafts starts saving after the next pause; a clean tree has nothing to save.");
            }
            for draft in versions {
                let title = format!(
                    "{} · {} · {} files",
                    crate::git::relative_time(draft.metadata.created, crate::git::now()),
                    draft.metadata.reason,
                    draft.metadata.files.len()
                );
                egui::CollapsingHeader::new(title)
                    .id_salt(&draft.id.reference)
                    .show(ui, |ui| {
                        ui.label(format!(
                            "{} · {}",
                            draft.metadata.branch.as_deref().unwrap_or("detached/unborn"),
                            draft.metadata.head.as_deref().unwrap_or("no HEAD").chars().take(12).collect::<String>()
                        ));
                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .id_salt((&draft.id.reference, "files"))
                            .show_rows(ui, 26.0, draft.metadata.files.len(), |ui, rows| {
                                for file in &draft.metadata.files[rows] {
                                    let mut label: String = file.path.chars().take(75).collect();
                                    if file.untracked {
                                        label.push_str(" (untracked)");
                                    }
                                    if ui.add_enabled(idle, egui::Button::new(label))
                                        .on_hover_text(&file.path).clicked()
                                    {
                                        outcome = Outcome::Done(Action::DraftPreview {
                                            id: draft.id.clone(),
                                            path: file.path.clone(),
                                            index: *index,
                                        });
                                    }
                                }
                            });
                    });
            }
        });
    if ui.button("Close").clicked() {
        outcome = Outcome::Cancel;
    }
    outcome
}
fn hunk(ui: &mut Ui, on: &mut bool, hunk: &RestoreHunk) {
    ui.checkbox(on, &hunk.label);
}
pub(super) fn recovery(
    ui: &mut Ui,
    preview: &RestorePreview,
    selected: &mut BTreeSet<usize>,
    idle: bool,
) -> Outcome {
    ui.set_width(760.0);
    ui.heading(format!("Recover {}", preview.path));
    ui.label(format!(
        "Current: {} bytes · saved {}: {} bytes",
        preview.before_len,
        if preview.index_layer {
            "staged version"
        } else {
            "working version"
        },
        preview.saved_len
    ));
    ui.label("Choose the hunks to put back. Other working edits and the actual index stay as they are. Recovery first saves a safety version, even when automatic drafts are off; Undo recovery opens that version.");
    if preview.binary {
        ui.label("Binary files are recovered as a whole.");
    }
    if preview.hunks.is_empty() {
        ui.label("No content changes to recover. Mode-only changes are read-only here.");
    }
    egui::ScrollArea::vertical()
        .max_height(120.0)
        .id_salt("draft_hunks")
        .show(ui, |ui| {
            for (i, item) in preview.hunks.iter().enumerate() {
                let mut on = selected.contains(&i);
                hunk(ui, &mut on, item);
                if on {
                    selected.insert(i);
                } else {
                    selected.remove(&i);
                }
            }
        });
    egui::ScrollArea::both()
        .max_height(320.0)
        .id_salt("draft_patch")
        .show_rows(ui, 18.0, preview.lines.len(), |ui, rows| {
            for line in &preview.lines[rows] {
                ui.monospace(line);
            }
        });
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                idle && !selected.is_empty(),
                egui::Button::new("Back up current, then recover selected"),
            )
            .clicked()
        {
            outcome = Outcome::Done(Action::DraftRecover {
                preview: Box::new(preview.clone()),
                hunks: selected.iter().copied().collect(),
            });
        }
        if ui.button("Back").clicked() {
            outcome = Outcome::Done(Action::EarlierVersions);
        }
        if ui.button("Cancel").clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}
