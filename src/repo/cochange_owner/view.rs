use super::*;

impl RepoTab {
    pub(in crate::repo) fn cochange_hints(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        if self.work.staged.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(crate::i18n::text("repo-core-0325"))
            .id_salt("cochange-hints")
            .default_open(true)
            .show(ui, |ui| {
                if self.cochanges.reader.busy() {
                    ui.horizontal(|ui| {
                        ui.add(crate::spinner::Spinner::new());
                        ui.label(crate::i18n::text("repo-core-0327"));
                    });
                }
                if let Some(error) = &self.cochanges.error {
                    ui.label(crate::i18n::message(
                        "repo-core-0328",
                        &[("error", error.to_string().into())],
                    ));
                }
                if let Some(history) = &self.cochanges.history {
                    for note in &history.notes {
                        ui.small(note);
                    }
                    ui.small(crate::i18n::message(
                        "repo-core-0329",
                        &[
                            ("count", (history.records.len()).into()),
                            ("skipped", format!("{}", history.skipped).into()),
                            (
                                "else",
                                (if history.limited {
                                    crate::i18n::text("repo-core-0330")
                                } else {
                                    ""
                                })
                                .to_string()
                                .into(),
                            ),
                        ],
                    ));
                }
                let mut dismissed = None;
                let mut shown = 0;
                for hint in &self.cochanges.hints {
                    if self
                        .cochanges
                        .dismissed
                        .contains(&(hint.source.clone(), hint.companion.clone()))
                    {
                        continue;
                    }
                    shown += 1;
                    ui.group(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(crate::i18n::message(
                                "repo-core-0331",
                                &[
                                    ("companion", hint.companion.to_string().into()),
                                    ("source", hint.source.to_string().into()),
                                    ("count", (hint.evidence.len()).into()),
                                    ("source-changes", format!("{}", hint.source_changes).into()),
                                    (
                                        "source-changes2",
                                        format!(
                                            "{}",
                                            hint.evidence.len() * 100 / hint.source_changes
                                        )
                                        .into(),
                                    ),
                                ],
                            ));
                            if ui
                                .small_button(crate::i18n::text("repo-core-0332"))
                                .clicked()
                            {
                                dismissed = Some((hint.source.clone(), hint.companion.clone()));
                            }
                        });
                        egui::CollapsingHeader::new(crate::i18n::message(
                            "repo-core-0333",
                            &[("count", (hint.evidence.len()).into())],
                        ))
                        .id_salt((&hint.source, &hint.companion))
                        .show(ui, |ui| {
                            for (hash, subject) in &hint.evidence {
                                if ui
                                    .button(format!("{} · {}", crate::repo::short(hash), subject))
                                    .on_hover_text(crate::i18n::text("repo-core-0334"))
                                    .clicked()
                                {
                                    acts.push(Action::SelectHash(hash.clone()));
                                }
                            }
                        });
                    });
                }
                if let Some(key) = dismissed {
                    self.cochanges.dismissed.insert(key);
                }
                if shown == 0 && !self.cochanges.reader.busy() {
                    ui.small(crate::i18n::text("repo-core-0335"));
                }
                ui.horizontal(|ui| {
                    if !self.cochanges.dismissed.is_empty()
                        && ui
                            .small_button(crate::i18n::text("repo-core-0336"))
                            .clicked()
                    {
                        self.cochanges.dismissed.clear();
                    }
                    if ui
                        .small_button(crate::i18n::text("repo-core-0337"))
                        .clicked()
                    {
                        self.cochanges.refresh();
                    }
                });
            });
    }
}
