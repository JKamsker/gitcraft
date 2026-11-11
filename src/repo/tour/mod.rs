//! Background first-day tour with actions using the repository's ordinary guarded routes.
use super::{Action, Msg, Refresh, RepoTab, messages::spawn_or, ops};
use crate::{
    tour::History,
    tour::{self, Tour},
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

static REQUESTS: AtomicU64 = AtomicU64::new(0);
pub(super) struct TourView {
    generation: Arc<AtomicU64>,
    report: Option<Tour>,
    loading: bool,
    error: Option<String>,
    name: String,
    email: String,
    show_help: bool,
    _jobs: crate::tasks::Scope,
}
impl TourView {
    pub(super) fn history(&self) -> Option<Arc<History>> {
        self.report.as_ref().map(|r| r.history.clone())
    }
}
impl RepoTab {
    pub(super) fn open_tour(&mut self, refresh: bool, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                "Finish or close the editor first".into(),
                true,
            )));
            return;
        }
        let _ = refresh;
        let cached = if refresh {
            None
        } else {
            self.tour.as_ref().and_then(TourView::history)
        };
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.close_pr();
        let generation = REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
        let token = Arc::new(AtomicU64::new(generation));
        let alive = Arc::downgrade(&token);
        let jobs = crate::tasks::Scope(self.tx.cancel.child());
        let mut tx = self.tx.clone();
        tx.cancel = jobs.0.clone();
        self.tour = Some(TourView {
            generation: token,
            report: None,
            loading: true,
            error: None,
            name: String::new(),
            email: String::new(),
            show_help: false,
            _jobs: jobs,
        });
        let path = self.path.clone();
        let original = path.clone();
        let failed_path = path.clone();
        spawn_or(
            &tx,
            ctx,
            move |reason| {
                Msg::Then(Box::new(move |tab, _| {
                    if tab.path == failed_path {
                        tab.tour_arrived(generation, Err(reason));
                    }
                }))
            },
            move || {
                let active = || {
                    alive
                        .upgrade()
                        .is_some_and(|t| t.load(Ordering::Relaxed) == generation)
                };
                let days = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| (d.as_secs() / 86400) as i64)
                    .unwrap_or(0);
                let report = tour::read(&path, cached, &active, &AtomicUsize::new(0), days);
                Msg::Then(Box::new(move |tab, _| {
                    if tab.path == original {
                        tab.tour_arrived(generation, report);
                    }
                }))
            },
        );
    }
    fn tour_arrived(&mut self, generation: u64, result: Result<Tour, String>) {
        if let Some(view) = &mut self.tour
            && view.generation.load(Ordering::Relaxed) == generation
        {
            view.loading = false;
            match result {
                Ok(report) => {
                    view.name = report.identity.0.clone();
                    view.email = report.identity.1.clone();
                    view.report = Some(report);
                }
                Err(e) => view.error = Some(e),
            }
        }
    }
    pub(super) fn tour_view(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let Some(view) = &mut self.tour else {
            return;
        };
        let (close, refresh) = contents(ui, view, self.busy_op.is_some(), acts);
        if close {
            self.tour = None;
        } else if refresh {
            self.open_tour(true, ui.ctx());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_or_replaced_tour_rejects_stale_results() -> Result<(), String> {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/tour-generation".into(), &ctx);
        tab.tour = Some(TourView {
            generation: Arc::new(AtomicU64::new(2)),
            report: None,
            loading: true,
            error: None,
            name: String::new(),
            email: String::new(),
            show_help: false,
            _jobs: crate::tasks::Scope::default(),
        });
        tab.tour_arrived(1, Err("stale".into()));
        assert!(
            tab.tour
                .as_ref()
                .is_some_and(|v| v.loading && v.error.is_none())
        );
        let cancelled = tab
            .tour
            .as_ref()
            .map(|v| v._jobs.0.clone())
            .ok_or("fixture tour missing")?;
        assert!(
            tab.current_view().is_none(),
            "the tour must not be recorded as the graph"
        );
        tab.leave_pages();
        assert!(
            cancelled.is_cancelled(),
            "leaving the tour releases its work"
        );
        tab.tour_arrived(2, Err("closed".into()));
        assert!(tab.tour.is_none());
        Ok(())
    }
}

#[cfg(test)]
mod fluent_tour_paint_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_tour_actual_cached_error_repaints_without_discovery() {
        let raw = "Invalid repository HEAD";
        let state = TourView {
            generation: Arc::new(AtomicU64::new(1)),
            report: None,
            loading: false,
            error: Some(raw.into()),
            name: String::new(),
            email: String::new(),
            show_help: false,
            _jobs: crate::tasks::Scope::default(),
        };
        let mut initialized = false;
        let mut h = Harness::new_ui_state(
            |ui, state: &mut TourView| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    initialized = true;
                    ui.ctx().request_discard("install supplied fonts");
                    return;
                }
                assert_eq!(contents(ui, state, false, &mut vec![]), (false, false));
            },
            state,
        );
        for lang in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(lang);
            h.run();
            h.get_by_label(&tour::presentation::diagnostic(raw));
            assert_eq!(h.state().error.as_deref(), Some(raw));
            assert_eq!(h.state().generation.load(Ordering::Relaxed), 1);
            let path = "/supplied/path: punctuation/ß";
            let unavailable = format!(
                "{path}: content unavailable locally or above the reading limit (not downloaded)"
            );
            assert!(tour::presentation::document_unavailable(&unavailable).contains(path));
            assert_eq!(
                tour::presentation::diagnostic("Provider opaque: ß\n1234"),
                "Provider opaque: ß\n1234"
            );
            assert_eq!(tour::presentation::need("Git LFS"), "Git LFS");
            assert_eq!(tour::presentation::need("Custom marker"), "Custom marker");
        }
    }
}

mod view;
use view::contents;
