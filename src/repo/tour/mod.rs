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

fn contents(
    ui: &mut egui::Ui,
    view: &mut TourView,
    busy: bool,
    acts: &mut Vec<Action>,
) -> (bool, bool) {
    let mut close = false;
    let mut refresh = false;
    ui.horizontal(|ui| {
        ui.heading(crate::i18n::text("repo-workflows-tour-tour-view-3174faca"));
        refresh = ui
            .button(crate::i18n::text("repo-workflows-tour-tour-view-f98bf467"))
            .clicked();
        close = ui
            .button(if view.loading {
                crate::i18n::text("repo-workflows-tour-tour-view-712320ba")
            } else {
                crate::i18n::text("repo-workflows-tour-tour-view-2cf3aea7")
            })
            .clicked();
    });
    ui.label(crate::i18n::text("repo-workflows-tour-tour-view-49fa26c7"));
    if view.loading {
        ui.add(crate::spinner::Spinner::new());
        ui.label(crate::i18n::text("repo-workflows-tour-tour-view-aaf81fb7"));
    }
    if let Some(e) = &view.error {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).danger,
            tour::presentation::diagnostic(e),
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("repository_tour")
        .show(ui, |ui| {
            let Some(report) = &view.report else {
                return;
            };
            let h = &report.history;
            ui.label(crate::i18n::message(
                "repo-workflows-tour-scan-summary",
                &[
                    ("count", (h.commits.len()).into()),
                    ("partial", (if h.more { "yes" } else { "no" }).into()),
                    ("shallow", (if h.shallow { "yes" } else { "no" }).into()),
                    (
                        "head",
                        (report.head.get(..12).unwrap_or(&report.head)).into(),
                    ),
                ],
            ));
            ui.label(crate::i18n::text("repo-workflows-tour-tour-view-55b8885f"));
            if h.bytes_limited {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-af4145bc"));
            }
            ui.separator();
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-17cc361f"));
            for document in &report.documents {
                let readme = document.path.to_ascii_lowercase().starts_with("readme");
                egui::CollapsingHeader::new(&document.path)
                    .default_open(readme || view.show_help)
                    .show(ui, |ui| {
                        ui.label(&document.text);
                        if document.truncated {
                            ui.label(crate::i18n::text("repo-workflows-tour-tour-view-ddb4a38f"));
                        }
                    });
            }
            for unavailable in &report.documents_unavailable {
                ui.label(tour::presentation::document_unavailable(unavailable));
            }
            if report.documents.is_empty() {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-0715e061"));
            }
            ui.separator();
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-8e490f8b"));
            let w = &report.workflow;
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-75d74f36",
                &[
                    ("w-inspected-1", (w.inspected).clone().into()),
                    ("w-merges-2", (w.merges).clone().into()),
                    ("w-conventional-3", (w.conventional).clone().into()),
                    (
                        "w-signature-headers-4",
                        (w.signature_headers).clone().into(),
                    ),
                    ("w-signoffs-5", (w.signoffs).clone().into()),
                ],
            ));
            ui.label(if w.merges == 0 {
                crate::i18n::text("repo-workflows-tour-tour-view-f462deb1")
            } else {
                crate::i18n::text("repo-workflows-tour-tour-view-7ba62dbe")
            });
            if let Some(e) = &w.incomplete {
                ui.label(tour::presentation::diagnostic(e));
            }
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-f2c602f9"));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-17f176ca",
                &[
                    (
                        "report-branches-len-1",
                        (report.branches.len()).clone().into(),
                    ),
                    ("report-tags-len-2", (report.tags.len()).clone().into()),
                ],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-b71999b4",
                &[(
                    "report-branches-iter-take-30-clo-1",
                    (report
                        .branches
                        .iter()
                        .take(30)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "))
                    .clone()
                    .into(),
                )],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-dfbf1d23",
                &[(
                    "report-tags-iter-take-30-cloned--1",
                    (report
                        .tags
                        .iter()
                        .take(30)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "))
                    .clone()
                    .into(),
                )],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-152f32cc",
                &[
                    (
                        "report-branches-iter-filter-s-s--1",
                        (report
                            .branches
                            .iter()
                            .filter(|s| s.contains("/release/"))
                            .count())
                        .clone()
                        .into(),
                    ),
                    (
                        "report-tags-iter-filter-s-s-star-2",
                        (report.tags.iter().filter(|s| s.starts_with('v')).count())
                            .clone()
                            .into(),
                    ),
                ],
            ));
            ui.separator();
            ui.strong(crate::i18n::message(
                "repo-workflows-tour-tour-view-325dcc8e",
                &[
                    ("report-since-1", report.since.display().into()),
                    ("report-through-2", report.through.display().into()),
                ],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-e78ace8e",
                &[(
                    "report-recent-commits-1",
                    (report.recent_commits).clone().into(),
                )],
            ));
            for folder in &report.folders {
                ui.label(format!(
                    "{}: {}",
                    folder.display_path(),
                    folder
                        .authors
                        .iter()
                        .map(|(n, e, c)| crate::i18n::message(
                            "repo-workflows-tour-tour-view-009719be",
                            &[
                                ("n", (n).clone().into()),
                                ("e", (e).clone().into()),
                                ("c", (c).clone().into())
                            ]
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-d00db6bc"));
            for (path, n) in &report.hot_files {
                ui.label(crate::i18n::message(
                    "repo-workflows-tour-tour-view-6a2e97ea",
                    &[
                        ("n", (n).clone().into()),
                        (
                            "string-from-utf8-lossy-path-1",
                            (String::from_utf8_lossy(path)).clone().into(),
                        ),
                    ],
                ));
            }
            ui.separator();
            ui.strong(crate::i18n::text("repo-workflows-tour-tour-view-0e4924f9"));
            ui.label(crate::i18n::message(
                "repo-workflows-tour-tour-view-939f2714",
                &[(
                    "if-report-needs-is-empty-no-list-1",
                    (if report.needs.is_empty() {
                        crate::i18n::text("repo-workflows-tour-tour-view-78ab5e15").into()
                    } else {
                        report
                            .needs
                            .iter()
                            .map(|raw| tour::presentation::need(raw))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .clone()
                    .into(),
                )],
            ));
            ui.label(crate::i18n::text("repo-workflows-tour-tour-view-96c17e04"));
            ui.horizontal(|ui| {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-dcd1d522"));
                ui.text_edit_singleline(&mut view.name);
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-969ccbd3"));
                ui.text_edit_singleline(&mut view.email);
            });
            let valid = !view.name.trim().is_empty()
                && !view.email.trim().is_empty()
                && !view.name.chars().any(char::is_control)
                && !view.email.chars().any(char::is_control)
                && !busy;
            if ui
                .add_enabled(
                    valid,
                    egui::Button::new(crate::i18n::text("repo-workflows-tour-tour-view-095444dc")),
                )
                .clicked()
            {
                acts.push(Action::SetIdentity(view.name.clone(), view.email.clone()));
            }
            if report.needs.contains("Git LFS") {
                ui.hyperlink_to(
                    crate::i18n::text("repo-workflows-tour-tour-view-4015deeb"),
                    "https://git-lfs.com/",
                );
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-tour-tour-view-f0a6629b",
                        )),
                    )
                    .clicked()
                {
                    acts.push(Action::Ops {
                        label: "Install local Git LFS hooks".into(),
                        ops: ops(&[&["lfs", "install", "--local"]]),
                        refresh: Refresh::All,
                    });
                }
            }
            if report.needs.contains("Submodules")
                && ui
                    .button(crate::i18n::text("repo-workflows-tour-tour-view-2189fda2"))
                    .clicked()
            {
                acts.push(Action::Dialog(super::Dialog::confirm(
                    "Initialize submodules",
                    crate::i18n::text("repo-workflows-tour-tour-view-e64cb6f2").into(),
                    crate::i18n::text("repo-workflows-tour-tour-view-7bfe85f2"),
                    ops(&[&["submodule", "update", "--init", "--recursive"]]),
                    None,
                )));
            }
            if ui
                .button(crate::i18n::text("repo-workflows-tour-tour-view-d65a30c1"))
                .clicked()
            {
                view.show_help = !view.show_help;
            }
            if view.show_help {
                ui.label(crate::i18n::text("repo-workflows-tour-tour-view-6f54c5e0"));
            }
            if valid
                && ui
                    .button(crate::i18n::text("repo-workflows-tour-tour-view-5b0a8af2"))
                    .clicked()
            {
                ui.ctx()
                    .copy_text(format!("Signed-off-by: {} <{}>", view.name, view.email));
            }
        });

    (close, refresh)
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
