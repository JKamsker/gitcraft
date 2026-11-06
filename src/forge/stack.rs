//! A stack's pull requests: each layer's into the layer below it (the bottom one's into the
//! branch the stack is on), published bottom-up and updated together later through whichever
//! forge hosts the repository ([`Provider`]). What was opened is recorded on the branch
//! (`branch.<name>.gitcraft-pr`) as soon as it is, so a publish that stopped halfway and is
//! run again never opens one twice. A layer whose parent's pull request is no longer open
//! (merged, or closed) goes into what that one went into instead.

use std::path::Path;

use super::{Hosting, NewPullRequest, Provider, PullRequest};
use crate::git::stack::{PARENT_KEY, config_key, local_ref, set_op};
use crate::git::{self, Credentials};

/// The config key (under `branch.<name>`) holding the number of the branch's pull request.
pub const PR_KEY: &str = "gitcraft-pr";

/// One layer, as publishing will leave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerPlan {
    pub branch: String,
    /// The branch its pull request goes into.
    pub base: String,
    /// Its parent before, when the parent's pull request is gone (merged or closed) and it
    /// goes into the parent's base instead.
    pub reparent_from: Option<String>,
    /// Its open pull request.
    pub existing: Option<PullRequest>,
    /// The pull request recorded for it, when that one isn't open any more.
    pub gone: Option<u64>,
    /// The title and description of one to open.
    pub title: String,
    /// Its own commits' subjects, oldest first.
    pub commits: Vec<String>,
    /// `git diff --shortstat` of its own changes.
    pub stat: String,
}

/// What publishing does with a layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Push, and open its pull request.
    Create,
    /// Push, and change where its pull request goes.
    Retarget { from: String },
    /// Push: its pull request goes where it should.
    Push,
    /// Nothing: its pull request is merged or closed.
    Skip(u64),
}

impl LayerPlan {
    pub fn step(&self) -> Step {
        match (&self.existing, self.gone) {
            (Some(pr), _) if pr.base != self.base => Step::Retarget {
                from: pr.base.clone(),
            },
            (Some(_), _) => Step::Push,
            (None, Some(n)) => Step::Skip(n),
            (None, None) => Step::Create,
        }
    }
}

/// A layer of the stack: its branch, its recorded parent, and the stack's root for the bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub branch: String,
    pub parent: String,
}

/// Plans publishing `layers` (parents before the branches on them), `open` being the
/// pull requests open now.
pub fn plan(repo: &Path, layers: &[Input], open: &[PullRequest]) -> Result<Vec<LayerPlan>, String> {
    let mut out: Vec<LayerPlan> = Vec::new();
    for layer in layers {
        let recorded = recorded_pr(repo, &layer.branch);
        let existing = open
            .iter()
            .find(|p| p.branch == layer.branch)
            .or_else(|| recorded.and_then(|n| open.iter().find(|p| p.number == n)))
            .cloned();
        let gone = recorded.filter(|_| existing.is_none());
        // A parent whose pull request is gone: into what that one went into.
        let (base, reparent_from) = match out.iter().find(|p| p.branch == layer.parent) {
            Some(parent) if parent.gone.is_some() => {
                (parent.base.clone(), Some(layer.parent.clone()))
            }
            _ => (layer.parent.clone(), None),
        };
        let range = format!("{}..{}", local_ref(&layer.parent), local_ref(&layer.branch));
        let subjects = git::run(
            Some(repo),
            &[
                "log",
                "--reverse",
                "--format=%s",
                "--end-of-options",
                &range,
                "--",
            ],
        )?;
        let commits: Vec<String> = subjects.lines().map(str::to_owned).collect();
        let stat = git::run(
            Some(repo),
            &[
                "diff",
                "--shortstat",
                "--end-of-options",
                &format!(
                    "{}...{}",
                    local_ref(&layer.parent),
                    local_ref(&layer.branch)
                ),
                "--",
            ],
        )?
        .trim()
        .to_owned();
        out.push(LayerPlan {
            title: commits
                .first()
                .cloned()
                .unwrap_or_else(|| layer.branch.clone()),
            branch: layer.branch.clone(),
            base,
            reparent_from,
            existing,
            gone,
            commits,
            stat,
        });
    }
    Ok(out)
}

/// The pull request recorded for `branch`.
fn recorded_pr(repo: &Path, branch: &str) -> Option<u64> {
    let key = config_key(branch, PR_KEY);
    git::run_ok_codes(Some(repo), &["config", "--local", "--get", &key], &[0, 1])
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// How publishing went: what was done, in order, and where it stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// (branch, what was done), e.g. ("api", "opened #12").
    pub done: Vec<(String, String)>,
    /// (branch, why) for the layer it stopped at; the ones after weren't touched.
    pub failed: Option<(String, String)>,
}

