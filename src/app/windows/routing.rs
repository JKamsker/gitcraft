//! Window routing, restoration, and live session transfer.

use super::*;

impl App {
    /// Run a model completion against its initiating window, regardless of which
    /// viewport polled it. A closed owner falls back to the root, never a repainting peer.
    pub(in crate::app) fn in_window<R>(
        &mut self,
        requested: ViewportId,
        work: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let previous = self.windows.current;
        let owner = if requested == previous || self.windows.parked.contains_key(&requested) {
            requested
        } else {
            ViewportId::ROOT
        };
        if owner == previous {
            return work(self);
        }
        let Some(mut view) = self.windows.parked.remove(&owner) else {
            return work(self);
        };
        view.swap(self);
        self.windows.parked.insert(previous, view);
        self.windows.current = owner;
        let result = work(self);
        if let Some(mut view) = self.windows.parked.remove(&previous) {
            view.swap(self);
            self.windows.parked.insert(owner, view);
        }
        self.windows.current = previous;
        result
    }

    pub(in crate::app) fn window_has_dialog(&self) -> bool {
        self.clone_form.is_some()
            || self.practice_open
            || self.remote_form.is_some()
            || self.teleport.is_some()
            || self.repo_scan.is_some()
            || self.library_open
            || self.my_work.open
            || self.askpass_open()
    }

    pub(in crate::app) fn credentials_for(
        &mut self,
        path: &Path,
        credentials: crate::git::Credentials,
    ) {
        if let Some(repo) = self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
            .find_map(|tab| match tab {
                Tab::Repo(repo) if repo.path == path => Some(repo),
                _ => None,
            })
        {
            repo.set_credentials(credentials);
        }
    }

    /// Poll window-owned continuation state even if none of its UI is painted.
    pub(in crate::app) fn poll_window_models(&mut self, ctx: &egui::Context) {
        let owners = std::iter::once(self.windows.current)
            .chain(self.windows.parked.keys().copied())
            .collect::<Vec<_>>();
        for owner in owners {
            self.in_window(owner, |app| {
                app.poll_teleport(ctx);
                app.poll_repo_scan();
                app.poll_my_work(ctx);
            });
        }
    }

    pub(in crate::app) fn restore_windows(
        &mut self,
        windows: Vec<SavedWindow>,
        ctx: &egui::Context,
    ) {
        let mut known = self
            .all_repo_paths()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        for saved in windows {
            let mut kept = Vec::new();
            let mut tabs = Vec::new();
            for path in saved.open {
                let keep = super::super::tabs::reopenable(&path) && known.insert(path.clone());
                kept.push(keep);
                if keep {
                    tabs.push(Tab::Repo(Box::new(crate::repo::RepoTab::new_deferred(
                        path, ctx,
                    ))));
                }
            }
            if tabs.is_empty() {
                continue;
            }
            let id = self.next_window_id();
            let mut window = Window::new(
                tabs,
                ViewportId::ROOT,
                // The saved monitor may have been disconnected. Let the compositor
                // place restored windows on an available monitor, rather than reuse
                // an unverifiable absolute origin from a different scale/layout.
                None,
            );
            window.active =
                super::super::active_among_kept(&kept, saved.active).min(window.tabs.len() - 1);
            window.size = saved
                .size
                .map(egui::Vec2::from)
                .filter(|s| s.is_finite() && s.x >= 800.0 && s.y >= 500.0);
            self.windows.parked.insert(id, window);
        }
    }

    pub(super) fn next_window_id(&mut self) -> ViewportId {
        self.windows.serial += 1;
        ViewportId::from_hash_of(("repository-window", self.windows.serial))
    }

    pub(in crate::app) fn saved_windows(&self) -> Vec<SavedWindow> {
        self.windows
            .parked
            .values()
            .map(Window::saved)
            .filter(|w| !w.open.is_empty())
            .collect()
    }

    pub(in crate::app) fn all_repo_paths(&self) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .chain(self.windows.parked.values().flat_map(|w| &w.tabs))
            .filter_map(repo_path)
            .collect()
    }

    pub(in crate::app) fn focus_elsewhere(&mut self, path: &Path, ctx: &egui::Context) -> bool {
        for (&id, w) in &mut self.windows.parked {
            if let Some(i) = w
                .tabs
                .iter()
                .position(|t| repo_path(t).as_deref() == Some(path))
            {
                w.active = i;
                self.windows.focused_elsewhere = true;
                show(ctx, id);
                return true;
            }
        }
        false
    }

    pub(in crate::app) fn window_targets(&self) -> Vec<(ViewportId, String)> {
        let mut targets = self
            .windows
            .parked
            .iter()
            .map(|(&id, w)| {
                let label = if id == ViewportId::ROOT {
                    crate::i18n::text("interaction-tabs-main-window").into()
                } else {
                    w.tabs
                        .get(w.active)
                        .and_then(repo_path)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| crate::i18n::text("interaction-tabs-new-tab").into())
                };
                (id, label)
            })
            .collect::<Vec<_>>();
        if self.windows.current != ViewportId::ROOT
            && !targets.iter().any(|(id, _)| *id == ViewportId::ROOT)
        {
            targets.push((
                ViewportId::ROOT,
                crate::i18n::text("interaction-tabs-main-window").into(),
            ));
        }
        targets
    }

    /// Moves exactly the captured session into an existing window or a newly allocated one.
    pub(in crate::app) fn move_to_window(
        &mut self,
        index: usize,
        target: Option<ViewportId>,
        position: Option<Pos2>,
        ctx: &egui::Context,
    ) {
        if !self.windows.enabled
            || ctx.embed_viewports()
            || !matches!(self.tabs.get(index), Some(Tab::Repo(_)))
        {
            return;
        }
        if target == Some(self.windows.current) {
            return;
        }
        if let Some(id) = target
            && !self.windows.parked.contains_key(&id)
        {
            return;
        }
        let mut tab = self.tabs.remove(index);
        self.tab_drag = None;
        self.teleport_drag = None;
        self.palette = None;
        // Invalidate any prepared keyboard mutation across ownership transfer, even if the
        // same path/hunk still appears in the new window.
        if let Tab::Repo(r) = &mut tab {
            r.cancel_keyboard_ownership(ctx);
        }
        let id = match target {
            Some(id) => {
                if let Some(window) = self.windows.parked.get_mut(&id) {
                    if window.tabs.len() == 1 && matches!(window.tabs.first(), Some(Tab::Home)) {
                        window.tabs.clear();
                    }
                    window.tabs.push(tab);
                    window.active = window.tabs.len() - 1;
                }
                id
            }
            None => {
                let id = self.next_window_id();
                let mut window = Window::new(vec![tab], self.windows.current, None);
                window.place_at_pixels = position;
                self.windows.parked.insert(id, window);
                id
            }
        };
        if self.tabs.is_empty() {
            self.tabs.push(Tab::Home);
        }
        self.active = self
            .active
            .saturating_sub(usize::from(self.active >= index))
            .min(self.tabs.len() - 1);
        show(ctx, id);
        ctx.request_repaint();
    }
}
