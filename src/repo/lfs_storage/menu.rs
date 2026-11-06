//! The LFS storage in a file's row and menu: a mark on files that are only their pointer, and
//! Download, Fetch only this folder and Track in LFS.

use egui::{CornerRadius, Rect, Response, Ui, pos2, vec2};

use super::{StorageAct, download_action, track_action};
use crate::git::{folder_pattern, track_pattern};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

impl RepoTab {
    /// Whether `rev` (the commit a file list or diff is of) is the checkout: what the LFS
    /// files of the checkout say holds there only.
    pub(in crate::repo) fn is_checkout(&self, rev: Option<&str>) -> bool {
        rev.is_none_or(|r| !self.refs.head.is_empty() && r == self.refs.head)
    }

    /// A cloud on the row of a file of the checkout that is only its LFS pointer (its object
    /// never downloaded); the row's tooltip says so. Not drawn while the row shows buttons.
    pub(in crate::repo) fn pointer_mark(&self, ui: &Ui, row: &Response, path: &str) {
        if !self.lfs_storage.only_pointer(path) {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let rect = row.rect;
        let mark =
            Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(20.0, 16.0));
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            mark,
            CornerRadius::same(8),
            t.text_faint.gamma_multiply(0.18),
        );
        icons::paint(ui, mark, "cloud-download", 12.0, t.text_dim);
        row.clone().on_hover_text(
            "Only its LFS pointer is here: the file was never downloaded. Right-click → LFS: Download.",
        );
    }

    /// Download, Fetch only this folder and Track in LFS for `path`, in its menu; only for the
    /// checkout (`rev` none or HEAD), where they apply.
    pub(in crate::repo) fn storage_menu_items(
        &self,
        ui: &mut Ui,
        path: &str,
        rev: Option<&str>,
        acts: &mut Vec<Action>,
    ) {
        if !self.is_checkout(rev) {
            return;
        }
        let in_lfs = self.lfs_storage.in_lfs(path);
        let can_track = self.lfs_storage.installed == Some(true) && !in_lfs;
        if !in_lfs && !can_track {
            return;
        }
        ui.separator();
        if self.lfs_storage.only_pointer(path) {
            let paths = [path.to_owned()];
            match download_action(&paths) {
                Ok(action) => {
                    if ui
                        .button("LFS: Download")
                        .on_hover_text("Download this file's LFS object and check it out")
                        .clicked()
                    {
                        acts.push(action);
                    }
                }
                Err(e) => {
                    ui.add_enabled(false, egui::Button::new("LFS: Download"))
                        .on_disabled_hover_text(e);
                }
            }
        }
        if in_lfs
            && let Some(folder) = path
                .rsplit_once('/')
                .and_then(|(dir, _)| folder_pattern(dir))
            && ui
                .button(format!("LFS: Fetch only {folder}"))
                .on_hover_text(
                    "From now on, LFS: Fetch and Pull download only this folder (and any others \
                     set in LFS: Storage…): lfs.fetchinclude in this repository's config",
                )
                .clicked()
        {
            acts.push(Action::LfsStorage(StorageAct::FetchOnly(folder)));
        }
        if can_track && let Some(pattern) = track_pattern(path) {
            let r = ui.button(format!("Track {pattern} in LFS")).on_hover_text(
                "git lfs track: writes the pattern to .gitattributes. Files already committed \
                 stay in git until they change; commit .gitattributes to share it.",
            );
            if r.clicked() {
                acts.push(track_action(&pattern));
            }
        }
    }
}
