//! Starting work from an issue assigned to you: the issues listed from the forge, a branch (or a
//! worktree) named after the one picked, its acceptance criteria kept in view on a card while
//! that branch is checked out, and the pull request of the branch saying it closes the issue.

mod card;
pub(crate) mod criteria;
pub(crate) mod link;
pub(crate) mod naming;
pub(crate) use naming::branch_name as issue_branch;
pub(crate) use naming::tracker_branch;
mod picker;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use link::{IssueLink, Place, Source, Start};

use super::commands::remote_start;
use super::{OpKind, Refresh, RepoTab};
use crate::forge::Issue;
use crate::git;

/// What the tab keeps of the issues: the links of its branches, and the list being picked from.
#[derive(Default)]
pub(crate) struct IssueWork {
    /// The issue each branch was made for, by branch.
    links: HashMap<String, IssueLink>,
    /// The branch each remote's `HEAD` points to (`origin` → `dev`): its default branch, as the
    /// clone (or `git remote set-head`) learned it.
    remote_heads: HashMap<String, String>,
    picker: Option<picker::Picker>,
    /// Counts the lists asked for: one that comes after the picker was closed or asked again
    /// is dropped.
    requests: u64,
    /// The branch whose issue is being read again.
    refreshing: Option<String>,
}

/// What each read of the repository reads of the issues, beside the rest (see `load.rs`).
pub(crate) struct Read {
    links: Result<HashMap<String, IssueLink>, String>,
    remote_heads: HashMap<String, String>,
}

impl Default for Read {
    fn default() -> Read {
        Read {
            links: Ok(HashMap::new()),
            remote_heads: HashMap::new(),
        }
    }
}

/// Reads the branches' links and the remotes' default branches, on the reload's thread.
pub(crate) fn read(repo: &Path) -> Read {
    Read {
        links: link::read(repo),
        remote_heads: remote_heads(repo),
    }
}

/// `origin` → `dev` for each `refs/remotes/<remote>/HEAD` there is (a remote's name can have
/// slashes in it: what is under `refs/remotes/` before `/HEAD`).
fn remote_heads(repo: &Path) -> HashMap<String, String> {
    let out = git::run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(refname)%00%(symref)",
            "refs/remotes/**/HEAD",
        ],
    )
    .unwrap_or_default();
    out.lines()
        .filter_map(|line| {
            let (name, target) = line.split_once('\0')?;
            let remote = name.strip_prefix("refs/remotes/")?.strip_suffix("/HEAD")?;
            let branch = target.strip_prefix(&format!("refs/remotes/{remote}/"))?;
            Some((remote.to_owned(), branch.to_owned()))
        })
        .collect()
}

/// What is asked of the issues.
pub(crate) enum IssueAct {
    /// List the issues assigned to you, to pick one.
    OpenPicker,
    OpenSource(Option<crate::trackers::Kind>),
    /// Start work on `issue` on the branch `branch`, in a worktree of its own or here.
    Start {
        issue: Box<Issue>,
        branch: String,
        worktree: bool,
    },
    /// Read the branch's issue again, for its criteria as they are now.
    Refresh(String),
    /// Forget which issue the branch is for.
    Unlink(String),
}

/// The branch work on an issue is merged into, and starts from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Base {
    /// The default remote, if there is one.
    pub remote: Option<String>,
    pub name: String,
    /// The remote says it's its default branch (its `HEAD`); else `name` is a guess.
    pub known: bool,
}

/// What starting work does, worked out from the name and the place picked: what the picker says
/// and what Start runs, from one place.
pub(super) struct Plan {
    /// Why it can't start, when it can't.
    pub start: Result<Start, String>,
    /// The worktree to open once it's started.
    pub open: Option<PathBuf>,
    /// What will happen, in a sentence.
    pub note: String,
    /// What to know before: changes that go along, a link to another issue taken over.
    pub warnings: Vec<String>,
}

impl RepoTab {
    pub(super) fn issue_act(&mut self, act: IssueAct, ctx: &egui::Context) {
        match act {
            IssueAct::OpenPicker => self.open_issue_picker(None, ctx),
            IssueAct::OpenSource(source) => self.open_issue_picker(source, ctx),
            IssueAct::Start {
                issue,
                branch,
                worktree,
            } => self.start_issue(&issue, &branch, worktree, ctx),
            IssueAct::Refresh(branch) => self.refresh_issue(branch, ctx),
            IssueAct::Unlink(branch) => self.unlink_issue(branch, ctx),
        }
    }

    /// The title and description a pull request of `branch` starts with, and the branch it goes
    /// into, when it was made for an issue.
    pub(super) fn issue_pull_request(&self, branch: &str) -> Option<(String, String, String)> {
        let link = self.issue_work.links.get(branch)?;
        let (title, body) = link.pull_request_text();
        Some((title, body, self.issue_base().name))
    }

