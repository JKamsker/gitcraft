//! The CI critical path and regression explorer (#175 item 2), apart from its drawing: one run's
//! jobs and steps with their queued and running times, the dependencies between the jobs, the
//! critical path through them, and the run against an earlier successful one.
//!
//! Dependencies come from the workflow file at the run's commit, read from Git (`needs:`, and
//! GitLab's stages); when it can't be read, they are inferred from when jobs started after
//! others ended, and said to be. Runs are read through `gh` and `glab`; nothing is re-run.

pub mod alignment;
pub mod compare;
pub mod diagnostics;
mod load;
pub mod workflow;
pub use load::{read, read_pinned};

#[cfg(test)]
mod tests;

use std::path::Path;

use self::compare::Comparison;
use self::workflow::Workflow;
use super::{Evidence, Provenance};
use crate::ci::timing::{self, RunInfo, TimedJob, TimedRun};
use crate::ci::{CheckState, Project};
use crate::forge::Forge;

/// A job that started (or was queued) within so many seconds of another's end is taken to
/// have waited for it, when nothing says what it waited for.
pub const SLACK: i64 = 10;
/// So many earlier successful runs are offered to compare with.
pub const BASELINES: usize = 10;

/// Job `from` has to finish before job `to` starts (indices into the run's jobs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dep {
    pub from: usize,
    pub to: usize,
    pub evidence: Evidence,
}

/// Where the dependencies came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepSource {
    /// The workflow file at the run's commit.
    File,
    /// GitLab's stages as the forge lists them (the file wasn't read).
    Stages,
    /// When jobs started after others ended.
    Timing,
}

impl DepSource {
    pub fn label(self) -> &'static str {
        match self {
            DepSource::File => "the workflow file",
            DepSource::Stages => "the stages",
            DepSource::Timing => "timing",
        }
    }
}

/// The dependencies between a run's jobs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deps {
    pub edges: Vec<Dep>,
    pub source: DepSource,
    /// For each job, the file job it was matched to (its id) and how surely.
    pub matched: Vec<Option<(String, Evidence)>>,
    /// What is worth knowing of how they were found: a file not read, jobs not matched.
    pub notes: Vec<String>,
}

impl Deps {
    /// The jobs job `to` waits for.
    pub fn before(&self, to: usize) -> impl Iterator<Item = &Dep> {
        self.edges.iter().filter(move |d| d.to == to)
    }
}

/// The chain of jobs that decided when the run ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Critical {
    /// In the order they ran.
    pub jobs: Vec<usize>,
    /// The weakest of its links.
    pub evidence: Option<Evidence>,
    /// From the first one's queueing (or start) to the last one's end.
    pub span: Option<i64>,
}

impl Critical {
    pub fn contains(&self, job: usize) -> bool {
        self.jobs.contains(&job)
    }

    /// Whether `from` → `to` is one of its links.
    pub fn link(&self, from: usize, to: usize) -> bool {
        self.jobs.windows(2).any(|w| w[0] == from && w[1] == to)
    }
}

/// Dependencies from what `workflow` declares: `needs:`, or for a GitLab job without them, every
/// job of the stage before its own.
pub fn file_deps(jobs: &[TimedJob], workflow: &Workflow, gitlab: bool) -> Deps {
    let matched: Vec<Option<(usize, Evidence)>> = jobs
        .iter()
        .map(|job| workflow::match_job(&job.name, workflow))
        .collect();
    let mut edges = Vec::new();
    for (to, m) in matched.iter().enumerate() {
        let Some((file_job, to_evidence)) = *m else {
            continue;
        };
        let declared = &workflow.jobs[file_job];
        if declared.declared || !gitlab {
            for need in &declared.needs {
                for (from, n) in matched.iter().enumerate() {
                    if let Some((other, from_evidence)) = *n
                        && workflow.jobs[other].id == *need
                        && from != to
                    {
                        edges.push(Dep {
                            from,
                            to,
                            evidence: to_evidence.max(from_evidence),
                        });
                    }
                }
            }
        } else if let Some(stage) = workflow.stages.iter().position(|s| *s == jobs[to].stage) {
            let earlier = workflow.stages[..stage]
                .iter()
                .rev()
                .find(|s| jobs.iter().any(|j| j.stage == **s));
            for (from, job) in jobs.iter().enumerate() {
                if Some(&job.stage) == earlier {
                    edges.push(Dep {
                        from,
                        to,
                        evidence: to_evidence,
                    });
                }
            }
        }
    }
    let unmatched = matched.iter().filter(|m| m.is_none()).count();
    let mut notes = Vec::new();
    if unmatched > 0 {
        notes.push(format!(
            "{unmatched} of {} jobs couldn't be matched to the workflow file: their dependencies \
             aren't known",
            jobs.len()
        ));
    }
    if workflow.includes {
        notes.push(
            "The file includes or extends others, which weren't read: dependencies may be missing"
                .to_owned(),
        );
    }
    Deps {
        edges,
        source: DepSource::File,
        matched: matched
            .iter()
            .map(|m| m.map(|(i, e)| (workflow.jobs[i].id.clone(), e)))
            .collect(),
        notes,
    }
}

