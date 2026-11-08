use crate::git::drafts::{Draft, RestoreHunk, RestorePreview};
use crate::repo::Action;
use crate::repo::dialogs::Outcome;
use crate::theme::Tokens;
use egui::Ui;
use std::collections::BTreeSet;

pub(in crate::repo) fn history(
    ui: &mut Ui,
    on: &mut bool,
    versions: &[Draft],
    error: Option<&str>,
    index: &mut bool,
    idle: bool,
    undo: bool,
) -> Outcome {
    ui.set_width(700.0);
    ui.heading(crate::i18n::text(
        "repo-workflows-working_drafts-history-d0ea2534",
    ));
    let t = Tokens::get(ui.ctx());
    let mut outcome = Outcome::Open;
    if ui
        .add_enabled(
            idle && (*on || error.is_none()),
            egui::Checkbox::new(
                on,
                crate::i18n::text("repo-workflows-working_drafts-history-210ff933"),
            ),
        )
        .changed()
    {
        return done(Action::DraftEnable(*on));
    }
    ui.label(crate::i18n::text(
        "repo-workflows-working_drafts-history-65b35d4c",
    ));
    ui.label(
        egui::RichText::new(crate::i18n::text(
            "repo-workflows-working_drafts-history-ffdc452f",
        ))
        .small()
        .color(t.text_dim),
    );
    if let Some(error) = error {
        ui.colored_label(t.warning, error.chars().take(500).collect::<String>())
            .on_hover_text(error);
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                idle && *on,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-working_drafts-history-a2a28532",
                )),
            )
            .clicked()
        {
            outcome = done(Action::DraftSnapshot);
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-working_drafts-history-0e916101",
            ))
            .clicked()
        {
            outcome = done(Action::EarlierVersions);
        }
        if ui
            .add_enabled(
                idle && undo,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-working_drafts-history-2f1765a5",
                )),
            )
            .clicked()
        {
            outcome = done(Action::DraftUndo);
        }
    });
    ui.separator();
    ui.checkbox(
        index,
        crate::i18n::text("repo-workflows-working_drafts-history-171888ea"),
    );
    ui.label(crate::i18n::text(
        "repo-workflows-working_drafts-history-5e6c0d68",
    ));
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .show(ui, |ui| {
            if versions.is_empty() {
                ui.label(crate::i18n::text(
                    "repo-workflows-working_drafts-history-51423780",
                ));
            }
            for draft in versions {
                let title = crate::i18n::message(
                    "repo-workflows-draft-history-title",
                    &[
                        (
                            "when",
                            (crate::git::relative_time(draft.metadata.created, crate::git::now()))
                                .into(),
                        ),
                        ("reason", (draft_reason(&draft.metadata.reason)).into()),
                        ("count", (draft.metadata.files.len()).into()),
                    ],
                );
                egui::CollapsingHeader::new(title)
                    .id_salt(&draft.id.reference)
                    .show(ui, |ui| {
                        ui.label(format!(
                            "{} · {}",
                            draft
                                .metadata
                                .branch
                                .as_deref()
                                .unwrap_or("detached/unborn"),
                            draft
                                .metadata
                                .head
                                .as_deref()
                                .unwrap_or(crate::i18n::text(
                                    "repo-workflows-working_drafts-history-9d0c13d9"
                                ))
                                .chars()
                                .take(12)
                                .collect::<String>()
                        ));
                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .id_salt((&draft.id.reference, "files"))
                            .show_rows(ui, 26.0, draft.metadata.files.len(), |ui, rows| {
                                for file in &draft.metadata.files[rows] {
                                    let mut label: String = file.path.chars().take(75).collect();
                                    if file.untracked {
                                        label.push_str(crate::i18n::text(
                                            "repo-workflows-working_drafts-history-36ceae04",
                                        ));
                                    }
                                    if ui
                                        .add_enabled(idle, egui::Button::new(label))
                                        .on_hover_text(&file.path)
                                        .clicked()
                                    {
                                        outcome = done(Action::DraftPreview {
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
    if ui
        .button(crate::i18n::text(
            "repo-workflows-working_drafts-history-7d9eb7ac",
        ))
        .clicked()
    {
        outcome = Outcome::Cancel;
    }
    outcome
}
fn hunk(ui: &mut Ui, on: &mut bool, hunk: &RestoreHunk) {
    ui.checkbox(on, &hunk.label);
}
pub(in crate::repo) fn recovery(
    ui: &mut Ui,
    preview: &RestorePreview,
    selected: &mut BTreeSet<usize>,
    idle: bool,
) -> Outcome {
    ui.set_width(760.0);
    ui.heading(crate::i18n::message(
        "repo-workflows-working_drafts-recovery-86201db4",
        &[("preview-path-1", (preview.path).clone().into())],
    ));
    ui.label(crate::i18n::message(
        "repo-workflows-working_drafts-recovery-eb7ee658",
        &[
            ("preview-before-len-1", (preview.before_len).clone().into()),
            (
                "if-preview-index-layer-staged-ve-2",
                (if preview.index_layer {
                    crate::i18n::text("repo-workflows-working_drafts-recovery-87ca587d")
                } else {
                    crate::i18n::text("repo-workflows-working_drafts-recovery-1d540f7f")
                })
                .clone()
                .into(),
            ),
            ("preview-saved-len-3", (preview.saved_len).clone().into()),
        ],
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-working_drafts-recovery-3ef7c934",
    ));
    if preview.binary {
        ui.label(crate::i18n::text(
            "repo-workflows-working_drafts-recovery-33fc31a9",
        ));
    }
    if preview.hunks.is_empty() {
        ui.label(crate::i18n::text(
            "repo-workflows-working_drafts-recovery-092db2a8",
        ));
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
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-working_drafts-recovery-ec7046f0",
                )),
            )
            .clicked()
        {
            outcome = done(Action::DraftRecover {
                preview: Box::new(preview.clone()),
                hunks: selected.iter().copied().collect(),
            });
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-working_drafts-recovery-76900f1b",
            ))
            .clicked()
        {
            outcome = done(Action::EarlierVersions);
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-working_drafts-recovery-19766ed6",
            ))
            .clicked()
        {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

fn done(action: Action) -> Outcome {
    Outcome::Done(Box::new(action))
}

fn draft_reason(raw: &str) -> &str {
    match raw {
        "Pause" => crate::i18n::text("repo-workflows-draft-reason-pause"),
        "Saved manually" => crate::i18n::text("repo-workflows-draft-reason-manual"),
        other => other,
    }
}
