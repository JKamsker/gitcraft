//! Worktrees in the same tab: switching the active tab to one, closing it, parked views.

use std::path::Path;

use super::panels::Toast;
use super::{App, Tab, repo_root};
use crate::repo::RepoTab;

impl App {
    /// Moves the active tab to a worktree of its repository, when the settings say so, parking
    /// what it was showing. A worktree open in another tab just gets that tab shown.
    pub(super) fn open_worktree(&mut self, path: &Path, ctx: &egui::Context) {
        if !self.settings.worktrees_in_same_tab {
            self.open_repo(path, ctx);
            return;
        }
        let root = match worktree_root(path) {
            Ok(root) => root,
            Err(e) => {
                self.toasts.push(Toast::new(e, true));
                return;
            }
        };
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| matches!(t, Tab::Repo(r) if r.path == root))
        {
            self.active = i;
            return;
        }
        let parked = self.take_parked(&root);
        let Some(Tab::Repo(current)) = self.tabs.get_mut(self.active) else {
            self.open_repo(&root, ctx);
            return;
        };
        // The tab keeps the look it was given.
        if let Some(look) = self.tab_looks.get(&current.path).cloned() {
            self.tab_looks.entry(root.clone()).or_insert(look);
        }
        let came_back = parked.is_some();
        let mut next = parked.unwrap_or_else(|| Box::new(RepoTab::new(root, ctx)));
        next.continue_from(current);
        if came_back {
            next.resume(ctx);
        }
        let old = std::mem::replace(current, next);
        current.adopt_parked(old);
    }

    /// Closes the worktree the active tab shows and goes back to the repository's main one. What
    /// was going on in it ends, as when closing a tab; the worktree itself stays on disk.
    pub(super) fn close_current_worktree(&mut self, ctx: &egui::Context) {
        let main = match self.tabs.get(self.active) {
            Some(Tab::Repo(r)) => r.main_worktree().map(worktree_root),
            _ => return,
        };
        let root = match main {
            Some(Ok(root)) => root,
            // Not found (its folder gone): nothing to go back to, so closing the worktree closes
            // the tab, and says why. (One elsewhere is never looked up here: see
            // `worktree_root`.)
            Some(Err(e)) => {
                self.toasts.push(Toast::new(e, true));
                self.close_tab(self.active, ctx);
                return;
            }
            // No main worktree to show (a bare repository): this is the end of the tab.
            None => {
                self.close_tab(self.active, ctx);
                return;
            }
        };
        let shows_main = |t: &Tab| matches!(t, Tab::Repo(r) if r.path == root);
        if self.tabs.iter().any(shows_main) {
            self.close_tab(self.active, ctx);
            if let Some(i) = self.tabs.iter().position(shows_main) {
                self.active = i;
            }
            return;
        }
        let parked = self.take_parked(&root);
        let Some(Tab::Repo(current)) = self.tabs.get_mut(self.active) else {
            return;
        };
        let came_back = parked.is_some();
        let mut next = parked.unwrap_or_else(|| Box::new(RepoTab::new(root.clone(), ctx)));
        next.continue_from(current);
        next.inherit_parked(current);
        if came_back {
            next.resume(ctx);
        }
        *current = next;
    }

    /// Takes the parked view of the folder `path` out of whichever tab has it.
    pub(super) fn take_parked(&mut self, path: &Path) -> Option<Box<RepoTab>> {
        self.tabs.iter_mut().find_map(|tab| match tab {
            Tab::Repo(r) => r.take_parked(path),
            Tab::Home | Tab::Settings => None,
        })
    }
}

/// The top folder of the worktree at `path`, in the form tabs are told apart by. One on
/// another machine is one git listed, already its top folder: not asked again (a round trip on
/// the UI thread).
fn worktree_root(path: &Path) -> Result<std::path::PathBuf, String> {
    if crate::remote::is_remote(path) {
        Ok(path.to_owned())
    } else {
        repo_root(path)
    }
}
