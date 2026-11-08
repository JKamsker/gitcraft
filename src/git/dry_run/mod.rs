//! Working out what an operation would do without doing it, for the ghost run (and anything
//! else that wants to know before acting): commits replayed onto another one as a rebase or
//! cherry-pick would, in memory with `git replay` (git 2.44+) or with `git merge-tree` one
//! commit at a time (2.40+); merges with `git merge-tree --write-tree` (2.38+); on an older
//! git, both in a scratch clone in the temporary folder (see [`scratch`]).
//!
//! Only objects are written, which no ref holds and gc clears: no ref, index, working tree,
//! reflog or config of the repository changes, nor does anything of its hooks run. Every git
//! call goes through [`super::run`], so a repository in WSL or over SSH is simulated there.

mod failure;
mod picks;
mod reach;
mod scratch;
mod version;

pub(crate) use failure::Failure;
pub(crate) use picks::replay_with_witness;
pub use picks::{Replay, Replayed, Stop, rebase_picks, replay};
pub(crate) use reach::unreachable_bounded;
pub use reach::{Merge, is_ancestor, leaving, merge, unreachable};
pub(crate) use reach::{
    leaving_with_witness, merge_with_witness, unreachable_bounded_with_witness,
    unreachable_with_witness,
};
pub use version::GitVersion;

/// What a simulation ran on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// `git replay`, all commits in one call.
    Replay,
    /// `git merge-tree --write-tree`, a commit (or a merge) at a time.
    MergeTree,
    /// A scratch clone in the temporary folder, for a git too old for the other two.
    Scratch,
}

impl Engine {
    /// How the ghost run says what it ran on.
    pub fn describe(self) -> &'static str {
        match self {
            Engine::Replay => crate::i18n::text("backend-git-replay-in-memory-a8f3e8"),
            Engine::MergeTree => crate::i18n::text("backend-git-merge-tree-in-memory-565553"),
            Engine::Scratch => {
                crate::i18n::text("backend-a-scratch-clone-in-the-temporary-folder-3c56aa")
            }
        }
    }
}

/// What a simulation that was asked to stop returns: the one asking has gone (the hover ended).
pub const CANCELLED: &str = "cancelled";

/// Options before the subcommand of every git call that writes a commit object: an identity
/// that always works (the copies are only looked at, never kept), no signing (no key prompt,
/// no pinentry window), no hooks, and no gc or maintenance started by the writes.
const WRITE_OPTS: [&str; 12] = [
    "-c",
    "user.name=gitcraft ghost run",
    "-c",
    "user.email=ghost-run@gitcraft.invalid",
    "-c",
    "commit.gpgSign=false",
    "-c",
    "gc.auto=0",
    "-c",
    "maintenance.auto=false",
    "-c",
    "core.hooksPath=.gitcraft-ghost-run-no-hooks",
];

/// [`WRITE_OPTS`] followed by `args`.
fn with_write_opts<'a>(args: &[&'a str]) -> Vec<&'a str> {
    WRITE_OPTS
        .iter()
        .copied()
        .chain(args.iter().copied())
        .collect()
}

/// Whether `s` is a full object name (what the simulation passes on to git, never a name a
/// ref or an option could take).
fn is_oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `repo` is a partial clone (`git clone --filter`), whose missing files a merge in
/// memory would download from the remote: a preview must not do that behind the user's back.
pub fn partial_clone(repo: &std::path::Path) -> bool {
    super::run(Some(repo), &["config", "--get", "extensions.partialClone"])
        .is_ok_and(|s| !s.trim().is_empty())
}

/// Refuses to work in a partial clone (see [`partial_clone`]).
fn not_partial(repo: &std::path::Path) -> Result<(), Failure> {
    if partial_clone(repo) {
        Err(Failure::PartialClone)
    } else {
        Ok(())
    }
}

/// The full object name `rev` resolves to, as a commit.
fn commit_oid(repo: &std::path::Path, rev: &str) -> Result<String, Failure> {
    super::resolve(repo, rev).map_err(Failure::Opaque)
}

/// The tree of the commit `oid`.
fn tree_of(repo: &std::path::Path, oid: &str) -> Result<String, Failure> {
    super::run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{oid}^{{tree}}")],
    )
    .map(|s| s.trim().to_owned())
    .map_err(|_| Failure::NoTree {
        oid: oid.to_owned(),
    })
}

#[cfg(test)]
pub(crate) mod tests;
