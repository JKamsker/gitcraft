//! Working out, in the background, what a ghost run shows: where the refs go, the commits it
//! makes and those it leaves on no ref, the files of the working tree that change, and where
//! it stops on a conflict. Nothing here changes the repository (see [`git::dry_run`]).

#[path = "plan/notice.rs"]
mod notice;
#[path = "plan/pull.rs"]
mod pull;
#[path = "plan/refs.rs"]
mod refs;
#[path = "plan/rewrite.rs"]
mod rewrite;
pub(super) use notice::display_preview_text;
#[cfg(test)]
use notice::diverged_pull;
#[cfg(test)]
use notice::dropped_note;
use std::collections::HashSet;
use std::path::Path;

use super::PreviewFailure;
use super::op::{Facts, GhostOp, Upstream, short};
use crate::git::dry_run::{self, CANCELLED, Engine, GitVersion};
use crate::git::{self, Branch, ChangedFile, Commit};
use crate::repo::pull::PullMode;

/// How many of the commits left on no ref are listed (and marked in the graph).
const LOST_LIMIT: usize = 200;
/// How many commits a rewrite may replay before the preview gives up: each is a git call or
/// two (a round trip each on another machine).
const PICKS_LIMIT: usize = 300;

/// The commit a merge would make, which a ghost run never makes: its stand-in id.
pub(crate) const GHOST_MERGE: &str = "\u{0}ghost-merge";

/// A ref that moves, appears or goes: its full name, and the commits before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefMove {
    pub(crate) full: String,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
}

/// Where the operation would stop on a conflict.
#[derive(Clone, Debug)]
pub(crate) struct Conflict {
    /// Which step stops (from 1), of how many.
    pub(crate) step: usize,
    pub(crate) of: usize,
    /// The commit being applied; `None` for a merge.
    pub(crate) commit: Option<Commit>,
    pub(crate) files: Vec<String>,
}

/// What an operation would do.
#[derive(Clone, Debug, Default)]
pub(crate) struct Outcome {
    pub(crate) moves: Vec<RefMove>,
    /// Commits that would be in the graph and aren't yet, newest first: copies a rebase makes,
    /// a merge's commit, what a pull brings in.
    pub(crate) created: Vec<Commit>,
    /// How many of `created` are copies of commits (or a merge), not fetched ones.
    pub(crate) copies: usize,
    /// The commits listed as lost, newest first: left on no ref (a force push: leaving the
    /// remote's branch).
    pub(crate) lost: Vec<Commit>,
    /// More were lost than are listed.
    pub(crate) lost_more: bool,
    /// `lost` are the commits leaving a remote's branch (a force push), not the graph.
    pub(crate) from_remote: bool,
    /// How many of `lost` are rewritten, not gone: a copy of each is in `created`.
    pub(crate) replaced: usize,
    /// The graph's commits that would be on no ref: drawn as going.
    pub(crate) vanish: HashSet<String>,
    /// The files of the working tree that would change; `None` when not worked out.
    pub(crate) files: Option<Vec<ChangedFile>>,
    pub(crate) conflict: Option<Conflict>,
    /// The tree the branch would have, when commits are replayed or merged.
    pub(crate) tree: Option<String>,
    /// Why git would refuse it outright, as it stands.
    pub(crate) refused: Option<String>,
    /// What to be wary of: work thrown away, a remote that couldn't be asked, a delete that
    /// needs forcing.
    pub(crate) warnings: Vec<String>,
    /// It throws away the uncommitted changes: the changes row goes too.
    pub(crate) discards_work: bool,
    /// The commit it takes out (Drop commit).
    pub(crate) dropped: Option<Commit>,
    pub(crate) notes: Vec<String>,
    /// What the commits were replayed or merged on, when they were.
    pub(crate) engine: Option<Engine>,
}

/// What `op` would do in `repo`, as `facts` describe it. `cancel` is asked between the git
/// calls; `true` ends it with [`CANCELLED`].
#[cfg(test)]
pub(crate) fn plan(
    repo: &Path,
    op: &GhostOp,
    facts: &Facts,
    version: GitVersion,
    cancel: &dyn Fn() -> bool,
) -> Result<Outcome, String> {
    plan_with_witness(repo, op, facts, version, cancel).map_err(|failure| failure.legacy())
}

