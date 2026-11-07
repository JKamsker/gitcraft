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

    /// A cloud on the row of a file of the checkout that is only its LFS pointer; the row's
    /// tooltip says so. For rows without buttons or stats on their right (`selected`: drawn as
    /// such).
    pub(in crate::repo) fn pointer_mark(
        &self,
        ui: &Ui,
        row: &Response,
        path: &str,
        selected: bool,
    ) {
        if !self.lfs_storage.only_pointer(path) {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let rect = row.rect;
        let mark =
            Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), vec2(20.0, 16.0));
        let painter = ui.painter_at(rect);
        // Over the end of the name: the row's own background first, as file_row paints it.
        let under = if selected {
            t.row_selected
        } else if ui.rect_contains_pointer(rect) {
            t.hover
        } else {
            t.card
        };
        let cover = Rect::from_min_max(pos2(mark.left() - 6.0, rect.top()), rect.max);
        painter.rect_filled(cover, 0.0, t.card);
        painter.rect_filled(cover, 0.0, under);
        painter.rect_filled(
            mark,
            CornerRadius::same(8),
            t.text_faint.gamma_multiply(0.18),
        );
        icons::paint(ui, mark, "cloud-download", 12.0, t.text_dim);
        row.clone().on_hover_text(
            "Only its LFS pointer is here (not downloaded). Right-click → LFS: Download.",
        );
    }

    /// Download, Fetch only this folder and Track in LFS for `path`, in its menu; only for the
    /// checkout (`rev` none or HEAD) of a repository that uses LFS, once its files are listed.
    pub(in crate::repo) fn storage_menu_items(
        &self,
        ui: &mut Ui,
        path: &str,
        rev: Option<&str>,
        acts: &mut Vec<Action>,
    ) {
        let state = &self.lfs_storage;
        if !self.lfs || !self.is_checkout(rev) || !state.files_read || state.files_error.is_some() {
            return;
        }
        let idle = self.busy_op.is_none();
        let in_lfs = state.in_lfs(path);
        let track = (!in_lfs)
            .then(|| track_pattern(path))
            .flatten()
            .filter(|p| !state.patterns.contains(p));
        let folder = path
            .rsplit_once('/')
            .and_then(|(dir, _)| folder_pattern(dir).map(|p| (dir.to_owned(), p)));
        if !(state.only_pointer(path) || in_lfs && folder.is_some()) && track.is_none() {
            return;
        }
        ui.separator();
        if state.only_pointer(path) {
            match download_action(&[path.to_owned()]) {
                Ok(action) => {
                    let r = ui
                        .add_enabled(idle, egui::Button::new("LFS: Download"))
                        .on_hover_text("Download this file's LFS object and check it out");
                    if r.clicked() {
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
            && let Some((dir, pattern)) = folder
            && ui
                .button(format!("LFS: Fetch only {dir}"))
                .on_hover_text(
                    "From now on git-lfs downloads only this folder (and any others set in LFS: \
                     Storage…) on fetch, pull and checkout; other files stay pointers. Set in \
                     this repository's config.",
                )
                .clicked()
        {
            acts.push(Action::LfsStorage(StorageAct::FetchOnly(pattern)));
        }
        if let Some(pattern) = track {
            let label = if pattern.starts_with("*.") {
                format!("Track {pattern} in LFS")
            } else {
                format!("Track {path} in LFS")
            };
            let r = ui
                .add_enabled(idle, egui::Button::new(label))
                .on_hover_text(
                    "git lfs track: writes the pattern to .gitattributes. Files already committed \
                 stay ordinary git files until changed and staged again; commit .gitattributes \
                 to share it.",
                );
            if r.clicked() {
                acts.push(track_action(&pattern));
            }
        }
    }
}
