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

use link::{IssueLink, Place, Start};

use super::commands::remote_start;
use super::{OpKind, Refresh, RepoTab};
use crate::forge::Issue;

/// What the tab keeps of the issues: the links of its branches, and the list being picked from.
#[derive(Default)]
pub(crate) struct IssueWork {
    /// The issue each branch was made for, by branch.
    links: HashMap<String, IssueLink>,
    /// The read of the repository (its `history_gen`) the links were read after.
    links_read: Option<u64>,
    reading: bool,
    picker: Option<picker::Picker>,
    /// Counts the lists asked for: one that comes after the picker was closed or asked again
    /// is dropped.
    requests: u64,
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
    /// Forget which issue the branch is for.
    Unlink(String),
}

impl RepoTab {
    pub(super) fn issue_act(&mut self, act: IssueAct, ctx: &egui::Context) {
        match act {
            IssueAct::OpenPicker => self.open_issue_picker(ctx),
            IssueAct::Start {
                issue,
                branch,
                worktree,
            } => self.start_issue(&issue, &branch, worktree, ctx),
            IssueAct::Unlink(branch) => {
                let label = format!("Forget the issue of {branch}");
                self.run_ops(
                    &label,
                    link::unlink_ops(&branch),
                    Refresh::All,
                    OpKind::plain(),
                    ctx,
                );
            }
        }
    }

    /// The title and description a pull request of `branch` starts with, when it was made for
    /// an issue.
    pub(super) fn issue_pull_request_text(&self, branch: &str) -> Option<(String, String)> {
        self.issue_work
            .links
            .get(branch)
            .map(IssueLink::pull_request_text)
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
        self.issue_work.picker = Some(picker::Picker::new(hosting.forge.name()));
        let path = self.path.clone();
        self.then(
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
        if self.refs.head.is_empty() {
            self.status = Some((
                "The repository has no commits yet: commit something to start work on an issue \
                 from"
                    .to_owned(),
                true,
            ));
            return;
        }
        if !super::sidebar::plain_branch_name(branch) {
            self.status = Some((format!("{branch} can't be a branch's name"), true));
            return;
        }
        let exists = self.refs.local.iter().any(|b| b.name == branch);
        let (place, open) = self.issue_place(branch, worktree);
        let start = Start {
            branch: branch.to_owned(),
            from: self.issue_start_point(),
            exists,
            place,
        };
        let label = format!("Start work on #{}", issue.number);
        let ops = link::start_ops(&start, &IssueLink::of(issue));
        if let Some(path) = open {
            self.open_when_done = Some((label.clone(), path));
        }
        self.run_ops(&label, ops, Refresh::All, OpKind::plain(), ctx);
    }

    /// Where the work on `branch` goes, and the worktree to open once it's started: a worktree
    /// that has the branch already, else a new one beside the repository (when `worktree`), or
    /// this one.
    fn issue_place(&self, branch: &str, worktree: bool) -> (Place, Option<std::path::PathBuf>) {
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

    /// Where a new branch for an issue starts: the default branch as the remote has it (what
    /// the work will be merged into), else as it is here, else HEAD.
    fn issue_start_point(&self) -> String {
        let base = self.default_base();
        if let Some(remote) = self.default_remote() {
            let name = format!("{remote}/{base}");
            let known = self.refs.remotes.iter().flat_map(|(_, l)| l);
            if known.into_iter().any(|b| b.name == name) {
                return remote_start(&name);
            }
        }
        if self.refs.local.iter().any(|b| b.name == base) {
            return format!("refs/heads/{base}");
        }
        self.refs.head.clone()
    }

    /// Reads the links again after each read of the repository (a branch made, renamed or
    /// deleted, here or in a terminal), in the background.
    fn refresh_issue_links(&mut self, ctx: &egui::Context) {
        let read = self.history_gen;
        if self.issue_work.reading || self.issue_work.links_read == Some(read) {
            return;
        }
        self.issue_work.reading = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || link::read(&path),
            move |tab, result, _| {
                tab.issue_work.reading = false;
                tab.issue_work.links_read = Some(read);
                match result {
                    Ok(links) => tab.issue_work.links = links,
                    Err(e) => {
                        tab.status =
                            Some((format!("Couldn't read the branches' issues: {e}"), true))
                    }
                }
            },
        );
    }
}