/// Dependencies from GitLab's stages in the order the forge lists their jobs: each job waits
/// for those of the stage before. Inferred, since `needs:` may say otherwise.
pub fn stage_deps(jobs: &[TimedJob]) -> Deps {
    let mut order: Vec<&str> = Vec::new();
    for job in jobs {
        if !order.contains(&job.stage.as_str()) {
            order.push(&job.stage);
        }
    }
    let mut edges = Vec::new();
    for (to, job) in jobs.iter().enumerate() {
        let Some(stage) = order.iter().position(|s| *s == job.stage) else {
            continue;
        };
        if stage == 0 {
            continue;
        }
        for (from, other) in jobs.iter().enumerate() {
            if other.stage == order[stage - 1] {
                edges.push(Dep {
                    from,
                    to,
                    evidence: Evidence::Inferred,
                });
            }
        }
    }
    Deps {
        edges,
        source: DepSource::Stages,
        matched: vec![None; jobs.len()],
        notes: Vec::new(),
    }
}

/// Dependencies inferred from timing: a job queued or started within [`SLACK`] seconds of
/// another's end is taken to have waited for it.
pub fn timing_deps(jobs: &[TimedJob]) -> Deps {
    let mut edges = Vec::new();
    for (to, job) in jobs.iter().enumerate() {
        let marks: Vec<i64> = [job.ready(), job.started].into_iter().flatten().collect();
        for (from, other) in jobs.iter().enumerate() {
            let (Some(end), Some(began)) = (other.finished, other.started) else {
                continue;
            };
            let first = marks.iter().copied().min();
            if from != to
                && first.is_some_and(|f| began + SLACK < f)
                && marks.iter().any(|m| (m - end).abs() <= SLACK)
            {
                edges.push(Dep {
                    from,
                    to,
                    evidence: Evidence::Inferred,
                });
            }
        }
    }
    Deps {
        edges,
        source: DepSource::Timing,
        matched: vec![None; jobs.len()],
        notes: Vec::new(),
    }
}

/// The critical path: from the job that ended last, back through the dependency of each that
/// ended last, to one that waited for nothing. Jobs whose end isn't known aren't on it.
pub fn critical_path(jobs: &[TimedJob], deps: &Deps) -> Critical {
    let Some(mut at) = (0..jobs.len())
        .filter(|&i| jobs[i].finished.is_some())
        .max_by_key(|&i| (jobs[i].finished, jobs[i].started, std::cmp::Reverse(i)))
    else {
        return Critical::default();
    };
    let base = match deps.source {
        DepSource::Timing | DepSource::Stages => Evidence::Inferred,
        DepSource::File => deps.matched[at]
            .as_ref()
            .map_or(Evidence::Unknown, |(_, e)| *e),
    };
    let mut evidence = base;
    let mut path = vec![at];
    loop {
        let before = deps
            .before(at)
            .filter(|d| jobs[d.from].finished.is_some() && !path.contains(&d.from))
            .max_by_key(|d| (jobs[d.from].finished, std::cmp::Reverse(d.from)));
        let Some(dep) = before else { break };
        evidence = evidence.max(dep.evidence);
        at = dep.from;
        path.push(at);
    }
    path.reverse();
    let first = &jobs[path[0]];
    let last = &jobs[path[path.len() - 1]];
    let span = first
        .ready()
        .or(first.started)
        .zip(last.finished)
        .map(|(s, f)| (f - s).max(0));
    Critical {
        jobs: path,
        evidence: Some(evidence),
        span,
    }
}

