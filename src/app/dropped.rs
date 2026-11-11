//! Files and folders dropped on the window: a folder opens as a repository, a patch is applied
//! to the repository tab it was dropped on, one at a time in the order dropped.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Align2, Color32, Id, LayerId, Order, Stroke};

use super::home::AppError;
use super::panels::Toast;
use super::{App, Tab, repo_root};
use crate::repo::RepoTab;
use crate::theme::{self, Tokens};

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
    if has_patch_extension(path) {
        DropKind::Patch
    } else {
        DropKind::Other
    }
}

fn has_patch_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    PATCH_EXTENSIONS.contains(&ext.as_str())
}

/// What the dropped `path` is. One on another machine (a `\\wsl.localhost\…` path) is told by
/// its name only, never by asking that machine from the window's thread: a patch by its
/// extension, anything else a folder (opening it says if it isn't one).
pub(super) fn kind_of(path: &Path) -> DropKind {
    if crate::remote::is_remote(path) {
        return if has_patch_extension(path) {
            DropKind::Patch
        } else {
            DropKind::Folder
        };
    }
    classify(path, path.is_dir())
}

/// The name of a dropped item, for a message.
pub(super) fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// What to say when a dropped folder can't be opened: git's reason, or plainly that it isn't in
/// a repository.
pub(super) fn retained_folder_error(folder: &Path, err: &str) -> AppError {
    let lower = err.to_lowercase();
    AppError::folder(
        folder,
        err,
        lower.contains("not a git repository") || lower.contains("not inside a git repository"),
    )
}

#[cfg(test)]
fn folder_error(folder: &Path, err: &str) -> String {
    retained_folder_error(folder, err).delivery_text()
}

/// The repository tab of `repo` among `tabs`.
fn repo_tab<'a>(tabs: &'a mut [&mut Tab], repo: &Path) -> Option<&'a mut RepoTab> {
    tabs.iter_mut().find_map(|t| match &mut **t {
        Tab::Repo(r) if r.path == repo => Some(&mut **r),
        _ => None,
    })
}

/// The dropped patches, each with the repository whose tab it was dropped on, applied one at a
/// time; and the one being applied.
#[derive(Default)]
pub(super) struct PatchQueue {
    waiting: VecDeque<(PathBuf, PathBuf)>,
    current: Option<(PathBuf, PathBuf)>,
}

/// The queue's session boundary also permits recording-only continuation tests.
trait PatchTargets {
    fn busy(&mut self, repo: &Path) -> Option<bool>;
    fn outcome(&mut self, repo: &Path) -> Option<Result<(), String>>;
    fn apply(&mut self, repo: &Path, file: &Path) -> Result<(), String>;
}

struct LiveTargets<'a, 'b> {
    tabs: &'a mut [&'b mut Tab],
    ctx: &'a egui::Context,
}

impl PatchTargets for LiveTargets<'_, '_> {
    fn busy(&mut self, repo: &Path) -> Option<bool> {
        repo_tab(self.tabs, repo).map(|tab| tab.dropped_patch_waits())
    }
    fn outcome(&mut self, repo: &Path) -> Option<Result<(), String>> {
        repo_tab(self.tabs, repo).and_then(RepoTab::take_dropped_patch_outcome)
    }
    fn apply(&mut self, repo: &Path, file: &Path) -> Result<(), String> {
        repo_tab(self.tabs, repo)
            .ok_or_else(|| closed(file))?
            .apply_dropped_patch(file.to_path_buf(), self.ctx)
    }
}

impl PatchQueue {
    pub(super) fn push(&mut self, file: PathBuf, repo: PathBuf) {
        self.waiting.push_back((file, repo));
    }

    pub(super) fn is_empty(&self) -> bool {
        self.waiting.is_empty() && self.current.is_none()
    }

    /// Takes in how the patch being applied went and gives the next one to its tab, until one
    /// has to wait. Returns what to tell (each an error).
    #[cfg(test)]
    pub(super) fn advance(&mut self, tabs: &mut [Tab], ctx: &egui::Context) -> Vec<String> {
        self.advance_in(&mut tabs.iter_mut().collect::<Vec<_>>(), ctx)
    }

    fn advance_in(&mut self, tabs: &mut [&mut Tab], ctx: &egui::Context) -> Vec<String> {
        self.advance_with(&mut LiveTargets { tabs, ctx })
    }

