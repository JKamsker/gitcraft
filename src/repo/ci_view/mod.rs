//! A commit's builds in the big area: its workflow runs (GitHub) or pipelines (GitLab), the
//! jobs of the one picked as a graph, the steps of the job picked on a time line, and running
//! the failed jobs again, cancelling and the failed jobs' log.

mod graph;
mod page;
mod steps;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::{Msg, RepoTab, spawn};
use crate::ci::{self, CheckState, Job, Project, Run};

/// How often a run that's still going is read again.
const POLL: Duration = Duration::from_secs(10);
/// How soon a run is read again after it was re-run or cancelled.
const AFTER_ACTING: Duration = Duration::from_secs(3);

/// Numbers the reads of every tab's view, so an answer for an older one is told apart.
static REQUESTS: AtomicU64 = AtomicU64::new(0);

fn next_request() -> u64 {
    REQUESTS.fetch_add(1, Ordering::Relaxed) + 1
}

/// What the pipeline view asks for.
pub(crate) enum CiAct {
    SelectRun(Project, u64),
    SelectJob(u64),
    Reload,
    RerunFailed,
    Cancel,
    /// Show the log of the failed jobs, or hide it.
    ToggleLog,
}

pub(crate) struct CiView {
    pub(super) sha: String,
    request: u64,
    loading: bool,
    runs: Option<Result<Builds, String>>,
    /// Project of the selected run, retained independently of status refreshes.
    project: Option<Project>,
    /// The run shown, by id.
    run: Option<u64>,
    /// The jobs of a run (by id), the one shown once they're here.
    jobs: Option<(u64, Result<Vec<Job>, String>)>,
    /// The job whose steps are shown, by id.
    job: Option<u64>,
    /// The failed jobs' log, `Some(None)` while it's read.
    log: Option<Option<Result<String, String>>>,
    log_request: u64,
    /// When to read the run again, while it runs.
    next: Option<Instant>,
    /// It was seen running: once it has finished, the dots are read again.
    was_running: bool,
    /// Re-running or cancelling is under way.
    acting: bool,
}

/// Runs retain their owning project; IDs can overlap between GitLab projects.
#[derive(Debug, Default)]
pub(super) struct Builds {
    runs: Vec<(Project, Run)>,
    errors: Vec<String>,
}

/// Searches every candidate, retaining readable runs and reporting partial failures.
fn read_builds(
    projects: Vec<Project>,
    mut read: impl FnMut(&Project) -> Result<Vec<Run>, String>,
) -> Result<Builds, String> {
    let mut builds = Builds::default();
    let mut answered = false;
    for project in projects {
        match read(&project) {
            Ok(runs) => {
                answered = true;
                builds
                    .runs
                    .extend(runs.into_iter().map(|run| (project.clone(), run)));
            }
            Err(e) => builds.errors.push(format!("{}: {e}", project.path)),
        }
    }
    if !answered {
        return Err(builds.errors.join("; "));
    }
    Ok(builds)
}

impl CiView {
    /// Selected run in its original project, even if the commit's status source changes.
    fn shown_run(&self) -> Option<&Run> {
        let builds = self.runs.as_ref()?.as_ref().ok()?;
        builds
            .runs
            .iter()
            .find(|(p, run)| Some(p) == self.project.as_ref() && Some(run.id) == self.run)
            .map(|(_, run)| run)
    }

    /// The jobs of the run shown, once they're here.
    fn shown_jobs(&self) -> Option<&Result<Vec<Job>, String>> {
        self.jobs
            .as_ref()
            .filter(|(run, _)| Some(*run) == self.run)
            .map(|(_, jobs)| jobs)
    }

    /// Whether either the selected run or one of its jobs is still active.
    fn running(&self) -> bool {
        let jobs = self.shown_jobs().and_then(|jobs| jobs.as_ref().ok());
        self.shown_run()
            .is_some_and(|run| run.state == CheckState::Running)
            || jobs.is_some_and(|jobs| jobs.iter().any(|j| j.state == CheckState::Running))
    }
}