pub(super) fn plan_with_witness(
    repo: &Path,
    op: &GhostOp,
    facts: &Facts,
    version: GitVersion,
    cancel: &dyn Fn() -> bool,
) -> Result<Outcome, PreviewFailure> {
    let ctx = Ctx {
        repo,
        facts,
        version,
        cancel,
    };
    match op {
        GhostOp::Timeline { state, .. } => ctx.timeline(state),
        GhostOp::Rebase { onto, .. } => ctx.rebase(onto),
        GhostOp::ResetHard { to } => ctx.reset_hard(to),
        GhostOp::Pull(mode) => ctx.pull(*mode),
        GhostOp::ForcePush { branch } => ctx.force_push(branch),
        GhostOp::DeleteBranch { branch, remote } => ctx.delete_branch(branch, *remote),
        GhostOp::DropCommit { hash } => ctx.drop_commit(hash),
    }
}

struct Ctx<'a> {
    repo: &'a Path,
    facts: &'a Facts,
    version: GitVersion,
    cancel: &'a dyn Fn() -> bool,
}

/// A rewrite of the checked-out branch: its commits `picks` replayed onto `onto`, as a rebase
/// of it from `base` does.
struct Rewrite<'a> {
    branch: &'a Branch,
    onto: &'a str,
    base: &'a str,
    picks: Vec<String>,
    /// An interactive rebase (a quick edit): it stops on a commit that becomes empty, where
    /// a plain one drops it.
    interactive: bool,
    /// Other refs that move along (a pull's remote-tracking branch): (full name, from, to).
    along: Vec<(String, String, String)>,
}

