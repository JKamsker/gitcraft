use super::*;
use crate::repo::{Action, dialogs::Outcome};
use crate::theme::Tokens;
use engine::{Origin, Target};
use std::time::Duration;
impl State {
    pub(in crate::repo) fn show(&mut self, ui: &mut egui::Ui, idle: bool) -> Outcome {
        ui.set_width(760.0);
        ui.heading(crate::i18n::text(
            "repo-workflows-content_recovery-show-55bda87c",
        ));
        ui.label(crate::i18n::text(
            "repo-workflows-content_recovery-show-67802d1b",
        ));
        let mut outcome = Outcome::Open;
        let changed = ui
            .add(
                egui::TextEdit::multiline(&mut self.query)
                    .desired_rows(2)
                    .hint_text(crate::i18n::text(
                        "repo-workflows-content_recovery-show-11f20956",
                    )),
            )
            .changed();
        if changed {
            self.cancel();
            self.results = None;
            self.preview = None;
            self.prepared = None;
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    idle && !self.query.trim().is_empty(),
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-content_recovery-show-4c76d94b",
                    )),
                )
                .clicked()
            {
                outcome = done(Action::ContentSearch(self.query.clone()));
            }
            if self.loading() {
                ui.add(crate::spinner::Spinner::new());
                ui.label(crate::i18n::message(
                    "repo-workflows-content_recovery-show-5ec718b0",
                    &[(
                        "self-began-map-or-0-at-at-elapse-1",
                        (self.began.map_or(0, |at| at.elapsed().as_secs()))
                            .clone()
                            .into(),
                    )],
                ));
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-content_recovery-show-30ea379c",
                    ))
                    .clicked()
                {
                    self.cancel();
                }
            }
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-content_recovery-show-7d9eb7ac",
                ))
                .clicked()
            {
                outcome = Outcome::Cancel;
            }
        });
        ui.label(
            egui::RichText::new(crate::i18n::text(
                "repo-workflows-content_recovery-show-1e53ab9e",
            ))
            .small(),
        );
        if let Some(error) = &self.error {
            ui.colored_label(
                Tokens::get(ui.ctx()).warning,
                error.chars().take(1000).collect::<String>(),
            );
        }
        let enabled = idle && !self.loading();
        if let Some(preview) = self.preview.clone() {
            ui.separator();
            ui.label(crate::i18n::message(
                "repo-workflows-content_recovery-show-c006906c",
                &[
                    ("preview-bytes-len-1", (preview.bytes.len()).clone().into()),
                    ("preview-blob-12-2", (&preview.blob[..12]).into()),
                ],
            ));
            if let Some(source) = &preview.source {
                ui.label(format!(
                    "{} · {} · {}",
                    source.path,
                    source.layer,
                    crate::git::relative_time(source.when, crate::git::now())
                ));
                ui.label(&source.subject);
            } else {
                ui.label(crate::i18n::text(
                    "repo-workflows-content_recovery-show-a6dcec6a",
                ));
            }
            if let (Some(results), Some(index)) = (&self.results, self.selected)
                && let Some(hit) = results.report.hits.get(index)
            {
                egui::CollapsingHeader::new(crate::i18n::message(
                    "repo-workflows-content_recovery-show-34ac4d52",
                    &[(
                        "hit-provenance-len-1",
                        (hit.provenance.len()).clone().into(),
                    )],
                ))
                .show(ui, |ui| {
                    for (i, source) in hit.provenance.iter().enumerate() {
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(format!(
                                    "{} · {} · {}",
                                    source.path,
                                    source.layer,
                                    crate::git::relative_time(source.when, crate::git::now())
                                )),
                            )
                            .on_hover_text(&source.subject)
                            .clicked()
                        {
                            outcome = done(Action::ContentInspect(index, Some(i)));
                        }
                    }
                });
            }
            egui::ScrollArea::vertical()
                .id_salt("content-preview")
                .max_height(260.0)
                .show_rows(
                    ui,
                    ui.text_style_height(&egui::TextStyle::Monospace),
                    preview.lines.len(),
                    |ui, rows| {
                        for i in rows {
                            ui.label(
                                egui::RichText::new(format!("{:>5} {}", i + 1, preview.lines[i]))
                                    .monospace(),
                            );
                        }
                    },
                );
            if preview.display_limited {
                ui.label(crate::i18n::text(
                    "repo-workflows-content_recovery-show-334a8ddd",
                ));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text(
                    "repo-workflows-content_recovery-show-cc6d8221",
                ));
                if ui.text_edit_singleline(&mut self.file).changed() {
                    self.prepared = None;
                }
                if ui
                    .add_enabled(
                        enabled && preview.identity.root_pin.is_some(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-content_recovery-show-da1ba0df",
                        )),
                    )
                    .clicked()
                {
                    outcome = done(Action::ContentPrepare(Target::File(self.file.clone())));
                }
            });
            ui.label(crate::i18n::text(
                "repo-workflows-content_recovery-show-8cd42b70",
            ));
            if let Some(source) = &preview.source {
                ui.horizontal(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-content_recovery-show-50b4d1c2",
                    ));
                    if ui.text_edit_singleline(&mut self.branch).changed() {
                        self.prepared = None;
                    }
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-content_recovery-show-1f3ee4b2",
                            )),
                        )
                        .clicked()
                    {
                        outcome = done(Action::ContentPrepare(Target::Branch(self.branch.clone())));
                    }
                });
                if source.origin == Origin::DroppedStash
                    && ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-content_recovery-show-c8d6eb2f",
                            )),
                        )
                        .clicked()
                {
                    outcome = done(Action::ContentPrepare(Target::Stash));
                }
            }
            if let Some(prepared) = self.prepared.clone() {
                ui.group(|ui| {
                    match &prepared.target {
                        Target::File(path) => {
                            ui.label(crate::i18n::message(
                                "repo-workflows-content_recovery-show-9cba18de",
                                &[
                                    (
                                        "prepared-preview-identity-root-p-1",
                                        (prepared
                                            .preview
                                            .identity
                                            .root_pin
                                            .as_ref()
                                            .map(|pin| pin.physical.as_str())
                                            .or(prepared.preview.identity.root.as_deref())
                                            .unwrap_or(crate::i18n::text(
                                                "repo-workflows-content_recovery-show-fbdaf564",
                                            )))
                                        .into(),
                                    ),
                                    ("path", (path).clone().into()),
                                    (
                                        "prepared-preview-bytes-len-2",
                                        (prepared.preview.bytes.len()).clone().into(),
                                    ),
                                ],
                            ));
                        }
                        Target::Branch(name) => {
                            ui.label(crate::i18n::message(
                                "repo-workflows-content_recovery-show-8d5a103e",
                                &[("name", (name).clone().into())],
                            ));
                        }
                        Target::Stash => {
                            ui.label(crate::i18n::text(
                                "repo-workflows-content_recovery-show-eff2c65f",
                            ));
                        }
                    }
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-content_recovery-show-93547d7d",
                            )),
                        )
                        .clicked()
                    {
                        outcome = done(Action::ContentPerform(prepared));
                    }
                });
            }
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-content_recovery-show-beb0df0b",
                ))
                .clicked()
            {
                self.cancel();
                self.preview = None;
                self.prepared = None;
            }
        } else if let Some(results) = &self.results {
            let report = &results.report;
            ui.separator();
            ui.label(crate::i18n::message(
                "repo-workflows-content_recovery-show-52545e10",
                &[
                    ("report-hits-len-1", (report.hits.len()).clone().into()),
                    ("report-scanned-2", (report.scanned).clone().into()),
                    ("report-bytes-3", (report.bytes).clone().into()),
                    ("report-binary-4", (report.binary).clone().into()),
                    ("report-large-5", (report.large).clone().into()),
                    ("report-missing-6", (report.missing).clone().into()),
                ],
            ));
            if report.limited {
                ui.colored_label(
                    Tokens::get(ui.ctx()).warning,
                    crate::i18n::text("repo-workflows-content_recovery-show-8ef50fb8"),
                );
            }
            if report.shallow {
                ui.label(crate::i18n::text(
                    "repo-workflows-content_recovery-show-1762ea22",
                ));
            }
            for note in &report.notes {
                ui.label(note);
            }
            ui.label(crate::i18n::text(
                "repo-workflows-content_recovery-show-abadbc35",
            ));
            egui::ScrollArea::vertical()
                .id_salt("content-matches")
                .max_height(420.0)
                .show_rows(ui, 80.0, report.hits.len(), |ui, rows| {
                    for i in rows {
                        let hit = &report.hits[i];
                        let source = hit.provenance.first();
                        let date = hit
                            .when()
                            .map(|when| crate::git::relative_time(when, crate::git::now()))
                            .unwrap_or_else(|| {
                                crate::i18n::text("repo-workflows-content_recovery-show-6f803fc9")
                                    .into()
                            });
                        let title = source
                            .map(|source| {
                                format!("{} · {} · {}", date, source.path, origin(&source.origin))
                            })
                            .unwrap_or_else(|| {
                                crate::i18n::message(
                                    "repo-workflows-content_recovery-show-a963e860",
                                    &[("date", (date).clone().into())],
                                )
                            });
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(title.chars().take(130).collect::<String>())
                                    .truncate(),
                            )
                            .on_hover_text(&title)
                            .clicked()
                        {
                            outcome = done(Action::ContentInspect(i, source.map(|_| 0)));
                        }
                        let snippet: String = hit
                            .excerpt
                            .chars()
                            .take(140)
                            .map(|c| if c.is_control() { ' ' } else { c })
                            .collect();
                        ui.add(
                            egui::Label::new(egui::RichText::new(snippet).monospace()).truncate(),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "line {} · {} matches{} · {} bytes",
                                hit.line,
                                hit.occurrences.min(1000),
                                if hit.occurrences > 1000 { "+" } else { "" },
                                hit.size
                            ))
                            .small(),
                        );
                    }
                });
        }
        if self.loading() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        outcome
    }
}
fn origin(origin: &Origin) -> &'static str {
    match origin {
        Origin::History => crate::i18n::text("repo-workflows-content_recovery-origin-53bdf21a"),
        Origin::Unreachable => crate::i18n::text("repo-workflows-content_recovery-origin-d8ee810d"),
        Origin::DroppedStash => {
            crate::i18n::text("repo-workflows-content_recovery-origin-ebcaaeb6")
        }
        Origin::LiveStash => crate::i18n::text("repo-workflows-content_recovery-origin-856aca6d"),
    }
}

fn done(action: Action) -> Outcome {
    Outcome::Done(Box::new(action))
}
