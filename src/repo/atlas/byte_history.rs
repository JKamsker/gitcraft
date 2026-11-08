//! History over the exact byte paths in the pinned, all-ref analysis snapshot.
use super::{Clock, Reader};
use crate::git::activity::History;
use std::{path::PathBuf, sync::Arc};

pub(super) struct ByteHistory {
    history: Arc<History>,
    label: String,
    reader: Reader<Vec<usize>>,
    pub(super) rows: Vec<usize>,
    page: usize,
    clock: Clock,
    error: Option<String>,
}
impl ByteHistory {
    pub(super) fn new(
        history: Arc<History>,
        path: Vec<u8>,
        profile: Option<PathBuf>,
        clock: Clock,
    ) -> Self {
        // Escape bytes, so two invalid UTF-8 names never acquire the same label.
        let label = path.escape_ascii().to_string();
        let mut reader = Reader::default();
        let snapshot = history.clone();
        reader.queue(move |owned| {
            let mut rows = Vec::new();
            for (i, record) in snapshot.commits.iter().enumerate() {
                if !owned() || crate::git::profile_config() != profile {
                    return Err("File history cancelled".into());
                }
                if record.paths.contains(&path) {
                    rows.push(i);
                }
            }
            Ok(rows)
        });
        Self {
            history,
            label,
            reader,
            rows: Vec::new(),
            page: 0,
            clock,
            error: None,
        }
    }
    pub(super) fn show(
        &mut self,
        owner: &crate::repo::messages::JobSender,
        ctx: &egui::Context,
    ) -> (bool, Option<String>) {
        if let Some(result) = self.reader.poll(owner, ctx) {
            match result {
                Ok(rows) => self.rows = rows,
                Err(error) => self.error = Some(error),
            }
        }
        let mut open = true;
        let mut chosen = None;
        egui::Window::new(crate::i18n::text("repo-workflows-atlas-show-7269bb4e"))
            .id(egui::Id::new(("atlas-byte-history", &owner.repo)))
            .open(&mut open)
            .default_size([780.0, 520.0])
            .show(ctx, |ui| {
                ui.label(crate::i18n::message(
                    "repo-workflows-atlas-show-8647a64f",
                    &[(
                        "clipped-self-label-256-1",
                        (clipped(&self.label, 256)).clone().into(),
                    )],
                ))
                .on_hover_text(&self.label);
                ui.label(crate::i18n::message(
                    "repo-workflows-atlas-touching-summary",
                    &[
                        ("touching", (self.rows.len()).into()),
                        ("scanned", (self.history.commits.len()).into()),
                        ("tips", (self.history.tips).into()),
                        (
                            "partial",
                            (if self.history.more { "yes" } else { "no" }).into(),
                        ),
                        (
                            "shallow",
                            (if self.history.shallow { "yes" } else { "no" }).into(),
                        ),
                    ],
                ));
                ui.small(crate::i18n::text("repo-workflows-atlas-show-076017ad"));
                if self.reader.busy() {
                    ui.label(crate::i18n::text("repo-workflows-atlas-show-01efcd54"));
                }
                if let Some(error) = &self.error {
                    ui.colored_label(crate::theme::Tokens::get(ctx).danger, error);
                }
                let pages = self.rows.len().div_ceil(100).max(1);
                self.page = self.page.min(pages - 1);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.page > 0,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-atlas-show-56fe4d78",
                            )),
                        )
                        .clicked()
                    {
                        self.page -= 1;
                    }
                    ui.label(format!(
                        "{} / {}",
                        crate::i18n::number(self.page + 1),
                        crate::i18n::number(pages)
                    ));
                    if ui
                        .add_enabled(
                            self.page + 1 < pages,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-atlas-show-1abc4096",
                            )),
                        )
                        .clicked()
                    {
                        self.page += 1;
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, |ui| {
                        for &at in self.rows.iter().skip(self.page * 100).take(100) {
                            let record = &self.history.commits[at];
                            if ui
                                .button(format!(
                                    "{} · {} · {} · {}",
                                    record.day(self.clock),
                                    record.hash.get(..8).unwrap_or(&record.hash),
                                    clipped(&record.name, 96),
                                    clipped(&record.subject, 200)
                                ))
                                .clicked()
                            {
                                chosen = Some(record.hash.clone());
                            }
                        }
                        if self.rows.is_empty() && !self.reader.busy() && self.error.is_none() {
                            ui.label(crate::i18n::text("repo-workflows-atlas-show-c256cd01"));
                        }
                    });
            });
        (open, chosen)
    }
}
fn clipped(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let mut label: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        label.push('…');
    }
    label
}