/// Which run to compare with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Baseline {
    /// The latest earlier successful run of the same workflow on the same branch.
    #[default]
    Latest,
    /// None: the run alone.
    None,
    /// This run.
    Run(u64),
    /// The immediately previous workflow run, regardless of conclusion.
    Previous,
    /// The preceding attempt of this run at exactly the same revision.
    PreviousAttempt,
    /// Exact immutable selection after a shortcut or picker resolved.
    Attempt(timing::history::RunKey),
}

/// The earlier run compared with, and the comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compared {
    pub run: TimedRun,
    pub critical: Critical,
    pub comparison: Comparison,
}

/// Everything the view shows of one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explorer {
    pub project: Project,
    pub run: TimedRun,
    pub deps: Deps,
    pub critical: Critical,
    /// The earlier successful runs it can be compared with, newest first.
    pub baselines: Vec<RunInfo>,
    pub recent: Vec<RunInfo>,
    pub history: timing::history::History,
    pub diagnostics: Vec<diagnostics::Job>,
    /// Why they, or the one picked, couldn't be read.
    pub baseline_error: Option<String>,
    pub compared: Option<Compared>,
    /// When it was read: what still runs is drawn up to here.
    pub now: i64,
    pub provenance: Provenance,
}

/// The dependencies of `run`'s jobs: from its workflow file at its commit when that can be
/// read, else from the stages (GitLab) or timing. `file` reads a file at a commit.
pub fn deps_of(
    run: &TimedRun,
    forge: Forge,
    file: impl Fn(&str, &str) -> Result<String, String>,
) -> Deps {
    let gitlab = forge == Forge::GitLab;
    let read = if run.info.path.is_empty() {
        Err("the run didn't come from a workflow file of the repository".to_owned())
    } else {
        file(&run.info.sha, &run.info.path).and_then(|text| {
            if gitlab {
                workflow::parse_gitlab(&text)
            } else {
                workflow::parse_github(&text)
            }
        })
    };
    match read {
        Ok(w) => {
            let mut deps = file_deps(&run.jobs, &w, gitlab);
            deps.notes.insert(
                0,
                format!(
                    "Dependencies from {} at {}",
                    run.info.path,
                    run.info.sha.get(..7).unwrap_or(&run.info.sha)
                ),
            );
            deps
        }
        Err(why) => {
            let mut deps = if gitlab && run.jobs.iter().any(|j| !j.stage.is_empty()) {
                stage_deps(&run.jobs)
            } else {
                timing_deps(&run.jobs)
            };
            deps.notes.push(format!(
                "No dependencies known ({why}): they are inferred from {}",
                deps.source.label()
            ));
            deps
        }
    }
}

/// Reads run `run` of `project`, its dependencies and critical path, and the earlier run to
/// compare it with. `subject`: the commit the view is about, by name (`HEAD`) and id.
/// Where it came from: the forge's runs of the commit, and how complete they are.
pub fn provenance(
    project: &Project,
    run: &TimedRun,
    compared: Option<&Compared>,
    subject: (&str, &str),
) -> Provenance {
    let source = match project.forge {
        Forge::GitLab => "GitLab CI (glab)",
        _ => "GitHub Actions (gh)",
    };
    let mut scope = format!("{} #{} of {}", run.info.name, run.info.number, project.path);
    if run.info.attempt > 1 {
        scope.push_str(&format!(", attempt {}", run.info.attempt));
    }
    if let Some(c) = compared {
        scope.push_str(&format!(" against #{}", c.run.info.number));
    }
    let mut p = Provenance::new(source, scope).tip(subject.0, subject.1);
    let mut partial = Vec::new();
    if run.more {
        partial.push(format!(
            "only the first {} jobs were read",
            timing::MAX_JOBS
        ));
    }
    if run.info.state == CheckState::Running || run.jobs.iter().any(|j| j.finished.is_none()) {
        partial.push("some jobs hadn't finished when it was read".to_owned());
    }
    if compared.is_some_and(|c| c.run.more) {
        partial.push("the earlier run was read in part".to_owned());
    }
    if !partial.is_empty() {
        p = p.partial(partial.join("; "));
    }
    p
}