/// What publishing needs besides the plan.
pub struct Target<'a> {
    pub provider: &'a dyn Provider,
    pub hosting: &'a Hosting,
    /// The remote the forge is on, where the branches are pushed.
    pub remote: &'a str,
    pub draft: bool,
    pub creds: Option<&'a Credentials>,
}

/// Publishes `plans` bottom-up: pushes each layer (with a lease: a restacked one is pushed
/// over its old commits, but never over someone else's), then opens its pull request or
/// points it at its base. Stops at the first failure; what was done stays recorded.
pub fn publish(repo: &Path, plans: &[LayerPlan], to: &Target) -> Report {
    let mut report = Report::default();
    // Pull requests by branch, as they are now, for the descriptions of the ones above.
    let mut numbers: Vec<(String, u64)> = plans
        .iter()
        .filter_map(|p| Some((p.branch.clone(), p.existing.as_ref()?.number)))
        .collect();
    for plan in plans {
        match publish_one(repo, plan, to, &numbers) {
            Ok(Some((what, number))) => {
                if let Some(n) = number {
                    numbers.push((plan.branch.clone(), n));
                }
                report.done.push((plan.branch.clone(), what));
            }
            Ok(None) => {}
            Err(e) => {
                report.failed = Some((plan.branch.clone(), e));
                break;
            }
        }
    }
    report
}

/// One layer: what was done (and the number of a pull request opened), `None` for nothing.
fn publish_one(
    repo: &Path,
    plan: &LayerPlan,
    to: &Target,
    numbers: &[(String, u64)],
) -> Result<Option<(String, Option<u64>)>, String> {
    let step = plan.step();
    if let Step::Skip(n) = step {
        return Ok(Some((
            format!("#{n} is merged or closed: left alone"),
            None,
        )));
    }
    if let Some(from) = &plan.reparent_from {
        // Its parent's pull request is gone: based on what that one went into now.
        let set = set_op(&config_key(&plan.branch, PARENT_KEY), &plan.base);
        let args: Vec<&str> = set.iter().map(String::as_str).collect();
        git::run(Some(repo), &args).map_err(|e| format!("couldn't move it off {from}: {e}"))?;
    }
    push(repo, &plan.branch, to)?;
    match step {
        Step::Create => {
            let body = match numbers.iter().find(|(b, _)| *b == plan.base) {
                Some((parent, n)) => format!("Depends on #{n} ({parent})."),
                None => String::new(),
            };
            let new = NewPullRequest {
                branch: &plan.branch,
                base: &plan.base,
                title: &plan.title,
                body: &body,
                draft: to.draft,
            };
            let said = to.provider.create(repo, to.hosting, &new)?;
            let number = number_in(&said);
            // Recorded at once: run again, this is never opened a second time.
            if let Some(n) = number {
                let set = set_op(&config_key(&plan.branch, PR_KEY), &n.to_string());
                let args: Vec<&str> = set.iter().map(String::as_str).collect();
                git::run(Some(repo), &args)?;
            }
            let what = match number {
                Some(n) => format!("opened #{n} into {}", plan.base),
                None => format!("opened into {}: {said}", plan.base),
            };
            Ok(Some((what, number)))
        }
        Step::Retarget { from } => {
            let pr = plan.existing.as_ref().ok_or("its pull request is gone")?;
            to.provider
                .retarget(repo, to.hosting, pr.number, &plan.base)?;
            Ok(Some((
                format!("#{} now into {} (was {from})", pr.number, plan.base),
                None,
            )))
        }
        Step::Push => {
            let n = plan.existing.as_ref().map_or(0, |p| p.number);
            Ok(Some((format!("#{n} updated"), None)))
        }
        Step::Skip(_) => Ok(None),
    }
}

/// Pushes `branch` to the forge's remote, setting it as its upstream.
fn push(repo: &Path, branch: &str, to: &Target) -> Result<(), String> {
    // No `:dst`, which would bypass `remote.<name>.push`; push.default=upstream would push it
    // to the branch it tracks.
    git::run_with(
        Some(repo),
        &[
            "-c",
            "push.default=simple",
            "push",
            "-u",
            "--force-with-lease",
            to.remote,
            &local_ref(branch),
        ],
        to.creds,
    )
    .map(drop)
    .map_err(|e| format!("pushing {branch} failed: {e}"))
}

/// The number of the pull request a forge said it opened: the last run of digits of its URL
/// (`…/pull/12`, `…/merge_requests/12`, `…/pulls/12`).
pub fn number_in(said: &str) -> Option<u64> {
    let url = said.lines().rev().find(|l| l.contains("://"))?.trim();
    let tail: String = url
        .trim_end_matches('/')
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect();
    tail.chars().rev().collect::<String>().parse().ok()
}
