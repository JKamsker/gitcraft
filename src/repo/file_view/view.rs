//! File pane presentation and tab actions.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn file_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &self.file_view else { return };
        if view.lines.is_some() {
            self.line_history_view(ui, acts);
            return;
        }

        self.intent_badge(ui, &view.path);
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "history", 16.0, t.icon);
                    ui.label(egui::RichText::new(&view.path).font(theme::medium(13.0)));
                    if let Some(rev) = &view.rev {
                        widgets::chip(
                            ui,
                            &crate::i18n::message(
                                "repo-core-2494",
                                &[("rev", rev.get(..8).unwrap_or(rev).to_string().into())],
                            ),
                            t.text_dim,
                        );
                    }
                    ui.add_space(12.0);
                    let tabs = [
                        (FileTab::History, crate::i18n::text("repo-core-2497")),
                        (FileTab::Blame, crate::i18n::text("repo-core-2498")),
                        (FileTab::File, crate::i18n::text("repo-core-2368")),
                    ];
                    for (tab, label) in tabs {
                        let on = view.tab == tab;
                        let b = ui.add(
                            egui::Button::new(egui::RichText::new(label).color(if on {
                                t.text
                            } else {
                                t.text_dim
                            }))
                            .frame_when_inactive(on),
                        );
                        if on {
                            widgets::active_underline(ui, b.rect, t.accent);
                        }
                        if b.clicked() && !on {
                            acts.push(Action::FileView {
                                path: view.path.clone(),
                                rev: view.rev.clone(),
                                tab,
                            });
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", crate::i18n::text("repo-core-0711"))
                            .clicked()
                        {
                            acts.push(Action::CloseFileView);
                        }
                    });
                });
            });

        // A diff beside the history has its own bar.
        if self.diff.is_none()
            && let Some(what) = self.find_in()
        {
            self.find.bar(ui, what.name());
        }
        let Some(view) = &self.file_view else { return };
        match view.tab {
            FileTab::History => self.file_history(ui, acts),
            FileTab::Blame => self.blame_lines(ui, acts),
            FileTab::File => {
                // Lines link to the commit the file is at; on disk, they may be in none.
                let rev = view.rev.clone();
                let menu = self.link_menu(None);
                let mut link = |ui: &mut Ui, lines: Lines| {
                    if let Some(rev) = &rev
                        && crate::git::investigations::oid(rev)
                        && ui.button(crate::i18n::text("repo-core-2325")).clicked()
                    {
                        acts.push(Action::OpenLineDossier(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: rev.clone(),
                                path: view.path.clone(),
                                line: lines.first as usize,
                            },
                        )));
                    }
                    if let Some(rev) = &rev
                        && crate::git::investigations::oid(rev)
                        && ui.button(crate::i18n::text("repo-core-2326")).clicked()
                    {
                        acts.push(Action::OpenSymbolBiography(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: rev.clone(),
                                path: view.path.clone(),
                                line: lines.first as usize,
                            },
                        )));
                    }
                    if let (Some(rev), Some(menu)) = (&rev, menu.clone()) {
                        let target = Target::File {
                            rev: rev.clone(),
                            path: view.path.clone(),
                            lines: Some(lines),
                        };
                        menu.items(ui, &lines.label(), &target, acts);
                    }
                };
                // Lines are picked only when there's something to link them to.
                let links = view
                    .rev
                    .is_some()
                    .then_some(&mut link as file_text::LineMenu<'_>);
                let picked = file_text::show(
                    ui,
                    ("file_view", &view.path, &view.rev),
                    view.text.as_ref(),
                    &mut self.find,
                    || Source::File {
                        path: view.path.clone(),
                        rev: view.rev.clone(),
                    },
                    Ok(()),
                    links,
                    Some((&view.goto_line, view.marked_line)),
                );
                if let Some(range) = picked {
                    let at = view
                        .rev
                        .clone()
                        .map_or(git::LinesAt::WorkTree, git::LinesAt::Rev);
                    acts.push(Action::LineHistory {
                        path: view.path.clone(),
                        at,
                        range,
                    });
                }
            }
        }
    }
}

pub(super) fn error(ui: &mut Ui, e: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.colored_label(
            t.danger,
            crate::git::display_error(&crate::forge::inbox::display_error(e)),
        );
    });
}
