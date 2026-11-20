//! The metadata edits in a working-tree file's menu, and the background read that fills a
//! dialog opened from there.

use egui::Ui;

use super::{Edit, FileMetaForm};
use crate::git::file_metadata as meta;
use crate::git::{Area, StatusFile};
use crate::repo::{Action, Dialog, RepoTab};

impl RepoTab {
    /// The executable bit (a staged file) and a case-only rename (any tracked file) in the
    /// menu of the working-tree file `file`.
    pub(in crate::repo::detail) fn file_metadata_menu_items(
        &self,
        ui: &mut Ui,
        file: &StatusFile,
        acts: &mut Vec<Action>,
    ) {
        let edits = offered(file);
        if edits.is_empty() {
            return;
        }
        ui.separator();
        let idle = self.busy_op.is_none();
        for edit in edits {
            let (label, tip, icon) = match edit {
                Edit::Executable => (
                    "file-metadata-executable-menu",
                    "file-metadata-executable-menu-tip",
                    "square-terminal",
                ),
                Edit::LetterCase => (
                    "file-metadata-case-menu",
                    "file-metadata-case-menu-tip",
                    "arrow-down-a-z",
                ),
            };
            let r = crate::menu_item::MenuItem::new(crate::i18n::text(label))
                .icon(icon)
                .enabled(idle)
                .show(ui)
                .on_hover_text(crate::i18n::text(tip));
            if r.clicked() {
                let form = FileMetaForm::new(&file.path, edit);
                self.read_file_metadata(&form, ui.ctx());
                acts.push(Action::Dialog(Dialog::FileMetadata(Box::new(form))));
            }
        }
    }

    /// Reads what `form`'s edit needs on a background thread, into the form's own cell.
    fn read_file_metadata(&self, form: &FileMetaForm, ctx: &egui::Context) {
        let (repo, path, edit) = (self.path.clone(), form.path.clone(), form.edit);
        let cell = form.read.clone();
        self.then(
            ctx,
            move || {
                let read = meta::snapshot(&repo, &path, edit == Edit::LetterCase).map(|found| {
                    let patch = (edit == Edit::Executable)
                        .then(|| meta::save_executable_patch(&repo, &found));
                    (found, patch)
                });
                // A fresh cell, set only here: it can't be full already.
                let _ = cell.set(read);
            },
            // The answer is in the cell; coming back only repaints the dialog.
            |_, (), _| {},
        );
    }
}

/// The edits a working-tree file's menu offers: only for a file in the index and the working
/// tree, not conflicted; the executable bit only on the staged side, as it is an index edit.
/// An unstaged `A` is an intent-to-add entry (`git add -N`): nothing real is in the index yet.
pub(super) fn offered(file: &StatusFile) -> Vec<Edit> {
    match (file.area, file.code) {
        (_, 'D' | '?' | 'U') | (Area::Unstaged, 'A') => Vec::new(),
        (Area::Staged, _) => vec![Edit::Executable, Edit::LetterCase],
        (Area::Unstaged, _) => vec![Edit::LetterCase],
        (Area::Untracked | Area::Conflicted, _) => Vec::new(),
    }
}
