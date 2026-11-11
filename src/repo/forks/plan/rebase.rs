//! Fork rebase plans and exact-tip undo/redo leases.

use super::*;

/// Rebasing the contributor's branch onto its base and pushing it back to them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rebase {
    pub local: String,
    pub fork: String,
    /// The branch in the fork.
    pub branch: String,
    /// `origin/main`: the base as fetched.
    pub onto: String,
    pub head: String,
    /// Where the fork's branch was last fetched, which the push expects to replace.
    pub seen: String,
    pub tracking: String,
    /// The commits the base has that the branch hasn't.
    pub behind: usize,
    /// Captured effective URLs, rechecked before every step and push.
    pub(in crate::repo::forks) destinations: Vec<Destination>,
    pub(in crate::repo::forks) base_head: String,
}

/// Plans rebasing the checked-out branch, set up for `info`'s fork, onto `base` of `upstream`
/// (fetched first, as is the fork's branch). Refused when the fork's branch has commits the
/// branch hasn't (the push would throw them away), or with changes not committed.
pub(in crate::repo::forks) fn plan_rebase(
    repo: &Path,
    info: &ForkInfo,
    upstream: &Remote,
    setup: &Setup,
    creds: Option<&git::Credentials>,
    hosting: Option<&Hosting>,
    scope: Option<&crate::repo::forks::guard::Scope>,
) -> Result<Rebase, String> {
    if let Some(scope) = scope {
        scope.check_remote(repo)?;
    }
    let remotes = git::remotes(repo)?;
    if !remotes.iter().any(|r| r == upstream)
        || hosting.is_some_and(|h| !h.matches_git_remote(&upstream.url))
    {
        return Err("The selected primary remote changed; plan the rebase again".into());
    }
    let base_destination = Destination::read(repo, &upstream.name)?;
    base_destination.validate(hosting, false)?;
    let fork = fork_remote(info, &remotes, hosting).ok_or_else(|| {
        crate::i18n::message(
            "repo-workflows-forks-plan-rebase-b597bd2f",
            &[
                ("info-number-1", (info.number).to_string().into()),
                ("info-owner-2", (info.owner).clone().into()),
            ],
        )
    })?;
    let fork_destination = Destination::read(repo, &fork)?;
    let fork_hosting = hosting.map(|h| Hosting {
        path: info.repo_path.clone(),
        ..h.clone()
    });
    fork_destination.validate(fork_hosting.as_ref(), false)?;
    fork_destination.validate(fork_hosting.as_ref(), true)?;
    let full = git::run_ok_codes(Some(repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])?;
    let local = full
        .trim()
        .strip_prefix("refs/heads/")
        .unwrap_or_default()
        .to_owned();
    // The branch checked out must be the one pushing to this pull request's fork: another
    // fork's branch of the same name is someone else's.
    setup
        .pushing_to(&fork, &info.branch)
        .filter(|b| b.name == local)
        .ok_or_else(|| {
            crate::i18n::message(
                "repo-workflows-forks-plan-rebase-a0b078e0",
                &[
                    ("info-number-1", (info.number).to_string().into()),
                    ("info-owner-2", (info.owner).clone().into()),
                ],
            )
        })?;
    let dirty = git::run(
        Some(repo),
        &["status", "--porcelain", "--untracked-files=no"],
    )?;
    if !dirty.trim().is_empty() {
        return Err(crate::i18n::text("repo-workflows-forks-plan-rebase-e28bbaf9").to_owned());
    }
    let tracking = format!("refs/remotes/{fork}/{}", info.branch);
    let onto = format!("refs/remotes/{}/{}", upstream.name, info.base);
    if let Some(scope) = scope {
        scope.check_remote(repo)?;
    }
    git::run_with(
        Some(repo),
        &[
            "fetch",
            "--",
            &fork_destination.fetch[0],
            &format!("+refs/heads/{}:{tracking}", info.branch),
        ],
        creds,
    )?;
    if let Some(scope) = scope {
        scope.check_remote(repo)?;
    }
    git::run_with(
        Some(repo),
        &[
            "fetch",
            "--",
            &base_destination.fetch[0],
            &format!("+refs/heads/{}:{onto}", info.base),
        ],
        creds,
    )?;
    if let Some(scope) = scope {
        scope.check_remote(repo)?;
    }
    let head = git::head(repo);
    let seen = git::resolve(repo, &tracking)?;
    let missing = count(repo, &format!("{head}..{seen}"))?;
    if missing > 0 {
        return Err(crate::i18n::message(
            "repo-workflows-fork-unpulled-commits",
            &[
                ("fork", (fork.as_str()).into()),
                ("branch", (info.branch.as_str()).into()),
                ("count", (missing).into()),
                ("local", (local.as_str()).into()),
            ],
        ));
    }
    let base_head = git::resolve(repo, &onto)?;
    let behind = count(repo, &format!("{head}..{base_head}"))?;
    if behind == 0 {
        return Err(crate::i18n::message(
            "repo-workflows-forks-plan-rebase-6ccf1bc5",
            &[
                ("local", (local).clone().into()),
                ("upstream", upstream.name.as_str().into()),
                ("info-base-1", (info.base).clone().into()),
            ],
        ));
    }
    Ok(Rebase {
        local,
        fork,
        branch: info.branch.clone(),
        onto: onto.clone(),
        head,
        seen,
        tracking,
        behind,
        base_head,
        destinations: vec![base_destination, fork_destination],
    })
}

