//! What folding the staged changes into a commit decides: why it can't be done, what it
//! rewrites, and the git steps that do it, undo it, abort it and continue it after a conflict.

use crate::git::{Commit, FoldRead, InProgress, RebaseCommit};
use crate::repo::operations::{expect_ref, expect_ref_at, expect_symref};
use crate::repo::rebase::{RebaseEditor, Step};
use crate::repo::{OpKind, Undo, short};

/// A fold as its dialog shows it, read just before.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    /// The commit the staged changes go into.
    pub target: RebaseCommit,
    /// The commits after it up to HEAD, oldest first: rewritten too.
    pub after: Vec<RebaseCommit>,
    /// The target and those after it that a remote-tracking branch already has.
    pub pushed: Vec<String>,
    /// The target is the first commit: the rebase starts from the root.
    pub root: bool,
    /// HEAD when it was read: the fold runs only if it is still there.
    pub head: String,
    pub branch: String,
    /// How many files are staged.
    pub staged: usize,
    /// Commit a `fixup!` commit on top and rewrite nothing now.
    pub keep_fixup: bool,
}

/// Why nothing can be folded right now, whatever the commit.
pub(crate) fn blocked(
    staged: usize,
    branch: Option<&str>,
    in_progress: Option<InProgress>,
) -> Option<String> {
    if let Some(state) = in_progress {
        return Some(format!(
            "A {} is in progress: finish or abort it first",
            state.label().to_lowercase()
        ));
    }
    if branch.is_none() {
        return Some("HEAD is detached: check out a branch first".to_owned());
    }
    (staged == 0).then(|| "Nothing is staged: stage the changes to fold in first".to_owned())
}

/// Why the staged changes can't go into `hash`. `line`: the checked-out branch from HEAD down
/// its first parents.
pub(crate) fn refusal(line: &[&Commit], hash: &str) -> Option<&'static str> {
    let Some(pos) = line.iter().position(|c| c.hash == hash) else {
        return Some("Only commits of the checked-out branch can take the staged changes");
    };
    if line[..pos].iter().any(|c| c.parents.len() > 1) {
        return Some(MERGES_AFTER);
    }
    (line[pos].parents.len() > 1).then_some(MERGE_TARGET)
}

const MERGES_AFTER: &str = "Merges came after this commit: rewriting it would flatten them";
const MERGE_TARGET: &str = "Merge commits can't take the staged changes";

impl Plan {
    /// The fold `read` describes, unless something there stands in its way.
    pub(crate) fn from_read(read: FoldRead) -> Result<Plan, String> {
        if let Some(why) = blocked(read.staged, read.branch.as_deref(), read.in_progress) {
            return Err(why);
        }
        if !read.ancestor {
            return Err(format!(
                "{} is not on the checked-out branch (not an ancestor of HEAD)",
                short(&read.target)
            ));
        }
        // Merges first: they bring in commits of other branches, listed before the target.
        let mut commits = read.commits;
        if commits.iter().any(|c| c.merge && c.hash == read.target) {
            return Err(MERGE_TARGET.to_owned());
        }
        if commits.iter().any(|c| c.merge) {
            return Err(MERGES_AFTER.to_owned());
        }
        if commits.first().is_none_or(|c| c.hash != read.target) {
            return Err(
                "only commits of the checked-out branch can take the staged changes".to_owned(),
            );
        }
        let after = commits.split_off(1);
        let Some(target) = commits.pop() else {
            return Err("the commit could not be read".to_owned());
        };
        let pushed = std::iter::once(&target)
            .chain(&after)
            .filter(|c| !read.unpushed.contains(&c.hash))
            .map(|c| c.hash.clone())
            .collect();
        Ok(Plan {
            target,
            after,
            pushed,
            root: read.root,
            head: read.head,
            branch: read.branch.unwrap_or_default(),
            staged: read.staged,
            keep_fixup: false,
        })
    }

    /// What the operation is called, in the status line and the undo history.
    pub(crate) fn label(&self) -> String {
        if self.keep_fixup {
            format!("Commit fixup! for {}", short(&self.target.hash))
        } else {
            format!("Fold staged changes into {}", short(&self.target.hash))
        }
    }

    /// Whether going ahead rewrites commits a remote already has.
    pub(crate) fn rewrites_pushed(&self) -> bool {
        !self.keep_fixup && !self.pushed.is_empty()
    }

    fn branch_ref(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }

