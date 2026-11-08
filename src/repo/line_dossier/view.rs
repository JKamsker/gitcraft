use super::*;
fn clip(text: &str, n: usize) -> String {
    let mut value = text.chars().take(n).collect::<String>();
    if text.chars().nth(n).is_some() {
        value.push('…');
    }
    value
}
impl RepoTab {
    pub(in crate::repo) fn line_dossier_view(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let t = crate::theme::Tokens::get(ui.ctx());
        let mut trace = None;
        let mut append = None;
        let mut cancel = false;
        let mut prepare_ignore = None;
        ui.horizontal_wrapped(|ui| {
            ui.heading(crate::i18n::text(
                "repo-workflows-line_dossier-line-dossier-view-74b10c62",
            ));
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-line_dossier-line-dossier-view-9f405685",
                ))
                .clicked()
            {
                acts.push(Action::CloseLineDossier);
            }
            ui.label(crate::i18n::text(
                "repo-workflows-line_dossier-line-dossier-view-f420ab1f",
            ));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-line_dossier-line-dossier-view-82a9c46f",
            ));
            ui.add(
                egui::TextEdit::singleline(&mut self.dossier.revision)
                    .desired_width(160.0)
                    .char_limit(64),
            );
            ui.label(crate::i18n::text(
                "repo-workflows-line_dossier-line-dossier-view-62fa5a5b",
            ));
            ui.add(
                egui::TextEdit::singleline(&mut self.dossier.path)
                    .desired_width(240.0)
                    .char_limit(4096),
            );
            ui.label(crate::i18n::text(
                "repo-workflows-line_dossier-line-dossier-view-d7852cd0",
            ));
            ui.add(egui::DragValue::new(&mut self.dossier.line).range(1..=20000));
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-line_dossier-line-dossier-view-aa686e13",
                ))
                .clicked()
            {
                trace = Some(Seed {
                    commit: self.dossier.revision.clone(),
                    path: self.dossier.path.clone(),
                    line: self.dossier.line,
                });
            }
            if self.dossier.reader.busy() {
                ui.add(crate::spinner::Spinner::new());
                cancel = ui
                    .button(crate::i18n::text(
                        "repo-workflows-line_dossier-line-dossier-view-37a046ba",
                    ))
                    .clicked();
            }
        });
        ui.small(crate::i18n::text(
            "repo-workflows-line_dossier-line-dossier-view-f31d5024",
        ));
        if let Some(error) = &self.dossier.error {
            ui.colored_label(t.danger, clip(error, 2000));
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.dossier.result.is_some()
                        && !self.dossier.summary_reader.busy()
                        && crate::settings::Settings::get(ui.ctx()).ai.tool
                            != crate::ai::AiTool::Off,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-line_dossier-line-dossier-view-60cc9462",
                    )),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-line_dossier-line-dossier-view-2de6a99b",
                ))
                .clicked()
            {
                self.summarize_dossier(ui.ctx());
            }
            if self.dossier.summary_reader.busy() {
                ui.add(crate::spinner::Spinner::new());
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-line_dossier-line-dossier-view-0ac12d95",
                    ))
                    .clicked()
                {
                    self.dossier.summary_reader.cancel();
                }
            }
        });
        if let Some(summary) = &self.dossier.summary {
            match summary {
                Ok(text) => {
                    ui.collapsing(
                        crate::i18n::text("repo-workflows-line_dossier-line-dossier-view-55b46438"),
                        |ui| {
                            ui.label(clip(text, 32000));
                        },
                    );
                }
                Err(error) => {
                    ui.colored_label(t.warning, error);
                }
            }
        }
        if self.dossier.ignore.is_some() {
            self.dossier_ignore_view(ui);
            return;
        }
        if let Some(result) = &self.dossier.result {
            ui.small(crate::i18n::message(
                "repo-workflows-line_dossier-line-dossier-view-054bd44a",
                &[
                    (
                        "super-super-short-result-start-c-1",
                        (super::super::short(&result.start.commit)).clone().into(),
                    ),
                    (
                        "clip-result-start-path-120-2",
                        (clip(&result.start.path, 120)).clone().into(),
                    ),
                    ("result-start-line-3", (result.start.line).clone().into()),
                    ("result-cards-len-4", (result.cards.len()).clone().into()),
                ],
            ));
            ui.horizontal_wrapped(|ui| {
                if let Some(seed) = &result.continuation
                    && ui
                        .add_enabled(
                            !self.dossier.reader.busy(),
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-line_dossier-line-dossier-view-5853cee3",
                            )),
                        )
                        .clicked()
                {
                    append = Some(seed.clone());
                }
                for (i, seed) in result.alternatives.iter().enumerate() {
                    if ui
                        .add_enabled(
                            !self.dossier.reader.busy(),
                            egui::Button::new(crate::i18n::message(
                                "repo-workflows-line_dossier-line-dossier-view-ab2f98d8",
                                &[
                                    ("i-1-1", (i + 1).clone().into()),
                                    (
                                        "super-super-short-seed-commit-2",
                                        (super::super::short(&seed.commit)).clone().into(),
                                    ),
                                ],
                            )),
                        )
                        .clicked()
                    {
                        append = Some(seed.clone());
                    }
                }
            });
            ui.label(clip(&result.stopped, 1000));
            ui.separator();
            ui.columns(2, |columns| {
                egui::ScrollArea::vertical()
                    .id_salt("dossier_timeline")
                    .max_height(columns[0].available_height())
                    .show_rows(&mut columns[0], 30.0, result.cards.len(), |ui, range| {
                        for i in range {
                            let card = &result.cards[i];
                            if ui
                                .selectable_label(
                                    self.dossier.selected == i,
                                    format!(
                                        "[C{}] {} · {}:{}",
                                        i + 1,
                                        super::super::short(&card.at.commit),
                                        clip(&card.at.path, 55),
                                        card.at.line
                                    ),
                                )
                                .clicked()
                            {
                                self.dossier.selected = i;
                            }
                        }
                    });
                let ui = &mut columns[1];
                if let Some(card) = result.cards.get(self.dossier.selected) {
                    ui.heading(format!(
                        "[C{}] {}",
                        self.dossier.selected + 1,
                        super::super::short(&card.at.commit)
                    ));
                    ui.label(format!(
                        "{} <{}> · {}",
                        clip(&card.author, 120),
                        clip(&card.email, 160),
                        crate::git::relative_time(card.time, crate::git::now())
                    ));
                    ui.label(clip(&card.subject, 500));
                    ui.label(format!("{}:{}", clip(&card.at.path, 300), card.at.line));
                    egui::ScrollArea::both()
                        .id_salt("dossier_line")
                        .max_height(180.0)
                        .show(ui, |ui| {
                            ui.monospace(clip(&card.text, 4000));
                        });
                    ui.small(clip(&card.note, 1000));
                    ui.small(crate::i18n::message(
                        "repo-workflows-line_dossier-line-dossier-view-9f68f322",
                        &[
                            (
                                "super-super-short-card-attributi-1",
                                (super::super::short(&card.attribution.commit))
                                    .clone()
                                    .into(),
                            ),
                            (
                                "clip-card-attribution-path-200-2",
                                (clip(&card.attribution.path, 200)).clone().into(),
                            ),
                            (
                                "card-attribution-line-3",
                                (card.attribution.line).clone().into(),
                            ),
                        ],
                    ));
                    if card.whitespace_only {
                        ui.label(crate::i18n::text(
                            "repo-workflows-line_dossier-line-dossier-view-9a2e3782",
                        ));
                        if ui
                            .add_enabled(
                                !self.dossier.reader.busy() && self.busy_op.is_none(),
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-line_dossier-line-dossier-view-efa2521b",
                                )),
                            )
                            .clicked()
                        {
                            prepare_ignore = Some(card.at.commit.clone());
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-line_dossier-line-dossier-view-6fe17b05",
                            ))
                            .clicked()
                        {
                            acts.push(Action::CloseLineDossier);
                            acts.push(Action::SelectHash(card.at.commit.clone()));
                        }
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-line_dossier-line-dossier-view-562e7c08",
                            ))
                            .clicked()
                        {
                            acts.push(Action::CloseLineDossier);
                            acts.push(Action::OpenPinnedFileLine(card.at.clone()));
                        }
                    });
                }
            });
        } else if !self.dossier.reader.busy() {
            ui.label(crate::i18n::text(
                "repo-workflows-line_dossier-line-dossier-view-eeeadb1d",
            ));
        }
        ui.separator();
        ui.heading(crate::i18n::text(
            "repo-workflows-line_dossier-line-dossier-view-2a0caf4a",
        ));
        if self.dossier.forge_reader.busy() {
            ui.add(crate::spinner::Spinner::new());
            ui.label(crate::i18n::text(
                "repo-workflows-line_dossier-line-dossier-view-d419751a",
            ));
        }
        if let Some(card) = self
            .dossier
            .result
            .as_ref()
            .and_then(|r| r.cards.get(self.dossier.selected))
            && self.dossier.evidence_for.as_ref() == Some(&card.at)
            && let Some(evidence) = &self.dossier.evidence
        {
            match evidence {
                Err(error) => {
                    ui.colored_label(t.warning, clip(error, 1000));
                }
                Ok(evidence) => {
                    egui::ScrollArea::vertical().id_salt("dossier-evidence").max_height(280.0).show(ui,|ui|{
                        for note in &evidence.notes{ui.label(note);}
                        if evidence.pulls.is_empty(){ui.label(crate::i18n::text("repo-workflows-line_dossier-line-dossier-view-bc45a9dc"));}
                        for pull in &evidence.pulls {ui.group(|ui| {
                            ui.hyperlink_to(format!("#{} {} ({})",pull.number,clip(&pull.title,250),pull.state),&pull.url);
                            ui.label(&pull.association);
                            for (label,url) in &pull.issues{ui.hyperlink_to(crate::i18n::message("repo-workflows-line_dossier-line-dossier-view-afd58bc7", &[("label", (label).clone().into())]),url);}
                            for comment in &pull.comments{ui.label(format!("{}: {}",comment.author,comment.body));ui.small(&comment.proof);if !comment.url.is_empty(){ui.hyperlink_to(crate::i18n::text("repo-workflows-line_dossier-line-dossier-view-6104eb29"),&comment.url);}}
                            ui.small(crate::i18n::message("repo-workflows-line_dossier-line-dossier-view-ad6fe29b", &[("pull-checked-1", (pull.checked).clone().into()), ("pull-unmapped-2", (pull.unmapped).clone().into())]));
                            for note in &pull.notes{ui.label(note);}if pull.limited||evidence.limited{ui.label(crate::i18n::text("repo-workflows-line_dossier-line-dossier-view-a17a38a1"));}
                        });}
                    });
                }
            }
        }
        if cancel {
            self.dossier.reader.cancel();
            self.dossier.error = Some(
                crate::i18n::text("repo-workflows-line_dossier-line-dossier-view-a8198076").into(),
            );
        }
        if let Some(commit) = prepare_ignore {
            self.prepare_dossier_ignore(commit, ui.ctx());
        }
        if let Some(seed) = trace {
            self.trace_line_dossier(seed, false, ui.ctx());
        } else if let Some(seed) = append {
            self.trace_line_dossier(seed, true, ui.ctx());
        }
    }
}

