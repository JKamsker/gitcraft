//! Working out, in the background, what a ghost run shows: where the refs go, the commits it
//! makes and those it leaves on no ref, the files of the working tree that change, and where
//! it stops on a conflict. Nothing here changes the repository (see [`git::dry_run`]).

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

    fn pull(&self, mode: PullMode) -> Result<Outcome, PreviewFailure> {
        let branch = self.facts.current()?;
        let up = self.facts.upstream(branch).ok_or_else(|| {
            format!(
                "{} tracks no branch on a remote: nothing to pull",
                branch.name
            )
        })?;
        let mut out = Outcome::default();
        // Never the network from a hover: asking the remote can hang, ask for a passphrase or
        // a key's touch, or download a lot, for a pointer passing by. What the last fetch
        // brought (the background fetch keeps it recent) stands for what the pull fetches.
        out.notes.push(format!(
            "As of the last fetch of {}: what was pushed there since isn't shown.",
            up.remote
        ));
        let head = branch.target.clone();
        let tracking = up.tracking.target.clone();
        let new = tracking.clone();
        self.check()?;
        if dry_run::is_ancestor(self.repo, &new, &head) {
            out.notes
                .push("Already up to date: the branch has everything its remote has.".to_owned());
            return Ok(out);
        }
        if dry_run::is_ancestor(self.repo, &head, &new) {
            out.moves.push(RefMove {
                full: local_ref(&branch.name),
                from: Some(head.clone()),
                to: Some(new.clone()),
            });
            out.notes
                .push("Fast-forward: the branch moves to its remote's commit.".to_owned());
            self.check()?;
            let moved = [
                local_ref(&branch.name),
                "HEAD".to_owned(),
                tracking_ref(&up),
            ];
            self.lose(
                &mut out,
                &[head.clone(), tracking.clone()],
                std::slice::from_ref(&new),
                &moved,
            )?;
            out.files = Some(git::diff_files(self.repo, &head, Some(&new))?);
            return Ok(out);
        }
        match mode {
            PullMode::FastForwardOnly => {
                let fetched = if new != tracking {
                    format!(" (only {} moves, with what it fetches)", up.tracking.name)
                } else {
                    String::new()
                };
                out.refused = Some(format!(
                    "{} and {} have diverged, so a fast-forward-only pull refuses{fetched}",
                    branch.name, up.tracking.name
                ));
                Ok(out)
            }
            PullMode::Merge => self.pull_merge(branch, &up, &new, out),
            PullMode::Rebase => {
                if self.facts.dirty {
                    out.notes.push(
                        "Uncommitted changes are stashed first and put back after.".to_owned(),
                    );
                }
                let base = self.pull_rebase_base(&up, &new, &head);
                self.check()?;
                let picks = dry_run::rebase_picks(self.repo, &base, &local_ref(&branch.name))?;
                self.rewrite(
                    &Rewrite {
                        branch,
                        onto: &new,
                        base: &base,
                        picks,
                        interactive: false,
                        along: vec![(tracking_ref(&up), tracking.clone(), new.clone())],
                    },
                    &mut out,
                )?;
                Ok(out)
            }
        }
    }

    /// The merge a diverged pull makes: its commit drawn, or where it conflicts.
    fn pull_merge(
        &self,
        branch: &Branch,
        up: &Upstream<'_>,
        new: &str,
        mut out: Outcome,
    ) -> Result<Outcome, PreviewFailure> {
        let head = branch.target.as_str();
        let merged = dry_run::merge_with_witness(self.repo, head, new, self.version)?;
        out.engine = Some(merged.engine);
        out.tree = Some(merged.tree.clone()).filter(|t| !t.is_empty());
        if !merged.conflicts.is_empty() {
            out.conflict = Some(Conflict {
                step: 1,
                of: 1,
                commit: None,
                files: merged.conflicts,
            });
            return Ok(out);
        }
        let you = git::identity(self.repo);
        out.created.insert(
            0,
            Commit {
                hash: GHOST_MERGE.to_owned(),
                parents: vec![head.to_owned(), new.to_owned()],
                summary: format!("Merge branch '{}' of {}", up.name, up.remote),
                author: you.name,
                email: you.email,
                time: git::now(),
                ..Commit::default()
            },
        );
        out.copies = 1;
        out.moves.push(RefMove {
            full: local_ref(&branch.name),
            from: Some(head.to_owned()),
            to: Some(GHOST_MERGE.to_owned()),
        });
        if merged.engine != Engine::Scratch {
            self.check()?;
            out.files = Some(git::diff_files(self.repo, head, Some(&merged.tree))?);
        }
        Ok(out)
    }

    fn has_commit(&self, oid: &str) -> bool {
        git::run(
            Some(self.repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    }

    /// The upstream `git pull --rebase` rebases from (`git rebase --onto new <it>`), as
    /// git's pull works it out: the fork point of HEAD in the remote-tracking branch's history
    /// (read before the fetch, as git does), unless that is just where HEAD and `new` meet
    /// anyway, or there is none: then `new` itself, whose commits are compared with the
    /// branch's to leave out those already upstream.
    fn pull_rebase_base(&self, up: &Upstream<'_>, new: &str, head: &str) -> String {
        let fork = git::run(
            Some(self.repo),
            &["merge-base", "--fork-point", &tracking_ref(up), head],
        )
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty());
        let Some(fork) = fork else {
            return new.to_owned();
        };
        let meet = git::run(
            Some(self.repo),
            &["merge-base", "--octopus", head, new, &fork],
        )
        .map(|s| s.trim().to_owned());
        if meet.as_deref() == Ok(fork.as_str()) {
            new.to_owned()
        } else {
            fork
        }
    }

    /// Whether the git config `key` is set to anything but false.
    fn config_set(&self, key: &str) -> bool {
        git::run(Some(self.repo), &["config", "--get", key]).is_ok_and(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "" | "false" | "no" | "off" | "0"
            )
        })
    }

    fn force_push(&self, name: &str) -> Result<Outcome, PreviewFailure> {
        let branch = self
            .facts
            .local(name)
            .ok_or_else(|| format!("there is no branch {name} here"))?;
        let up = self
            .facts
            .upstream(branch)
            .ok_or_else(|| format!("{name} tracks no branch on a remote"))?;
        let (old, new) = (up.tracking.target.clone(), branch.target.clone());
        let tracking = format!("refs/remotes/{}", up.tracking.name);
        let mut out = Outcome {
            moves: vec![RefMove {
                full: tracking.clone(),
                from: Some(old.clone()),
                to: Some(new.clone()),
            }],
            from_remote: true,
            ..Outcome::default()
        };
        let leaving = dry_run::leaving_with_witness(self.repo, &old, &new, LOST_LIMIT + 1)?;
        out.lost_more = leaving.len() > LOST_LIMIT;
        out.lost = git::commits_of(self.repo, &leaving[..leaving.len().min(LOST_LIMIT)])?;
        self.check()?;
        out.vanish =
            dry_run::unreachable_with_witness(self.repo, &[old], &[new], &[tracking], LOST_LIMIT)?
                .into_iter()
                .collect();
        if out.lost.is_empty() {
            out.notes.push(format!(
                "{} loses nothing: a plain push would do.",
                up.tracking.name
            ));
        }
        out.notes.push(format!(
            "As of the last fetch of {}: with the lease, the push is refused if someone pushed since.",
            up.remote
        ));
        Ok(out)
    }

    fn delete_branch(&self, name: &str, remote: bool) -> Result<Outcome, PreviewFailure> {
        let branch = self
            .facts
            .local(name)
            .ok_or_else(|| format!("there is no branch {name} here"))?;
        let up = self.facts.upstream(branch);
        let mut out = Outcome {
            moves: vec![RefMove {
                full: local_ref(name),
                from: Some(branch.target.clone()),
                to: None,
            }],
            ..Outcome::default()
        };
        let mut old = vec![branch.target.clone()];
        if remote && let Some(up) = &up {
            out.moves.push(RefMove {
                full: format!("refs/remotes/{}", up.tracking.name),
                from: Some(up.tracking.target.clone()),
                to: None,
            });
            old.push(up.tracking.target.clone());
        }
        let moved: Vec<String> = out.moves.iter().map(|m| m.full.clone()).collect();
        self.lose(&mut out, &old, &[], &moved)?;
        self.check()?;
        let head = self.facts.refs.head.as_str();
        // As `git branch -d` decides: merged into its upstream when it has one, else HEAD.
        let into = up.as_ref().map_or(head, |u| u.tracking.target.as_str());
        let merged = dry_run::is_ancestor(self.repo, &branch.target, into);
        if !merged {
            let what = if up.is_some() {
                "its upstream"
            } else {
                "the checked-out branch"
            };
            out.warnings.push(format!(
                "{name} isn't merged into {what}: Delete refuses unless Force (-D) is ticked, which leaves its own commits on no branch."
            ));
        }
        Ok(out)
    }

    fn drop_commit(&self, hash: &str) -> Result<Outcome, PreviewFailure> {
        let branch = self.facts.current()?;
        let hash = git::resolve(self.repo, hash)?;
        let parent = git::resolve(self.repo, &format!("{hash}^"))
            .map_err(|_| "the first commit has no parent to rebase from".to_owned())?;
        let tip = local_ref(&branch.name);
        let range = format!("{hash}..{tip}");
        let picks: Vec<String> = git::run(
            Some(self.repo),
            &[
                "rev-list",
                "--reverse",
                "--topo-order",
                "--end-of-options",
                &range,
            ],
        )?
        .lines()
        .map(str::to_owned)
        .collect();
        let merges = git::run(
            Some(self.repo),
            &[
                "rev-list",
                "--merges",
                "--count",
                "--end-of-options",
                &range,
            ],
        )?;
        if merges.trim() != "0" {
            return Err(
                ("merges came after the commit, which this would flatten".to_owned()).into(),
            );
        }
        let mut out = Outcome {
            dropped: git::commits_of(self.repo, std::slice::from_ref(&hash))?
                .into_iter()
                .next(),
            ..Outcome::default()
        };
        if self.facts.dirty {
            out.notes
                .push("Uncommitted changes are stashed first and put back after.".to_owned());
        }
        self.rewrite(
            &Rewrite {
                branch,
                onto: &parent,
                base: &hash,
                picks,
                interactive: true,
                along: Vec::new(),
            },
            &mut out,
        )?;
        Ok(out)
    }

    /// The commits of `w` replayed: the copies drawn, where it stops, the branch moved (and
    /// with `rebase.updateRefs` the branches on its commits), what it leaves behind and the
    /// files that change.
    fn rewrite(&self, w: &Rewrite<'_>, out: &mut Outcome) -> Result<(), PreviewFailure> {
        let tip = local_ref(&w.branch.name);
        if w.picks.len() > PICKS_LIMIT {
            return Err((format!(
                "{} commits to replay, too many to preview (at most {PICKS_LIMIT})",
                w.picks.len()
            ))
            .into());
        }
        if self.config_set("rebase.rebaseMerges") && self.has_merges(w.base, &tip)? {
            return Err(
                ("rebase.rebaseMerges is set and merges are among the commits: git keeps them, which the preview can't replay yet"
                    .to_owned()).into(),
            );
        }
        let replay = dry_run::replay_with_witness(
            self.repo,
            w.onto,
            (w.base, &tip),
            &w.picks,
            self.version,
            self.cancel,
        )?;
        out.engine = Some(replay.engine);
        self.check()?;
        let originals = git::commits_of(self.repo, &w.picks)?;
        let find = |hash: &str| originals.iter().find(|c| c.hash == hash);
        // An interactive rebase stops on a commit that becomes empty, asking what to do.
        let emptied = w
            .interactive
            .then(|| replay.steps.iter().position(|s| s.copy.is_none()))
            .flatten();
        let steps = &replay.steps[..emptied.unwrap_or(replay.steps.len())];
        out.tree = steps.last().map(|s| s.tree.clone());
        let mut parent = replay.onto.clone();
        let mut copies = Vec::new();
        for step in steps {
            let (Some(copy), Some(original)) = (&step.copy, find(&step.original)) else {
                continue;
            };
            copies.push(Commit {
                hash: copy.clone(),
                parents: vec![parent.clone()],
                refs: Vec::new(),
                ..original.clone()
            });
            parent.clone_from(copy);
        }
        out.copies = copies.len();
        copies.reverse();
        copies.append(&mut out.created);
        out.created = copies;
        let dropped = steps.iter().filter(|s| s.copy.is_none()).count();
        if dropped > 0 {
            out.notes.push(format!(
                "{dropped} commit{} dropped: {} already there.",
                if dropped == 1 { " is" } else { "s are" },
                if dropped == 1 {
                    "its change is"
                } else {
                    "their changes are"
                }
            ));
        }
        let stop = match (&replay.stop, emptied) {
            (_, Some(i)) => Some((i, replay.steps[i].original.clone(), Vec::new())),
            (Some(stop), None) => Some((stop.step, stop.commit.clone(), stop.files.clone())),
            (None, None) => None,
        };
        if let Some((step, commit, files)) = stop {
            let why = if files.is_empty() {
                "The rebase stops there, asking whether to keep the commit that became empty; the branch doesn't move yet."
            } else {
                "The rebase stops there until the conflicts are resolved; the branch doesn't move yet."
            };
            out.conflict = Some(Conflict {
                step: step + 1,
                of: w.picks.len(),
                commit: find(&commit).cloned(),
                files,
            });
            out.notes.push(why.to_owned());
            let waits = out.created.iter().take(out.copies).next().map_or_else(
                || short(&replay.onto).to_owned(),
                |c| short(&c.hash).to_owned(),
            );
            out.notes.push(format!(
                "Meanwhile HEAD is detached at {waits}, mid-rebase."
            ));
            return Ok(());
        }
        let head = w.branch.target.clone();
        let new_tip = replay.tip().to_owned();
        out.moves.push(RefMove {
            full: tip.clone(),
            from: Some(head.clone()),
            to: Some(new_tip.clone()),
        });
        if self.config_set("rebase.updateRefs") {
            out.moves.extend(self.updated_refs(w, &replay));
        }
        let mut old = vec![head.clone()];
        let mut new = vec![replay.onto.clone()];
        let mut moved = vec!["HEAD".to_owned()];
        for m in &out.moves {
            moved.push(m.full.clone());
            old.extend(m.from.clone());
            new.extend(m.to.clone().filter(|t| t.as_str() != new_tip.as_str()));
        }
        for (full, from, to) in &w.along {
            moved.push(full.clone());
            old.push(from.clone());
            new.push(to.clone());
        }
        // Copies aren't in the repository when made in a scratch clone: their originals'
        // parents (where they go) stand in, as `onto` does for the branch.
        let known: Vec<String> = new.into_iter().filter(|h| self.has_commit(h)).collect();
        old.retain(|h| self.has_commit(h));
        old.dedup();
        self.check()?;
        self.lose(out, &old, &known, &moved)?;
        let rewritten: HashSet<&str> = steps
            .iter()
            .filter(|s| s.copy.is_some())
            .map(|s| s.original.as_str())
            .collect();
        out.replaced = out
            .lost
            .iter()
            .filter(|c| rewritten.contains(c.hash.as_str()))
            .count();
        if replay.engine == Engine::Scratch {
            out.notes.push(format!(
                "The files that change aren't listed: git {} replays in a scratch clone.",
                self.version
            ));
        } else {
            self.check()?;
            out.files = Some(git::diff_files(self.repo, &head, Some(&new_tip))?);
        }
        Ok(())
    }

    /// Whether the commits from `base` to `tip` include a merge.
    fn has_merges(&self, base: &str, tip: &str) -> Result<bool, String> {
        let range = format!("{base}..{tip}");
        let count = git::run(
            Some(self.repo),
            &[
                "rev-list",
                "--merges",
                "--count",
                "--end-of-options",
                &range,
            ],
        )?;
        Ok(count.trim() != "0")
    }

    /// With `rebase.updateRefs`: the other local branches on the rebased commits move to the
    /// copies, as git moves them (one on a dropped commit to the copy before it).
    fn updated_refs(&self, w: &Rewrite<'_>, replay: &dry_run::Replay) -> Vec<RefMove> {
        let mut moves = Vec::new();
        for b in &self.facts.refs.local {
            if b.name == w.branch.name {
                continue;
            }
            let Some(i) = replay.steps.iter().position(|s| s.original == b.target) else {
                continue;
            };
            let to = replay.steps[..=i]
                .iter()
                .rev()
                .find_map(|s| s.copy.clone())
                .unwrap_or_else(|| replay.onto.clone());
            moves.push(RefMove {
                full: local_ref(&b.name),
                from: Some(b.target.clone()),
                to: Some(to),
            });
        }
        moves
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

/// Paint only finite original preview notes/errors; cached outcomes remain raw.
pub(super) fn display_preview_text(raw: &str) -> String {
    match raw {
        "Uncommitted changes are stashed first and put back after (rebase.autoStash)." => {
            return crate::i18n::text("repo-workflows-ghost-rebase-e550da8a").to_owned();
        }
        "the rebase refuses to start with uncommitted changes: commit or stash them first" => {
            return crate::i18n::text("repo-workflows-ghost-rebase-b1f8a147").to_owned();
        }
        "Already up to date: nothing to replay, nothing changes." => {
            return crate::i18n::text("repo-workflows-ghost-rebase-1f963916").to_owned();
        }
        "Invalid timeline commit ID" => {
            return crate::i18n::text("repo-workflows-ghost-timeline-48223518").to_owned();
        }
        "Timeline preview is incomplete: required local history is missing or the bounded read stopped. Hover never downloads objects." =>
        {
            return crate::i18n::text("repo-workflows-ghost-timeline-5a1baa67").to_owned();
        }
        "File-change preview is incomplete: required blobs are unavailable locally or the bounded read stopped. No objects were downloaded." =>
        {
            return crate::i18n::text("repo-workflows-ghost-timeline-18df3f5f").to_owned();
        }
        "Uncommitted changes are preserved with --keep; a conflicting checkout will refuse the jump." =>
        {
            return crate::i18n::text("repo-workflows-ghost-timeline-6f3d9015").to_owned();
        }
        "Saved refs keep earlier states recoverable. This preview moves no refs or files." => {
            return crate::i18n::text("repo-workflows-ghost-timeline-c25867a7").to_owned();
        }
        "Uncommitted changes to these files are thrown away, and can't be brought back." => {
            return crate::i18n::text("repo-workflows-ghost-reset-hard-3126bbde").to_owned();
        }
        "Already up to date: the branch has everything its remote has." => {
            return crate::i18n::text("repo-workflows-ghost-pull-dd489c0a").to_owned();
        }
        "Fast-forward: the branch moves to its remote's commit." => {
            return crate::i18n::text("repo-workflows-ghost-pull-c7917d08").to_owned();
        }
        "Uncommitted changes are stashed first and put back after." => {
            return crate::i18n::text("repo-workflows-ghost-pull-3a19c0c2").to_owned();
        }
        "its upstream" => {
            return crate::i18n::text("repo-workflows-ghost-delete-branch-dde58e5a").to_owned();
        }
        "the checked-out branch" => {
            return crate::i18n::text("repo-workflows-ghost-delete-branch-fadd1b38").to_owned();
        }
        "the first commit has no parent to rebase from" => {
            return crate::i18n::text("repo-workflows-ghost-drop-commit-da2d4ead").to_owned();
        }
        "merges came after the commit, which this would flatten" => {
            return crate::i18n::text("repo-workflows-ghost-drop-commit-181a54e0").to_owned();
        }
        "rebase.rebaseMerges is set and merges are among the commits: git keeps them, which the preview can't replay yet" =>
        {
            return crate::i18n::text("repo-workflows-ghost-rewrite-3d8b966c").to_owned();
        }
        "The rebase stops there, asking whether to keep the commit that became empty; the branch doesn't move yet." =>
        {
            return crate::i18n::text("repo-workflows-ghost-rewrite-3477e4cb").to_owned();
        }
        "The rebase stops there until the conflicts are resolved; the branch doesn't move yet." => {
            return crate::i18n::text("repo-workflows-ghost-rewrite-e69265a2").to_owned();
        }
        _ => {}
    }
    if let Some(rest) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" tracks no branch on a remote: nothing to pull"))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-pull-f53705f4",
            &[("branch-name-1", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("As of the last fetch of ")
        .and_then(|s| s.strip_suffix(": what was pushed there since isn't shown."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-pull-9045dbaa",
            &[("up-remote-1", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("there is no branch ")
        .and_then(|s| s.strip_suffix(" here"))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-a9c01edc",
            &[("name", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" tracks no branch on a remote"))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-f06fee94",
            &[("name", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("")
        .and_then(|s| s.strip_suffix(" loses nothing: a plain push would do."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-c637c04f",
            &[("up-tracking-name-1", value0.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("As of the last fetch of ").and_then(|s| {
        s.strip_suffix(": with the lease, the push is refused if someone pushed since.")
    }) {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-force-push-2d4460fd",
            &[("up-remote-1", value0.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("").and_then(|s| s.strip_suffix(": Delete refuses unless Force (-D) is ticked, which leaves its own commits on no branch."))
        && let Some((value0, value1)) = rest.split_once(" isn't merged into ") {
            let value1 = match value1 { "its upstream" => crate::i18n::text("repo-workflows-ghost-delete-branch-dde58e5a"), "the checked-out branch" => crate::i18n::text("repo-workflows-ghost-delete-branch-fadd1b38"), _ => return raw.to_owned() };
            return crate::i18n::message("repo-workflows-ghost-delete-branch-26d05fbc", &[("name", value0.into()), ("what", value1.into())]);
        }
    if let Some(rest) = raw.strip_prefix("").and_then(|s| s.strip_suffix(")"))
        && let Some((value0, value1)) =
            rest.split_once(" commits to replay, too many to preview (at most ")
    {
        let Ok(value0) = value0.parse::<u64>() else {
            return raw.to_owned();
        };
        let Ok(value1) = value1.parse::<u64>() else {
            return raw.to_owned();
        };
        return crate::i18n::message(
            "repo-workflows-ghost-rewrite-c84070ef",
            &[
                ("w-picks-len-1", value0.into()),
                ("PICKS-LIMIT", value1.into()),
            ],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("Meanwhile HEAD is detached at ")
        .and_then(|s| s.strip_suffix(", mid-rebase."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-rewrite-fb29af2f",
            &[("waits", value0.into())],
        );
    }
    if let Some(rest) = raw
        .strip_prefix("The files that change aren't listed: git ")
        .and_then(|s| s.strip_suffix(" replays in a scratch clone."))
    {
        let value0 = rest;
        return crate::i18n::message(
            "repo-workflows-ghost-rewrite-af60968a",
            &[("self-version-1", value0.into())],
        );
    }
    if let Some((count, rest)) = raw.split_once(' ')
        && let Ok(count) = count.parse::<usize>()
        && ((count == 1 && rest == "commit is dropped: its change is already there.")
            || (count != 1 && rest == "commits are dropped: their changes are already there."))
    {
        return dropped_note(count);
    }
    if let Some((branch, tail)) = raw.split_once(" and ")
        && !branch.chars().any(char::is_whitespace)
    {
        if let Some(tracking) =
            tail.strip_suffix(" have diverged, so a fast-forward-only pull refuses")
            && !tracking.chars().any(char::is_whitespace)
        {
            return diverged_pull(branch, tracking, false);
        }
        if let Some((tracking, fetched)) =
            tail.split_once(" have diverged, so a fast-forward-only pull refuses (only ")
            && !tracking.chars().any(char::is_whitespace)
            && fetched == format!("{tracking} moves, with what it fetches)")
        {
            return diverged_pull(branch, tracking, true);
        }
    }
    if raw == "HEAD is detached: check out a branch first" {
        return crate::i18n::text("repo-workflows-ghost-current-7776bacf").to_owned();
    }
    crate::repo::activity::display_workflow_error(raw)
}
fn diverged_pull(branch: &str, tracking: &str, fetched: bool) -> String {
    crate::i18n::message(
        "repo-workflows-ghost-diverged-pull",
        &[
            ("branch", (branch).into()),
            ("tracking", (tracking).into()),
            ("fetched", (if fetched { "yes" } else { "no" }).into()),
        ],
    )
}

fn dropped_note(count: usize) -> String {
    crate::i18n::message(
        "repo-workflows-ghost-dropped-note",
        &[("count", (count).into())],
    )
}
#[cfg(test)]
mod fluent_notice_tests {
    use super::*;

    #[test]
    fn fluent_partition_ghost_actual_notices_keep_refs_opaque_and_counts_numeric() {
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            let branch = "feature/ß-provider";
            let tracking = "upstream/ß (opaque)";
            let plain = diverged_pull(branch, tracking, false);
            let fetched = diverged_pull(branch, tracking, true);
            assert!(plain.contains(branch) && plain.contains(tracking));
            assert!(fetched.contains(branch) && fetched.contains(tracking));
            assert_ne!(plain, fetched);
            let one = dropped_note(1);
            let many = dropped_note(1234);
            if language == "de" {
                assert!(plain.contains("Fast-Forward"));
                assert_eq!(
                    one,
                    "Ein Commit ausgelassen: Seine Änderung ist bereits vorhanden."
                );
                assert!(many.starts_with("1.234 Commits ausgelassen:"));
            } else {
                assert!(plain.contains("fast-forward-only"));
                assert_eq!(one, "One commit dropped: its change is already there.");
                assert!(many.starts_with("1,234 commits dropped:"));
            }
        }
        crate::i18n::set_language("en-US");
    }
}
