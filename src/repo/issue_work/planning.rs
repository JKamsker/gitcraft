//! Issue planning owns decisions over an immutable repository snapshot, without a tab.

use super::remote_start;
use super::{Base, Path, PathBuf, Place, Plan, Source, Start, naming};

mod context;
mod tab;
#[cfg(test)]
mod tests;

use context::StartPoint;

/// Captured evidence only: no mutable tab state, persistence or execution access.
pub(super) struct Inputs<'a> {
    pub refs: &'a crate::git::Refs,
    pub links: &'a std::collections::HashMap<String, super::IssueLink>,
    pub remote_heads: &'a std::collections::HashMap<String, String>,
    pub worktrees: &'a [crate::git::Worktree],
    pub primary: Option<String>,
    pub primary_branches: Vec<String>,
    /// Ordered remotes belonging to the selected forge (all remotes without a forge).
    pub eligible: Vec<&'a str>,
    pub dirty: bool,
}

/// Short-lived planning owner; its captured state cannot be mutated by callers.
pub(super) struct Planner<'a> {
    inputs: Inputs<'a>,
}

impl<'a> Planner<'a> {
    pub(super) fn new(inputs: Inputs<'a>) -> Self {
        Self { inputs }
    }

    /// What starting work on issue `number` on `branch` does, here or in a new worktree (when
    /// `worktree`), or why it can't. The picker asks it every frame. Folder allocation is
    /// an injected capability, called only when a new worktree is actually needed.
    pub(super) fn plan(
        &self,
        number: impl ToString,
        branch: &str,
        worktree: bool,
        folder: impl FnOnce() -> PathBuf,
    ) -> Plan {
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
        if self.inputs.refs.head.is_empty() {
            return refuse(
                crate::i18n::text("repo-workflows-issue_work-issue-plan-2bd468f0").into(),
            );
        }
        let base = self.base();
        if branch == base.name {
            return refuse(crate::i18n::message(
                "repo-workflows-issue_work-issue-plan-0dd9c97f",
                &[("branch", (branch).into())],
            ));
        }
        let mut warnings = Vec::new();
        if let Some(other) = self.inputs.links.get(branch)
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
        let exists = self.inputs.refs.local.iter().any(|b| b.name == branch);
        let remote_branch = (!exists)
            .then(|| self.remote_branch_named(branch))
            .flatten();
        let (place, open) = self.place(branch, worktree, folder);
        let folder = |path: &Option<PathBuf>| {
            path.as_deref()
                .and_then(Path::file_name)
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let checkout_here =
            place == Place::Here && self.inputs.refs.current.as_deref() != Some(branch);
        // Whether there are any, not how many: this is asked every frame.
        if checkout_here && self.inputs.dirty {
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
            let StartPoint { from, fetch, said } = self.start_point(&base);
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
}