impl RepoTab {
    fn dossier_ignore_view(&mut self, ui: &mut egui::Ui) {
        let Some(preview) = self.dossier.ignore.as_ref() else {
            return;
        };
        ui.heading(crate::i18n::text(
            "repo-workflows-line_dossier-dossier-ignore-view-7f80e3ed",
        ));
        ui.monospace(&preview.prepared.commit);
        ui.label(crate::i18n::text(
            "repo-workflows-line_dossier-dossier-ignore-view-f5736f62",
        ));
        ui.checkbox(
            &mut self.dossier.ignore_reviewed,
            crate::i18n::text("repo-workflows-line_dossier-dossier-ignore-view-8dac5aad"),
        );
        let mut append = false;
        let mut back = false;
        ui.horizontal_wrapped(|ui| {
            append = ui
                .add_enabled(
                    self.dossier.ignore_reviewed && self.busy_op.is_none(),
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-line_dossier-dossier-ignore-view-b784191c",
                    )),
                )
                .clicked();
            back = ui
                .button(crate::i18n::text(
                    "repo-workflows-line_dossier-dossier-ignore-view-dbcd08f5",
                ))
                .clicked();
            ui.selectable_value(
                &mut self.dossier.show_existing,
                false,
                crate::i18n::text("repo-workflows-line_dossier-dossier-ignore-view-66c640dc"),
            );
            ui.selectable_value(
                &mut self.dossier.show_existing,
                true,
                crate::i18n::text("repo-workflows-line_dossier-dossier-ignore-view-cfd1a00e"),
            );
        });
        ui.small(crate::i18n::text(
            "repo-workflows-line_dossier-dossier-ignore-view-a29947af",
        ));
        let text = if self.dossier.show_existing {
            &preview.prepared.existing
        } else {
            &preview.prepared.patch
        };
        let lines = text.lines().collect::<Vec<_>>();
        egui::ScrollArea::both()
            .id_salt("dossier-ignore-review")
            .max_height(420.0)
            .show_rows(ui, 19.0, lines.len(), |ui, range| {
                for line in &lines[range] {
                    ui.monospace(*line);
                }
            });
        if back {
            self.dossier.ignore = None;
            self.dossier.ignore_reviewed = false;
        } else if append {
            self.append_dossier_ignore(ui.ctx());
        }
    }
}
