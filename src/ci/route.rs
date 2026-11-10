//! Which project on the forge a commit's builds are read from.
//!
//! A clone often has more than one remote, such as a fork and the repository it was forked
//! from. Builds run where a branch is pushed, so the tips of `fork/*` are asked of the fork,
//! those of `origin/*` of upstream, and local branches of their push and tracking remotes.
//! HEAD follows the checked-out branch; recent commits are also asked of the main project
//! (the selected forge in the UI). Pull requests are asked of that project, which lists them.

use std::collections::{HashMap, HashSet};

use super::{CheckState, Project, Status};
use crate::git::{Refs, Remote};

/// At most so many projects are asked at a time; the commits of any further remote are asked
/// of the main project.
const MAX_PROJECTS: usize = 4;

/// The commits to ask about, grouped by the project they are asked of, in the order the
/// projects came up (the checked-out branch's first).
pub(crate) type Plan = Vec<(Project, Vec<String>)>;

/// What each project answered.
pub(crate) type Answers = Vec<(Project, Result<HashMap<String, Status>, String>)>;

/// Last successful contribution from each project, in lookup order.
pub(crate) type Cached = Vec<(Project, HashMap<String, Status>)>;

/// What a plan is made from.
pub(crate) struct Sources<'a> {
    /// The caller's main project: the selected forge in the UI.
    pub main: &'a Project,
    pub remotes: &'a [Remote],
    pub refs: &'a Refs,
    /// The pull requests' head commits.
    pub prs: Vec<&'a str>,
    /// The latest commits of the graph.
    pub recent: Vec<&'a str>,
}

/// Plans which project each commit is asked of, at most `max` commits in all.
pub(crate) fn plan(sources: &Sources, max: usize) -> Plan {
    let mut router = Router {
        main: sources.main,
        remotes: sources.remotes,
        chosen: vec![sources.main.clone()],
    };
    let refs = sources.refs;
    let home = refs
        .current
        .as_deref()
        .and_then(|name| refs.local.iter().find(|b| b.name == name))
        .map(|b| router.of_branch(b, refs))
        .unwrap_or_else(|| vec![sources.main.clone()]);

    let mut wanted: Vec<(Project, &str)> = home
        .iter()
        .map(|p| (p.clone(), refs.head.as_str()))
        .collect();
    for branch in &refs.local {
        let projects =
            if refs.push_remotes.contains_key(&branch.name) || !branch.upstream.is_empty() {
                router.of_branch(branch, refs)
            } else {
                home.clone()
            };
        wanted.extend(projects.into_iter().map(|p| (p, branch.target.as_str())));
    }
    wanted.extend(sources.prs.iter().map(|sha| (sources.main.clone(), *sha)));
    for sha in &sources.recent {
        wanted.extend(home.iter().chain([sources.main]).map(|p| (p.clone(), *sha)));
    }
    for (remote, branches) in &refs.remotes {
        let project = router.of_remote(remote);
        wanted.extend(
            branches
                .iter()
                .map(|b| (project.clone(), b.target.as_str())),
        );
    }
    group(wanted, max)
}

/// The projects whose builds commit `sha` may have, likeliest first: those of the branches
/// that point at it, then the checked-out branch's, then the main project, then every other
/// project of the remotes (an older commit is in a branch's history without being its tip).
pub(crate) fn projects_for(
    sha: &str,
    main: &Project,
    remotes: &[Remote],
    refs: &Refs,
) -> Vec<Project> {
    let holders = |recent: Vec<&str>| {
        let sources = Sources {
            main,
            remotes,
            refs,
            prs: Vec::new(),
            recent,
        };
        plan(&sources, usize::MAX)
            .into_iter()
            .filter(|(_, shas)| shas.iter().any(|s| s == sha))
            .map(|(project, _)| project)
    };
    let mut projects: Vec<Project> = holders(Vec::new()).collect();
    let rest = holders(vec![sha])
        .chain([main.clone()])
        .chain(known(main, remotes));
    for project in rest {
        if projects.len() == MAX_PROJECTS {
            break;
        }
        if !projects.contains(&project) {
            projects.push(project);
        }
    }
    projects
}

/// Every project the repository's remotes point at that the main project's tool can ask.
pub(crate) fn known(main: &Project, remotes: &[Remote]) -> Vec<Project> {
    let mut projects = vec![main.clone()];
    for remote in remotes {
        if let Some(project) =
            Project::of_remote(main.forge, remote).filter(|p| p.host == main.host)
            && !projects.contains(&project)
        {
            projects.push(project);
        }
    }
    projects
}

/// The asked pairs, deduplicated and cut to `max`, grouped by project in order.
fn group(wanted: Vec<(Project, &str)>, max: usize) -> Plan {
    let mut seen = HashSet::new();
    let mut plan: Plan = Vec::new();
    let asked = wanted
        .into_iter()
        .filter(|(_, sha)| !sha.is_empty())
        .filter(|(project, sha)| seen.insert((project.clone(), *sha)))
        .take(max);
    for (project, sha) in asked {
        match plan.iter_mut().find(|(p, _)| *p == project) {
            Some((_, shas)) => shas.push(sha.to_owned()),
            None => plan.push((project, vec![sha.to_owned()])),
        }
    }
    plan
}

