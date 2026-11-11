//! Owned asynchronous CI discovery and pinned comparison snapshot reads.
use super::*;

impl RepoTab {
    /// Reads what the view shows (again): the runs of the commit, then the one shown.
    pub(in crate::repo) fn ci_path_load(&mut self, ctx: &egui::Context) {
        let subject = self.ci_path_subject();
        let projects = subject
            .as_ref()
            .map(|(_, sha)| self.ci_projects_for(sha))
            .unwrap_or_default();
        let forge = self.forge();
        let path = self.path.clone();
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        if state.subject != subject {
            state.explorer = Slot::default();
            state.run = None;
            state.pinned = None;
            state.workflow = None;
            state.baseline = Baseline::Latest;
            state.selected = None;
            state.expanded.clear();
        }
        state.subject = subject.clone();
        state.unavailable = None;
        let Some((_, sha)) = subject else {
            state.unavailable(
                "No commit",
                "Select a commit, or open a repository with one, to see its CI runs.".to_owned(),
            );
            return;
        };
        if projects.is_empty() {
            let why = match forge {
                None => "This repository has no remote on GitHub or GitLab: runs are read \
                         through gh or glab from the repository's forge."
                    .to_owned(),
                Some(forge) => format!(
                    "gitcraft reads CI runs from GitHub and GitLab only; {} isn't supported yet.",
                    forge.name()
                ),
            };
            state.unavailable("No CI to read", why);
            return;
        }
        let request = state.runs.start();
        self.insight_forge_then(
            ctx,
            move || {
                let mut runs = Runs::default();
                let mut answered = false;
                for project in projects {
                    match ci::runs(&path, &project, &sha) {
                        Ok(list) => {
                            answered = true;
                            runs.list
                                .extend(list.into_iter().map(|run| (project.clone(), run)));
                        }
                        Err(e) => runs.errors.push(format!("{}: {e}", project.path)),
                    }
                }
                if answered {
                    Ok(runs)
                } else {
                    Err(runs.errors.join("; "))
                }
            },
            move |tab, result: Result<Runs, String>, ctx| tab.ci_path_runs(request, result, ctx),
        );
    }

    /// The runs of the commit arrived: the one shown before stays, else the newest.
    pub(in crate::repo) fn ci_path_runs(
        &mut self,
        request: u64,
        result: Result<Runs, String>,
        ctx: &egui::Context,
    ) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        if !state.runs.accept(request, result) {
            return;
        }
        state.unavailable = state
            .runs
            .error()
            .filter(|error| tool_problem(error))
            .map(|error| ("The forge's tool is needed".to_owned(), error.to_owned()));
        if state.unavailable.is_some() {
            return;
        }
        let Some(runs) = state.runs.ready() else {
            return;
        };
        let keep = runs
            .list
            .iter()
            .find(|(p, r)| state.run.as_ref() == Some(&(p.clone(), r.id)));
        let pick = keep
            .or_else(|| runs.list.first())
            .map(|(p, r)| (p.clone(), r.id));
        let picked_attempt = keep.or_else(|| runs.list.first()).map(|(_, run)| RunKey {
            run: run.id,
            attempt: run.attempt.max(1),
        });
        if pick != state.run {
            state.baseline = Baseline::Latest;
            state.pinned = picked_attempt;
            state.workflow = None;
            state.selected = None;
            state.expanded.clear();
        }
        if state.pinned.is_none() {
            state.pinned = picked_attempt;
        }
        state.run = pick;
        if state.run.is_some() {
            self.ci_path_read_run(ctx);
        }
    }

    /// Reads the run shown, its dependencies and the earlier run it is compared with.
    pub(in crate::repo) fn ci_path_read_run(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        let (Some((project, run)), Some((name, sha))) = (state.run.clone(), state.subject.clone())
        else {
            return;
        };
        let baseline = state.baseline;
        let pinned = state.pinned;
        let request = state.explorer.restart();
        self.insight_forge_then(
            ctx,
            move || match pinned {
                Some(key) => analysis::read_pinned(&path, &project, key, baseline, (&name, &sha)),
                None => analysis::read(&path, &project, run, baseline, (&name, &sha)),
            },
            move |tab, result, _ctx| tab.ci_path_run(request, result),
        );
    }

    pub(in crate::repo) fn ci_path_run(&mut self, request: u64, result: Result<Explorer, String>) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        if result
            .as_ref()
            .is_ok_and(|explorer| !state.accepts(explorer))
        {
            return;
        }
        if state.explorer.accept(request, result)
            && let Some(explorer) = state.explorer.ready()
        {
            state.pinned = Some(RunKey::of(&explorer.run.info));
            state.workflow = explorer.run.info.workflow_id;
            if let Some(compared) = &explorer.compared {
                state.baseline = Baseline::Attempt(RunKey::of(&compared.run.info));
            }
            let jobs = explorer.run.jobs.len();
            state.selected = state.selected.filter(|&i| i < jobs);
            state.expanded.retain(|&i| i < jobs);
        }
    }
}
