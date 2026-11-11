//! Timeline saves and release of stale keeper refs.

use super::*;

impl RepoTab {
    /// Writes the timeline to disk in the background (not over a file that couldn't be read).
    pub(in crate::repo) fn save_timeline(&mut self, ctx: &egui::Context) {
        if !self.timeline.loaded || self.timeline.read_only {
            return;
        }
        let saved = Saved::of(&self.timeline, &self.path);
        let generation = store::next_generation();
        let folder = self.path.clone();
        self.then(
            ctx,
            move || store::save(&folder, &saved, generation),
            |tab, result, _| {
                if let Err(e) = result {
                    tab.timeline_error = Some(e);
                }
            },
        );
    }

    /// Lets go, in the background, of the commits of the entries forgotten.
    pub(in crate::repo) fn release_dropped(&mut self, ctx: &egui::Context) {
        let ids = std::mem::take(&mut self.timeline.dropped);
        if ids.is_empty() {
            return;
        }
        let (path, key) = (self.path.clone(), store::key(&self.path));
        self.then(
            ctx,
            move || snapshot::release(&path, &key, &ids),
            |tab, result, _| {
                if let Err(e) = result {
                    tab.timeline_error = Some(crate::i18n::message(
                        "repo-workflows-timeline-release-dropped-005d7b4f",
                        &[("e", (e).clone().into())],
                    ));
                }
            },
        );
    }

    /// Lets go of refs kept for entries no longer in the timeline.
    pub(in crate::repo::timeline::tab) fn release_stale_refs(&mut self, ctx: &egui::Context) {
        let ids: HashSet<u64> = self.timeline.entries.iter().map(|e| e.id).collect();
        let (path, key) = (self.path.clone(), store::key(&self.path));
        self.then(
            ctx,
            move || snapshot::release_others(&path, &key, &ids),
            |_, _, _| {},
        );
    }
}
