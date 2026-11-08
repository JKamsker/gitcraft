//! Immutable previews and mutation execution using the shared cancellable Git transport.
use crate::git;
use std::{path::Path, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub identity: String,
    pub refs: String,
    pub config: String,
    pub worktrees: String,
    pub status: String,
    profile: Option<std::path::PathBuf>,
}
pub(super) fn text(repo: &Path, args: &[&str]) -> Result<String, String> {
    String::from_utf8(super::read(repo, args, Vec::new(), 32 << 20)?)
        .map_err(|_| "Repository metadata is not UTF-8".into())
}
impl Snapshot {
    #[cfg(test)]
    pub(crate) fn recording() -> Self {
        Self {
            identity: "/recorded/repository".into(),
            refs: String::new(),
            config: String::new(),
            worktrees: String::new(),
            status: String::new(),
            profile: None,
        }
    }
    pub(crate) fn capture(repo: &Path) -> Result<Self, String> {
        Ok(Self {
            identity: text(repo, &["rev-parse", "--absolute-git-dir"])?,
            refs: text(
                repo,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)",
                ],
            )?,
            config: text(
                repo,
                &["config", "--null", "--list", "--includes", "--show-origin"],
            )?,
            worktrees: text(repo, &["worktree", "list", "--porcelain"])?,
            status: text(
                repo,
                &["status", "--porcelain=v2", "-z", "--untracked-files=no"],
            )?,
            profile: git::profile_config(),
        })
    }
    pub(crate) fn check_publication(&self, repo: &Path) -> Result<(), String> {
        let mut now = Self::capture(repo)?;
        let without_tracking = |s: &str| {
            s.lines()
                .filter(|line| !line.starts_with("refs/remotes/"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut expected = self.clone();
        now.refs = without_tracking(&now.refs);
        expected.refs = without_tracking(&expected.refs);
        if now != expected {
            return Err("Source refs, worktree, repository or routing configuration changed. Prepare publication again.".into());
        }
        Ok(())
    }
    pub(crate) fn check(&self, repo: &Path) -> Result<(), String> {
        if *self != Self::capture(repo)? {
            return Err("Repository, refs, configuration or worktree changed. Read a fresh preview before applying.".into());
        }
        Ok(())
    }
}

pub(crate) struct Runner<'a> {
    pub repo: &'a Path,
    pub control: &'a Arc<git::OpControl>,
    pub credentials: Option<&'a git::Credentials>,
}
impl Runner<'_> {
    pub fn run(&self, args: &[&str]) -> Result<String, String> {
        self.in_repo(self.repo, args)
    }
    pub fn in_repo(&self, repo: &Path, args: &[&str]) -> Result<String, String> {
        if self.control.is_cancelled() {
            return Err(git::CANCELLED.into());
        }
        let (result, record) = git::command::run_watched_extra_recorded(
            Some(repo),
            args,
            None,
            false,
            self.control,
            git::command::Extra::repository(),
        );
        self.control.record(record);
        result
    }
    pub fn endpoint(
        &self,
        repo: &Path,
        url: &str,
        remote: &str,
        before: &[&str],
        after: &[&str],
    ) -> Result<String, String> {
        git::command::repository_endpoint(
            repo,
            url,
            remote,
            before,
            after,
            self.credentials,
            self.control,
        )
    }
}
