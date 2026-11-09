//! The right-hand panel of a merge put aside, its row selected in the graph: what it is and
//! where its agent is, with Bring it back; then its conflicted files and the rest of what it
//! had, each opening its diff (the files as they were put aside, conflict markers and all,
//! against the commit it was put aside on).

use egui::{RichText, Ui};

use super::graph::Standing;
use crate::git::ChangedFile;
use crate::icons;
use crate::repo::{Action, DiffKey, RepoTab, widgets};
use crate::resolver::park::Parked;
use crate::theme::{self, Tokens};

impl RepoTab {
    /// The merge put aside whose row is selected, if one is.
    pub(in crate::repo) fn selected_parked(&self) -> Option<&Parked> {
        self.selected.and_then(|i| self.parked_at(i))
    }

    pub(in crate::repo) fn parked_panel(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(parked) = self.selected_parked() else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        widgets::panel_header(ui, "Merge put aside", |_| {});
        self.parked_banner(ui, &t, parked, acts);
        let Some(detail) = self.detail.as_ref().filter(|d| d.hash == parked.snapshot) else {
            ui.centered_and_justified(|ui| {
                ui.add(crate::spinner::Spinner::new());
            });
            return;
        };
        let (conflicted, rest): (Vec<ChangedFile>, Vec<ChangedFile>) = detail
            .files
            .iter()
            .cloned()
            .partition(|f| parked.files.contains(&f.path));
        let key = |path: &str| DiffKey::Commit {
            hash: parked.snapshot.clone(),
            path: path.to_owned(),
        };
        egui::ScrollArea::vertical()
            .id_salt("parked_scroll")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                section(ui, &t, "CONFLICTED", conflicted.len(), t.danger);
                self.file_list(
                    ui,
                    acts,
                    "parked-conflicted",
                    &conflicted,
                    Some(&parked.snapshot),
                    key,
                );
                section(ui, &t, "ALSO IN THE MERGE", rest.len(), t.text_dim);
                self.file_list(ui, acts, "parked-rest", &rest, Some(&parked.snapshot), key);
            });
    }

    /// What is put aside, where its agent is, and how to have it back.
    fn parked_banner(&self, ui: &mut Ui, t: &Tokens, parked: &Parked, acts: &mut Vec<Action>) {
        let (color, words) = match self.parked_standing(parked) {
            Standing::Working(over, total) if total > 0 => (
                t.accent,
                format!("An agent is resolving it: {over} of {total} steps done"),
            ),
            Standing::Working(..) => (t.accent, "An agent is resolving it".to_owned()),
            Standing::Ready => (
                t.success,
                "The agent is done: bring it back to review".to_owned(),
            ),
            Standing::Partly(ready, total) => (
                t.warning,
                format!(
                    "The agent is done, {ready} of {total} files resolved: bring it back to \
                     review"
                ),
            ),
            Standing::Waiting => (t.warning, "Waiting for you to bring it back".to_owned()),
        };
        egui::Frame::new()
            .fill(color.gamma_multiply(0.10))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    icons::inline(ui, "bot", 16.0, color);
                    let what = parked.what();
                    let mut chars = what.chars();
                    let first: String = chars
                        .next()
                        .map(|c| c.to_uppercase().collect())
                        .unwrap_or_default();
                    let into = parked.branch.as_deref().unwrap_or("a detached HEAD");
                    ui.label(
                        RichText::new(format!("{first}{} into {into}", chars.as_str()))
                            .font(theme::medium(13.0)),
                    );
                });
                ui.label(RichText::new(words).color(color).small());
                ui.label(
                    RichText::new(
                        "Your conflicts and staged files are kept as they were; the worktree was \
                         reset to its branch, free meanwhile.",
                    )
                    .color(t.text_dim)
                    .small(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| super::panel::bring_back_button(ui, parked, acts));
            });
    }
}

fn section(ui: &mut Ui, t: &Tokens, title: &str, n: usize, color: egui::Color32) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(title).color(color).small().strong());
        ui.label(RichText::new(n.to_string()).color(t.text_faint).small());
    });
    ui.add_space(4.0);
}
