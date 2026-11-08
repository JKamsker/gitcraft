//! "View all files": every file in the tree of the commit shown, not only those it changed,
//! each opening as it is at the commit.

use std::collections::HashMap;

use egui::Ui;

use crate::git::{ChangedFile, CommitDetail};
use crate::repo::file_view::FileTab;
use crate::repo::widgets::{self, RowExtra};
use crate::repo::{Action, RepoTab, branch_tree};
use crate::theme::Tokens;

#[derive(Default)]
pub(in crate::repo) struct AllFiles {
    /// The box is ticked.
    pub(in crate::repo) on: bool,
    /// The commit the files are read for.
    pub(in crate::repo) of: Option<String>,
    /// Its files, once read.
    pub(in crate::repo) files: Option<Result<Vec<String>, String>>,
}

impl AllFiles {
    /// Starts reading the files of `hash`.
    pub(in crate::repo) fn wait_for(&mut self, hash: &str) {
        self.of = Some(hash.to_owned());
        self.files = None;
    }

    /// Takes in the files of `hash`, if they are still the ones wanted.
    pub(in crate::repo) fn arrived(&mut self, hash: &str, result: Result<Vec<String>, String>) {
        if self.of.as_deref() == Some(hash) {
            self.files = Some(result);
        }
    }
}

impl RepoTab {
    /// Reads the files of the commit shown, when they are to be listed and aren't yet.
    pub(super) fn want_all_files(&mut self, ctx: &egui::Context) {
        let Some(hash) = self.detail.as_ref().map(|d| &d.hash) else {
            return;
        };
        if self.all_files.on && self.all_files.of.as_ref() != Some(hash) {
            self.load_tree_files(hash.clone(), ctx);
        }
    }

    /// The "View all files" box.
    pub(super) fn all_files_toggle(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if crate::icons::toggle_button(
            ui,
            "folder-tree",
            crate::i18n::text("repo-core-0806"),
            self.all_files.on,
        )
        .on_hover_text(if self.all_files.on {
            crate::i18n::text("dev-integration-ui-all-files-on")
        } else {
            crate::i18n::text("dev-integration-ui-all-files-off")
        })
        .clicked()
        {
            acts.push(Action::ViewAllFiles(!self.all_files.on));
        }
    }

    /// Every file of `detail`'s commit, those it changed with their status; each opens as it is
    /// at the commit.
    pub(super) fn all_files_list(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        detail: &CommitDetail,
    ) {
        let t = Tokens::get(ui.ctx());
        let files = match &self.all_files.files {
            Some(files) if self.all_files.of.as_ref() == Some(&detail.hash) => files,
            _ => {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
                return;
            }
        };
        let files = match files {
            Ok(files) => files,
            Err(e) => {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.colored_label(
                        t.danger,
                        crate::git::display_error(&crate::forge::inbox::display_error(e)),
                    );
                });
                return;
            }
        };
        let changed: HashMap<&str, &ChangedFile> =
            detail.files.iter().map(|f| (f.path.as_str(), f)).collect();
        let shown = |path: &str| {
            self.file_view.as_ref().is_some_and(|v| {
                v.tab == FileTab::File
                    && v.path == path
                    && v.rev.as_deref() == Some(detail.hash.as_str())
            })
        };
        let id = egui::Id::new(("file-folder", self.repo_key(), "all"));
        // Rows are known by their path within this commit's files.
        let paths: Vec<&str> = files.iter().map(String::as_str).collect();
        let mut reveal = None;
        super::horizontal_files(ui, ("all-files-scroll", &detail.hash), &paths, |ui| {
            ui.push_id(("all-files", &detail.hash), |ui| {
                reveal = branch_tree::files(
                    ui,
                    (id, false),
                    files,
                    |p| p.as_str(),
                    &|p: &&String| shown(p),
                    &mut |ui, indent, path| {
                        let change = changed.get(path.as_str());
                        let extra = match change {
                            Some(f) => RowExtra::Stats(f.added, f.removed),
                            None => RowExtra::Buttons(&[]),
                        };
                        let code = change.map_or(' ', |f| f.status);
                        let r = widgets::file_row(ui, code, path, None, indent, shown(path), extra);
                        self.keyboard_file_row(
                            egui::Id::new(("all-files", &detail.hash)),
                            &r.row,
                            &crate::repo::DiffKey::Commit {
                                hash: detail.hash.clone(),
                                path: path.clone(),
                            },
                        );
                        // The checkout's files only, and not over a changed file's stats.
                        if change.is_none() && self.is_checkout(Some(&detail.hash)) {
                            self.pointer_mark(ui, &r.row, path, shown(path));
                        }
                        // Over the stats of a changed file, the padlock would hide them.
                        if change.is_none() {
                            self.lock_badge(ui, &r.row, path, (false, shown(path)));
                        }
                        let locks = |ui: &mut Ui, acts: &mut Vec<Action>| {
                            self.lock_menu_items(ui, path, Some(&detail.hash), acts);
                            self.storage_menu_items(ui, path, Some(&detail.hash), acts);
                        };
                        let links = || self.link_menu(None);
                        widgets::file_menu_with(
                            &r.row,
                            path,
                            Some(&detail.hash),
                            false,
                            None,
                            &links,
                            r.row.has_focus(),
                            acts,
                            locks,
                        );
                        if r.row.clicked() {
                            acts.push(Action::FileView {
                                path: path.clone(),
                                rev: Some(detail.hash.clone()),
                                tab: FileTab::File,
                            });
                        }
                    },
                )
                .or(reveal);
            });
        });
        // egui consumes both scroll targets at the inner horizontal list. Forward the
        // same typed row rectangle to its outer vertical scroller after that list ends.
        if let Some(rect) = reveal {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
        }
    }
}