    /// Steps that refuse the fold unless the branch read is still checked out, at the commit
    /// read: a commit made since (in the terminal) would be rewritten without being shown.
    fn guards(&self) -> [Vec<String>; 2] {
        [
            expect_symref("HEAD", &self.branch_ref()),
            expect_ref_at("HEAD", &self.head),
        ]
    }

    /// The staged changes as a commit `fixup! <target's summary>` on top (with the hooks and
    /// signing a commit has).
    fn fixup_commit(&self) -> Vec<String> {
        vec!["commit".to_owned(), format!("--fixup={}", self.target.hash)]
    }

    /// Keep as fixup: the commit only, undone like a commit (its changes staged again).
    pub(crate) fn keep_steps(&self) -> (Vec<Vec<String>>, Undo) {
        let steps = [self.guards().to_vec(), vec![self.fixup_commit()]].concat();
        (steps, Undo::HeadMove { mode: "--soft" })
    }

    /// The interactive rebase that melds the fixup commit into the target: the target, the
    /// fixup commit right after it, then the commits that came after the target. The fixup
    /// commit is named by the branch, which points at it until the rebase is done; so the todo
    /// list is written before the commit exists, and it is found whatever its message became
    /// (a `prepare-commit-msg` hook may change it, which autosquash's matching wouldn't
    /// survive).
    pub(crate) fn editor(&self) -> Result<RebaseEditor, String> {
        let fixup = RebaseCommit {
            hash: self.branch_ref(),
            summary: format!("fixup! {}", self.target.summary),
            ..RebaseCommit::default()
        };
        let commits = [vec![self.target.clone(), fixup], self.after.clone()].concat();
        let onto = if self.root {
            String::new()
        } else {
            format!("{}^", self.target.hash)
        };
        let mut editor = RebaseEditor::new(onto, short(&self.target.hash).to_owned(), commits);
        if let Some(entry) = editor.entries.get_mut(1) {
            entry.step = Step::Fixup;
        }
        match editor.problem() {
            Some(problem) => Err(problem),
            None => Ok(editor),
        }
    }

    /// Fold now: the commit, then `rebase` (the command [`Self::editor`] gives), undone with a
    /// soft reset, which puts the branch back and leaves the changes staged.
    pub(crate) fn fold_steps(&self, rebase: Vec<String>) -> (Vec<Vec<String>>, Undo) {
        let steps = [self.guards().to_vec(), vec![self.fixup_commit(), rebase]].concat();
        (steps, Undo::HeadMove { mode: "--soft" })
    }

    /// The fold stopped on a conflict, to finish or abort.
    pub(crate) fn stopped(&self) -> Stopped {
        Stopped {
            label: self.label(),
            before: self.head.clone(),
            branch: self.branch.clone(),
            read: None,
        }
    }
}

/// A fold whose rebase stopped (on a conflict): aborting it goes back to before the fold,
/// continuing it records the whole fold as one undo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stopped {
    pub label: String,
    /// HEAD before the fold.
    pub before: String,
    pub branch: String,
    /// The repository read after it stopped (see `RepoTab::reads`), once one started: a status
    /// from that read on tells whether the rebase is still going on.
    pub read: Option<u64>,
}

impl Stopped {
    /// Whether a status from the read numbered `read`, with `state` in progress, says the fold
    /// is no longer stopped (done or aborted elsewhere, or it never got as far as the rebase).
    pub(crate) fn over(&self, read: u64, state: Option<InProgress>) -> bool {
        self.read.is_some_and(|r| read >= r) && state != Some(InProgress::Rebase)
    }

    /// Aborting: the rebase, then the fixup commit it goes back to, its changes staged again.
    /// Only while the branch is still one commit (the fixup) past where the fold started.
    pub(crate) fn abort_steps(&self) -> Vec<Vec<String>> {
        vec![
            expect_ref_at(&format!("refs/heads/{}~1", self.branch), &self.before),
            vec!["rebase".to_owned(), "--abort".to_owned()],
            soft_reset(&self.before),
        ]
    }

    /// Continuing: done, it is undone as the whole fold, back to before it with the changes
    /// staged. Redo goes forward to where it ended (`ORIG_HEAD`, which the undo's reset left
    /// there, checked to still be so).
    pub(in crate::repo) fn continue_kind(&self) -> OpKind {
        let on_branch = expect_symref("HEAD", &format!("refs/heads/{}", self.branch));
        let undo = vec![
            on_branch.clone(),
            expect_ref("HEAD"),
            soft_reset(&self.before),
        ];
        let redo = vec![
            on_branch,
            expect_ref("HEAD"),
            expect_ref("ORIG_HEAD"),
            soft_reset("ORIG_HEAD"),
        ];
        OpKind::Normal(Undo::Custom { undo, redo }, self.before.clone())
    }
}

