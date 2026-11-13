//! Snapshot assembly and baseline shortcuts, apart from dependency analysis and painting.

use super::*;
use crate::ci::timing::history::{self, RunKey};

pub fn read(
    repo: &Path,
    project: &Project,
    run: u64,
    baseline: Baseline,
    subject: (&str, &str),
) -> Result<Explorer, String> {
    assemble(
        repo,
        project,
        timing::timed_run(repo, project, run)?,
        baseline,
        subject,
    )
}

pub fn read_pinned(
    repo: &Path,
    project: &Project,
    key: RunKey,
    baseline: Baseline,
    subject: (&str, &str),
) -> Result<Explorer, String> {
    let run = if project.forge == Forge::GitHub {
        history::attempt(repo, project, key)?
    } else {
        timing::timed_run(repo, project, key.run)?
    };
    assemble(repo, project, run, baseline, subject)
}

pub(super) fn baseline_key(
    info: &RunInfo,
    baseline: Baseline,
    greens: &[RunInfo],
    recent: &[RunInfo],
) -> Result<Option<RunKey>, String> {
    let selected = match baseline {
        Baseline::Latest => greens.first().map(RunKey::of),
        Baseline::None => None,
        Baseline::Attempt(key) => Some(key),
        Baseline::PreviousAttempt => Some(RunKey {
            run: info.id,
            attempt: info
                .attempt
                .checked_sub(1)
                .filter(|n| *n > 0)
                .ok_or("No previous attempt of this run at the same SHA.")?,
        }),
        Baseline::Previous => recent
            .iter()
            .find(|candidate| earlier(candidate, info))
            .map(RunKey::of),
        Baseline::Run(id) => Some(
            greens
                .iter()
                .chain(recent)
                .find(|run| run.id == id)
                .map(RunKey::of)
                .ok_or("That run is absent from the recorded baseline inventory.")?,
        ),
    };
    if selected == Some(RunKey::of(info)) {
        return Err("Choose a different run or attempt to compare.".into());
    }
    Ok(selected)
}

fn workflow_at(repo: &Path, run: &TimedRun) -> Option<Workflow> {
    let text = workflow::at_revision(repo, &run.info.sha, &run.info.path).ok()?;
    workflow::parse_github(&text).ok()
}

fn assemble(
    repo: &Path,
    project: &Project,
    timed: TimedRun,
    baseline: Baseline,
    subject: (&str, &str),
) -> Result<Explorer, String> {
    if timed.info.sha != subject.1 || timed.info.sha.is_empty() {
        return Err("The selected run does not belong to the selected revision.".into());
    }
    let file = |sha: &str, path: &str| workflow::at_revision(repo, sha, path);
    let deps = deps_of(&timed, project.forge, file);
    let critical = critical_path(&timed.jobs, &deps);
    let (mut baselines, mut baseline_error) =
        match timing::successful_runs(repo, project, &timed.info, BASELINES + 1) {
            Ok(runs) => (runs, None),
            Err(error) => (Vec::new(), Some(error)),
        };
    baselines.retain(|run| earlier(run, &timed.info));
    baselines.truncate(BASELINES);
    let (recent, history) = github_history(repo, project, &timed);
    let key = baseline_key(&timed.info, baseline, &baselines, &recent);
    let before = key.and_then(|key| {
        key.map(|key| {
            if project.forge == Forge::GitHub {
                history::attempt(repo, project, key)
            } else {
                timing::timed_run(repo, project, key.run)
            }
        })
        .transpose()
    });
    let mut diagnostics = Vec::new();
    let compared = match before {
        Ok(Some(before)) => match comparison(repo, project, &timed, before) {
            Ok((compared, observed)) => {
                diagnostics = observed;
                Some(compared)
            }
            Err(error) => {
                baseline_error = Some(error);
                None
            }
        },
        Ok(None) => None,
        Err(error) => {
            baseline_error = Some(error);
            None
        }
    };
    let mut provenance = provenance(project, &timed, compared.as_ref(), subject);
    if history.more || !history.notes.is_empty() {
        let note = "Same-SHA history is bounded or incomplete; missing outcomes are unknown";
        provenance.partial = Some(
            provenance
                .partial
                .map_or_else(|| note.into(), |prior| format!("{prior}; {note}")),
        );
    }
    Ok(Explorer {
        project: project.clone(),
        run: timed,
        deps,
        critical,
        baselines,
        recent,
        history,
        diagnostics,
        baseline_error,
        compared,
        now: crate::git::now(),
        provenance,
    })
}

fn earlier(candidate: &RunInfo, current: &RunInfo) -> bool {
    if candidate.id == current.id {
        return false;
    }
    if candidate.number > 0 && current.number > 0 {
        candidate.number < current.number
    } else {
        candidate
            .created
            .zip(current.created)
            .is_some_and(|(a, b)| a < b)
    }
}

fn github_history(
    repo: &Path,
    project: &Project,
    timed: &TimedRun,
) -> (Vec<RunInfo>, history::History) {
    if project.forge != Forge::GitHub {
        return (Vec::new(), history::History::default());
    }
    let recent = history::recent(repo, project, &timed.info, false);
    let mut history = history::read(repo, project, timed);
    match recent {
        Ok((runs, more)) => {
            history.more |= more;
            (runs, history)
        }
        Err(error) => {
            history
                .notes
                .push(format!("Previous-run inventory unknown: {error}"));
            (Vec::new(), history)
        }
    }
}

fn comparison(
    repo: &Path,
    project: &Project,
    now: &TimedRun,
    before: TimedRun,
) -> Result<(Compared, Vec<diagnostics::Job>), String> {
    if project.forge == Forge::GitHub
        && (!history::same_workflow(&now.info, &before.info)
            || (before.info.id == now.info.id && before.info.sha != now.info.sha))
    {
        return Err("The baseline has a different workflow, branch, or attempt revision.".into());
    }
    let deps = deps_of(&before, project.forge, |sha, path| {
        workflow::at_revision(repo, sha, path)
    });
    let critical = critical_path(&before.jobs, &deps);
    let workflow = super::revision::compare(repo, &now.info, &before.info);
    if project.forge != Forge::GitHub {
        let comparison = compare::compare(now, &before);
        return Ok((
            Compared {
                run: before,
                critical,
                comparison,
                workflow,
            },
            Vec::new(),
        ));
    }
    let (a, b) = (workflow_at(repo, now), workflow_at(repo, &before));
    let observed = diagnostics::compare(now, &before, a.as_ref(), b.as_ref());
    let comparison = compare::compare_aligned(
        now,
        &before,
        &observed.iter().map(|job| job.pair).collect::<Vec<_>>(),
    );
    Ok((
        Compared {
            run: before,
            critical,
            comparison,
            workflow,
        },
        observed,
    ))
}

#[cfg(test)]
mod tests;
