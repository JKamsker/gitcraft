//! Starting work from an issue assigned to you: the issues listed from the forge, a branch (or a
//! worktree) named after the one picked, its acceptance criteria kept in view on a card while
//! that branch is checked out, and the pull request of the branch saying it closes the issue.

mod planning;

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
    /// Independent tracker accounts use their Context/request checks; Git forge reads also
    /// carry the selected forge generation through the existing sender.
    fn issue_then<T: Send + 'static>(
        &self,
        independent: bool,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        let sender = if independent {
            self.tx.clone()
        } else {
            self.forge_sender()
        };
        super::spawn(&sender, ctx, move || {
            let result = work();
            super::Msg::Then(Box::new(move |tab, ctx| then(tab, result, ctx)))
        });
    }
    pub(super) fn forge_issues_changed(&mut self) {
        self.issue_work.requests += 1;
        self.issue_work.picker = None;
        self.issue_work.refreshing = None;
    }

    fn issue_matches_forge(&self, link: &IssueLink) -> bool {
        if link.tracker.is_some() {
            return true;
        }
        let Some(hosting) = &self.hosting else {
            return true;
        };
        let place = link
            .url
            .rsplit_once("/-/issues/")
            .or_else(|| link.url.rsplit_once("/issues/"))
            .map(|(repo, _)| repo);
        place.is_some_and(|repo| hosting.matches_http_repository(repo))
    }

    pub(super) fn issue_act(&mut self, act: IssueAct, ctx: &egui::Context) {
        self.sync_forge_selection(ctx);
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
        if !self.issue_matches_forge(link) {
            return None;
        }
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
        self.issue_then(
            source.is_some(),
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
        if issue.tracker.is_none() && !self.issue_matches_forge(&IssueLink::of(issue)) {
            self.status = Some(crate::repo::Notice::plain((
                "This issue belongs to another primary forge; choose it again".into(),
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

    /// Reads the issue of `branch` again, in the background, and writes what is kept of it
    /// over what was.
    fn refresh_issue(&mut self, branch: String, ctx: &egui::Context) {
        let hosting = self.hosting.clone();
        let Some(link) = self.issue_work.links.get(&branch).cloned() else {
            return;
        };
        if link.tracker.is_none() && !self.issue_matches_forge(&link) {
            self.status = Some(crate::repo::Notice::plain((
                "This issue belongs to another forge repository; open its saved URL to view it"
                    .into(),
                true,
            )));
            return;
        }
        let independent = link.tracker.is_some();
        let previous = link.clone();
        let connection = crate::connections::current();
        if self.issue_work.refreshing.is_some() {
            return;
        }
        self.issue_work.refreshing = Some(branch.clone());
        let path = self.path.clone();
        #[cfg(test)]
        let recorded = hosting
            .as_ref()
            .filter(|_| !independent)
            .and_then(|hosting| tests::record_refresh(hosting, link.number));
        self.issue_then(
            independent,
            ctx,
            move || match &link.tracker {
                Some(identity) => crate::trackers::one(identity),
                None => {
                    let hosting = hosting.ok_or_else(|| {
                        crate::i18n::text("repo-workflows-issue_work-refresh-issue-0c6fd7a6")
                            .to_owned()
                    })?;
                    #[cfg(test)]
                    if let Some(issue) = recorded {
                        return Ok(issue);
                    }
                    hosting.forge.provider().issue(&path, &hosting, link.number)
                }
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
