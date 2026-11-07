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
            ui.heading("Line dossier");
            if ui.button("Close dossier").clicked() {
                acts.push(Action::CloseLineDossier);
            }
            ui.label("Committed versions · read only");
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Commit");
            ui.add(
                egui::TextEdit::singleline(&mut self.dossier.revision)
                    .desired_width(160.0)
                    .char_limit(64),
            );
            ui.label("Path");
            ui.add(
                egui::TextEdit::singleline(&mut self.dossier.path)
                    .desired_width(240.0)
                    .char_limit(4096),
            );
            ui.label("Line");
            ui.add(egui::DragValue::new(&mut self.dossier.line).range(1..=20000));
            if ui.button("Explain history").clicked() {
                trace = Some(Seed {
                    commit: self.dossier.revision.clone(),
                    path: self.dossier.path.clone(),
                    line: self.dossier.line,
                });
            }
            if self.dossier.reader.busy() {
                ui.add(crate::spinner::Spinner::new());
                cancel = ui.button("Cancel trace").clicked();
            }
        });
        ui.small("Traces pinned regular text blobs without fetching or touching working/staged contents. Parent uncertainty and merge alternatives stay explicit.");
        if let Some(error) = &self.dossier.error {
            ui.colored_label(t.danger, clip(error, 2000));
        }
        if self.dossier.ignore.is_some() {
            self.dossier_ignore_view(ui);
            return;
        }
        if let Some(result) = &self.dossier.result {
            ui.small(format!(
                "Pinned {} · {}:{} · {} versions",
                super::super::short(&result.start.commit),
                clip(&result.start.path, 120),
                result.start.line,
                result.cards.len()
            ));
            ui.horizontal_wrapped(|ui| {
                if let Some(seed) = &result.continuation {
                    if ui
                        .add_enabled(
                            !self.dossier.reader.busy(),
                            egui::Button::new("Trace earlier versions"),
                        )
                        .clicked()
                    {
                        append = Some(seed.clone());
                    }
                }
                for (i, seed) in result.alternatives.iter().enumerate() {
                    if ui
                        .add_enabled(
                            !self.dossier.reader.busy(),
                            egui::Button::new(format!(
                                "Follow mapped parent {} · {}",
                                i + 1,
                                super::super::short(&seed.commit)
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
            ui.columns(2,|columns|{
    egui::ScrollArea::vertical().id_salt("dossier_timeline").max_height(columns[0].available_height()).show_rows(&mut columns[0],30.0,result.cards.len(),|ui,range|{for i in range{let card=&result.cards[i];if ui.selectable_label(self.dossier.selected==i,format!("[C{}] {} · {}:{}",i+1,super::super::short(&card.at.commit),clip(&card.at.path,55),card.at.line)).clicked(){self.dossier.selected=i;}}});
    let ui=&mut columns[1];if let Some(card)=result.cards.get(self.dossier.selected){
     ui.heading(format!("[C{}] {}",self.dossier.selected+1,super::super::short(&card.at.commit)));
     ui.label(format!("{} <{}> · {}",clip(&card.author,120),clip(&card.email,160),crate::git::relative_time(card.time,crate::git::now())));ui.label(clip(&card.subject,500));ui.label(format!("{}:{}",clip(&card.at.path,300),card.at.line));
     egui::ScrollArea::both().id_salt("dossier_line").max_height(180.0).show(ui,|ui|{ui.monospace(clip(&card.text,4000));});
     ui.small(clip(&card.note,1000));ui.small(format!("Whitespace/copy/move-aware blame: {} at {}:{}",super::super::short(&card.attribution.commit),clip(&card.attribution.path,200),card.attribution.line));
     if card.whitespace_only{ui.label("Whitespace-only file changes detected; review the full commit before choosing any ignore revision.");if ui.add_enabled(!self.dossier.reader.busy()&&self.busy_op.is_none(),egui::Button::new("Review formatting ignore…")).clicked(){prepare_ignore=Some(card.at.commit.clone());}}
     ui.horizontal_wrapped(|ui|{if ui.button("Show commit").clicked(){acts.push(Action::CloseLineDossier);acts.push(Action::SelectHash(card.at.commit.clone()));}if ui.button("Open file at this version").clicked(){acts.push(Action::CloseLineDossier);acts.push(Action::OpenPinnedFileLine(card.at.clone()));}});
    }
   });
        } else if !self.dossier.reader.busy() {
            ui.label("Choose a committed file and its 1-based line number, then Explain history. Working/index line numbers are not silently treated as HEAD line numbers.");
        }
        if cancel {
            self.dossier.reader.cancel();
            self.dossier.error = Some("Trace cancelled".into());
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
        ui.heading("Preview .git-blame-ignore-revs append");
        ui.monospace(&preview.prepared.commit);
        ui.label("Git -w detects whitespace changes, including inside strings. Inspect the full patch and judge whether ignoring this commit is appropriate. Existing bytes and the staged index will be preserved; nothing is changed before confirmation.");
        ui.checkbox(
            &mut self.dossier.ignore_reviewed,
            "I reviewed the full patch and want this revision ignored",
        );
        let mut append = false;
        let mut back = false;
        ui.horizontal_wrapped(|ui| {
            append = ui
                .add_enabled(
                    self.dossier.ignore_reviewed && self.busy_op.is_none(),
                    egui::Button::new("Append reviewed revision"),
                )
                .clicked();
            back = ui.button("Back to versions").clicked();
            ui.selectable_value(&mut self.dossier.show_existing, false, "Full commit patch");
            ui.selectable_value(
                &mut self.dossier.show_existing,
                true,
                "Existing ignore file",
            );
        });
        ui.small("Append adds a leading newline and one complete object ID. A missing file is created exclusively; links, hardlinks, special files, changed snapshots and changed roots refuse. Remote append requires Python3. Dossier history itself continues to show ignored formatting versions.");
        if self.dossier.show_existing {
            // FileText is constructed on the read worker; painting cannot invoke Git or filters.
            super::super::file_text::show(
                ui,
                ("dossier_ignore_existing", &preview.prepared.commit),
                Some(&preview.existing),
            );
        } else {
            super::super::file_text::show(
                ui,
                ("dossier_ignore_patch", &preview.prepared.commit),
                Some(&preview.patch),
            );
        }
        if back {
            self.dossier.ignore = None;
            self.dossier.ignore_reviewed = false;
        } else if append {
            self.append_dossier_ignore(ui.ctx());
        }
    }
}
