//! Read-only GitHub run/attempt identities and bounded actual same-SHA history.

use super::*;

pub const LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RunKey {
    pub run: u64,
    pub attempt: u64,
}

impl RunKey {
    pub fn of(info: &RunInfo) -> Self {
        Self {
            run: info.id,
            attempt: info.attempt.max(1),
        }
    }
}

impl RunInfo {
    pub fn attempt_url(&self) -> String {
        if self.url.contains("/actions/runs/") {
            let base = self.url.split("/attempts/").next().unwrap_or(&self.url);
            format!("{base}/attempts/{}", self.attempt.max(1))
        } else {
            self.url.clone()
        }
    }
}

pub fn attempt(repo: &Path, project: &Project, key: RunKey) -> Result<TimedRun, String> {
    if project.forge != Forge::GitHub {
        return Err("Pinned workflow attempts are available for GitHub Actions.".into());
    }
    read_attempt(&project.path, key, |endpoint| {
        gh_api(repo, project, endpoint)
    })
}

fn read_attempt(
    project: &str,
    key: RunKey,
    mut api: impl FnMut(&str) -> Result<String, String>,
) -> Result<TimedRun, String> {
    if key.run == 0 || key.attempt == 0 {
        return Err("The workflow run and attempt must have exact nonzero identities.".into());
    }
    let endpoint = format!(
        "repos/{project}/actions/runs/{}/attempts/{}",
        key.run, key.attempt
    );
    let info = parse_github_run(&api(&endpoint)?)?;
    if RunKey::of(&info) != key || info.sha.is_empty() {
        return Err("The forge returned a different run/attempt or no revision.".into());
    }
    let (jobs, more) = parse_github_jobs(&api(&format!("{endpoint}/jobs?per_page={MAX_JOBS}"))?)?;
    Ok(TimedRun { info, jobs, more })
}

/// Recent workflow runs on the same branch, optionally restricted to one SHA.
/// `more` honestly marks a full first page; no assumption of complete history.
pub fn recent(
    repo: &Path,
    project: &Project,
    info: &RunInfo,
    same_sha: bool,
) -> Result<(Vec<RunInfo>, bool), String> {
    if project.forge != Forge::GitHub {
        return Err("Attempt history is available for GitHub Actions.".into());
    }
    let endpoint = recent_endpoint(&project.path, info, same_sha)?;
    let mut runs = parse_github_runs(&gh_api(repo, project, &endpoint)?)?;
    let more = runs.len() >= LIMIT;
    runs.retain(|run| same_workflow(info, run) && (!same_sha || run.sha == info.sha));
    runs.sort_by_key(|run| std::cmp::Reverse((run.number, run.created, run.id)));
    Ok((runs, more))
}

fn recent_endpoint(project: &str, info: &RunInfo, same_sha: bool) -> Result<String, String> {
    let workflow = info.workflow_id.ok_or("The workflow has no recorded ID.")?;
    let mut endpoint =
        format!("repos/{project}/actions/workflows/{workflow}/runs?per_page={LIMIT}");
    if !info.branch.is_empty() {
        endpoint.push_str(&format!("&branch={}", query_value(&info.branch)));
    }
    if same_sha {
        endpoint.push_str(&format!("&head_sha={}", query_value(&info.sha)));
    }
    Ok(endpoint)
}

pub fn same_workflow(a: &RunInfo, b: &RunInfo) -> bool {
    a.workflow_id.is_some() && a.workflow_id == b.workflow_id && a.branch == b.branch
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    pub runs: Vec<TimedRun>,
    pub notes: Vec<String>,
    pub more: bool,
}

/// Read earlier attempts and other actual runs at exactly this workflow/branch/SHA.
pub fn read(repo: &Path, project: &Project, current: &TimedRun) -> History {
    from_recent(current, recent(repo, project, &current.info, true), |key| {
        attempt(repo, project, key)
    })
}

fn from_recent(
    current: &TimedRun,
    recent: Result<(Vec<RunInfo>, bool), String>,
    fetch: impl FnMut(RunKey) -> Result<TimedRun, String>,
) -> History {
    let (runs, more) = match recent {
        Ok(found) => found,
        Err(error) => {
            return History {
                runs: vec![current.clone()],
                notes: vec![format!("Same-SHA history unknown: {error}")],
                ..History::default()
            };
        }
    };
    collect(current, &runs, more, fetch)
}

fn collect(
    current: &TimedRun,
    runs: &[RunInfo],
    more: bool,
    mut fetch: impl FnMut(RunKey) -> Result<TimedRun, String>,
) -> History {
    let mut history = History {
        runs: vec![current.clone()],
        more,
        ..History::default()
    };
    let (keys, limited) = attempt_keys(current, runs);
    history.more |= limited;
    for key in keys {
        let number = key.attempt;
        match fetch(key) {
            Ok(run)
                if same_workflow(&current.info, &run.info) && run.info.sha == current.info.sha =>
            {
                history.runs.push(run)
            }
            Ok(_) => history.notes.push(format!(
                "Run {} attempt {number}: revision/workflow mismatch; history unknown",
                key.run
            )),
            Err(error) => history.notes.push(format!(
                "Run {} attempt {number}: history unknown ({error})",
                key.run
            )),
        }
    }
    history.runs.sort_by_key(|run| {
        (
            run.info.number,
            run.info.created,
            run.info.id,
            run.info.attempt,
        )
    });
    history
}

/// Plan a bounded number of reads even if every requested attempt fails.
fn attempt_keys(current: &TimedRun, runs: &[RunInfo]) -> (Vec<RunKey>, bool) {
    let mut candidates = runs.to_vec();
    if !candidates.iter().any(|run| run.id == current.info.id) {
        candidates.insert(0, current.info.clone());
    }
    let mut keys = Vec::new();
    let mut seen = std::collections::HashSet::from([RunKey::of(&current.info)]);
    for info in candidates {
        if !same_workflow(&current.info, &info) || info.sha != current.info.sha {
            continue;
        }
        for number in (1..=info.attempt.max(1)).rev() {
            let key = RunKey {
                run: info.id,
                attempt: number,
            };
            if !seen.insert(key) {
                continue;
            }
            if keys.len() >= LIMIT - 1 {
                return (keys, true);
            }
            keys.push(key);
        }
    }
    (keys, false)
}

#[cfg(test)]
mod tests;
