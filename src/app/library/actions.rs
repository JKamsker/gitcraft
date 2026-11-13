//! What the window asks of the library: polling it each frame, telling it a repository was
//! opened, and the start page's and dialogs' requests.

use std::path::{Path, PathBuf};

use super::{LibraryClick, model};
use crate::app::App;

impl App {
    pub(in crate::app) fn profile(&self) -> u64 {
        self.settings.active_profile
    }

    /// The window's part of the library: applying what scans sent, starting those due.
    pub(in crate::app) fn poll_library(&mut self, ctx: &egui::Context) {
        let active =
            ctx.input(|i| i.focused || i.raw.viewports.values().any(|v| v.focused == Some(true)));
        let profile = self.profile();
        self.library.tick(profile, active, ctx);
    }

    /// A local repository was opened (by the user): its siblings are looked for.
    pub(in crate::app) fn library_noticed_open(&mut self, root: &Path) {
        let profile = self.profile();
        self.library.noticed_open(profile, root);
    }

    /// Does what the start page's section asked.
    pub(in crate::app) fn library_click(&mut self, click: LibraryClick, ctx: &egui::Context) {
        let profile = self.profile();
        match click {
            LibraryClick::Open(path) => self.open_repo(&path, ctx),
            LibraryClick::Hide(path) => self.library.hide(profile, &path),
            LibraryClick::Manage => self.library_open = true,
            LibraryClick::AddRoot => {
                self.library_open = true;
                self.pick_library_root(None);
            }
        }
    }

    /// Asks for a folder: a new root, or root `id`'s new folder.
    pub(in crate::app) fn pick_library_root(&mut self, id: Option<u64>) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title(crate::i18n::text("interaction-library-pick"))
            .pick_folder()
        else {
            return;
        };
        let profile = self.profile();
        match id {
            Some(id) => self.library.move_root(profile, id, dir),
            None => self.library.add_root(profile, dir, 1),
        }
    }

    /// _Save as root_ after _Scan a folder_: as deep as the deepest repository it found
    /// there (at least 1, at most [`model::MAX_DEPTH`]); the roots dialog shows it.
    pub(in crate::app) fn save_scan_as_root(&mut self, root: PathBuf, deepest: usize) {
        let depth = deepest.clamp(1, usize::from(model::MAX_DEPTH)) as u8;
        let profile = self.profile();
        self.library.add_root(profile, root, depth);
        self.library_open = true;
    }
}
