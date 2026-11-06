//! A tab's life while it isn't looked at. A tab restored on start reads nothing and watches
//! nothing until it is first shown, so fifteen saved tabs no longer mean fifteen history loads
//! and fifteen folder watchers at startup. A tab left in the background for [`IDLE_AFTER`],
//! with nothing going on in it, is suspended: it drops its graph, its diff and what the detail
//! pane read, and stops watching the folder; shown again, it watches and reads afresh.

use std::time::{Duration, Instant};

/// How long a tab stays in the background before it is suspended.
pub(crate) const IDLE_AFTER: Duration = Duration::from_secs(10 * 60);

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

    /// The tab is in the background at `now`; `idle`: nothing goes on in it that suspending
    /// would end or lose (an operation, a terminal, something typed, an editor open).
    pub fn background(&mut self, now: Instant, idle: bool) -> Step {
        let long_enough = now.saturating_duration_since(self.shown_at) >= IDLE_AFTER;
        if self.phase == Phase::Live && idle && long_enough {
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
        match self.life.shown(Instant::now()) {
            Step::Start | Step::Resume => {
                self.watch(ctx);
                self.reload(ctx);
            }
            Step::Nothing | Step::Suspend => {}
        }
    }

    /// The tab is in the background: suspends it once it has been idle long enough.
    pub(crate) fn background_tick(&mut self) {
        let idle = self.idle();
        if self.life.background(Instant::now(), idle) == Step::Suspend {
            self.suspend();
        }
    }

    /// Nothing is read or watched now (see [`Lifecycle::sleeping`]).
    pub(crate) fn sleeping(&self) -> bool {
        self.life.sleeping()
    }

    /// Nothing goes on that suspending would end or lose.
    fn idle(&mut self) -> bool {
        !self.is_busy()
            && !self.loading_more
            && !self.counting_worktrees
            && self.queued.is_empty()
            && self.unfinished_anywhere().is_none()
    }

    /// Drops what the tab read and stops watching; the commit shown is selected again once it
    /// is read afresh (see [`Self::shown_now`]).
    fn suspend(&mut self) {
        self.watcher = None;
        self.poller = None;
        self.fs_rx = None;
        self.fs_pending = None;
        // An open `git log` reading older pages.
        *self
            .log_stream
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.pending_goto = self.detail_hash.clone().or(self.pending_goto.take());
        self.history = Vec::new();
        self.commits = Vec::new();
        self.graph = crate::graph::Graph {
            rows: Vec::new(),
            width: 0,
        };
        self.index = Default::default();
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
    fn a_restored_tab_starts_only_when_first_shown() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        assert!(life.sleeping());
        // In the background however long: still nothing.
        assert_eq!(life.background(t0 + IDLE_AFTER * 3, true), Step::Nothing);
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
        assert_eq!(life.background(just_before, true), Step::Nothing);
        assert_eq!(life.background(t0 + IDLE_AFTER, true), Step::Suspend);
        assert!(life.sleeping());
        // Once is enough.
        assert_eq!(life.background(t0 + IDLE_AFTER * 2, true), Step::Nothing);
        assert_eq!(life.shown(t0 + IDLE_AFTER * 2), Step::Resume);
        assert!(!life.sleeping());
    }

    #[test]
    fn a_tab_with_work_going_on_is_never_suspended() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        life.shown(t0);
        assert_eq!(life.background(t0 + IDLE_AFTER * 5, false), Step::Nothing);
        assert_eq!(life.phase, Phase::Live);
        // Its work done, the time counts from when it was last shown.
        assert_eq!(life.background(t0 + IDLE_AFTER * 5, true), Step::Suspend);
    }

    #[test]
    fn being_shown_again_restarts_the_clock() {
        let t0 = Instant::now();
        let mut life = Lifecycle::new(t0);
        life.shown(t0);
        let later = t0 + IDLE_AFTER - Duration::from_secs(5);
        life.shown(later);
        assert_eq!(life.background(t0 + IDLE_AFTER, true), Step::Nothing);
        assert_eq!(life.background(later + IDLE_AFTER, true), Step::Suspend);
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
            tab.start_deferred(&ctx);
            settle(&mut tab, "the read after resuming", &|t| {
                t.history.len() == 2 && !t.loading
            })?;
            settle(&mut tab, "the watcher again", &|t| t.watcher.is_some())?;
            Ok::<_, String>((untouched, picked, first, suspended, tab.selected_hash()))
        })();
        let _ = std::fs::remove_dir_all(&dir);
        let (untouched, picked, first, suspended, reselected) = result?;
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
}
