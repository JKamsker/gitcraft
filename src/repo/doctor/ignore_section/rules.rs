//! Ignore-rule editing and preview presentation.

use super::*;

/// The files of rules, and the one being edited.
pub(super) fn rules_card(
    ui: &mut Ui,
    state: &mut IgnoreState,
    locked: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let unsaved = state.unsaved();
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-rules-card-91b1b197"),
        Some(crate::i18n::text(
            "repo-workflows-doctor-rules-card-379ee96f",
        )),
        |ui| {
            let Some(files) = reading(ui, &state.files) else {
                return;
            };
            let editing = state.editing.as_ref().map(|e| e.file.label.clone());
            for (i, file) in files.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(code(&file.label).color(t.text));
                    ui.label(RichText::new(file.kind).color(t.text_faint));
                    let open = editing.as_deref() == Some(file.label.as_str());
                    if !open
                        && ui
                            .add_enabled(
                                !unsaved,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-doctor-rules-card-464c4ffd",
                                )),
                            )
                            .on_disabled_hover_text(crate::i18n::text(
                                "repo-workflows-doctor-rules-card-55b528ba",
                            ))
                            .clicked()
                    {
                        acts.push(act(IgnoreAct::Edit(i)));
                    }
                });
            }
        },
    );
    if let Some(editing) = &mut state.editing {
        editor(ui, editing, locked, acts);
    }
}

pub(super) fn editor(ui: &mut Ui, e: &mut Editing, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let unsaved = e.unsaved();
    let title = if unsaved {
        crate::i18n::message(
            "repo-workflows-doctor-editor-b5bfc677",
            &[("e-file-label-1", (e.file.label).clone().into())],
        )
    } else {
        crate::i18n::message(
            "repo-workflows-doctor-editor-52448408",
            &[("e-file-label-1", (e.file.label).clone().into())],
        )
    };
    card(ui, &title, Some(e.file.kind), |ui| {
        if std::mem::take(&mut e.scroll_to) {
            ui.scroll_to_cursor(Some(egui::Align::TOP));
        }
        let Some(original) = reading(ui, &e.original).map(|o| lf(o)) else {
            return;
        };
        let edit = ui.add(
            egui::TextEdit::multiline(&mut e.text)
                .font(crate::theme::mono(12.0))
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        );
        if edit.changed() {
            e.preview = None;
            e.saved = None;
            e.confirm_save = false;
        }
        let changed = e.text != original;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    changed,
                    egui::Button::new(crate::i18n::text("repo-workflows-doctor-editor-324b134f")),
                )
                .on_disabled_hover_text(crate::i18n::text("repo-workflows-doctor-editor-a875327e"))
                .clicked()
            {
                acts.push(act(IgnoreAct::Preview));
            }
            let save_text = if e.confirm_save {
                crate::i18n::text("repo-workflows-doctor-editor-5902680c")
            } else {
                crate::i18n::text("repo-workflows-doctor-editor-1509f561")
            };
            let save = ui
                .add_enabled(
                    changed && !e.saving && !locked,
                    egui::Button::new(save_text),
                )
                .on_disabled_hover_text(if e.saving {
                    crate::i18n::text("repo-workflows-doctor-editor-23e39291")
                } else if locked {
                    crate::i18n::text("repo-workflows-doctor-editor-b5c82673")
                } else {
                    crate::i18n::text("repo-workflows-doctor-editor-0ba2a573")
                });
            if save.clicked() {
                acts.push(act(IgnoreAct::Save));
            }
            let close = if unsaved {
                crate::i18n::text("repo-workflows-doctor-editor-f9bfa3dc")
            } else {
                crate::i18n::text("repo-workflows-doctor-editor-7d9eb7ac")
            };
            if ui.button(close).clicked() {
                acts.push(act(IgnoreAct::CloseEditor));
            }
            if e.saving {
                ui.add(crate::spinner::Spinner::new());
            }
        });
        if e.confirm_save {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-doctor-editor-1ee81c8e",
                    &[("e-file-label-1", (e.file.label).clone().into())],
                ))
                .color(t.warning),
            );
        }
        match &e.saved {
            Some(Ok(())) => {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-doctor-editor-fca046cc"))
                        .color(t.success),
                );
            }
            Some(Err(err)) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(err).color(t.danger));
                    if ui
                        .button(crate::i18n::text("repo-workflows-doctor-editor-6c1adba2"))
                        .clicked()
                    {
                        acts.push(act(IgnoreAct::Reread));
                    }
                });
            }
            None => {}
        }
        if let Some(read) = &e.preview {
            ui.add_space(6.0);
            if let Some(p) = reading(ui, read) {
                preview(ui, p);
            }
        }
    });
}

pub(super) fn preview(ui: &mut Ui, p: &Preview) {
    let t = Tokens::get(ui.ctx());
    faint(
        ui,
        crate::i18n::text("repo-workflows-doctor-preview-cd2605b4"),
    );
    let list = |ui: &mut Ui, title: &str, paths: &[String], color| {
        if paths.is_empty() {
            return;
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("{title} ({})", crate::i18n::number(paths.len())))
                .font(crate::theme::semibold(12.5))
                .color(color),
        );
        for path in paths.iter().take(PREVIEW_SHOWN) {
            ui.label(code(path).color(t.text_dim));
        }
        if paths.len() > PREVIEW_SHOWN {
            faint(
                ui,
                crate::i18n::message(
                    "repo-workflows-doctor-preview-683768ab",
                    &[(
                        "paths-len-preview-shown-1",
                        (paths.len() - PREVIEW_SHOWN).into(),
                    )],
                ),
            );
        }
    };
    if p.newly_ignored.is_empty() && p.no_longer_ignored.is_empty() {
        faint(
            ui,
            crate::i18n::text("repo-workflows-doctor-preview-bbb0c0cc"),
        );
    }
    list(
        ui,
        crate::i18n::text("repo-workflows-doctor-preview-63666b35"),
        &p.newly_ignored,
        t.danger,
    );
    list(
        ui,
        crate::i18n::text("repo-workflows-doctor-preview-0f241c6b"),
        &p.no_longer_ignored,
        t.success,
    );
    list(
        ui,
        crate::i18n::text("repo-workflows-doctor-preview-8c8b9b2a"),
        &p.tracked_matching,
        t.warning,
    );
}
