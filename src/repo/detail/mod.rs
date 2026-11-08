//! Right pane: the selected commit, or the working-tree changes with staging and the commit box.

mod all_files;
mod changes;
mod commit;
mod commit_box;
mod compare;
pub(super) mod compose;
mod extras;
mod stash_picks;

pub(super) use all_files::AllFiles;
pub(super) use changes::{discard_dialog, stage_action};
pub(super) use extras::Extras;

use egui::Ui;

use super::branch_tree;
use super::widgets::{self, RowExtra};
use super::{Action, DiffKey, DiffView, RepoTab, folding};
use crate::git::ChangedFile;
use crate::settings::Settings;
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(super) fn detail_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let wip = self.selected.is_some_and(|r| self.is_wip(r));
        if self.pr_view.is_some() {
            self.pr_files_panel(ui, acts);
        } else if self.compare.is_some() {
            self.compare_panel(ui, acts);
        } else if wip {
            self.changes_panel(ui, acts);
        } else if self.selected_parked().is_some() {
            self.parked_panel(ui, acts);
        } else {
            self.commit_panel(ui, acts);
        }
    }

    fn open_key(&self) -> Option<&DiffKey> {
        self.diff.as_ref().map(|d: &DiffView| &d.key)
    }

    /// Changed files, each opening its diff (`key`) when clicked; `rev`: where the files are,
    /// for their menu (`None`: the working tree); `tree`: which list this is, for the folders
    /// opened and closed in it. Generated files and lockfiles are folded at the end.
    pub(super) fn file_list(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        tree: &str,
        files: &[ChangedFile],
        rev: Option<&str>,
        key: impl Fn(&str) -> DiffKey,
    ) {
        let id = egui::Id::new(("file-folder", self.repo_key(), tree));
        let rules = folding::Rules::from_settings(&Settings::get(ui.ctx()));
        let group = folding::split(
            ui,
            id.with(rev),
            files,
            |f| &f.path,
            (&self.folding, &rules),
            acts,
        );
        let selected = |f: &&&ChangedFile| self.open_key() == Some(&key(&f.path));
        let (scope, generation) = super::weakened_marks::scope_of(&key(""), self.work_read);
        let found = self.weakened.get(ui.ctx(), &self.path, scope, generation);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        self.owners_summary(ui, &paths);
        let row = |ui: &mut Ui, indent, f: &ChangedFile, acts: &mut Vec<Action>| {
            let stats = RowExtra::Stats(f.added, f.removed);
            let (extra, flags) = super::weakened_marks::extra(found.as_ref(), &f.path, &stats);
            let r = widgets::file_row_owned(
                ui,
                f.status,
                &f.path,
                f.old_path.as_deref(),
                indent,
                self.open_key() == Some(&key(&f.path)),
                extra,
                self.owner_tag(&f.path).as_ref(),
            );
            super::weakened_marks::hover(ui, &r, flags);
            // A file the commit deleted isn't in it, and has nothing to link to.
            let links = || self.link_menu(None).filter(|_| f.status != 'D');
            widgets::file_menu(
                &r.row,
                &f.path,
                rev,
                false,
                Some(&key(&f.path)),
                &links,
                acts,
            );
            if r.row.clicked() {
                acts.push(Action::OpenDiff(key(&f.path)));
                acts.push(Action::UnfoldDiff);
            }
        };
        // Rows are known by their path within the list of this commit (or range): a menu left
        // open while another one is shown is not that one's file of the same path.
        horizontal_files(ui, ("changed-files-scroll", tree, rev), &paths, |ui| {
            ui.push_id(("file-list", tree, rev), |ui| {
                branch_tree::files(
                    ui,
                    (id, true),
                    &group.shown,
                    |f| &f.path,
                    &selected,
                    &mut |ui, indent, f| row(ui, indent, f, acts),
                );
                let stats = group.folded.iter().fold((0u32, 0u32), |(a, d), f| {
                    (
                        a.saturating_add(f.added.unwrap_or(0)),
                        d.saturating_add(f.removed.unwrap_or(0)),
                    )
                });
                let holds_open = group.folded.iter().any(|f| selected(&f));
                let open_id = id.with(("folded-open", rev));
                if group.header(ui, open_id, Some(stats), holds_open) {
                    branch_tree::files(
                        ui,
                        (id.with("folded"), true),
                        &group.folded,
                        |f| &f.path,
                        &selected,
                        &mut |ui, indent, f| row(ui, indent, f, acts),
                    );
                }
            });
        });
        super::weakened_marks::error_note(ui, found.as_ref());
    }
}

/// "N files changed +a −d", and an optional note on the right.
pub(super) fn files_header(ui: &mut Ui, files: &[ChangedFile], note: Option<&str>) {
    files_header_with(ui, files, note, |_| {});
}

pub(super) fn files_header_with(
    ui: &mut Ui,
    files: &[ChangedFile],
    note: Option<&str>,
    controls: impl FnOnce(&mut Ui),
) {
    let t = Tokens::get(ui.ctx());
    let (adds, dels) = files.iter().fold((0u32, 0u32), |(a, d), f| {
        (
            a.saturating_add(f.added.unwrap_or(0)),
            d.saturating_add(f.removed.unwrap_or(0)),
        )
    });
    let summary = format!(
        "{} file{} changed  +{adds}  −{dels}",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
    egui::Frame::new()
        .fill(t.tab_strip)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    controls(ui);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&summary)
                                    .font(theme::semibold(11.5))
                                    .color(t.text_dim),
                            )
                            .truncate(),
                        )
                        .on_hover_text(format!(
                            "{summary}{}",
                            note.map(|n| format!(" · {n}")).unwrap_or_default()
                        ));
                    });
                });
            });
        });
}

/// Only this list grows to fit long paths. The message, metadata and editor keep the
/// panel's width, and each list keeps its own horizontal offset.
fn horizontal_files(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    paths: &[&str],
    body: impl FnOnce(&mut Ui),
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let width = paths
        .iter()
        .map(|path| {
            ui.painter()
                .layout_no_wrap(
                    (*path).to_owned(),
                    egui::FontId::proportional(12.5),
                    egui::Color32::WHITE,
                )
                .size()
                .x
                + 160.0
        })
        .fold(ui.available_width(), f32::max);
    egui::ScrollArea::horizontal()
        .id_salt(id)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_min_width(width);
            body(ui);
        })
}

#[cfg(test)]
pub(in crate::repo) mod responsive_tests;