    /// Takes in what a read of the repository read of the issues. A link that couldn't be read
    /// leaves the cards as they were, and says so only when the status line has nothing more
    /// important to say (an operation's end).
    pub(super) fn take_issue_read(&mut self, read: Read) {
        self.issue_work.remote_heads = read.remote_heads;
        match read.links {
            Ok(links) => self.issue_work.links = links,
            Err(e) if self.status.is_none() && self.busy_op.is_none() => {
                self.status = Some(crate::repo::Notice::plain((
                    format!("Couldn't read the branches' issues: {e}"),
                    true,
                )));
            }
            Err(_) => {}
        }
    }

    fn open_issue_picker(&mut self, source: Option<crate::trackers::Kind>, ctx: &egui::Context) {
        let hosting = self.hosting.clone();
        let label = source.map_or_else(
            || {
                hosting.as_ref().map_or(
                    crate::i18n::text("repo-workflows-issue_work-open-issue-picker-3a89bbe3"),
                    |h| h.forge.name(),
                )
            },
            |k| k.name(),
        );
        self.issue_work.requests += 1;
        let request = self.issue_work.requests;
        let filter = self
            .issue_work
            .picker
            .take()
            .map(|p| p.filter)
            .unwrap_or_default();
        let mut picker = picker::Picker::new(label, filter);
        picker.source = source;
        self.issue_work.picker = Some(picker);
        let path = self.path.clone();
        let connection = crate::connections::current();
        self.then(
            ctx,
            move || match source {
                Some(kind) => crate::trackers::assigned(kind),
                None => hosting
                    .ok_or_else(|| {
                        crate::i18n::text("repo-workflows-issue_work-open-issue-picker-2a41bd2f")
                            .to_owned()
                    })
                    .and_then(|h| h.forge.provider().assigned_issues(&path, &h)),
            },
            move |tab, result, _| {
                if tab.issue_work.requests == request
                    && connection == crate::connections::current()
                    && let Some(picker) = &mut tab.issue_work.picker
                {
                    picker.listed(result);
                }
            },
        );
    }

