//! Repository tab opening, error reporting, and recent-history updates.

use super::*;

impl App {
    /// Opens `path` as the start page's "Open" button does (for `examples/ui_shot.rs`).
    pub fn open_tab(&mut self, path: &Path, ctx: &egui::Context) {
        self.open_repo(path, ctx);
    }

    /// Opens `path` in the active tab if it's a start page, otherwise in a new tab. One on
    /// another machine opens once it is reached, in the background.
    pub(super) fn open_repo(&mut self, path: &Path, ctx: &egui::Context) {
        if self.focus_elsewhere(path, ctx) {
            return;
        }
        if crate::remote::is_remote(path) {
            // Already open: no need to reach it first.
            if let Some(i) = self
                .tabs
                .iter()
                .position(|t| matches!(t, Tab::Repo(r) if r.path == path))
            {
                self.remember(path);
                self.active = i;
                return;
            }
            self.connect(path.to_owned(), None, ctx);
            return;
        }
        match repo_root(path) {
            Ok(root) => self.open_root(root, ctx),
            Err(e) => {
                let old = self.error_card.take();
                self.error_card = match old {
                    // The same again: the card stays as it is.
                    Some(mut card) if card.is_about(&e) && card.dir() == path => {
                        card.failed_again();
                        Some(card)
                    }
                    old => match crate::error_card::ErrorCard::for_error(&e, path, "Open", ctx) {
                        Some(card) => Some(card.replacing(old)),
                        // A fix made stays undoable.
                        None => old.filter(|card| card.fixed()),
                    },
                };
                self.report(e);
            }
        }
    }

    /// Says why a repository couldn't be opened: on the start page, and in a toast when
    /// another tab is shown (where the start page's message would go unseen).
    pub(super) fn report(&mut self, e: impl Into<AppError>) {
        let e = e.into();
        if !matches!(self.tabs.get(self.active), Some(Tab::Home)) {
            self.toasts.push(Toast::from_app_error(&e));
        }
        self.error = Some(e);
    }

    /// Opens the repository whose top folder is `root` (see [`Self::open_repo`]).
    pub(super) fn open_root(&mut self, root: PathBuf, ctx: &egui::Context) {
        if self.focus_elsewhere(&root, ctx) {
            return;
        }
        self.remember(&root);
        if !crate::remote::is_remote(&root) {
            self.library_noticed_open(&root);
        }
        self.error = None;
        self.error_card.take_if(|card| !card.fixed());

        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| matches!(t, Tab::Repo(r) if r.path == root))
        {
            self.active = i;
            return;
        }
        // A worktree parked in some tab comes out with everything it had.
        let parked = self.take_parked(&root);
        let came_back = parked.is_some();
        let mut view = parked.unwrap_or_else(|| Box::new(RepoTab::new(root, ctx)));
        if came_back {
            view.resume(ctx);
        }
        let tab = Tab::Repo(view);
        if matches!(self.tabs.get(self.active), Some(Tab::Home)) {
            self.tabs[self.active] = tab;
        } else {
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
        }
    }

    pub(super) fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(MAX_RECENT);
        self.last_opened
            .insert(path.to_owned(), home_recent::now_secs());
        // Only what some profile's recent list still has.
        let kept: Vec<&PathBuf> = self
            .recent
            .iter()
            .chain(self.profile_states.values().flat_map(|s| &s.recent))
            .collect();
        self.last_opened.retain(|p, _| kept.contains(&p));
    }
}
