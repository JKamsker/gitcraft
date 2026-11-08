//! A stack's pull requests: each layer's into the layer below it (the bottom one's into the
//! branch the stack is on), published bottom-up and updated together later through whichever
//! forge hosts the repository ([`Provider`]). What was opened is recorded on the branch
//! (`branch.<name>.gitcraft-pr`) as soon as it is, so a publish that stopped halfway and is
//! run again never opens one twice. A layer whose parent's pull request is no longer open
//! (merged, or closed) goes into what that one went into instead.

use std::path::Path;

use super::{Found, Hosting, NewPullRequest, PrState, Provider};
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
    /// Its parent before, when the parent's pull request was merged and it goes into what
    /// that one went into instead.
    pub reparent_from: Option<String>,
    /// Its latest pull request (not a fork's), as the forge has it now.
    pub found: Option<Found>,
    /// The title of one to open: its oldest commit's subject, else its name.
    pub title: String,
    /// Its own commits' subjects, oldest first.
    pub commits: Vec<String>,
    /// `git diff --shortstat` of its own changes.
    pub stat: String,
}

/// What publishing does with a layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Push, and open its pull request (`replaces`: one closed without merging before).
    Create { replaces: Option<u64> },
    /// Push, and change where its open pull request goes.
    Retarget { number: u64, from: String },
    /// Push: its open pull request goes where it should.
    Push(u64),
    /// Nothing: its pull request was merged.
    Merged(u64),
    /// Push, but open nothing: it has no commits of its own, which no forge takes.
    Empty,
}

impl LayerPlan {
    pub fn step(&self) -> Step {
        match &self.found {
            Some(f) if f.state == PrState::Merged => Step::Merged(f.pr.number),
            Some(f) if f.state == PrState::Open && f.pr.base != self.base => Step::Retarget {
                number: f.pr.number,
                from: f.pr.base.clone(),
            },
            Some(f) if f.state == PrState::Open => Step::Push(f.pr.number),
            _ if self.commits.is_empty() => Step::Empty,
            Some(f) => Step::Create {
                replaces: Some(f.pr.number),
            },
            None => Step::Create { replaces: None },
        }
    }
}

/// A layer of the stack: its branch and its recorded parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub branch: String,
    pub parent: String,
}

/// Plans publishing `layers` (parents before the branches on them), each one's pull request
/// asked of the forge by `find` (see [`Provider::find`]).
pub fn plan(
    repo: &Path,
    layers: &[Input],
    find: &dyn Fn(&str) -> Result<Option<Found>, String>,
) -> Result<Vec<LayerPlan>, String> {
    let mut out: Vec<LayerPlan> = Vec::new();
    for layer in layers {
        let found = find(&layer.branch)?;
        // A parent whose pull request was merged: into what that one went into. Closed
        // without merging, it is opened anew, and the ones above stay on it.
        let (base, reparent_from) = match out.iter().find(|p| p.branch == layer.parent) {
            Some(parent) if matches!(parent.step(), Step::Merged(_)) => {
                let into = parent
                    .found
                    .as_ref()
                    .map_or(parent.base.clone(), |f| f.pr.base.clone());
                (into, Some(layer.parent.clone()))
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
            found,
            commits,
            stat,
        });
    }
    Ok(out)
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
        .filter_map(|p| match p.step() {
            Step::Push(n) | Step::Retarget { number: n, .. } => Some((p.branch.clone(), n)),
            _ => None,
        })
        .collect();
    for plan in plans {
        match publish_one(repo, plan, to, &numbers) {
            Ok((what, number)) => {
                if let Some(n) = number {
                    numbers.push((plan.branch.clone(), n));
                }
                report.done.push((plan.branch.clone(), what));
            }
            Err(e) => {
                report.failed = Some((plan.branch.clone(), e));
                break;
            }
        }
    }
    report
}