pub(super) fn count(repo: &Path, range: &str) -> Result<usize, String> {
    git::run(Some(repo), &["rev-list", "--count", range, "--"])?
        .trim()
        .parse::<usize>()
        .map_err(|e| {
            crate::i18n::message(
                "repo-workflows-forks-count-082131a5",
                &[("e", e.to_string().into())],
            )
        })
}

impl Rebase {
    pub(in crate::repo::forks::plan) fn local_ref(&self) -> String {
        format!("refs/heads/{}", self.local)
    }

    pub(in crate::repo::forks::plan) fn guards(&self) -> Vec<Vec<String>> {
        self.destinations.iter().map(Destination::step).collect()
    }
    pub(in crate::repo::forks::plan) fn rebase(&self) -> Vec<String> {
        strings(&["-c", "rebase.updateRefs=false", "rebase", &self.base_head])
    }

    /// The guarded local stage. Its worker records Rewrite before any push preview.
    pub(in crate::repo::forks) fn local_steps(&self) -> Vec<Vec<String>> {
        let local = self.local_ref();
        self.guards()
            .into_iter()
            .chain([
                expect_symref("HEAD", &local),
                expect_ref_at(&local, &self.head),
                self.rebase(),
            ])
            .collect()
    }

    /// Only successful, current, worker-owned rewrites can enter push review.
    /// Error/cancel/conflict completion keeps local undo without authorizing a send.
    pub(in crate::repo::forks) fn reviewed_tip<'a>(
        &self,
        rewrite: &'a crate::repo::forks::guard::Rewrite,
        succeeded: bool,
        current: bool,
    ) -> Option<&'a str> {
        (succeeded
            && current
            && !rewrite.conflicts
            && rewrite.branch == self.local_ref()
            && rewrite.before == self.head
            && matches!(rewrite.tip.len(), 40 | 64)
            && rewrite.tip.bytes().all(|b| b.is_ascii_hexdigit())
            && rewrite.tip != self.head)
            .then_some(rewrite.tip.as_str())
    }

    /// The separate reviewed unit changes only the fork; local undo is already
    /// retained by the completed local stage, including if review is cancelled.
    pub(in crate::repo::forks) fn push_pending(&self, tip: &str) -> crate::repo::forks::PendingOp {
        let steps = self.push_now(tip);
        crate::repo::forks::PendingOp {
            label: format!("Push {} to {}", self.local, self.fork),
            ops: steps.clone(),
            refresh: crate::repo::forks::Refresh::All,
            kind: crate::repo::forks::OpKind::Normal(
                crate::repo::forks::Undo::Custom {
                    undo: self.fork_undo_steps(),
                    redo: steps,
                },
                String::new(),
            ),
            quiet: false,
        }
    }

    /// The rebase and the push, only while the branch is where the plan saw it.
    pub(crate) fn steps(&self) -> Vec<Vec<String>> {
        let local = self.local_ref();
        self.guards()
            .into_iter()
            .chain([
                expect_symref("HEAD", &local),
                expect_ref_at(&local, &self.head),
                self.rebase(),
                self.push(&local, &self.seen),
            ])
            .collect()
    }

    /// Replace exactly the commit captured for this destination, independent of tracking refs.
    pub(in crate::repo::forks::plan) fn push(&self, what: &str, seen: &str) -> Vec<String> {
        let target = format!("refs/heads/{}", self.branch);
        strings(&[
            "push",
            &format!("--force-with-lease={target}:{seen}"),
            "--",
            &self.fork,
            &format!("{what}:{target}"),
        ])
    }

    /// The push alone, of the branch as it is now (after a rebase continued past conflicts, or
    /// to retry after signing in): replacing the fork's branch only if it is still `seen`.
    pub(crate) fn push_now(&self, tip: &str) -> Vec<Vec<String>> {
        self.guards()
            .into_iter()
            .chain([
                expect_symref("HEAD", &self.local_ref()),
                expect_ref_at(&self.local_ref(), tip),
                self.push(tip, &self.seen),
            ])
            .collect()
    }

    /// The fork's branch back as it was, only while it is what was pushed.
    pub(crate) fn fork_undo_steps(&self) -> Vec<Vec<String>> {
        self.guards()
            .into_iter()
            .chain([
                expect_ref(&self.tracking),
                self.push(&self.head, crate::repo::forks::guard::PUSHED_TIP),
            ])
            .collect()
    }

    /// Back as it was: on the fork first, then here.
    pub(crate) fn undo_steps(&self) -> Vec<Vec<String>> {
        let local = self.local_ref();
        self.guards()
            .into_iter()
            .chain([
                expect_symref("HEAD", &local),
                expect_ref(&local),
                expect_ref(&self.tracking),
                self.push(&self.head, crate::repo::forks::guard::PUSHED_TIP),
                strings(&["reset", "--keep", &self.head]),
            ])
            .collect()
    }

    pub(crate) fn redo_steps(&self) -> Vec<Vec<String>> {
        let local = self.local_ref();
        self.guards()
            .into_iter()
            .chain([
                expect_symref("HEAD", &local),
                expect_ref(&local),
                self.rebase(),
                self.push(&local, &self.head),
            ])
            .collect()
    }

    /// The undo of a rebase whose push failed: the branch back, here only.
    pub(crate) fn local_undo(&self, now: &str) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
        let local = self.local_ref();
        (
            vec![
                expect_symref("HEAD", &local),
                expect_ref_at(&local, now),
                strings(&["reset", "--keep", &self.head]),
            ],
            vec![
                expect_symref("HEAD", &local),
                expect_ref_at(&local, &self.head),
                strings(&["reset", "--keep", now]),
            ],
        )
    }
}