fn soft_reset(to: &str) -> Vec<String> {
    ["reset", "--soft", to].map(str::to_owned).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.to_owned(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            ..Commit::default()
        }
    }

    fn rebase_commit(hash: &str, merge: bool) -> RebaseCommit {
        RebaseCommit {
            hash: hash.to_owned(),
            summary: format!("summary {hash}"),
            merge,
            ..RebaseCommit::default()
        }
    }

    fn read(target: &str, hashes: &[&str]) -> FoldRead {
        FoldRead {
            commits: hashes.iter().map(|h| rebase_commit(h, false)).collect(),
            target: target.to_owned(),
            ancestor: true,
            head: hashes.last().copied().unwrap_or_default().to_owned(),
            branch: Some("main".to_owned()),
            unpushed: hashes.iter().map(|h| (*h).to_owned()).collect(),
            staged: 1,
            ..FoldRead::default()
        }
    }

    #[test]
    fn nothing_folds_without_staged_changes_a_branch_or_while_something_is_going_on() {
        assert!(blocked(1, Some("main"), None).is_none());
        let none_staged = blocked(0, Some("main"), None);
        assert!(none_staged.is_some_and(|w| w.contains("Nothing is staged")));
        assert!(blocked(1, None, None).is_some_and(|w| w.contains("detached")));
        let rebasing = blocked(1, Some("main"), Some(InProgress::Rebase));
        assert!(rebasing.is_some_and(|w| w.contains("rebase is in progress")));
        let merging = blocked(1, Some("main"), Some(InProgress::Merge));
        assert!(merging.is_some_and(|w| w.contains("merge is in progress")));
    }

    #[test]
    fn what_the_menu_allows() {
        // HEAD e, a merge d of x into c, then c, b and the root a.
        let commits = [
            commit("e", &["d"]),
            commit("d", &["c", "x"]),
            commit("x", &["b"]),
            commit("c", &["b"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ];
        let at = |h: &str| commits.iter().find(|c| c.hash == h);
        let line: Vec<&Commit> = ["e", "d", "c", "b", "a"]
            .iter()
            .filter_map(|h| at(h))
            .collect();
        assert_eq!(refusal(&line, "e"), None);
        assert_eq!(refusal(&line, "d"), Some(MERGE_TARGET));
        assert_eq!(refusal(&line, "c"), Some(MERGES_AFTER));
        assert!(
            refusal(&line, "x").is_some(),
            "not on the first-parent line"
        );
        let linear = &line[2..];
        assert_eq!(refusal(linear, "a"), None, "the root commit");
        assert_eq!(refusal(linear, "b"), None);
    }

    #[test]
    fn a_plan_from_what_was_read() {
        let mut found = read("b", &["b", "c", "d"]);
        found.unpushed.remove("b");
        let plan = Plan::from_read(found);
        assert!(plan.is_ok(), "{plan:?}");
        if let Ok(plan) = plan {
            assert_eq!(plan.target.hash, "b");
            let after: Vec<&str> = plan.after.iter().map(|c| c.hash.as_str()).collect();
            assert_eq!(after, ["c", "d"]);
            assert_eq!(plan.pushed, ["b"]);
            assert!(plan.rewrites_pushed());
            assert_eq!(plan.head, "d");
            let kept = Plan {
                keep_fixup: true,
                ..plan
            };
            assert!(
                !kept.rewrites_pushed(),
                "a fixup commit on top rewrites nothing"
            );
        }
    }

    #[test]
    fn plans_that_cant_be() {
        let refused = |r: FoldRead| Plan::from_read(r).err().unwrap_or_default();
        let mut not_ancestor = read("x", &[]);
        not_ancestor.ancestor = false;
        assert!(refused(not_ancestor).contains("not an ancestor of HEAD"));
        let mut detached = read("b", &["b", "c"]);
        detached.branch = None;
        assert!(refused(detached).contains("detached"));
        let mut nothing = read("b", &["b", "c"]);
        nothing.staged = 0;
        assert!(refused(nothing).contains("Nothing is staged"));
        let mut rebasing = read("b", &["b", "c"]);
        rebasing.in_progress = Some(InProgress::Rebase);
        assert!(refused(rebasing).contains("in progress"));
        let mut merged = read("b", &["b", "c", "d"]);
        merged.commits[1].merge = true;
        assert_eq!(refused(merged), MERGES_AFTER);
        let mut merge = read("b", &["b", "c"]);
        merge.commits[0].merge = true;
        assert_eq!(refused(merge), MERGE_TARGET);
        // The commits read don't start at the target.
        assert!(!refused(read("b", &["a", "b"])).is_empty());
    }

    /// The plan for `read(target, hashes)`, which these tests make valid.
    fn plan(target: &str, hashes: &[&str]) -> Plan {
        let made = Plan::from_read(read(target, hashes));
        assert!(made.is_ok(), "{made:?}");
        made.unwrap_or_else(|_| Plan {
            target: RebaseCommit::default(),
            after: Vec::new(),
            pushed: Vec::new(),
            root: false,
            head: String::new(),
            branch: String::new(),
            staged: 0,
            keep_fixup: false,
        })
    }

    #[test]
    fn the_rebase_puts_the_fixup_right_after_the_target() {
        let p = plan("b", &["b", "c", "d"]);
        let editor = p.editor();
        assert!(editor.is_ok());
        if let Ok(editor) = editor {
            assert_eq!(editor.onto, "b^");
            let steps: Vec<(&str, Step)> = editor
                .entries
                .iter()
                .map(|e| (e.commit.hash.as_str(), e.step))
                .collect();
            assert_eq!(
                steps,
                [
                    ("b", Step::Pick),
                    ("refs/heads/main", Step::Fixup),
                    ("c", Step::Pick),
                    ("d", Step::Pick),
                ]
            );
        }
        // The root commit: the rebase takes it in too (`--root`).
        let root = Plan {
            root: true,
            ..plan("a", &["a", "b"])
        };
        assert_eq!(root.editor().map(|e| e.onto), Ok(String::new()));
        // The latest commit: just the two.
        let latest = plan("d", &["d"]).editor().map(|e| e.entries.len());
        assert_eq!(latest, Ok(2));
    }

    #[test]
    fn the_steps_check_the_branch_and_head_first() {
        let p = plan("b", &["b", "c"]);
        let (steps, undo) = p.fold_steps(vec!["rebase".to_owned()]);
        assert_eq!(steps[0], expect_symref("HEAD", "refs/heads/main"));
        assert_eq!(steps[1], expect_ref_at("HEAD", "c"));
        assert_eq!(steps[2], ["commit", "--fixup=b"]);
        assert_eq!(steps[3], ["rebase"]);
        assert_eq!(undo, Undo::HeadMove { mode: "--soft" });
        let kept = Plan {
            keep_fixup: true,
            ..p
        };
        let (steps, _) = kept.keep_steps();
        assert_eq!(steps.len(), 3);
        assert_eq!(kept.label(), "Commit fixup! for b");
    }

    #[test]
    fn a_stopped_fold_is_over_once_a_later_status_has_no_rebase() {
        let mut stopped = plan("b", &["b", "c"]).stopped();
        // No read started since it stopped: nothing to go by yet.
        assert!(!stopped.over(5, None));
        stopped.read = Some(6);
        assert!(!stopped.over(5, None), "a read from before it stopped");
        assert!(!stopped.over(6, Some(InProgress::Rebase)));
        assert!(stopped.over(7, None));
        let mut never = plan("b", &["b", "c"]).stopped();
        never.read = Some(2);
        assert!(never.over(2, None), "the commit failed: no rebase ever ran");
    }

    #[test]
    fn aborting_a_stopped_fold_puts_the_staged_changes_back() {
        let stopped = plan("b", &["b", "c"]).stopped();
        let steps = stopped.abort_steps();
        assert_eq!(steps[0], expect_ref_at("refs/heads/main~1", "c"));
        assert_eq!(steps[1], ["rebase", "--abort"]);
        assert_eq!(steps[2], ["reset", "--soft", "c"]);
        let recorded = match stopped.continue_kind() {
            OpKind::Normal(Undo::Custom { undo, redo }, _) => Some((undo, redo)),
            _ => None,
        };
        assert!(recorded.is_some(), "continuing records the fold");
        if let Some((undo, redo)) = recorded {
            assert_eq!(undo.last(), Some(&soft_reset("c")));
            assert_eq!(redo.last(), Some(&soft_reset("ORIG_HEAD")));
        }
    }
}