/// One layer: what was done, and the number of its pull request when it has one.
fn publish_one(
    repo: &Path,
    plan: &LayerPlan,
    to: &Target,
    numbers: &[(String, u64)],
) -> Result<(String, Option<u64>), String> {
    let noun = to.provider.noun();
    let step = plan.step();
    if let Step::Merged(n) = step {
        return Ok((
            crate::i18n::message("backend-n-merged-left-alone-28e650", &[("count", n.into())]),
            None,
        ));
    }
    if let Some(from) = &plan.reparent_from {
        // Its parent's pull request was merged: based on what that one went into now.
        let set = set_op(&config_key(&plan.branch, PARENT_KEY), &plan.base);
        let args: Vec<&str> = set.iter().map(String::as_str).collect();
        git::run(Some(repo), &args).map_err(|e| {
            crate::i18n::message(
                "backend-couldn-t-move-it-off-from-e-9e58d5",
                &[
                    ("from", (from).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })?;
    }
    push(repo, &plan.branch, to)?;
    match step {
        Step::Create { .. } => {
            let body = match numbers.iter().find(|(b, _)| *b == plan.base) {
                Some((parent, n)) => format!("Depends on #{n} ({parent})."),
                None => String::new(),
            };
            let new = NewPullRequest {
                reviewers: &[],
                branch: &plan.branch,
                base: &plan.base,
                title: &plan.title,
                body: &body,
                draft: to.draft,
            };
            let said = to.provider.create(repo, to.hosting, &new)?;
            let number = number_in(&said);
            if let Some(n) = number {
                record(repo, &plan.branch, n)?;
            }
            let what = match number {
                Some(n) => crate::i18n::message(
                    "backend-opened-n-into-50a67c",
                    &[
                        ("count", n.into()),
                        ("base", (plan.base).to_string().into()),
                    ],
                ),
                None => crate::i18n::message(
                    "backend-opened-into-said-e75205",
                    &[
                        ("base", (plan.base).to_string().into()),
                        ("said", (said).to_string().into()),
                    ],
                ),
            };
            Ok((what, number))
        }
        Step::Retarget { number, from } => {
            to.provider
                .retarget(repo, to.hosting, number, &plan.base)
                .map_err(|e| {
                    crate::i18n::message(
                        "backend-pointing-noun-number-at-failed-e-5b522b",
                        &[
                            ("noun", (noun).to_string().into()),
                            ("number", (number).to_string().into()),
                            ("base", (plan.base).to_string().into()),
                            ("detail", (e).to_string().into()),
                        ],
                    )
                })?;
            record(repo, &plan.branch, number)?;
            Ok((
                crate::i18n::message(
                    "backend-number-now-into-was-from-5423c7",
                    &[
                        ("number", (number).to_string().into()),
                        ("base", (plan.base).to_string().into()),
                        ("from", (from).to_string().into()),
                    ],
                ),
                Some(number),
            ))
        }
        Step::Push(number) => {
            record(repo, &plan.branch, number)?;
            Ok((
                crate::i18n::message(
                    "backend-number-has-the-new-commits-ca1768",
                    &[("number", (number).to_string().into())],
                ),
                Some(number),
            ))
        }
        Step::Empty => Ok((
            crate::i18n::message(
                "backend-pushed-no-noun-nothing-of-its-own-244003",
                &[("noun", (noun).to_string().into())],
            ),
            None,
        )),
        Step::Merged(_) => Ok((String::new(), None)),
    }
}

/// Records the number of `branch`'s pull request on it, as soon as it is known.
fn record(repo: &Path, branch: &str, number: u64) -> Result<(), String> {
    let set = set_op(&config_key(branch, PR_KEY), &number.to_string());
    let args: Vec<&str> = set.iter().map(String::as_str).collect();
    git::run(Some(repo), &args).map(drop)
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
            // A lease alone is checked against the remote-tracking branch, which the background
            // fetch moves: also make sure what it holds is in the branch's own history, so a
            // teammate's commits fetched but not taken in are never pushed over (git 2.30+).
            "--force-with-lease",
            "--force-if-includes",
            to.remote,
            &local_ref(branch),
        ],
        to.creds,
    )
    .map(drop)
    .map_err(|e| {
        crate::i18n::message(
            "backend-pushing-branch-failed-e-6bb910",
            &[
                ("branch", (branch).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })
}

/// The number of the pull request a forge said it opened: the last run of digits of its URL
/// (`…/pull/12`, `…/merge_requests/12`, `…/pulls/12`).
pub fn number_in(said: &str) -> Option<u64> {
    let url = said
        .split_whitespace()
        .rev()
        .find(|word| word.contains("://"))?;
    let tail: String = url
        .trim_end_matches('/')
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect();
    tail.chars().rev().collect::<String>().parse().ok()
}