impl Ctx<'_> {
    fn check(&self) -> Result<(), PreviewFailure> {
        if (self.cancel)() {
            Err((CANCELLED.to_owned()).into())
        } else {
            Ok(())
        }
    }

    fn rebase(&self, onto: &str) -> Result<Outcome, PreviewFailure> {
        let branch = self.facts.current()?;
        let onto_oid = git::resolve(self.repo, onto)?;
        let head = branch.target.as_str();
        let mut out = Outcome::default();
        // git looks at the working tree first, even when there is nothing to replay.
        if self.facts.dirty {
            if self.config_set("rebase.autoStash") {
                out.notes.push(
                    "Uncommitted changes are stashed first and put back after (rebase.autoStash)."
                        .to_owned(),
                );
            } else {
                out.refused = Some(
                    "the rebase refuses to start with uncommitted changes: commit or stash them first"
                        .to_owned(),
                );
            }
        }
        if dry_run::is_ancestor(self.repo, &onto_oid, head) {
            out.notes
                .push("Already up to date: nothing to replay, nothing changes.".to_owned());
            return Ok(out);
        }
        self.check()?;
        let tip = local_ref(&branch.name);
        let picks = dry_run::rebase_picks(self.repo, onto, &tip)?;
        self.rewrite(
            &Rewrite {
                branch,
                onto: &onto_oid,
                base: onto,
                picks,
                interactive: false,
                along: Vec::new(),
            },
            &mut out,
        )?;
        Ok(out)
    }

    fn timeline(&self, state: &crate::repo::timeline::RefState) -> Result<Outcome, PreviewFailure> {
        let active = || !(self.cancel)();
        let reader = git::investigations::Reader::new(self.repo, &active);
        let now = crate::repo::timeline::RefState::of(&self.facts.refs);
        let mut out = Outcome::default();
        let names: std::collections::BTreeSet<_> =
            now.branches.keys().chain(state.branches.keys()).collect();
        for name in names {
            self.check()?;
            let from = now.branches.get(name).cloned().filter(|s| !s.is_empty());
            let to = state.branches.get(name).cloned().filter(|s| !s.is_empty());
            if to.as_ref().is_some_and(|s| !git::investigations::oid(s)) {
                return Err("Invalid timeline commit ID".to_owned().into());
            }
            out.moves.push(RefMove {
                full: local_ref(name),
                from,
                to,
            });
        }
        if state.branch.is_empty() && !state.head.is_empty() {
            out.moves.push(RefMove {
                full: "HEAD".into(),
                from: Some(now.head.clone()),
                to: Some(state.head.clone()),
            });
        }
        let old: Vec<_> = now
            .branches
            .values()
            .chain(std::iter::once(&now.head))
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        let new: Vec<_> = state
            .branches
            .values()
            .chain(std::iter::once(&state.head))
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        let moved: Vec<_> = out
            .moves
            .iter()
            .map(|m| m.full.clone())
            .chain(std::iter::once("HEAD".into()))
            .collect();
        let hashes = dry_run::unreachable_bounded_with_witness(&reader, &old, &new, &moved, LOST_LIMIT + 1)
            .map_err(|_| "Timeline preview is incomplete: required local history is missing or the bounded read stopped. Hover never downloads objects.".to_owned())?;
        out.lost_more = hashes.len() > LOST_LIMIT;
        let listed = &hashes[..hashes.len().min(LOST_LIMIT)];
        out.lost = reader.commits(listed)?;
        out.vanish = listed.iter().cloned().collect();
        self.check()?;
        if !new.is_empty() {
            out.created = reader.graph_page(&new, git::LOG_PAGE)?;
        }
        if !now.head.is_empty() && !state.head.is_empty() {
            match reader.diff_files(&now.head, &state.head) {
                Ok(files) => out.files = Some(files),
                Err(_) => out.warnings.push("File-change preview is incomplete: required blobs are unavailable locally or the bounded read stopped. No objects were downloaded.".into()),
            }
        }
        if self.facts.dirty {
            out.warnings.push("Uncommitted changes are preserved with --keep; a conflicting checkout will refuse the jump.".into());
        }
        out.notes.push(
            "Saved refs keep earlier states recoverable. This preview moves no refs or files."
                .into(),
        );
        Ok(out)
    }

    fn reset_hard(&self, to: &str) -> Result<Outcome, PreviewFailure> {
        let to = git::resolve(self.repo, to)?;
        let head = self.facts.refs.head.clone();
        let moved = match self.facts.refs.current.as_deref() {
            Some(name) => local_ref(name),
            None => "HEAD".to_owned(),
        };
        let mut out = Outcome {
            moves: vec![RefMove {
                full: moved.clone(),
                from: Some(head.clone()),
                to: Some(to.clone()),
            }],
            ..Outcome::default()
        };
        self.check()?;
        self.lose(
            &mut out,
            &[head],
            std::slice::from_ref(&to),
            &[moved, "HEAD".to_owned()],
        )?;
        self.check()?;
        // From the working tree to `to`: what the reset does to each file.
        out.files = Some(reversed(git::diff_files(self.repo, &to, None)?));
        if self.facts.dirty {
            out.discards_work = true;
            out.warnings.push(
                "Uncommitted changes to these files are thrown away, and can't be brought back."
                    .to_owned(),
            );
        }
        Ok(out)
    }

    /// Lists in `out` the commits left on no ref once `moved` go from `old` to `new`.
    fn lose(
        &self,
        out: &mut Outcome,
        old: &[String],
        new: &[String],
        moved: &[String],
    ) -> Result<(), PreviewFailure> {
        let hashes = dry_run::unreachable_with_witness(self.repo, old, new, moved, LOST_LIMIT + 1)?;
        out.lost_more = hashes.len() > LOST_LIMIT;
        let listed = &hashes[..hashes.len().min(LOST_LIMIT)];
        out.lost = git::commits_of(self.repo, listed)?;
        out.vanish = listed.iter().cloned().collect();
        Ok(())
    }
}

/// The full name of `up`'s remote-tracking branch.
fn tracking_ref(up: &Upstream<'_>) -> String {
    format!("refs/remotes/{}", up.tracking.name)
}

/// `files` as they change going the other way: added ones deleted and the other way round,
/// line counts swapped.
fn reversed(files: Vec<ChangedFile>) -> Vec<ChangedFile> {
    files
        .into_iter()
        .map(|f| {
            let (path, old_path) = match (f.status, f.old_path) {
                ('R', Some(old)) => (old, Some(f.path)),
                (_, old) => (f.path, old),
            };
            ChangedFile {
                status: match f.status {
                    'A' => 'D',
                    'D' => 'A',
                    other => other,
                },
                path,
                old_path,
                added: f.removed,
                removed: f.added,
            }
        })
        .collect()
}

fn local_ref(name: &str) -> String {
    format!("refs/heads/{name}")
}

/// The commit `c` as the strip names it.
pub(crate) fn describe(c: &Commit) -> String {
    format!("{} {}", short(&c.hash), c.summary)
}

#[cfg(test)]
#[path = "plan/fluent_notice_tests.rs"]
mod fluent_notice_tests;
