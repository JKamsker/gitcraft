//! Starting work from an issue assigned to you: the issues listed from the forge, a branch (or a
//! worktree) named after the one picked, its acceptance criteria kept in view on a card while
//! that branch is checked out, and the pull request of the branch saying it closes the issue.

mod card;
pub(crate) mod criteria;
pub(crate) mod link;
pub(crate) mod naming;
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
    pub(super) fn forge_issues_changed(&mut self) {
        self.issue_work.requests += 1;
        self.issue_work.picker = None;
        self.issue_work.refreshing = None;
    }

    fn issue_matches_forge(&self, link: &IssueLink) -> bool {
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
        match act {
            IssueAct::OpenPicker => self.open_issue_picker(ctx),
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
                self.status = Some((format!("Couldn't read the branches' issues: {e}"), true));
            }
            Err(_) => {}
        }
    }

    fn open_issue_picker(&mut self, ctx: &egui::Context) {
        let Some(hosting) = self.hosting.clone() else {
            self.status = Some((
                "Issues come from the forge the repository is on, and its remote is on none \
                 gitcraft knows"
                    .to_owned(),
                true,
            ));
            return;
        };
        self.issue_work.requests += 1;
        let request = self.issue_work.requests;
        // Asked again (Try again): what was typed in the filter stays.
        let filter = self
            .issue_work
            .picker
            .take()
            .map(|p| p.filter)
            .unwrap_or_default();
        self.issue_work.picker = Some(picker::Picker::new(hosting.forge.name(), filter));
        let path = self.path.clone();
        self.forge_then(
            ctx,
            move || hosting.forge.provider().assigned_issues(&path, &hosting),
            move |tab, result, _| {
                if tab.issue_work.requests == request
                    && let Some(picker) = &mut tab.issue_work.picker
                {
                    picker.listed(result);
                }
            },
        );
    }

    fn start_issue(&mut self, issue: &Issue, branch: &str, worktree: bool, ctx: &egui::Context) {
        if let Some(busy) = &self.busy_op {
            self.status = Some((
                format!("Start work on #{}: wait for {busy}", issue.number),
                true,
            ));
            return;
        }
        let plan = self.issue_plan(issue.number, branch, worktree);
        let start = match plan.start {
            Ok(start) => start,
            Err(why) => {
                self.status = Some((format!("Start work on #{}: {why}", issue.number), true));
                return;
            }
        };
        let label = format!("Start work on #{}", issue.number);
        let ops = link::start_ops(&start, &IssueLink::of(issue));
        if let Some(path) = plan.open {
            self.open_when_done = Some((label.clone(), path));
        }
        self.run_ops(&label, ops, Refresh::All, OpKind::plain(), ctx);
    }

    /// What starting work on issue `number` on `branch` does, here or in a new worktree (when
    /// `worktree`), or why it can't. Reads only what the tab holds: the picker asks it every
    /// frame.
    pub(super) fn issue_plan(&self, number: u64, branch: &str, worktree: bool) -> Plan {
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
                "the repository has no commits yet: commit something to start work from".into(),
            );
        }
        let base = self.issue_base();
        if branch == base.name {
            return refuse(format!(
                "{branch} is the default branch: the work on an issue goes on a branch of its own"
            ));
        }
        let mut warnings = Vec::new();
        if let Some(other) = self.issue_work.links.get(branch)
            && other.number != number
        {
            warnings.push(format!(
                "{branch} is for #{} already: starting makes it the branch of #{number} instead",
                other.number
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
            warnings.push(format!(
                "The working tree has changes: they go along to {branch}, unless they'd be \
                 overwritten (then nothing is done)"
            ));
        }
        let where_ = match &place {
            Place::Here => "checked out here".to_owned(),
            Place::NewWorktree(_) => format!("in a new worktree, {}", folder(&open)),
            Place::Worktree => String::new(),
        };
        let (source, fetch, note) = if exists {
            let note = match &place {
                Place::Here if !checkout_here => {
                    format!("{branch} is checked out here already: it's linked to #{number}.")
                }
                Place::Worktree => format!(
                    "{branch} is checked out in {} already: it's linked to #{number}, and that \
                     worktree opens.",
                    folder(&open)
                ),
                _ => format!("{branch} exists already: work goes on on it as it is, {where_}."),
            };
            (Source::Exists, None, note)
        } else if let Some(remote) = remote_branch {
            let note = format!("{remote} is there: a branch tracking it, {where_}.");
            let from = remote_start(&remote);
            (Source::Track { from }, None, note)
        } else {
            let (from, fetch, said) = self.issue_start_point(&base);
            let note = format!("A new branch from {said}, {where_}.");
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
        let remote = self
            .primary_forge_remote()
            .map(str::to_owned)
            .or_else(|| self.default_remote());
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
                    format!("{tracking} (the remote's default branch), fetched first")
                } else {
                    format!("{tracking}, fetched first")
                };
                let fetch = Some((remote.clone(), name.clone()));
                return (remote_start(&tracking), fetch, said);
            }
        }
        if self.refs.local.iter().any(|b| b.name == *name) {
            let said = match &base.remote {
                Some(remote) => format!("{name} as it is here ({remote} has no {name})"),
                None => format!("{name} (there is no remote)"),
            };
            return (format!("refs/heads/{name}"), None, said);
        }
        let said = format!(
            "HEAD ({}): there's no {name} here or on the remote to start from",
            super::short(&self.refs.head)
        );
        (self.refs.head.clone(), None, said)
    }

    /// Reads the issue of `branch` again, in the background, and writes what is kept of it
    /// over what was.
    fn refresh_issue(&mut self, branch: String, ctx: &egui::Context) {
        let Some(hosting) = self.hosting.clone() else {
            self.status = Some((
                "The issue comes from the forge the repository is on, and its remote is on none \
                 gitcraft knows"
                    .to_owned(),
                true,
            ));
            return;
        };
        let Some(number) = self
            .issue_work
            .links
            .get(&branch)
            .filter(|l| self.issue_matches_forge(l))
            .map(|l| l.number)
        else {
            self.status = Some((
                "This issue belongs to another forge repository; open its saved URL to view it"
                    .into(),
                true,
            ));
            return;
        };
        if self.issue_work.refreshing.is_some() {
            return;
        }
        self.issue_work.refreshing = Some(branch.clone());
        let path = self.path.clone();
        #[cfg(test)]
        let recorded = tests::record_refresh(&hosting, number);
        self.forge_then(
            ctx,
            move || {
                #[cfg(test)]
                if let Some(issue) = recorded {
                    return Ok(issue);
                }
                hosting.forge.provider().issue(&path, &hosting, number)
            },
            move |tab, result, ctx| {
                tab.issue_work.refreshing = None;
                let label = format!("Read #{number} again");
                match result {
                    Ok(issue) => match &tab.busy_op {
                        Some(busy) => {
                            tab.status = Some((format!("{label}: wait for {busy}"), true));
                        }
                        None => {
                            let ops = link::write_ops(&branch, &IssueLink::of(&issue));
                            tab.run_ops(&label, ops, Refresh::All, OpKind::plain(), ctx);
                        }
                    },
                    Err(e) => tab.status = Some((format!("{label} failed: {e}"), true)),
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
                    tab.status = Some((format!("Forget the issue of {branch} failed: {e}"), true));
                }
                tab.reload(ctx);
            },
        );
    }
}