    fn advance_with(&mut self, targets: &mut impl PatchTargets) -> Vec<String> {
        let mut said = Vec::new();
        loop {
            if let Some((file, repo)) = self.current.clone() {
                let Some(busy) = targets.busy(&repo) else {
                    self.current = None;
                    said.push(closed(&file));
                    continue;
                };
                match targets.outcome(&repo) {
                    None if busy => return said,
                    None | Some(Ok(())) => self.current = None,
                    Some(Err(e)) => {
                        self.current = None;
                        said.push(self.failed(&file, &repo, &e));
                    }
                }
            }
            let Some((file, repo)) = self.waiting.pop_front() else {
                return said;
            };
            let Some(busy) = targets.busy(&repo) else {
                said.push(closed(&file));
                continue;
            };
            if busy {
                self.waiting.push_front((file, repo));
                return said;
            }
            match targets.apply(&repo, &file) {
                Ok(()) => self.current = Some((file, repo)),
                Err(e) => said.push(crate::i18n::message(
                    "interaction-drop-not-applied",
                    &[
                        ("name", name_of(&file).into()),
                        ("error", crate::git::display_error(&e).into()),
                    ],
                )),
            }
        }
    }

    /// `file` failed in `repo`: the patches dropped on it after it are left out.
    fn failed(&mut self, file: &Path, repo: &Path, err: &str) -> String {
        let before = self.waiting.len();
        self.waiting.retain(|(_, r)| r != repo);
        let left = before - self.waiting.len();
        crate::i18n::message(
            if left > 0 {
                "interaction-drop-failed-following"
            } else {
                "interaction-drop-failed"
            },
            &[
                ("name", name_of(file).into()),
                ("error", crate::git::display_error(err).into()),
                ("count", left.into()),
            ],
        )
    }
}

/// What is said of a patch whose repository's tab was closed before it was applied.
fn closed(file: &Path) -> String {
    crate::i18n::message("interaction-drop-closed", &[("name", name_of(file).into())])
}

#[cfg(test)]
#[path = "dropped/recording_tests.rs"]
mod recording_tests;
impl App {
    /// Takes in what was dropped on the window and handles it: folders open, patches go to the
    /// repository tab they were dropped on. Shows a hint while files are dragged over it.
    pub(super) fn dropped_files(&mut self, ctx: &egui::Context) {
        let (dropped, hovered) =
            ctx.input(|i| (i.raw.dropped_files.clone(), !i.raw.hovered_files.is_empty()));
        // The tab dropped on, before a dropped folder opens another.
        let target = match self.tabs.get(self.active) {
            Some(Tab::Repo(r)) => Some(r.path.clone()),
            _ => None,
        };
        for file in dropped {
            let path = file.path().to_owned();
            match kind_of(&path) {
                DropKind::Folder => self.open_dropped_folder(&path, ctx),
                DropKind::Patch => match &target {
                    Some(repo) => self.patches.push(path, repo.clone()),
                    None => {
                        let msg = self.no_repo_for(&path);
                        self.report(msg);
                    }
                },
                DropKind::Other => self.report(AppError::unsupported(name_of(&path))),
            }
        }
        if hovered {
            let on_repo = target.is_some();
            drop_hint(ctx, on_repo);
        }
    }

    /// Continuations belong to model ticks, including passes with no visible UI.
    pub(super) fn advance_patches(&mut self, ctx: &egui::Context) {
        if !self.patches.is_empty() {
            let mut tabs = self
                .tabs
                .iter_mut()
                .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
                .collect::<Vec<_>>();
            for msg in self.patches.advance_in(&mut tabs, ctx) {
                self.toasts.push(Toast::new(msg, true));
            }
            if !self.patches.is_empty() {
                // Its tab may be busy with something that doesn't wake the window when done.
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
    }

    /// Why a patch dropped where no repository is shown wasn't applied.
    fn no_repo_for(&self, patch: &Path) -> AppError {
        AppError::patch_target(
            self.tabs.iter().any(|t| matches!(t, Tab::Repo(_))),
            &name_of(patch),
        )
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
            Err(e) => self.report(retained_folder_error(path, &e)),
        }
    }
}

#[cfg(test)]
fn patch_target_message(has_repo: bool, name: &str) -> String {
    AppError::patch_target(has_repo, name).delivery_text()
}

/// A veil over the window while files are dragged over it, saying what a drop does there.
fn drop_hint(ctx: &egui::Context, on_repo: bool) {
    let t = Tokens::get(ctx);
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("drop-hint")));
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(110));
    let text = if on_repo {
        crate::i18n::text("interaction-drop-hint-repo")
    } else {
        crate::i18n::text("interaction-drop-hint-folder")
    };
    let galley = painter.layout_no_wrap(text.to_owned(), theme::semibold(16.0), t.text);
    let pill = Align2::CENTER_CENTER
        .anchor_size(rect.center(), galley.size())
        .expand2(egui::vec2(22.0, 14.0));
    painter.rect(
        pill,
        t.radius_lg,
        t.card,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
    painter.galley(pill.shrink2(egui::vec2(22.0, 14.0)).min, galley, t.text);
}

#[cfg(test)]
#[path = "dropped/localization_tests.rs"]
mod localization_tests;
#[cfg(test)]
#[path = "dropped/tests.rs"]
mod tests;