impl RepoTab {
    /// Shows the builds of commit `sha` in the big area, run `run` first if it's given.
    pub(super) fn open_ci(&mut self, sha: String, run: Option<u64>, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            let editor = if self.conflict.is_some() {
                "conflict editor"
            } else {
                "interactive rebase"
            };
            self.status = Some((format!("Finish or close the {editor} first"), true));
            return;
        }
        self.diff = None;
        self.file_view = None;
        self.tour = None;
        self.ci_view = Some(CiView {
            sha,
            request: 0,
            loading: true,
            runs: None,
            project: None,
            run,
            jobs: None,
            job: None,
            log: None,
            log_request: 0,
            next: None,
            was_running: false,
            acting: false,
        });
        self.load_ci_view(ctx);
    }

    /// Reads the runs of the commit shown (again), then the jobs of the one shown.
    fn load_ci_view(&mut self, ctx: &egui::Context) {
        let projects = self
            .ci_view
            .as_ref()
            .map(|v| self.ci_projects_for(&v.sha))
            .unwrap_or_default();
        let Some(view) = &mut self.ci_view else {
            return;
        };
        view.next = None;
        if projects.is_empty() {
            view.loading = false;
            view.runs = Some(Err("This repository isn't on GitHub or GitLab.".to_owned()));
            return;
        };
        view.request = next_request();
        view.loading = true;
        let (request, path, sha) = (view.request, self.path.clone(), view.sha.clone());
        spawn(&self.tx, ctx, move || {
            Msg::CiRuns(
                request,
                read_builds(projects, |project| ci::runs(&path, project, &sha)),
            )
        });
    }

    /// The selected run's project, captured when the runs were loaded.
    fn ci_view_project(&self) -> Option<crate::ci::Project> {
        let view = self.ci_view.as_ref()?;
        view.shown_run()?;
        view.project.clone()
    }

    /// Reads jobs from the selected run's original project.
    fn load_ci_jobs(&mut self, ctx: &egui::Context) {
        let project = self.ci_view_project();
        let Some(view) = &mut self.ci_view else {
            return;
        };
        let (Some(project), Some(run)) = (project, view.run) else {
            return;
        };
        view.loading = true;
        let (request, path) = (view.request, self.path.clone());
        spawn(&self.tx, ctx, move || {
            Msg::CiJobs(request, run, ci::jobs(&path, &project, run))
        });
    }

    /// Accepts only the current run-list request and preserves selection by project and ID.
    pub(super) fn ci_runs_arrived(
        &mut self,
        request: u64,
        result: Result<Builds, String>,
        ctx: &egui::Context,
    ) {
        let Some(view) = self.ci_view.as_mut().filter(|v| v.request == request) else {
            return;
        };
        view.loading = false;
        if let Ok(builds) = &result {
            // Preserve the project together with the ID: pipeline IDs may overlap.
            let keep = builds.runs.iter().find(|(project, run)| {
                Some(run.id) == view.run && view.project.as_ref().is_none_or(|p| p == project)
            });
            let selected = keep.or_else(|| builds.runs.first());
            let project = selected.map(|(project, _)| project.clone());
            let run = selected.map(|(_, run)| run.id);
            if project != view.project || run != view.run {
                view.jobs = None;
                view.job = None;
                view.log = None;
                view.log_request = next_request();
            }
            view.project = project;
            view.run = run;
        }
        let load_jobs = result.as_ref().is_ok_and(|builds| !builds.runs.is_empty());
        // A read that failed while the run was going (the network blinked) is tried again,
        // rather than the view never being read again.
        if result.is_err() && view.was_running {
            view.next = Some(Instant::now() + POLL);
        }
        view.runs = Some(result);
        if load_jobs {
            self.load_ci_jobs(ctx);
        }
    }

    /// Accepts jobs only for the current request and selected run.
    pub(super) fn ci_jobs_arrived(
        &mut self,
        request: u64,
        run: u64,
        result: Result<Vec<Job>, String>,
        ctx: &egui::Context,
    ) {
        let Some(view) = self
            .ci_view
            .as_mut()
            .filter(|v| v.request == request && v.run == Some(run))
        else {
            return;
        };
        view.loading = false;
        if let Ok(jobs) = &result {
            // The job shown stays; else the first that failed, or the first.
            let keep = view.job.filter(|id| jobs.iter().any(|job| job.id == *id));
            let failed = jobs.iter().find(|job| job.state == CheckState::Failed);
            view.job = keep.or(failed.or(jobs.first()).map(|job| job.id));
        }
        let failed = result.is_err();
        view.jobs = Some((run, result));
        if view.running() || (failed && view.was_running) {
            view.next = Some(Instant::now() + POLL);
            view.was_running = true;
        } else if std::mem::take(&mut view.was_running) {
            // It has finished: the dots should say so too.
            self.load_ci(ctx);
        }
    }

    pub(super) fn ci_log_arrived(&mut self, request: u64, result: Result<String, String>) {
        if let Some(view) = self.ci_view.as_mut().filter(|v| v.log_request == request)
            && let Some(log) = &mut view.log
        {
            *log = Some(result);
        }
    }

    /// A re-run or a cancel went through, or didn't.
    pub(super) fn ci_acted(&mut self, label: String, result: Result<(), String>) {
        self.status = Some(match result {
            Ok(()) => (label, false),
            Err(e) => (format!("{label}: {e}"), true),
        });
        if let Some(view) = &mut self.ci_view {
            view.acting = false;
            view.next = Some(Instant::now() + AFTER_ACTING);
        }
    }

    /// Reads the run shown again while it runs; called every frame.
    pub(super) fn poll_ci_view(&mut self, ctx: &egui::Context) {
        let Some(next) = self.ci_view.as_ref().and_then(|v| v.next) else {
            return;
        };
        let now = Instant::now();
        if now >= next {
            self.load_ci_view(ctx);
        } else {
            ctx.request_repaint_after(next - now);
        }
    }

    /// Applies a view action using the selected run's retained project.
    pub(super) fn ci_act(&mut self, act: CiAct, ctx: &egui::Context) {
        let project = self.ci_view_project();
        let Some(view) = &mut self.ci_view else {
            return;
        };
        match act {
            CiAct::SelectRun(project, id) => {
                if view.run != Some(id) || view.project.as_ref() != Some(&project) {
                    view.request = next_request();
                    view.project = Some(project);
                    view.run = Some(id);
                    view.jobs = None;
                    view.job = None;
                    view.log = None;
                    view.log_request = next_request();
                    self.load_ci_jobs(ctx);
                }
            }
            CiAct::SelectJob(id) => view.job = Some(id),
            CiAct::Reload => self.load_ci_view(ctx),
            CiAct::RerunFailed | CiAct::Cancel => {
                let (Some(project), Some(run)) = (project, view.shown_run().cloned()) else {
                    return;
                };
                if view.acting {
                    return;
                }
                view.acting = true;
                let rerun = matches!(act, CiAct::RerunFailed);
                let path = self.path.clone();
                spawn(&self.tx, ctx, move || {
                    let what = format!("{} #{}", run.name, run.number);
                    let (label, result) = if rerun {
                        let result = ci::rerun_failed(&path, &project, run.id);
                        (format!("Re-ran the failed jobs of {what}"), result)
                    } else {
                        (
                            format!("Cancelled {what}"),
                            ci::cancel(&path, &project, run.id),
                        )
                    };
                    Msg::CiActed(label, result)
                });
            }
            CiAct::ToggleLog => {
                if view.log.take().is_some() {
                    return;
                }
                let (Some(project), Some(run)) = (project, view.run) else {
                    return;
                };
                let jobs = match view.shown_jobs() {
                    Some(Ok(jobs)) => jobs.clone(),
                    _ => Vec::new(),
                };
                view.log = Some(None);
                view.log_request = next_request();
                let (request, path) = (view.log_request, self.path.clone());
                spawn(&self.tx, ctx, move || {
                    Msg::CiLog(request, ci::failed_log(&path, &project, run, &jobs))
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(path: &str) -> Project {
        Project {
            forge: crate::forge::Forge::GitLab,
            host: "gitlab.com".to_owned(),
            path: path.to_owned(),
        }
    }

    /// Empty upstream results do not conceal an older fork build; run IDs may overlap.
    #[test]
    fn builds_search_all_candidates_and_retain_each_runs_project() -> Result<(), String> {
        let projects = vec![
            project("up/repo"),
            project("me/repo"),
            project("other/repo"),
        ];
        let builds = read_builds(projects, |p| {
            Ok(if p.path == "up/repo" {
                vec![]
            } else {
                vec![Run {
                    id: 7,
                    ..Run::default()
                }]
            })
        })?;
        let owners: Vec<_> = builds
            .runs
            .iter()
            .map(|(p, run)| (p.path.as_str(), run.id))
            .collect();
        assert_eq!(owners, [("me/repo", 7), ("other/repo", 7)]);
        assert!(builds.errors.is_empty());
        Ok(())
    }

    /// A partial read shows its error alongside readable builds rather than claiming no CI.
    #[test]
    fn builds_report_partial_project_failures() -> Result<(), String> {
        let builds = read_builds(vec![project("up/repo"), project("me/repo")], |p| {
            if p.path == "up/repo" {
                Err("offline".to_owned())
            } else {
                Ok(vec![Run {
                    id: 7,
                    ..Run::default()
                }])
            }
        })?;
        assert_eq!(builds.runs.len(), 1);
        assert_eq!(builds.errors, ["up/repo: offline"]);
        assert!(read_builds(vec![project("up/repo")], |_| Err("offline".to_owned())).is_err());
        Ok(())
    }

    /// Refreshing commit status cannot redirect a loaded pipeline's job/action requests.
    #[test]
    fn loaded_runs_keep_their_project_when_the_commit_source_changes() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/ci-project-test".into(), &ctx);
        tab.open_ci("abc".to_owned(), None, &ctx);
        if let Some(view) = &mut tab.ci_view {
            view.project = Some(project("me/repo"));
            view.run = Some(7);
            view.runs = Some(Ok(Builds {
                runs: vec![
                    (
                        project("up/repo"),
                        Run {
                            id: 7,
                            name: "up".to_owned(),
                            ..Run::default()
                        },
                    ),
                    (
                        project("me/repo"),
                        Run {
                            id: 7,
                            name: "fork".to_owned(),
                            ..Run::default()
                        },
                    ),
                ],
                errors: vec![],
            }));
        }
        assert_eq!(tab.ci_view_project(), Some(project("me/repo")));
        assert_eq!(
            tab.ci_view
                .as_ref()
                .and_then(|v| v.shown_run())
                .map(|r| r.name.as_str()),
            Some("fork")
        );
        // Refreshing status from another project does not change the loaded run's identity.
        tab.ci_arrived(
            false,
            vec![(
                project("up/repo"),
                Ok(std::collections::HashMap::from([(
                    "abc".to_owned(),
                    ci::Status {
                        state: CheckState::Passed,
                        checks: vec![],
                    },
                )])),
            )],
        );
        assert_eq!(tab.ci_view_project(), Some(project("me/repo")));
    }

    /// A poll of a running build that fails (the network blinked) is tried again: the view
    /// does not stop following the build.
    #[test]
    fn a_failed_poll_of_a_running_build_is_tried_again() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/ci-poll-test".into(), &ctx);
        tab.ci_view = Some(CiView {
            sha: "abc".to_owned(),
            request: 7,
            loading: true,
            runs: None,
            project: None,
            run: Some(1),
            jobs: None,
            job: None,
            log: None,
            log_request: 0,
            next: None,
            was_running: true,
            acting: false,
        });
        tab.ci_runs_arrived(7, Err("timed out".to_owned()), &ctx);
        assert!(tab.ci_view.as_ref().is_some_and(|v| v.next.is_some()));
        if let Some(view) = &mut tab.ci_view {
            view.next = None;
        }
        tab.ci_jobs_arrived(7, 1, Err("timed out".to_owned()), &ctx);
        assert!(tab.ci_view.as_ref().is_some_and(|v| v.next.is_some()));
    }
}
