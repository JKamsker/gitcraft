//! Postcard draft controls, preview, and export destination selection.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn postcard_window(&mut self, ctx: &egui::Context) {
        if !self.postcards.open {
            return;
        }
        let mut open = true;
        let mut range_changed = false;
        let mut changed = false;
        let mut load = false;
        let mut compose = false;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(crate::i18n::text(
            "repo-workflows-postcards-postcard-window-3304153d",
        ))
        .id(egui::Id::new(("release-postcard", &self.path)))
        .open(&mut open)
        .default_size([780.0, 660.0])
        .vscroll(true)
        .show(ctx, |ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-674f7ae2",
            ));
            for (label, is_base) in [
                (
                    crate::i18n::text("repo-workflows-postcards-postcard-window-2a433807"),
                    true,
                ),
                (
                    crate::i18n::text("repo-workflows-postcards-postcard-window-07ec4bb7"),
                    false,
                ),
            ] {
                ui.label(label);
                let value = if is_base {
                    &mut self.postcards.base
                } else {
                    &mut self.postcards.target
                };
                range_changed |= ui
                    .add(
                        egui::TextEdit::singleline(value)
                            .hint_text(crate::i18n::text(
                                "repo-workflows-postcards-postcard-window-5daeb684",
                            ))
                            .char_limit(64)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
                egui::ComboBox::from_id_salt(("postcard-end", is_base))
                    .selected_text(crate::i18n::text(
                        "repo-workflows-postcards-postcard-window-f4dc4046",
                    ))
                    .show_ui(ui, |ui| {
                        for branch in self.refs.tags.iter().chain(&self.refs.local) {
                            if git::postcards::oid(&branch.target)
                                && ui
                                    .selectable_label(*value == branch.target, &branch.name)
                                    .clicked()
                            {
                                *value = branch.target.clone();
                                range_changed = true;
                            }
                        }
                    });
            }
            load = ui
                .add_enabled(
                    !self.postcards.reader.busy()
                        && git::postcards::oid(&self.postcards.target)
                        && (self.postcards.base.is_empty()
                            || git::postcards::oid(&self.postcards.base)),
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-postcards-postcard-window-fdf4459d",
                    )),
                )
                .clicked();
            ui.separator();
            ui.label(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-85f01282",
            ));
            changed |= ui
                .add(egui::TextEdit::singleline(&mut self.postcards.name).char_limit(80))
                .changed();
            ui.label(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-2a22a9cd",
            ));
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.postcards.date)
                        .char_limit(32)
                        .hint_text("YYYY-MM-DD"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.postcards.include_names,
                    crate::i18n::text("repo-workflows-postcards-postcard-window-5725fbec"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.postcards.include_messages,
                    crate::i18n::text("repo-workflows-postcards-postcard-window-15a06afd"),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.postcards.anonymize,
                    crate::i18n::text("repo-workflows-postcards-postcard-window-7fc35df9"),
                )
                .changed();
            ui.small(crate::i18n::text(
                "repo-workflows-postcards-postcard-window-cc45ca72",
            ));
            if let Some(commits) = &self.postcards.commits {
                let snapshot = Snapshot::new(&self.postcards, commits);
                ui.collapsing(
                    crate::i18n::message(
                        "repo-workflows-postcards-postcard-window-77a98200",
                        &[
                            ("snapshot-messages-len-1", (snapshot.messages.len()).into()),
                            (
                                "snapshot-contributors-len-2",
                                (snapshot.contributors.len()).into(),
                            ),
                        ],
                    ),
                    |ui| {
                        ui.label(crate::i18n::message(
                            "repo-workflows-postcards-postcard-window-69241bd8",
                            &[
                                ("snapshot-name-1", (snapshot.name).clone().into()),
                                ("snapshot-date-2", (snapshot.date).clone().into()),
                            ],
                        ));
                        ui.monospace(&snapshot.range);
                        ui.label(if snapshot.contributors.is_empty() {
                            crate::i18n::text("repo-workflows-postcards-postcard-window-5db03eee")
                                .into()
                        } else {
                            crate::i18n::message(
                                "repo-workflows-postcards-postcard-window-3772a00f",
                                &[(
                                    "snapshot-contributors-join-1",
                                    (snapshot.contributors.join(", ")).clone().into(),
                                )],
                            )
                        });
                        egui::ScrollArea::vertical()
                            .id_salt("postcard-disclosures")
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for message in &snapshot.messages {
                                    ui.label(message);
                                }
                            });
                    },
                );
                compose = ui
                    .add_enabled(
                        !self.postcards.reader.busy()
                            && !snapshot.name.is_empty()
                            && !snapshot.date.is_empty(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-postcards-postcard-window-7f9661c5",
                        )),
                    )
                    .clicked();
            }
            if self.postcards.reader.busy() {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    cancel = ui
                        .button(crate::i18n::text(
                            "repo-workflows-postcards-postcard-window-5c4a8747",
                        ))
                        .clicked();
                });
            }
            if let Some(status) = &self.postcards.status {
                ui.label(crate::repo::activity::display_workflow_error(status));
            }
            if let Some(preview) = &self.postcards.preview {
                ui.separator();
                ui.label(crate::i18n::message(
                    "repo-workflows-postcards-postcard-window-5b3e5c5e",
                    &[
                        (
                            "preview-snapshot-name-1",
                            (preview.snapshot.name).clone().into(),
                        ),
                        (
                            "preview-snapshot-date-2",
                            (preview.snapshot.date).clone().into(),
                        ),
                    ],
                ));
                ui.add(egui::Image::new(&preview.texture).max_width(ui.available_width()));
                save = ui
                    .add_enabled(
                        !self.postcards.reader.busy(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-postcards-postcard-window-1aae6bb1",
                        )),
                    )
                    .clicked();
            }
        });
        if !open {
            self.postcards.open = false;
            self.postcards.reader.cancel();
            self.postcards.preview = None;
            return;
        }
        if range_changed || changed {
            self.postcards.reader.cancel();
            self.postcards.preview = None;
            self.postcards.status = None;
            if range_changed {
                self.postcards.commits = None;
            }
        }
        if cancel {
            self.postcards.reader.cancel();
            self.postcards.status = Some("Postcard work cancelled.".into());
        }
        if load {
            self.postcards.preview = None;
            self.postcards.commits = None;
            let repo = self.path.clone();
            let base = self.postcards.base.clone();
            let target = self.postcards.target.clone();
            let profile = crate::git::profile_config();
            self.postcards.context = Some(profile.clone());
            self.postcards.reader.queue(move |owned| {
                let active = || owned() && crate::git::profile_config() == profile;
                git::postcards::range(&repo, &base, &target, &active).map(Answer::Range)
            });
        }
        if compose && let Some(commits) = &self.postcards.commits {
            let snapshot = Snapshot::new(&self.postcards, commits);
            let profile = crate::git::profile_config();
            self.postcards.context = Some(profile.clone());
            self.postcards.reader.queue(move |owned| {
                let active = || owned() && crate::git::profile_config() == profile;
                let (image, png) = render::render(&snapshot, &active)?;
                Ok(Answer::Preview(snapshot, image, png))
            });
        }
        if save && let Some(preview) = &self.postcards.preview {
            let png = preview.png.clone();
            if let Some(path) = rfd::FileDialog::new()
                .set_title(crate::i18n::text(
                    "repo-workflows-postcards-postcard-window-06d79850",
                ))
                .set_file_name("release-postcard.png")
                .add_filter(
                    crate::i18n::text("repo-workflows-postcards-postcard-window-6ef9788d"),
                    &["png"],
                )
                .save_file()
            {
                self.queue_postcard_export(path, png, export::Destination::capture);
            }
        }
        self.poll_postcards(ctx);
    }
}
