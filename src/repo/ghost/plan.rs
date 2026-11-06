//! Working out, in the background, what a ghost run shows: where the refs go, the commits it
//! makes and those it leaves on no ref, the files of the working tree that change, and where
//! it stops on a conflict. Nothing here changes the repository (see [`git::simulate`]).

use std::collections::HashSet;
use std::path::Path;

use super::op::{Facts, GhostOp, Upstream, short};
use crate::git::simulate::{self, CANCELLED, Engine, GitVersion};
use crate::git::{self, Branch, ChangedFile, Commit};
use crate::repo::pull::PullMode;

/// How many of the commits left on no ref are listed (and marked in the graph).
const LOST_LIMIT: usize = 200;
/// How many commits a pull brings in are drawn.
const FETCHED_LIMIT: usize = 300;

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
    let ctx = Ctx {
        repo,
        facts,
        version,
        cancel,
    };
    match op {
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
}

impl Ctx<'_> {
    fn check(&self) -> Result<(), String> {
        if (self.cancel)() {
            Err(CANCELLED.to_owned())
        } else {
            Ok(())
        }
    }

    fn rebase(&self, onto: &str) -> Result<Outcome, String> {
        let branch = self.facts.current()?;
        let onto_oid = git::resolve(self.repo, onto)?;
        let head = branch.target.as_str();
        let mut out = Outcome::default();
        if simulate::is_ancestor(self.repo, &onto_oid, head) {
            out.notes
                .push("Already up to date: nothing to replay, nothing changes.".to_owned());
            return Ok(out);
        }
        self.check()?;
        let tip = local_ref(&branch.name);
        let picks = simulate::rebase_picks(self.repo, onto, &tip)?;
        if self.facts.dirty {
            out.refused = Some(
                "git rebase refuses to start with uncommitted changes: commit or stash them first"
                    .to_owned(),
            );
        }
        self.rewrite(
            &Rewrite {
                branch,
                onto: &onto_oid,
                base: onto,
                picks,
            },
            &mut out,
        )?;
        Ok(out)
    }

    fn reset_hard(&self, to: &str) -> Result<Outcome, String> {
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
        out.files = Some(git::diff_files(self.repo, &to, None)?);
        if self.facts.dirty {
            out.notes.push(
                "Uncommitted changes to these files are thrown away, and can't be brought back."
                    .to_owned(),
            );
        }
        Ok(out)
    }

    fn pull(&self, mode: PullMode) -> Result<Outcome, String> {
        let branch = self.facts.current()?;
        let up = self.facts.upstream(branch).ok_or_else(|| {
            format!(
                "{} tracks no branch on a remote: nothing to pull",
                branch.name
            )
        })?;
        let mut out = Outcome::default();
        let new = self.remote_tip(&up, &mut out.notes);
        self.check()?;
        let head = branch.target.clone();
        let tracking = up.tracking.target.clone();
        if new != tracking {
            out.moves.push(RefMove {
                full: format!("refs/remotes/{}", up.tracking.name),
                from: Some(tracking.clone()),
                to: Some(new.clone()),
            });
        }
        let not = |h: &str| format!("^{h}");
        let fetched = git::log(
            self.repo,
            Some(&[new.clone(), not(&head), not(&tracking)]),
            0,
            FETCHED_LIMIT,
        )?;
        out.created = fetched;
        self.check()?;
        if simulate::is_ancestor(self.repo, &new, &head) {
            out.notes
                .push("Already up to date: the branch has everything its remote has.".to_owned());
            return Ok(out);
        }
        if simulate::is_ancestor(self.repo, &head, &new) {
            out.moves.push(RefMove {
                full: local_ref(&branch.name),
                from: Some(head.clone()),
                to: Some(new.clone()),
            });
            out.notes
                .push("Fast-forward: the branch moves to its remote's commit.".to_owned());
            self.check()?;
            out.files = Some(git::diff_files(self.repo, &head, Some(&new))?);
            return Ok(out);
        }
        match mode {
            PullMode::FastForwardOnly => {
                out.refused = Some(format!(
                    "{} and {} have diverged: a fast-forward-only pull refuses; only {} moves",
                    branch.name, up.tracking.name, up.tracking.name
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
                let fork = self.fork_point(&up, &new, &head);
                self.check()?;
                let picks = simulate::rebase_picks(self.repo, &fork, &local_ref(&branch.name))?;
                self.rewrite(
                    &Rewrite {
                        branch,
                        onto: &new,
                        base: &fork,
                        picks,
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
    ) -> Result<Outcome, String> {
        let head = branch.target.as_str();
        let merged = simulate::merge(self.repo, head, new, self.version)?;
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
        out.created.insert(
            0,
            Commit {
                hash: GHOST_MERGE.to_owned(),
                parents: vec![head.to_owned(), new.to_owned()],
                summary: format!("Merge branch '{}' of {}", up.name, up.remote),
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

    /// The commit the upstream's branch is at on the remote, asked without changing any ref:
    /// `ls-remote`, then its objects fetched into nothing (no ref, no FETCH_HEAD, no tags). The
    /// remote-tracking branch as last fetched when the remote can't be asked.
    fn remote_tip(&self, up: &Upstream<'_>, notes: &mut Vec<String>) -> String {
        let known = up.tracking.target.clone();
        let mut as_fetched = |why: String| {
            notes.push(format!(
                "{why}: shown as of the last fetch of {}.",
                up.remote
            ));
            known.clone()
        };
        if !self.version.has_no_write_fetch_head() {
            return as_fetched(format!(
                "git {} can't fetch without writing FETCH_HEAD",
                self.version
            ));
        }
        if up.remote.starts_with('-') {
            return as_fetched(format!("{} isn't a remote's name", up.remote));
        }
        let creds = self.facts.credentials.as_ref();
        let branch_ref = format!("refs/heads/{}", up.name);
        let listed = git::run_unattended(
            Some(self.repo),
            &["ls-remote", "--end-of-options", up.remote, &branch_ref],
            creds,
        );
        let oid = match listed {
            Ok(out) => out
                .lines()
                .find_map(|l| l.split_once('\t').filter(|(_, r)| *r == branch_ref))
                .map(|(oid, _)| oid.trim().to_owned()),
            Err(e) => {
                return as_fetched(format!("Couldn't reach {} ({})", up.remote, first_line(&e)));
            }
        };
        let Some(oid) = oid else {
            return as_fetched(format!("{} has no branch {} any more", up.remote, up.name));
        };
        if oid == known || self.has_commit(&oid) {
            return oid;
        }
        let fetched = git::run_unattended(
            Some(self.repo),
            &[
                "-c",
                "gc.auto=0",
                "-c",
                "maintenance.auto=false",
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-prune",
                "--no-recurse-submodules",
                "--no-write-fetch-head",
                "--refmap=",
                up.remote,
                &branch_ref,
            ],
            creds,
        );
        match fetched {
            Ok(_) if self.has_commit(&oid) => oid,
            Ok(_) => as_fetched(format!("{}'s new commits didn't come", up.remote)),
            Err(e) => as_fetched(format!(
                "Couldn't fetch from {} ({})",
                up.remote,
                first_line(&e)
            )),
        }
    }

    fn has_commit(&self, oid: &str) -> bool {
        git::run(
            Some(self.repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    }

    /// Where `git pull --rebase` replays the branch's commits from: the fork point of HEAD
    /// from the remote-tracking branch's history, else the merge base with `new`.
    fn fork_point(&self, up: &Upstream<'_>, new: &str, head: &str) -> String {
        let tracking = format!("refs/remotes/{}", up.tracking.name);
        git::run(
            Some(self.repo),
            &["merge-base", "--fork-point", &tracking, head],
        )
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| git::merge_base(self.repo, new, head).ok())
        .unwrap_or_else(|| new.to_owned())
    }

    fn force_push(&self, name: &str) -> Result<Outcome, String> {
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
        let leaving = simulate::leaving(self.repo, &old, &new, LOST_LIMIT + 1)?;
        out.lost_more = leaving.len() > LOST_LIMIT;
        out.lost = git::commits_of(self.repo, &leaving[..leaving.len().min(LOST_LIMIT)])?;
        self.check()?;
        out.vanish = simulate::unreachable(self.repo, &[old], &[new], &[tracking], LOST_LIMIT)?
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

    fn delete_branch(&self, name: &str, remote: bool) -> Result<Outcome, String> {
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
        let merged = simulate::is_ancestor(self.repo, &branch.target, head)
            || up.as_ref().is_some_and(|u| {
                simulate::is_ancestor(self.repo, &branch.target, &u.tracking.target)
            });
        if !merged {
            out.notes.push(format!(
                "{name} isn't merged: Delete refuses, and Force (-D) leaves its own commits on no branch."
            ));
        }
        Ok(out)
    }

    fn drop_commit(&self, hash: &str) -> Result<Outcome, String> {
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
            return Err("merges came after the commit, which this would flatten".to_owned());
        }
        let mut out = Outcome::default();
        if self.facts.dirty {
            out.notes.push(
                "Uncommitted changes: the rebase may refuse to start if they touch the same files."
                    .to_owned(),
            );
        }
        self.rewrite(
            &Rewrite {
                branch,
                onto: &parent,
                base: &hash,
                picks,
            },
            &mut out,
        )?;
        Ok(out)
    }

    /// The commits of `w` replayed: the copies drawn, where it stops, the branch moved, what
    /// it leaves behind and the files that change.
    fn rewrite(&self, w: &Rewrite<'_>, out: &mut Outcome) -> Result<(), String> {
        let tip = local_ref(&w.branch.name);
        let replay = simulate::replay(
            self.repo,
            w.onto,
            (w.base, &tip),
            &w.picks,
            self.version,
            self.cancel,
        )?;
        out.engine = Some(replay.engine);
        out.tree = replay.steps.last().map(|s| s.tree.clone());
        self.check()?;
        let originals = git::commits_of(self.repo, &w.picks)?;
        let find = |hash: &str| originals.iter().find(|c| c.hash == hash);
        let mut parent = replay.onto.clone();
        let mut copies = Vec::new();
        for step in &replay.steps {
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
        let dropped = replay.steps.iter().filter(|s| s.copy.is_none()).count();
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
        if let Some(stop) = &replay.stop {
            out.conflict = Some(Conflict {
                step: stop.step + 1,
                of: w.picks.len(),
                commit: find(&stop.commit).cloned(),
                files: stop.files.clone(),
            });
            out.notes.push(
                "The rebase stops there until the conflicts are resolved; the branch doesn't move yet."
                    .to_owned(),
            );
            return Ok(());
        }
        let head = w.branch.target.clone();
        let new_tip = replay.tip().to_owned();
        out.moves.push(RefMove {
            full: tip.clone(),
            from: Some(head.clone()),
            to: Some(new_tip.clone()),
        });
        self.check()?;
        self.lose(
            out,
            std::slice::from_ref(&head),
            std::slice::from_ref(&replay.onto),
            &[tip, "HEAD".to_owned()],
        )?;
        let rewritten: HashSet<&str> = replay
            .steps
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

    /// Lists in `out` the commits left on no ref once `moved` go from `old` to `new`.
    fn lose(
        &self,
        out: &mut Outcome,
        old: &[String],
        new: &[String],
        moved: &[String],
    ) -> Result<(), String> {
        let hashes = simulate::unreachable(self.repo, old, new, moved, LOST_LIMIT + 1)?;
        out.lost_more = hashes.len() > LOST_LIMIT;
        let listed = &hashes[..hashes.len().min(LOST_LIMIT)];
        out.lost = git::commits_of(self.repo, listed)?;
        out.vanish = listed.iter().cloned().collect();
        Ok(())
    }
}

fn local_ref(name: &str) -> String {
    format!("refs/heads/{name}")
}

fn first_line(e: &str) -> &str {
    e.lines().next().unwrap_or(e)
}

/// The commit `c` as the strip names it.
pub(crate) fn describe(c: &Commit) -> String {
    format!("{} {}", short(&c.hash), c.summary)
}
