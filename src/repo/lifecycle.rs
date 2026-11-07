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
    /// The tab is shown: read it for the first time, or again after it was suspended.
    pub(crate) fn shown_now(&mut self, ctx: &egui::Context) {
        let step = self.life.shown(Instant::now());
        match step {
            Step::Start | Step::Resume => {
                // The opening screen's delay counts from now, not from when the tab was made.
                self.opened = Instant::now();
                self.resuming = step == Step::Resume;
                if step == Step::Start {
                    if !self.drafts_loaded {
                        self.restore_drafts();
                    }
                    self.load_timeline(ctx);
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
        // Cancel captures and child processes from this lifetime, and disconnect their
        // results so an old read cannot repopulate a suspended/resumed view.
        let (sender, receiver) = std::sync::mpsc::channel();
        self._jobs = crate::tasks::Scope::default();
        self.tx = super::messages::JobSender {
            sender,
            cancel: self._jobs.0.clone(),
            repo: self.path.clone(),
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
        self.commits = Vec::new();
        self.graph = crate::graph::Graph {
            rows: Vec::new(),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspension_retires_private_analysis_readers_and_their_pending_captures() -> Result<(), String>
    {
        let ctx = egui::Context::default();
        let mut tab =
            crate::repo::RepoTab::new_deferred("/nonexistent/suspend-analysis".into(), &ctx);
        tab.life.phase = Phase::Live;
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        tab.band_reader.queue(move |active| {
            entered_tx.send(()).map_err(|e| e.to_string())?;
            release_rx
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
            finished_tx.send(active()).map_err(|e| e.to_string())?;
            // A late success must be discarded even if its worker ignores cancellation.
            Ok(crate::graph::Graph {
                rows: Vec::new(),
                width: 42,
            })
        });
        tab.band_reader.poll(&tab.tx, &ctx);
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        tab.topology_reader.queue(|_| Ok(Default::default()));
        tab.atlas = Some(Default::default());
        tab.growth = Some(Default::default());
        tab.life.phase = Phase::Suspended;
        tab.suspend();
        release_tx.send(()).map_err(|e| e.to_string())?;
        assert!(
            !finished_rx
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?
        );
        assert!(!tab.band_reader.busy());
        assert!(!tab.topology_reader.busy());
        assert!(tab.atlas.is_none());
        assert!(tab.growth.is_none());
        tab.poll_parked_view(&ctx);
        assert_eq!(tab.graph.width, 0);
        assert!(tab.topology.parents.is_empty());
        assert!(!tab.band_reader.busy());
        assert!(!tab.topology_reader.busy());
        Ok(())
    }

    #[test]
    fn a_restored_tab_starts_only_when_first_shown() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        assert!(life.sleeping());
        // In the background however long: still nothing.
        assert_eq!(life.background(t0 + IDLE_AFTER * 3, || true), Step::Nothing);
        assert_eq!(life.phase, Phase::Deferred);
        assert_eq!(life.shown(t0 + IDLE_AFTER * 3), Step::Start);
        assert_eq!(life.phase, Phase::Live);
        assert_eq!(life.shown(t0 + IDLE_AFTER * 3), Step::Nothing);
    }

    #[test]
    fn an_idle_tab_is_suspended_after_a_while_and_resumes_when_shown() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        life.shown(t0);
        let just_before = t0 + IDLE_AFTER - Duration::from_secs(1);
        assert_eq!(life.background(just_before, || true), Step::Nothing);
        assert_eq!(life.background(t0 + IDLE_AFTER, || true), Step::Suspend);
        assert!(life.sleeping());
        // Once is enough.
        assert_eq!(life.background(t0 + IDLE_AFTER * 2, || true), Step::Nothing);
        assert_eq!(life.shown(t0 + IDLE_AFTER * 2), Step::Resume);
        assert!(!life.sleeping());
    }

    #[test]
    fn a_tab_with_work_going_on_is_never_suspended() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        life.shown(t0);
        assert_eq!(
            life.background(t0 + IDLE_AFTER * 5, || false),
            Step::Nothing
        );
        assert_eq!(life.phase, Phase::Live);
        // Its work done, the time counts from when it was last shown.
        assert_eq!(life.background(t0 + IDLE_AFTER * 5, || true), Step::Suspend);
    }

    #[test]
    fn being_shown_again_restarts_the_clock() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        life.shown(t0);
        let later = t0 + IDLE_AFTER - Duration::from_secs(5);
        life.shown(later);
        assert_eq!(life.background(t0 + IDLE_AFTER, || true), Step::Nothing);
        assert_eq!(life.background(later + IDLE_AFTER, || true), Step::Suspend);
    }

    /// A real repository (a name with spaces and letters outside ASCII), through the tab the
    /// UI uses: nothing read or watched until shown; suspended after being idle, it drops its
    /// graph and watcher, and shown again it reads afresh with the commit selected again.
    #[test]
    fn a_real_tab_reads_when_shown_and_reads_again_after_suspending() -> Result<(), String> {
        use crate::git;
        use crate::repo::RepoTab;
        let dir = std::env::temp_dir().join(format!("gitcraft-lazy tab ü-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let result = (|| {
            for args in [
                &["init", "-q"][..],
                &["config", "user.name", "t"],
                &["config", "user.email", "t@t"],
                &["commit", "-q", "--allow-empty", "-m", "one"],
                &["commit", "-q", "--allow-empty", "-m", "two"],
            ] {
                git::run(Some(&dir), args)?;
            }
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
            let settle = |tab: &mut RepoTab, what: &str, done: &dyn Fn(&RepoTab) -> bool| {
                let until = Instant::now() + Duration::from_secs(20);
                while !done(tab) {
                    if Instant::now() > until {
                        return Err(format!("timed out waiting for {what}"));
                    }
                    tab.poll(&ctx);
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(())
            };
            // Polled in the background for a while: nothing read, no watcher asked for.
            for _ in 0..20 {
                tab.poll(&ctx);
                tab.background_tick();
                std::thread::sleep(Duration::from_millis(5));
            }
            let untouched = (
                tab.history.is_empty(),
                tab.watcher.is_none(),
                tab.sleeping(),
            );
            tab.start_deferred(&ctx);
            settle(&mut tab, "the first read", &|t| {
                t.history.len() == 2 && !t.loading
            })?;
            settle(&mut tab, "the watcher", &|t| t.watcher.is_some())?;
            let first = git::run(Some(&dir), &["rev-parse", "HEAD~1"])?
                .trim()
                .to_owned();
            tab.goto(&first, &ctx);
            let picked = tab.selected_hash();
            // Left alone long enough, with nothing going on.
            tab.life.shown_at = Instant::now() - IDLE_AFTER * 2;
            tab.background_tick();
            let suspended = (
                tab.sleeping(),
                tab.history.is_empty(),
                tab.graph.rows.is_empty(),
                tab.watcher.is_none(),
                tab.fs_rx.is_none(),
            );
            // A read that lands while asleep lays out nothing and keeps the selection to
            // restore.
            tab.rebuild(&ctx);
            let kept = tab.resume_select.clone();
            tab.start_deferred(&ctx);
            let opening_while_resuming = tab.opening();
            settle(&mut tab, "the read after resuming", &|t| {
                t.history.len() == 2 && !t.loading
            })?;
            settle(&mut tab, "the watcher again", &|t| t.watcher.is_some())?;
            let detail_asked = tab.detail_hash.clone();
            Ok::<_, String>((
                untouched,
                picked,
                first,
                suspended,
                tab.selected_hash(),
                (kept, opening_while_resuming, tab.opening(), detail_asked),
            ))
        })();
        let _ = std::fs::remove_dir_all(&dir);
        let (untouched, picked, first, suspended, reselected, resume) = result?;
        let (kept, opening_while_resuming, opening_after, detail_asked) = resume;
        assert_eq!(kept.as_deref(), Some(first.as_str()));
        assert!(opening_while_resuming);
        assert!(!opening_after);
        assert_eq!(detail_asked.as_deref(), Some(first.as_str()));
        assert_eq!(untouched, (true, true, true));
        assert_eq!(picked.as_deref(), Some(first.as_str()));
        assert_eq!(suspended, (true, true, true, true, true));
        assert_eq!(reselected, Some(first));
        Ok(())
    }

    /// Something typed in the commit message keeps the tab from being suspended.
    #[test]
    fn a_tab_with_a_message_typed_stays_awake() {
        use crate::repo::RepoTab;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/lazy-typed".into(), &ctx);
        tab.life.phase = Phase::Live;
        tab.life.shown_at = Instant::now() - IDLE_AFTER * 2;
        tab.commit_summary = "half-written ✍".to_owned();
        tab.background_tick();
        assert!(!tab.sleeping());
        tab.commit_summary.clear();
        tab.background_tick();
        assert!(tab.sleeping());
    }

    #[test]
    fn whether_it_is_idle_is_asked_only_when_it_could_be_suspended() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        let asked = std::cell::Cell::new(0);
        let idle = || {
            asked.set(asked.get() + 1);
            true
        };
        // Not live yet, then live but not long enough.
        life.background(t0 + IDLE_AFTER * 2, idle);
        life.shown(t0);
        life.background(t0 + Duration::from_secs(1), idle);
        assert_eq!(asked.get(), 0);
        life.background(t0 + IDLE_AFTER, idle);
        assert_eq!(asked.get(), 1);
        // Asleep: not asked again.
        life.background(t0 + IDLE_AFTER * 2, idle);
        assert_eq!(asked.get(), 1);
    }

    /// A tab not read yet that is asked to reload (coming back to the window does that for the
    /// tab shown) reads once, and starts watching.
    #[test]
    fn a_reload_wakes_a_restored_tab_once() {
        use crate::repo::RepoTab;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/lazy-reload".into(), &ctx);
        tab.reload(&ctx);
        let reads = tab.reads;
        tab.start_deferred(&ctx);
        assert!(!tab.sleeping());
        assert_eq!(tab.reads, reads);
        assert!(!tab.reload_again);
        assert_eq!(tab.watches, 1);
    }

    /// A watcher set up before the tab was suspended, arriving after it was watched again, is
    /// not the tab's (it sends to a channel no longer read).
    #[test]
    fn a_watcher_from_before_is_dropped() {
        use crate::repo::RepoTab;
        use crate::repo::messages::Msg;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/lazy-watcher".into(), &ctx);
        tab.start_deferred(&ctx);
        let first = tab.watches;
        tab.life.shown_at = Instant::now() - IDLE_AFTER * 2;
        tab.loading = false;
        tab.background_tick();
        // Asleep: nothing is kept.
        tab.handle(Msg::Watcher(first, None), &ctx);
        assert!(tab.watcher.is_none());
        tab.start_deferred(&ctx);
        assert_eq!(tab.watches, first + 1);
        // The old one's arrival does not count as the new one's (no recheck asked for).
        tab.recheck_after_load = false;
        tab.handle(Msg::Watcher(first, None), &ctx);
        assert!(!tab.recheck_after_load);
    }
}