/// The commits of a plan, each with the project it is asked of.
pub(crate) fn pairs(plan: &Plan) -> HashSet<(Project, String)> {
    plan.iter()
        .flat_map(|(p, shas)| shas.iter().map(move |sha| (p.clone(), sha.clone())))
        .collect()
}

struct Router<'a> {
    main: &'a Project,
    remotes: &'a [Remote],
    /// The projects asked so far, the main one first.
    chosen: Vec<Project>,
}

impl Router<'_> {
    /// A branch's push destination first, followed by its tracking upstream if different.
    fn of_branch(&mut self, branch: &crate::git::Branch, refs: &Refs) -> Vec<Project> {
        let mut projects = Vec::new();
        if let Some(remote) = refs.push_remotes.get(&branch.name) {
            projects.push(self.of_remote(remote));
        }
        if let Some(project) = self.of_upstream(&branch.upstream)
            && !projects.contains(&project)
        {
            projects.push(project);
        }
        if projects.is_empty() {
            projects.push(self.main.clone());
        }
        projects
    }

    /// The project of remote `name`. A remote on another host than the main project (another
    /// forge, which the main project's tool can't ask), one gitcraft can't read, or one past
    /// [`MAX_PROJECTS`] is asked as the main project.
    fn of_remote(&mut self, name: &str) -> Project {
        let project = self
            .remotes
            .iter()
            .find(|r| r.name == name)
            .and_then(|r| Project::of_remote(self.main.forge, r))
            .filter(|p| p.host == self.main.host);
        match project {
            Some(p) if self.chosen.contains(&p) => p,
            Some(p) if self.chosen.len() < MAX_PROJECTS => {
                self.chosen.push(p.clone());
                p
            }
            _ => self.main.clone(),
        }
    }

    /// The project of the remote an upstream such as `fork/dev` is on, if it has one.
    fn of_upstream(&mut self, upstream: &str) -> Option<Project> {
        let remote = remote_of(upstream, self.remotes)?;
        Some(self.of_remote(remote))
    }
}

/// The remote `upstream` (`my/fork/dev`) is a branch of: the longest remote name it starts
/// with, as remote names can have slashes in them.
fn remote_of<'a>(upstream: &str, remotes: &'a [Remote]) -> Option<&'a str> {
    remotes
        .iter()
        .map(|r| r.name.as_str())
        .filter(|name| {
            upstream
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|name| name.len())
}

/// The statuses read, with the project each commit's came from.
#[derive(Debug, Default)]
pub(crate) struct Merged {
    pub by_sha: HashMap<String, Status>,
    pub cached: Cached,
    pub source: HashMap<String, Project>,
    /// Why a project couldn't be read, when another could.
    pub error: Option<String>,
}

/// Puts the projects' answers together. A commit two projects build is shown as both did
/// (the worse state, every check), and opens in the first one's builds. A project that
/// couldn't be read keeps its own cached contribution; when none could, the errors are returned.
pub(crate) fn merge(
    answers: Answers,
    before: &[(Project, HashMap<String, Status>)],
) -> Result<Merged, String> {
    let mut merged = Merged::default();
    let mut errors = Vec::new();
    let mut answered = false;
    for (project, answer) in answers {
        let statuses = match answer {
            Ok(statuses) => {
                answered = true;
                statuses
            }
            Err(e) => {
                errors.push(format!("{}: {e}", project.path));
                before
                    .iter()
                    .find(|(p, _)| *p == project)
                    .map(|(_, statuses)| statuses.clone())
                    .unwrap_or_default()
            }
        };
        for (sha, status) in &statuses {
            match merged.by_sha.get_mut(sha) {
                Some(known) => *known = combine(known, status.clone()),
                None => {
                    merged.source.insert(sha.clone(), project.clone());
                    merged.by_sha.insert(sha.clone(), status.clone());
                }
            }
        }
        merged.cached.push((project, statuses));
    }
    if !answered && !errors.is_empty() {
        return Err(errors.join("; "));
    }
    merged.error = (!errors.is_empty()).then(|| errors.join("; "));
    Ok(merged)
}

/// One commit's statuses from two projects, as one: the worse state and every check.
fn combine(a: &Status, b: Status) -> Status {
    let rank = |s: CheckState| match s {
        CheckState::Failed => 3,
        CheckState::Running => 2,
        CheckState::Passed => 1,
        CheckState::Skipped => 0,
    };
    let state = if rank(b.state) > rank(a.state) {
        b.state
    } else {
        a.state
    };
    let mut checks = a.checks.clone();
    checks.extend(b.checks);
    Status { state, checks }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fixtures;
