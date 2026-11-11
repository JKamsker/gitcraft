//! Tab adapter: capture repository evidence and supply folder allocation; no planning policy.
use super::{Base, Inputs, Plan, Planner};
use crate::repo::RepoTab;

impl RepoTab {
    fn issue_planner(&self) -> Planner<'_> {
        let primary = self.primary_forge_remote().map(str::to_owned).or_else(|| {
            self.hosting
                .is_none()
                .then(|| self.default_remote())
                .flatten()
        });
        // Use the existing remote-name parser, including remotes containing slashes.
        let primary_branches = primary
            .as_deref()
            .map(|r| self.remote_branches(r).map(|b| b.name.clone()).collect())
            .unwrap_or_default();
        let eligible = self
            .refs
            .remotes
            .iter()
            .filter(|(name, _)| {
                self.hosting.as_ref().is_none_or(|h| {
                    self.remotes
                        .iter()
                        .any(|r| &r.name == name && h.matches_git_remote(&r.url))
                })
            })
            .map(|(name, _)| name.as_str())
            .collect();
        Planner::new(Inputs {
            refs: &self.refs,
            links: &self.issue_work.links,
            remote_heads: &self.issue_work.remote_heads,
            worktrees: &self.worktrees,
            primary_branches,
            primary,
            eligible,
            dirty: [
                &self.work.staged,
                &self.work.unstaged,
                &self.work.conflicted,
            ]
            .iter()
            .any(|files| !files.is_empty()),
        })
    }

    pub(in crate::repo) fn issue_plan(
        &self,
        number: impl ToString,
        branch: &str,
        worktree: bool,
    ) -> Plan {
        self.issue_planner().plan(number, branch, worktree, || {
            self.free_folder(&self.worktree_folder(branch))
        })
    }

    pub(in crate::repo) fn issue_base(&self) -> Base {
        self.issue_planner().base()
    }

    #[cfg(test)]
    pub(in crate::repo::issue_work) fn remote_branch_named(&self, branch: &str) -> Option<String> {
        self.issue_planner().remote_branch_named(branch)
    }
}
