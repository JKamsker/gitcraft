//! Calendar and author statistics share one cancellable, immutable history snapshot.
#[cfg(test)]
mod tests;
mod view;

use super::RepoTab;
use crate::git::activity::{self, Clock, Day, History, Query, Summary};
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

static REQUESTS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Calendar,
    Contributors,
}

pub(super) struct Activity {
    page: Page,
    profile: Option<std::path::PathBuf>,
    reader: super::history_reader::Reader<(Arc<History>, Summary)>,
    generation: Arc<AtomicU64>,
    progress: Arc<AtomicUsize>,
    loading: bool,
    limit: usize,
    history: Option<Arc<History>>,
    summary: Option<Summary>,
    error: Option<String>,
    query: Query,
    clock: Clock,
    from: String,
    through: String,
    month: Option<Day>,
    months: bool,
    drag: Option<Day>,
    author: Option<(String, String)>,
}

impl Activity {
    fn new(page: Page) -> Self {
        Self {
            page,
            profile: crate::git::profile_config(),
            reader: Default::default(),
            generation: Arc::new(AtomicU64::new(0)),
            progress: Arc::new(AtomicUsize::new(0)),
            loading: false,
            limit: 5_000,
            history: None,
            summary: None,
            error: None,
            query: Query::default(),
            clock: Clock::Author,
            from: String::new(),
            through: String::new(),
            month: None,
            months: false,
            drag: None,
            author: None,
        }
    }
    fn range(&mut self, from: Day, through: Day) {
        let (a, b) = (from.min(through), from.max(through));
        self.from = a.to_string();
        self.through = b.to_string();
        self.query.from = Some(a);
        self.query.through = Some(b);
        self.author = None;
    }
    fn requested_range(&self) -> Result<Query, String> {
        let parse = |s: &str| {
            if s.is_empty() {
                Ok(None)
            } else {
                Day::parse(s)
                    .map(Some)
                    .ok_or("Use dates such as 2026-11-07".to_owned())
            }
        };
        let query = Query {
            clock: self.clock,
            from: parse(&self.from)?,
            through: parse(&self.through)?,
        };
        if matches!((query.from,query.through),(Some(a),Some(b))if a>b) {
            return Err("The start date must be on or before the end date".into());
        }
        Ok(query)
    }
    fn accept(&mut self, generation: u64, result: Result<(Arc<History>, Summary), String>) {
        if self.generation.load(Ordering::Relaxed) != generation {
            return;
        }
        self.loading = false;
        match result {
            Ok((history, summary)) => {
                if self.month.is_none() {
                    self.month = summary
                        .calendar
                        .last_key_value()
                        .map(|(d, _)| d.month_start());
                }
                self.history = Some(history);
                self.summary = Some(summary);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn begin(&mut self) -> u64 {
        let generation = REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
        self.generation.store(generation, Ordering::Relaxed);
        self.loading = true;
        self.summary = None;
        self.error = None;
        generation
    }
    fn cancel(&mut self) {
        self.reader.cancel();
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.loading = false;
        self.error = Some("Analysis cancelled. Refresh to try again.".into());
    }
}

impl RepoTab {
    pub(super) fn open_activity(&mut self, page: Page, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                "Finish or close the editor first".into(),
                true,
            )));
            return;
        }
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.close_pr();
        if let Some(view) = &mut self.history_activity {
            view.page = page;
        } else {
            self.history_activity = Some(Activity::new(page));
            self.analyze_activity(true, ctx);
        }
    }
    pub(super) fn analyze_activity(&mut self, reload: bool, ctx: &egui::Context) {
        let Some(view) = &mut self.history_activity else {
            return;
        };
        let query = match view.requested_range() {
            Ok(q) => q,
            Err(e) => {
                view.error = Some(e);
                return;
            }
        };
        view.query = query;
        view.author = None;
        let generation = view.begin();
        let alive = Arc::downgrade(&view.generation);
        let progress = view.progress.clone();
        progress.store(0, Ordering::Relaxed);
        let history = if reload { None } else { view.history.clone() };
        let limit = view.limit;

        let repo = self.path.clone();
        let profile = crate::git::profile_config();
        view.reader.queue(move |owned| {
            let active = || {
                owned()
                    && crate::git::profile_config() == profile
                    && alive
                        .upgrade()
                        .is_some_and(|g| g.load(Ordering::Relaxed) == generation)
            };
            let history = match history {
                Some(h) => h,
                None => Arc::new(activity::read(&repo, limit, &active, &progress)?),
            };
            let summary = activity::summarize(&history, query, &active)?;
            Ok((history, summary))
        });
        ctx.request_repaint();
    }
    pub(super) fn poll_history_activity(&mut self, ctx: &egui::Context) {
        if self
            .history_activity
            .as_ref()
            .is_some_and(|view| view.profile != crate::git::profile_config())
        {
            self.history_activity = None;
        }
        if let Some(view) = &mut self.history_activity
            && let Some(result) = view.reader.poll(&self.tx, ctx)
        {
            view.accept(view.generation.load(Ordering::Relaxed), result);
        }
    }
    pub(super) fn history_activity_window(
        &mut self,
        ctx: &egui::Context,
        acts: &mut Vec<super::Action>,
    ) {
        if self.history_activity.is_none() {
            return;
        }
        let mut open = true;
        egui::Window::new(crate::i18n::text(
            "repo-workflows-history_activity-history-activity-window-0ad38473",
        ))
        .id(egui::Id::new(("history-activity", &self.path)))
        .open(&mut open)
        .default_size(egui::vec2(780.0, 640.0))
        .show(ctx, |ui| self.activity_view(ui, acts));
        if !open {
            self.history_activity = None;
        }
    }
    /// Opens detail directly even if a bounded/filtered graph has not loaded this commit.
    pub(super) fn activity_commit(&mut self, hash: String, ctx: &egui::Context) {
        if self.index.contains_key(&hash) {
            self.select_hash(&hash, ctx);
        } else {
            self.selected = None;
            self.stash_view = None;
            self.load_detail(hash, ctx);
        }
    }
}

#[cfg(test)]
mod fluent_r1_cache_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_activity_cached_range_and_cancel_errors_keep_generation_and_query()
    -> Result<(), String> {
        crate::i18n::set_language("de");
        let mut state = Activity::new(Page::Calendar);
        state.from = "bad date".into();
        state.error = Some(state.requested_range().err().ok_or("expected an error")?);
        for cancel in [false, true] {
            if cancel {
                state.cancel();
            }
            let raw = state.error.clone().ok_or("missing test value")?;
            let generation = state.generation.load(Ordering::Relaxed);
            for lang in ["de", "en-US", "de"] {
                crate::i18n::set_language(lang);
                assert_eq!(
                    crate::repo::activity::display_workflow_error(&raw) == raw,
                    lang == "en-US"
                );
                assert_eq!(state.error.as_ref(), Some(&raw));
                assert_eq!(state.generation.load(Ordering::Relaxed), generation);
                assert_eq!(state.from, "bad date");
                assert!(!state.reader.busy());
            }
        }
        crate::i18n::set_language("en-US");

        Ok(())
    }
}
