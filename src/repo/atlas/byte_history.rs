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
        egui::Window::new("Scanned file history across refs")
            .id(egui::Id::new(("atlas-byte-history", &owner.repo)))
            .open(&mut open)
            .default_size([780.0, 520.0])
            .show(ctx, |ui| {
                ui.label(format!("Exact path bytes: {}", clipped(&self.label,256)))
                    .on_hover_text(&self.label);
                ui.label(format!("{} touching commits among {} scanned unique commits · {} reference tips{}{}",
                    self.rows.len(), self.history.commits.len(), self.history.tips,
                    if self.history.more {" · partial history"} else {" · all reachable history scanned"},
                    if self.history.shallow {" · shallow boundary"} else {""}));
                ui.small("This list matches this exact path across the analyzed refs, including deletions. It uses the full scanned snapshot, independent of the atlas time window; it does not follow renames to other paths. Open a commit to inspect its changes. For older changes, use Scan more atlas history in the atlas.");
                if self.reader.busy() {ui.label("Preparing file history…");}
                if let Some(error) = &self.error {ui.colored_label(crate::theme::Tokens::get(ctx).danger,error);}
                let pages = self.rows.len().div_ceil(100).max(1);
                self.page = self.page.min(pages-1);
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.page>0,egui::Button::new("Previous file commits")).clicked() {self.page-=1;}
                    ui.label(format!("{} / {pages}",self.page+1));
                    if ui.add_enabled(self.page+1<pages,egui::Button::new("Next file commits")).clicked() {self.page+=1;}
                });
                egui::ScrollArea::vertical().max_height(380.0).show(ui,|ui| {
                    for &at in self.rows.iter().skip(self.page*100).take(100) {
                        let record=&self.history.commits[at];
                        if ui.button(format!("{} · {} · {} · {}", record.day(self.clock), record.hash.get(..8).unwrap_or(&record.hash), clipped(&record.name,96), clipped(&record.subject,200))).clicked() {chosen=Some(record.hash.clone());}
                    }
                    if self.rows.is_empty() && !self.reader.busy() && self.error.is_none() {ui.label("No scanned commits touch this exact path. Its history may precede the scan boundary.");}
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
