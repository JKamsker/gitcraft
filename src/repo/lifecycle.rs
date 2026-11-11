//! A tab's life while it isn't looked at. A tab restored on start reads nothing and watches
//! nothing until it is first shown, so fifteen saved tabs no longer mean fifteen history loads
//! and fifteen folder watchers at startup. A tab left in the background for [`IDLE_AFTER`],
//! with nothing going on in it, is suspended: it drops its graph, its diff and what the detail
//! pane read, stops watching the folder, and neither fetches nor follows builds; shown again,
//! it watches and reads afresh, then selects the same commit and opens the same diff.

use std::time::{Duration, Instant};

/// How long a tab stays in the background before it is suspended.
pub(crate) const IDLE_AFTER: Duration = Duration::from_secs(30 * 60);

/// Where a tab is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Made (restored on start), nothing read or watched yet.
    Deferred,
    /// Read and watched.
    Live,
    /// Was live; its caches are dropped and it watches nothing until shown.
    Suspended,
}

/// What a tab has to do on a change of its [`Phase`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Nothing,
    /// Start watching and read for the first time.
    Start,
    /// Drop the caches and stop watching.
    Suspend,
    /// Watch again and read again.
    Resume,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Lifecycle {
    pub phase: Phase,
    /// When the tab was last shown (or made).
    shown_at: Instant,
}

impl Lifecycle {
    pub fn new(now: Instant) -> Self {
        Self {
            phase: Phase::Deferred,
            shown_at: now,
        }
    }

    /// The tab is shown (or something asked for it to be read, as shown).
    pub fn shown(&mut self, now: Instant) -> Step {
        self.shown_at = now;
        let step = match self.phase {
            Phase::Deferred => Step::Start,
            Phase::Suspended => Step::Resume,
            Phase::Live => Step::Nothing,
        };
        self.phase = Phase::Live;
        step
    }

    /// The tab is in the background at `now`; `idle` says whether nothing goes on in it that
    /// suspending would end or lose (an operation, a terminal, something typed, an editor
    /// open). Asked only once the tab is live and has been in the background long enough.
    pub fn background(&mut self, now: Instant, idle: impl FnOnce() -> bool) -> Step {
        let long_enough = now.saturating_duration_since(self.shown_at) >= IDLE_AFTER;
        if self.phase == Phase::Live && long_enough && idle() {
            self.phase = Phase::Suspended;
            Step::Suspend
        } else {
            Step::Nothing
        }
    }

    /// Nothing is read or watched now: deferred or suspended.
    pub fn sleeping(&self) -> bool {
        self.phase != Phase::Live
    }
}

impl super::RepoTab {
    /// Retire feature reads without cancelling or settling submitted mutations.
    pub(crate) fn invalidate_repository_context(&mut self) {
        self.search.retire();
        self.invalidate_repository_tool_requests();
        self.cochanges.invalidate_context();
        for parked in &mut self.parked {
            parked.invalidate_repository_context();
        }
    }

    /// The tab is shown: read it for the first time, or again after it was suspended.
    pub(crate) fn shown_now(&mut self, ctx: &egui::Context) {
        let step = self.life.shown(Instant::now());
        match step {
            Step::Start | Step::Resume => {
                self.invalidate_repository_context();
                // The opening screen's delay counts from now, not from when the tab was made.
                self.opened = Instant::now();
                self.resuming = step == Step::Resume;
                if step == Step::Start {
                    if !self.drafts_loaded {
                        self.restore_drafts();
                    }
                    self.load_timeline(ctx);
                    // As last read, until git has read it anew.
                    self.show_snapshot(ctx);
                }
                self.watch(ctx);
                self.reload(ctx);
            }
            Step::Nothing | Step::Suspend => {}
        }
    }

    /// The tab is in the background: suspends it once it has been idle long enough.
    pub(crate) fn background_tick(&mut self) {
        let mut life = self.life;
        let step = life.background(Instant::now(), || self.idle());
        self.life = life;
        if step == Step::Suspend {
            self.suspend();
        }
    }

    /// Once read again after a suspension: the commit and the diff it showed.
    pub(super) fn reopen_after_resume(&mut self, ctx: &egui::Context) {
        let hash = self.resume_select.take();
        let diff = self.resume_diff.take();
        // Selected again only if it is still among what was read, never by paging further.
        if let Some(hash) = hash
            && let Some(&row) = self.index.get(&hash)
        {
            self.selected = Some(row);
            self.scroll_to = Some(row);
            if hash != super::WIP_HASH && self.stash_view.is_none() {
                self.load_detail(hash, ctx);
            }
        }
        if let Some(key) = diff {
            self.open_diff(key, ctx);
        }
    }

    /// Nothing is read or watched now (see [`Lifecycle::sleeping`]).
    pub(crate) fn sleeping(&self) -> bool {
        self.life.sleeping()
    }

    /// Nothing goes on that suspending would end or lose.
    fn idle(&mut self) -> bool {
        !self.local_ci.busy()
            && !self.is_busy()
            && !self.loading_more
            && !self.counting_worktrees
            && self.queued.is_empty()
            // A pull request or the builds being followed.
            && self.pr_view.is_none()
            && self.ci_view.is_none()
            && self.unfinished_anywhere().is_none()
    }

    /// Drops what the tab read and stops watching; the commit shown is selected again once it
    /// is read afresh (see [`Self::shown_now`]).
    fn suspend(&mut self) {
        // Only idle tabs suspend: busy mutations retain this receiver until op_done.
        self.invalidate_repository_context();
        // Cancel captures and child processes from this lifetime, and disconnect their
        // results so an old read cannot repopulate a suspended/resumed view.
        if let Some(view) = &mut self.file_view {
            view.retire_reads();
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self._jobs = crate::tasks::Scope::default();
        self.tx = super::messages::JobSender {
            sender,
            forge_generation: None,
            forge_operation: false,
            cancel: self._jobs.0.clone(),
            repo: super::messages::mutation_key(&self.path),
        };
        self.rx = receiver;
        self.watcher = None;
        self.poller = None;
        self.fs_rx = None;
        self.fs_pending = None;
        // An open `git log` reading older pages.
        *self
            .log_stream
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.resume_select = self.selected_hash();
        self.resume_diff = self.diff.as_ref().map(|d| d.key.clone());
        self.pending_goto = None;
        self.history = Vec::new();
        self.history_index.clear();
        self.commits = Vec::new();
        self.graph = crate::graph::Graph {
            rows: Vec::new().into(),
            width: 0,
        };
        self.index = Default::default();
        // These readers have private result channels as well as the tab channel.
        // Retire both queued captures and completed results with the old lifetime.
        self.band_reader = Default::default();
        self.topology_reader = Default::default();
        self.topology = Default::default();
        self.graph_narration = Default::default();
        self.history_minimap = Default::default();
        self.history_activity = None;
        self.atlas = None;
        self.growth = None;
        self.postcards = Default::default();
        self.graph_motion = Default::default();
        self.celebration = Default::default();
        self.has_more = false;
        self.selected = None;
        self.scroll_to = None;
        self.detail = None;
        self.detail_error = None;
        self.diff = None;
        self.all_files = Default::default();
        self.files_cache.reading = false;
    }
}

#[cfg(test)]
#[path = "direct_tests/lifecycle_tests.rs"]
mod tests;
