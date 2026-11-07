use super::*;
use crate::repo::{Action, dialogs::Outcome};
use crate::theme::Tokens;
use engine::{Origin, Target};
impl State {
    pub(in crate::repo) fn show(&mut self, ui: &mut egui::Ui, idle: bool) -> Outcome {
        ui.set_width(760.0);
        ui.heading("Lost and found by content");
        ui.label("Find literal text in reflog history, unreachable commits, dropped stashes and unattached staged blobs. Search reads local objects without downloading or changing files.");
        let mut outcome = Outcome::Open;
        let changed = ui
            .add(
                egui::TextEdit::multiline(&mut self.query)
                    .desired_rows(2)
                    .hint_text("Text you remember, such as parse_header"),
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
                    egui::Button::new("Search literal text"),
                )
                .clicked()
            {
                outcome = Outcome::Done(Action::ContentSearch(self.query.clone()));
            }
            if self.loading() {
                ui.add(crate::spinner::Spinner::new());
                ui.label(format!(
                    "Reading local objects · {}s",
                    self.began.map_or(0, |at| at.elapsed().as_secs())
                ));
                if ui.button("Cancel read").clicked() {
                    self.cancel();
                }
            }
            if ui.button("Close").clicked() {
                outcome = Outcome::Cancel;
            }
        });
        ui.label(egui::RichText::new("Limits: 500 commits, 10000 objects, 15000 file entries, 2 MiB per blob, 64 MiB scanned content, 300 hits, 120 seconds. Binary/non-UTF-8 and oversized content is skipped.").small());
        if let Some(error) = &self.error {
            ui.colored_label(
                Tokens::get(ui.ctx()).warning,
                error.chars().take(1000).collect::<String>(),
            );
        }
        let enabled = idle && !self.loading();
        if let Some(preview) = self.preview.clone() {
            ui.separator();
            ui.label(format!(
                "Saved content · {} bytes · {}",
                preview.bytes.len(),
                &preview.blob[..12]
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
                ui.label("Unattached content: original filename and staging time are unknown. It can be recovered as a new file.");
            }
            if let (Some(results), Some(index)) = (&self.results, self.selected)
                && let Some(hit) = results.report.hits.get(index)
            {
                egui::CollapsingHeader::new(format!(
                    "{} known places / versions",
                    hit.provenance.len()
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
                            outcome = Outcome::Done(Action::ContentInspect(index, Some(i)));
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
                ui.label("Displayed text is clipped to 2000 lines and 400 characters per line. Recovery uses the complete saved bytes.");
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("New file in this repository:");
                if ui.text_edit_singleline(&mut self.file).changed() {
                    self.prepared = None;
                }
                if ui
                    .add_enabled(enabled, egui::Button::new("Preview file recovery…"))
                    .clicked()
                {
                    outcome =
                        Outcome::Done(Action::ContentPrepare(Target::File(self.file.clone())));
                }
            });
            ui.label("Creates a new regular file on the repository’s machine. Existing, tracked, ignored and linked destinations are refused; staged contents stay unchanged.");
            if let Some(source) = &preview.source {
                ui.horizontal(|ui| {
                    ui.label("New branch:");
                    if ui.text_edit_singleline(&mut self.branch).changed() {
                        self.prepared = None;
                    }
                    if ui
                        .add_enabled(enabled, egui::Button::new("Preview branch recovery…"))
                        .clicked()
                    {
                        outcome = Outcome::Done(Action::ContentPrepare(Target::Branch(
                            self.branch.clone(),
                        )));
                    }
                });
                if source.origin == Origin::DroppedStash
                    && ui
                        .add_enabled(enabled, egui::Button::new("Preview return to stash list…"))
                        .clicked()
                {
                    outcome = Outcome::Done(Action::ContentPrepare(Target::Stash));
                }
            }
            if let Some(prepared) = self.prepared.clone() {
                ui.group(|ui|{
                    match &prepared.target {Target::File(path)=>{ui.label(format!("Create {path} with {} raw bytes. Nothing existing will be replaced.",prepared.preview.bytes.len()));},Target::Branch(name)=>{ui.label(format!("Create branch {name} at the saved commit. HEAD and working files stay unchanged."));},Target::Stash=>{ui.label("Return this dropped stash to the stash list, preserving the current stash. Do not apply it or change staged/working files.");}}
                    if ui.add_enabled(enabled,egui::Button::new("Recover now")).clicked(){outcome=Outcome::Done(Action::ContentPerform(prepared));}
                });
            }
            if ui.button("Back to matches").clicked() {
                self.cancel();
                self.preview = None;
                self.prepared = None;
            }
        } else if let Some(results) = &self.results {
            let report = &results.report;
            ui.separator();
            ui.label(format!("{} matches · {} blobs / {} bytes read · {} binary, {} oversized, {} missing skipped",report.hits.len(),report.scanned,report.bytes,report.binary,report.large,report.missing));
            if report.limited {
                ui.colored_label(Tokens::get(ui.ctx()).warning,"Search reached a budget or skipped unsupported paths/provenance. These results are incomplete.");
            }
            if report.shallow {
                ui.label("This is a shallow repository; older content may not be present locally.");
            }
            for note in &report.notes {
                ui.label(note);
            }
            ui.label("Timeline is newest first where a commit date is known. Unattached blobs appear last.");
            egui::ScrollArea::vertical()
                .id_salt("content-matches")
                .max_height(420.0)
                .show_rows(ui, 65.0, report.hits.len(), |ui, rows| {
                    for i in rows {
                        let hit = &report.hits[i];
                        let source = hit.provenance.first();
                        let date = hit
                            .when()
                            .map(|when| crate::git::relative_time(when, crate::git::now()))
                            .unwrap_or_else(|| "Time unknown".into());
                        let title = source
                            .map(|source| {
                                format!("{} · {} · {}", date, source.path, origin(&source.origin))
                            })
                            .unwrap_or_else(|| format!("{date} · Unattached staged content"));
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new(title.chars().take(130).collect::<String>()),
                            )
                            .on_hover_text(&title)
                            .clicked()
                        {
                            outcome = Outcome::Done(Action::ContentInspect(i, source.map(|_| 0)));
                        }
                        ui.label(
                            egui::RichText::new(hit.excerpt.chars().take(140).collect::<String>())
                                .monospace(),
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
        Origin::History => "Reflog / history",
        Origin::Unreachable => "Unreachable commit",
        Origin::DroppedStash => "Dropped stash",
        Origin::LiveStash => "Current stash list",
    }
}
