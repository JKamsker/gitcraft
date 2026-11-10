//! Resolve repository evidence into base, source and placement decisions.
use super::*;

pub(super) struct StartPoint {
    pub from: String,
    pub fetch: Option<(String, String)>,
    pub said: String,
}

impl Planner<'_> {
    /// Prefer the primary repository's tracking branch. A branch of the same name on
    /// another forge is not evidence that this issue's work was pushed from elsewhere.
    pub(super) fn remote_branch_named(&self, branch: &str) -> Option<String> {
        let primary = self.inputs.primary.as_deref();
        let mut found = self
            .inputs
            .refs
            .remotes
            .iter()
            .filter(|(remote, list)| {
                self.inputs.eligible.contains(&remote.as_str())
                    && list.iter().any(|b| b.name == format!("{remote}/{branch}"))
            })
            .map(|(remote, _)| remote.as_str());
        let first = found.next()?;
        let remote = std::iter::once(first)
            .chain(found)
            .find(|r| Some(*r) == primary)
            .unwrap_or(first);
        Some(format!("{remote}/{branch}"))
    }

    /// Where the work on `branch` goes, and the worktree to open once it's started: a worktree
    /// that has the branch already, else a new one beside the repository (when `worktree`), or
    /// this one. A worktree git would prune holds nothing: the steps prune it first.
    pub(super) fn place(
        &self,
        branch: &str,
        worktree: bool,
        folder: impl FnOnce() -> PathBuf,
    ) -> (Place, Option<PathBuf>) {
        let has_it = self
            .inputs
            .worktrees
            .iter()
            .filter(|w| w.prunable.is_none())
            .find(|w| w.branch.as_deref() == Some(branch));
        match has_it {
            Some(w) if w.current => (Place::Here, None),
            Some(w) => (Place::Worktree, Some(w.path.clone())),
            None if worktree => {
                let path = folder();
                let there = crate::remote::git_path(&path);
                (Place::NewWorktree(there), Some(path))
            }
            None => (Place::Here, None),
        }
    }

    /// The branch pull requests of the work on an issue go into: the remote's default branch
    /// (where its `HEAD` points), else the usual guess (main, master or develop).
    pub(super) fn base(&self) -> Base {
        let remote = self.inputs.primary.clone();
        let known = remote
            .as_ref()
            .and_then(|r| self.inputs.remote_heads.get(r))
            .filter(|b| !b.is_empty() && b.as_str() != "HEAD");
        match known {
            Some(name) => Base {
                name: name.clone(),
                remote,
                known: true,
            },
            None => Base {
                name: remote
                    .as_deref()
                    .and_then(|remote| {
                        ["main", "master", "develop"].into_iter().find(|name| {
                            self.inputs
                                .primary_branches
                                .iter()
                                .any(|b| *b == format!("{remote}/{name}"))
                        })
                    })
                    .unwrap_or("main")
                    .to_owned(),
                remote,
                known: false,
            },
        }
    }

    /// Where a new branch for an issue starts: the default branch as the remote has it (what
    /// the work will be merged into), fetched first; else as it is here; else HEAD. With what
    /// to fetch, and how the picker says it.
    pub(super) fn start_point(&self, base: &Base) -> StartPoint {
        let name = &base.name;
        if let Some(remote) = &base.remote {
            let tracking = format!("{remote}/{name}");
            let known = self.inputs.refs.remotes.iter().flat_map(|(_, l)| l);
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
                return StartPoint {
                    from: remote_start(&tracking),
                    fetch,
                    said,
                };
            }
        }
        if self.inputs.refs.local.iter().any(|b| b.name == *name) {
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
            return StartPoint {
                from: format!("refs/heads/{name}"),
                fetch: None,
                said,
            };
        }
        let said = crate::i18n::message(
            "repo-workflows-issue_work-issue-start-point-3c20246c",
            &[
                (
                    "super-short-self-refs-head-1",
                    (crate::repo::short(&self.inputs.refs.head)).into(),
                ),
                ("name", (name).clone().into()),
            ],
        );
        StartPoint {
            from: self.inputs.refs.head.clone(),
            fetch: None,
            said,
        }
    }
}