    fn start_issue(&mut self, issue: &Issue, branch: &str, worktree: bool, ctx: &egui::Context) {
        if issue
            .tracker
            .as_ref()
            .is_some_and(|i| !i.selected(&crate::connections::current()))
        {
            self.status = Some(crate::repo::Notice::plain((
                "Tracker account changed; reload the issue source".into(),
                true,
            )));
            return;
        }

        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                format!("Start work on {}: wait for {busy}", issue.key()),
                true,
            )));
            return;
        }
        let plan = self.issue_plan(issue.key(), branch, worktree);
        let start = match plan.start {
            Ok(start) => start,
            Err(why) => {
                self.status = Some(crate::repo::Notice::plain((
                    format!("Start work on {}: {why}", issue.key()),
                    true,
                )));
                return;
            }
        };
        let label = format!("Start work on {}", issue.key());
        let ops = link::start_ops(&start, &IssueLink::of(issue));
        if let Some(path) = plan.open {
            self.open_when_done = Some((label.clone(), path));
        }
        self.run_ops(&label, ops, Refresh::All, OpKind::plain(), ctx);
    }

    /// What starting work on issue `number` on `branch` does, here or in a new worktree (when
    /// `worktree`), or why it can't. Reads only what the tab holds: the picker asks it every
    /// frame.
    pub(super) fn issue_plan(&self, number: impl ToString, branch: &str, worktree: bool) -> Plan {
        let number = number.to_string();
        let key = if number.bytes().all(|b| b.is_ascii_digit()) {
            format!("#{number}")
        } else {
            number
        };
        let refuse = |why: String| Plan {
            start: Err(why),
            open: None,
            note: String::new(),
            warnings: Vec::new(),
        };
        if let Some(why) = naming::name_problem(branch) {
            return refuse(why);
        }
        if self.refs.head.is_empty() {
            return refuse(
                crate::i18n::text("repo-workflows-issue_work-issue-plan-2bd468f0").into(),
            );
        }
        let base = self.issue_base();
        if branch == base.name {
            return refuse(crate::i18n::message(
                "repo-workflows-issue_work-issue-plan-0dd9c97f",
                &[("branch", (branch).into())],
            ));
        }
        let mut warnings = Vec::new();
        if let Some(other) = self.issue_work.links.get(branch)
            && other.key() != key
        {
            warnings.push(crate::i18n::message(
                "repo-workflows-issue_work-issue-plan-8dbd7c09",
                &[
                    ("branch", (branch).into()),
                    ("other-key-1", (other.key()).clone().into()),
                    ("key", (key).clone().into()),
                ],
            ));
        }
        let exists = self.refs.local.iter().any(|b| b.name == branch);
        let remote_branch = (!exists)
            .then(|| self.remote_branch_named(branch))
            .flatten();
        let (place, open) = self.issue_place(branch, worktree);
        let folder = |path: &Option<PathBuf>| {
            path.as_deref()
                .and_then(Path::file_name)
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let checkout_here = place == Place::Here && self.refs.current.as_deref() != Some(branch);
        // Whether there are any, not how many: this is asked every frame.
        let changes = [
            &self.work.staged,
            &self.work.unstaged,
            &self.work.conflicted,
        ];
        if checkout_here && changes.iter().any(|files| !files.is_empty()) {
            warnings.push(crate::i18n::message(
                "repo-workflows-issue_work-issue-plan-c9fafe01",
                &[("branch", (branch).into())],
            ));
        }
        let where_ = match &place {
            Place::Here => {
                crate::i18n::text("repo-workflows-issue_work-issue-plan-dbeefca3").to_owned()
            }
            Place::NewWorktree(_) => crate::i18n::message(
                "repo-workflows-issue_work-issue-plan-ae567fc0",
                &[("folder-open-1", (folder(&open)).clone().into())],
            ),
            Place::Worktree => String::new(),
        };
        let (source, fetch, note) = if exists {
            let note = match &place {
                Place::Here if !checkout_here => crate::i18n::message(
                    "repo-workflows-issue_work-issue-plan-952a7458",
                    &[("branch", (branch).into()), ("key", (key).clone().into())],
                ),
                Place::Worktree => crate::i18n::message(
                    "repo-workflows-issue_work-issue-plan-b341ef66",
                    &[
                        ("branch", (branch).into()),
                        ("folder-open-1", (folder(&open)).clone().into()),
                        ("key", (key).clone().into()),
                    ],
                ),
                _ => crate::i18n::message(
                    "repo-workflows-issue_work-issue-plan-cf90f159",
                    &[
                        ("branch", (branch).into()),
                        ("where-", (where_).clone().into()),
                    ],
                ),
            };
            (Source::Exists, None, note)
        } else if let Some(remote) = remote_branch {
            let note = crate::i18n::message(
                "repo-workflows-issue_work-issue-plan-1f8c993f",
                &[
                    ("remote", (remote).clone().into()),
                    ("where-", (where_).clone().into()),
                ],
            );
            let from = remote_start(&remote);
            (Source::Track { from }, None, note)
        } else {
            let (from, fetch, said) = self.issue_start_point(&base);
            let note = crate::i18n::message(
                "repo-workflows-issue_work-issue-plan-38012887",
                &[
                    ("said", (said).clone().into()),
                    ("where-", (where_).clone().into()),
                ],
            );
            (Source::New { from }, fetch, note)
        };
        Plan {
            start: Ok(Start {
                branch: branch.to_owned(),
                source,
                place,
                fetch,
            }),
            open,
            note,
            warnings,
        }
    }

    /// The remote-tracking branch (`origin/42-x`) of the name `branch`: on the default remote,
    /// else another one. The work on the issue was pushed from elsewhere.
    fn remote_branch_named(&self, branch: &str) -> Option<String> {
        let default = self.default_remote();
        let mut found = self
            .refs
            .remotes
            .iter()
            .filter(|(remote, list)| {
                let name = format!("{remote}/{branch}");
                list.iter().any(|b| b.name == name)
            })
            .map(|(remote, _)| remote.as_str());
        let first = found.next()?;
        let remote = std::iter::once(first)
            .chain(found)
            .find(|r| Some(*r) == default.as_deref())
            .unwrap_or(first);
        Some(format!("{remote}/{branch}"))
    }

    /// Where the work on `branch` goes, and the worktree to open once it's started: a worktree
    /// that has the branch already, else a new one beside the repository (when `worktree`), or
    /// this one. A worktree git would prune holds nothing: the steps prune it first.
    fn issue_place(&self, branch: &str, worktree: bool) -> (Place, Option<PathBuf>) {
        let has_it = self
            .worktrees
            .iter()
            .filter(|w| w.prunable.is_none())
            .find(|w| w.branch.as_deref() == Some(branch));
        match has_it {
            Some(w) if w.current => (Place::Here, None),
            Some(w) => (Place::Worktree, Some(w.path.clone())),
            None if worktree => {
                let path = self.free_folder(&self.worktree_folder(branch));
                let there = crate::remote::git_path(&path);
                (Place::NewWorktree(there), Some(path))
            }
            None => (Place::Here, None),
        }
    }

    /// The branch pull requests of the work on an issue go into: the remote's default branch
    /// (where its `HEAD` points), else the usual guess (main, master or develop).
    pub(super) fn issue_base(&self) -> Base {
        let remote = self.default_remote();
        let known = remote
            .as_ref()
            .and_then(|r| self.issue_work.remote_heads.get(r))
            .filter(|b| !b.is_empty() && b.as_str() != "HEAD");
        match known {
            Some(name) => Base {
                name: name.clone(),
                remote,
                known: true,
            },
            None => Base {
                name: self.default_base(),
                remote,
                known: false,
            },
        }
    }

    /// Where a new branch for an issue starts: the default branch as the remote has it (what
    /// the work will be merged into), fetched first; else as it is here; else HEAD. With what
    /// to fetch, and how the picker says it.
    fn issue_start_point(&self, base: &Base) -> (String, Option<(String, String)>, String) {
        let name = &base.name;
        if let Some(remote) = &base.remote {
            let tracking = format!("{remote}/{name}");
            let known = self.refs.remotes.iter().flat_map(|(_, l)| l);
            if known.into_iter().any(|b| b.name == tracking) {
                let said = if base.known {
                    crate::i18n::message(
                        "repo-workflows-issue_work-issue-start-point-ceee14ee",
                        &[("tracking", (tracking).clone().into())],
                    )
                } else {
                    crate::i18n::message(
                        "repo-workflows-issue_work-issue-start-point-212218a3",
                        &[("tracking", (tracking).clone().into())],
                    )
                };
                let fetch = Some((remote.clone(), name.clone()));
                return (remote_start(&tracking), fetch, said);
            }
        }
        if self.refs.local.iter().any(|b| b.name == *name) {
            let said = match &base.remote {
                Some(remote) => crate::i18n::message(
                    "repo-workflows-issue_work-issue-start-point-d844f1e9",
                    &[
                        ("name", (name).clone().into()),
                        ("remote", (remote).clone().into()),
                    ],
                ),
                None => crate::i18n::message(
                    "repo-workflows-issue_work-issue-start-point-497395b3",
                    &[("name", (name).clone().into())],
                ),
            };
            return (format!("refs/heads/{name}"), None, said);
        }
        let said = crate::i18n::message(
            "repo-workflows-issue_work-issue-start-point-3c20246c",
            &[
                (
                    "super-short-self-refs-head-1",
                    (super::short(&self.refs.head)).into(),
                ),
                ("name", (name).clone().into()),
            ],
        );
        (self.refs.head.clone(), None, said)
    }

    /// Reads the issue of `branch` again, in the background, and writes what is kept of it
    /// over what was.
    fn refresh_issue(&mut self, branch: String, ctx: &egui::Context) {
        let hosting = self.hosting.clone();
        let Some(link) = self.issue_work.links.get(&branch).cloned() else {
            return;
        };
        let previous = link.clone();
        let connection = crate::connections::current();
        if self.issue_work.refreshing.is_some() {
            return;
        }
        self.issue_work.refreshing = Some(branch.clone());
        let path = self.path.clone();
        self.then(
            ctx,
            move || match &link.tracker {
                Some(identity) => crate::trackers::one(identity),
                None => hosting
                    .ok_or_else(|| {
                        crate::i18n::text("repo-workflows-issue_work-refresh-issue-0c6fd7a6").into()
                    })
                    .and_then(|h| h.forge.provider().issue(&path, &h, link.number)),
            },
            move |tab, result, ctx| {
                tab.issue_work.refreshing = None;
                if connection != crate::connections::current()
                    || tab.issue_work.links.get(&branch) != Some(&previous)
                {
                    return;
                }
                let label = format!("Read {} again", previous.key());
                match result {
                    Ok(issue) => match &tab.busy_op {
                        Some(busy) => {
                            tab.status = Some(crate::repo::Notice::plain((
                                format!("{label}: wait for {busy}"),
                                true,
                            )));
                        }
                        None => {
                            let ops = link::write_ops(&branch, &IssueLink::of(&issue));
                            tab.run_ops(&label, ops, Refresh::All, OpKind::plain(), ctx);
                        }
                    },
                    Err(e) => {
                        tab.status = Some(crate::repo::Notice::plain((
                            format!("{label} failed: {e}"),
                            true,
                        )))
                    }
                }
            },
        );
    }

    /// Forgets the issue of `branch`: the card goes at once, the config is written in the
    /// background.
    fn unlink_issue(&mut self, branch: String, ctx: &egui::Context) {
        self.issue_work.links.remove(&branch);
        let path = self.path.clone();
        let target = branch.clone();
        self.then(
            ctx,
            move || link::unlink(&path, &target),
            move |tab, result, ctx| {
                if let Err(e) = result {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("Forget the issue of {branch} failed: {e}"),
                        true,
                    )));
                }
                tab.reload(ctx);
            },
        );
    }
}
