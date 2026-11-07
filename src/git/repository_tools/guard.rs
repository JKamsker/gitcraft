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
        // The shell is Git's installed sh, including on Windows. Fixed source only; arguments
        // remain separate argv elements. Strip inherited redirection on the repository machine.
        let mut guarded = vec![
            "-c",
            "alias.gitcraft-repository-write=!f() { unset GIT_DIR GIT_COMMON_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE GIT_REPLACE_REF_BASE GIT_SHALLOW_FILE GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS; git \"$@\"; }; f",
            "gitcraft-repository-write",
        ];
        guarded.extend(args);
        git::run_watched(Some(repo), &guarded, self.credentials, false, self.control)
    }
}
