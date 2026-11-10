use super::*;
impl RepoTab {
    pub(in crate::repo) fn review_document_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.heading(crate::i18n::text(
                "repo-workflows-review-label-review-all-changes-243201",
            ));
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-review-label-reload-bdc090",
                ))
                .clicked()
            {
                acts.push(Action::Review(ReviewAct::Reload));
            }
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-review_workflow-review-document-ui-7d9eb7ac",
                ))
                .clicked()
            {
                acts.push(Action::Review(ReviewAct::Close));
            }
        });
        if let Some(view) = &mut self.pr_view
            && let Some(draft) = self.pr_drafts.get_mut(&view.number)
            && draft.target.is_some()
        {
            egui::Panel::bottom("continuous-line-comment")
                .resizable(true)
                .show(ui, |ui| {
                    crate::repo::pr_review::line_panel(ui, view, draft, None, acts);
                });
        }
        let state = &self.review_document;
        ui.weak(crate::repo::messages::display_status(&state.identity));
        let Some(data) = &state.data else {
            ui.add(crate::spinner::Spinner::new());
            return;
        };
        let data = match data {
            Ok(data) => data,
            Err(e) => {
                ui.label(crate::repo::messages::display_status(e));
                return;
            }
        };
        let idle = self.busy_op.is_none();
        egui::ScrollArea::both()
            .id_salt(("continuous-review", state.request))
            .show(ui, |ui| {
                for (file, (key, data)) in state.keys.iter().zip(data).enumerate() {
                    egui::CollapsingHeader::new(match key {
                        DiffKey::Work(f) if f.area == Area::Staged => crate::i18n::message("repo-workflows-review-message-path-staged-3ef034", &[("path", (key.path()).into())]),
                        _ => key.path().to_owned(),
                    })
                    .id_salt(file)
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        file_intro(ui, key, data, acts);
                        let review = self.diff_review(key);
                        // Clip fixed-height rows before building widgets. Offscreen hunks do no layout work.
                        let row_height = 22.0;
                        let top = ui.cursor().top();
                        let visible = visible_rows(
                            data.lines.len(),
                            top,
                            ui.clip_rect().top(),
                            ui.clip_rect().bottom(),
                            row_height,
                        );
                        ui.add_space(visible.start as f32 * row_height);
                        for hunk in visible.clone() {
                            let line = &data.lines[hunk];
                            let row_top = ui.cursor().top();
                            if line.kind == diff::Kind::Hunk {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&line.text).monospace())
                                            .wrap_mode(egui::TextWrapMode::Extend),
                                    );
                                    for &pick in picks(key, data.partial, data.revertable) {
                                        if ui
                                            .add_enabled(idle, egui::Button::new(crate::repo::messages::display_operation(pick.verb())))
                                            .clicked()
                                        {
                                            acts.push(Action::Review(ReviewAct::Pick {
                                                request: state.request,
                                                file,
                                                hunk,
                                                pick,
                                            }));
                                        }
                                    }
                                    if ui.small_button(crate::i18n::text("repo-workflows-review-label-open-file-diff-5dcae6")).clicked() {
                                        acts.push(Action::OpenDiff(key.clone()));
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    if let Some(review) = &review
                                        && let Some((side, number)) =
                                            crate::repo::pr_review::target_of(
                                                line.kind, line.old, line.new, None,
                                            )
                                        && ui
                                            .small_button(format!("{}", number))
                                            .on_hover_text(crate::i18n::text("repo-workflows-review-label-comment-on-this-line-8ee642"))
                                            .clicked()
                                    {
                                        acts.push(Action::Pr(
                                            crate::repo::pr_review::PrAct::OpenLine(
                                                crate::repo::pr_review::LineTarget {
                                                    path: review.path.clone(),
                                                    line: number,
                                                    side,
                                                },
                                            ),
                                        ));
                                    }
                                    let color = match line.kind {
                                        diff::Kind::Add => egui::Color32::from_rgb(85, 185, 120),
                                        diff::Kind::Del => egui::Color32::from_rgb(230, 110, 110),
                                        _ => ui.visuals().text_color(),
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!(
                                                "{:>5} {:>5} {}",
                                                line.old.map(|n| n.to_string()).unwrap_or_default(),
                                                line.new.map(|n| n.to_string()).unwrap_or_default(),
                                                line.text
                                            ))
                                            .monospace()
                                            .color(color),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                    );
                                });
                            }
                            let used = ui.cursor().top() - row_top;
                            if used < row_height {
                                ui.add_space(row_height - used);
                            }
                        }
                        ui.add_space(
                            data.lines.len().saturating_sub(visible.end) as f32 * row_height,
                        );
                    });
                }
            });
    }
}
