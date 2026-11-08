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
        row.clone().on_hover_text(crate::i18n::text(
            "repo-workflows-lfs_storage-pointer-mark-efdc14b1",
        ));
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
                    let r = crate::menu_item::MenuItem::new(crate::i18n::text(
                        "repo-workflows-lfs_storage-storage-menu-items-1de3e10c",
                    ))
                    .icon("cloud-download")
                    .enabled(idle)
                    .show(ui)
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-lfs_storage-storage-menu-items-e2cb4df2",
                    ));
                    if r.clicked() {
                        acts.push(action);
                    }
                }
                Err(e) => {
                    crate::menu_item::MenuItem::new(crate::i18n::text(
                        "repo-workflows-lfs_storage-storage-menu-items-1de3e10c",
                    ))
                    .icon("cloud-download")
                    .refused(Some(&e))
                    .show(ui)
                    .on_disabled_hover_text(e);
                }
            }
        }
        if in_lfs
            && let Some((dir, pattern)) = folder
            && crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-workflows-lfs_storage-storage-menu-items-5ed965af",
                &[("dir", (dir).clone().into())],
            ))
            .icon("cloud-download")
            .show(ui)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-lfs_storage-storage-menu-items-ea226348",
            ))
            .clicked()
        {
            acts.push(Action::LfsStorage(StorageAct::FetchOnly(pattern)));
        }
        if let Some(pattern) = track {
            let label = if pattern.starts_with("*.") {
                crate::i18n::message(
                    "repo-workflows-lfs_storage-storage-menu-items-31a0cb12",
                    &[("pattern", (pattern).clone().into())],
                )
            } else {
                crate::i18n::message(
                    "repo-workflows-lfs_storage-storage-menu-items-4f441605",
                    &[("path", (path).into())],
                )
            };
            let r = crate::menu_item::MenuItem::new(label)
                .icon("command")
                .enabled(idle)
                .show(ui)
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-lfs_storage-storage-menu-items-1b8b17ee",
                ));
            if r.clicked() {
                acts.push(track_action(&pattern));
            }
        }
    }
}
