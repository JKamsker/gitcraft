//! Files and folders dropped on the window: a folder opens as a repository, a patch is applied
//! to the repository shown, one at a time in the order dropped.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Align2, Color32, Id, LayerId, Order};

use super::panels::Toast;
use super::{App, Tab, repo_root};
use crate::theme;

/// The extensions of the patch files a drop applies (as _Apply patch_ offers them).
const PATCH_EXTENSIONS: [&str; 4] = ["patch", "diff", "eml", "mbox"];

/// What a dropped item is, for what to do with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DropKind {
    Folder,
    Patch,
    Other,
}

/// What `path` is: a folder (`is_dir`), a patch by its extension (any case), or anything else.
pub(super) fn classify(path: &Path, is_dir: bool) -> DropKind {
    if is_dir {
        return DropKind::Folder;
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if PATCH_EXTENSIONS.contains(&ext.as_str()) {
        DropKind::Patch
    } else {
        DropKind::Other
    }
}

/// The name of a dropped item, for a message.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// What to say when a dropped folder can't be opened: git's reason, or plainly that it isn't in
/// a repository.
pub(super) fn folder_error(folder: &Path, err: &str) -> String {
    let name = name_of(folder);
    if err.to_lowercase().contains("not a git repository") {
        format!("{name} isn't in a git repository: drop a repository's folder to open it")
    } else {
        format!("Could not open {name}: {err}")
    }
}

/// What came of giving a dropped patch to the tab.
enum Applied {
    /// The tab is busy: it is given again later.
    Wait,
    /// Given to it, or refused with a message.
    Done,
    /// The patch dropped before it failed: it and the rest are left out.
    Stopped,
}

/// The dropped items still to be handled, in order.
#[derive(Default)]
pub(super) struct Drops {
    queue: VecDeque<(PathBuf, DropKind)>,
    /// A patch of this drop was given to the tab: the next waits until it is done, and stops
    /// if it failed.
    applied: bool,
}

impl App {
    /// Takes in what was dropped on the window, handles it one item at a time, and shows a
    /// hint while files are dragged over the window.
    pub(super) fn dropped_files(&mut self, ctx: &egui::Context) {
        let (dropped, hovered) =
            ctx.input(|i| (i.raw.dropped_files.clone(), !i.raw.hovered_files.is_empty()));
        for file in dropped {
            let path = file.path().to_owned();
            let kind = classify(&path, path.is_dir());
            self.drops.queue.push_back((path, kind));
        }
        self.handle_drops(ctx);
        if hovered {
            drop_hint(ctx);
        }
    }

    /// Handles the dropped items until one has to wait (a patch being applied).
    fn handle_drops(&mut self, ctx: &egui::Context) {
        while let Some((path, kind)) = self.drops.queue.front().cloned() {
            match kind {
                DropKind::Patch => match self.apply_dropped(&path, ctx) {
                    Applied::Wait => {
                        // Waits for the tab: looked at again shortly.
                        ctx.request_repaint_after(Duration::from_millis(150));
                        return;
                    }
                    Applied::Done => {
                        self.drops.queue.pop_front();
                    }
                    Applied::Stopped => self.stop_patches(),
                },
                DropKind::Folder => {
                    self.drops.queue.pop_front();
                    self.open_dropped_folder(&path, ctx);
                }
                DropKind::Other => {
                    self.drops.queue.pop_front();
                    self.report(format!(
                        "Can't open {}: drop a folder to open it, or a .patch, .diff, .eml or \
                         .mbox file to apply it",
                        name_of(&path)
                    ));
                }
            }
        }
        self.drops.applied = false;
    }

    /// Gives the patch `path` to the repository tab shown, once it is free.
    fn apply_dropped(&mut self, path: &Path, ctx: &egui::Context) -> Applied {
        let Some(Tab::Repo(repo)) = self.tabs.get_mut(self.active) else {
            self.report(format!(
                "Applying {} needs a repository open first: open one, then drop the patch on it",
                name_of(path)
            ));
            return Applied::Done;
        };
        if repo.applying_patch() {
            return Applied::Wait;
        }
        if self.drops.applied && repo.last_failed() {
            return Applied::Stopped;
        }
        repo.apply_dropped_patch(path.to_owned(), ctx);
        self.drops.applied = true;
        Applied::Done
    }

    /// The patch before failed: the patches dropped after it are left out, and a toast says so.
    fn stop_patches(&mut self) {
        let before = self.drops.queue.len();
        self.drops.queue.retain(|(_, k)| *k != DropKind::Patch);
        let left = before - self.drops.queue.len();
        self.drops.applied = false;
        self.toasts.push(Toast::new(
            format!(
                "The patch before failed: {left} more dropped {} not applied",
                if left == 1 { "was" } else { "were" }
            ),
            true,
        ));
    }

    /// Opens the dropped folder's repository, or says why it can't.
    fn open_dropped_folder(&mut self, path: &Path, ctx: &egui::Context) {
        // One in WSL is reached in the background, as when opened otherwise.
        if crate::remote::is_remote(path) {
            self.open_repo(path, ctx);
            return;
        }
        match repo_root(path) {
            Ok(root) => self.open_root(root, ctx),
            Err(e) => self.report(folder_error(path, &e)),
        }
    }
}

/// A veil over the window while files are dragged over it, saying what a drop does.
fn drop_hint(ctx: &egui::Context) {
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("drop-hint")));
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(140));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Drop a folder to open it, or a patch to apply it",
        theme::semibold(16.0),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_items_are_told_apart() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(classify(&p("/x/repo"), true), DropKind::Folder);
        // A folder named like a patch is still a folder.
        assert_eq!(classify(&p("/x/fix.patch"), true), DropKind::Folder);
        assert_eq!(classify(&p("/x/fix.patch"), false), DropKind::Patch);
        assert_eq!(classify(&p("/x/FIX.DIFF"), false), DropKind::Patch);
        assert_eq!(classify(&p("/x/mail.Eml"), false), DropKind::Patch);
        assert_eq!(classify(&p("/x/all.mbox"), false), DropKind::Patch);
        assert_eq!(
            classify(&p("/home/me/my patches/0001-Größe ändern.patch"), false),
            DropKind::Patch
        );
        assert_eq!(classify(&p("/x/notes.txt"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/Makefile"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/.patch"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/fix.patch.txt"), false), DropKind::Other);
        assert_eq!(classify(&p("/x/日本語.diff"), false), DropKind::Patch);
    }

    #[test]
    fn a_folder_outside_a_repository_says_so() {
        let msg = folder_error(
            Path::new("/home/me/My Stuff"),
            "fatal: not a git repository (or any of the parent directories): .git",
        );
        assert_eq!(
            msg,
            "My Stuff isn't in a git repository: drop a repository's folder to open it"
        );
        assert_eq!(
            folder_error(Path::new("/x/y"), "detected dubious ownership"),
            "Could not open y: detected dubious ownership"
        );
    }
}
