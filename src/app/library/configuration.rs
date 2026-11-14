//! User edits to configured roots and Nearby discovery.

use super::*;

impl LibraryState {
    /// Adds `path` as a root `depth` deep (a root already, at least that deep) and scans it.
    /// A path that isn't valid UTF-8 can't be saved: it isn't added.
    pub fn add_root(&mut self, profile: u64, path: PathBuf, depth: u8) {
        if path.to_str().is_none() {
            return;
        }
        let lib = self.lib(profile);
        let before = lib
            .root_at(&path, None)
            .and_then(|id| lib.root(id))
            .map(|r| r.depth);
        let id = lib.add_root(path, depth);
        let after = lib.root(id).map(|r| r.depth);
        // Deeper now: a scan under way for the old depth is dropped.
        if before.is_some() && before != after {
            self.scheduler.retire(&Source::Root(id));
        }
        self.rescan(profile, &Source::Root(id));
    }

    /// Takes root `id` away, with what only it found. Tabs, Recent and workspaces stay.
    pub fn remove_root(&mut self, profile: u64, id: u64) {
        self.scheduler.retire(&Source::Root(id));
        self.lib(profile).remove_root(id);
    }

    pub fn set_enabled(&mut self, profile: u64, id: u64, on: bool) {
        self.scheduler.retire(&Source::Root(id));
        self.lib(profile).set_enabled(id, on);
        if on {
            self.rescan(profile, &Source::Root(id));
        }
    }

    /// Scans root `id` `depth` deep from now on (a scan under way for the old depth is
    /// dropped; the complete one that follows settles what is in reach).
    pub fn set_depth(&mut self, profile: u64, id: u64, depth: u8) {
        self.scheduler.retire(&Source::Root(id));
        if let Some(root) = self.lib(profile).root_mut(id) {
            root.depth = depth.min(model::MAX_DEPTH);
        }
        self.rescan(profile, &Source::Root(id));
    }

    /// Root `id` is `path` now: what the old folder found goes. Not when another root has that
    /// folder already (nor for a path that couldn't be saved).
    pub fn move_root(&mut self, profile: u64, id: u64, path: PathBuf) {
        let taken = self
            .get(profile)
            .is_some_and(|l| l.root_at(&path, Some(id)).is_some());
        if taken || path.to_str().is_none() {
            return;
        }
        let source = Source::Root(id);
        self.scheduler.retire(&source);
        let lib = self.lib(profile);
        lib.forget_source(&source);
        if let Some(root) = lib.root_mut(id) {
            root.path = path;
            root.last = None;
        }
        self.rescan(profile, &source);
    }

    pub fn set_nearby(&mut self, profile: u64, on: bool) {
        if !on {
            self.checks.retire_all();
        }
        if !on && let Some(lib) = self.get(profile) {
            for n in lib.nearby.clone() {
                self.scheduler.retire(&Source::Nearby(n.parent));
            }
        }
        self.lib(profile).set_nearby_enabled(on);
    }

    pub fn forget_nearby(&mut self, profile: u64, parent: &Path) {
        self.checks.retire_nearby(profile, parent);
        let source = Source::Nearby(parent.to_owned());
        self.scheduler.retire(&source);
        let lib = self.lib(profile);
        if let Some(at) = lib.nearby_at(parent) {
            lib.nearby.remove(at);
        }
        lib.forget_source(&source);
    }

    pub fn stop(&mut self, source: &Source) {
        self.scheduler.stop(source);
    }

    pub fn hide(&mut self, profile: u64, path: &Path) {
        self.lib(profile).hide(path);
    }

    pub fn unhide_all(&mut self, profile: u64) {
        self.lib(profile).hidden.clear();
    }
}
