//! What folding the staged changes into a commit decides: why it can't be done, what it
//! rewrites, and the git steps that do it, undo it, abort it and continue it after a conflict.

use crate::git::{Commit, FoldRead, InProgress, RebaseCommit};
use crate::repo::operations::{expect_ref, expect_ref_at, expect_symref};
use crate::repo::rebase::{RebaseEditor, Step};
use crate::repo::{OpKind, Undo, UndoEntry, short};

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
        return Some(crate::i18n::message(
            "repo-core-2520",
            &[(
                "to-lowercase",
                state.label().to_lowercase().to_string().into(),
            )],
        ));
    }
    if branch.is_none() {
        return Some(crate::i18n::text("repo-core-0842").to_owned());
    }
    (staged == 0).then(|| crate::i18n::text("repo-core-2521").to_owned())
}

/// Why the staged changes can't go into `hash`. `line`: the checked-out branch from HEAD down
/// its first parents.
pub(crate) fn refusal(line: &[&Commit], hash: &str) -> Option<&'static str> {
    let Some(pos) = line.iter().position(|c| c.hash == hash) else {
        return Some(crate::i18n::text(OFF_BRANCH));
    };
    if line[..pos].iter().any(|c| c.parents.len() > 1) {
        return Some(crate::i18n::text(MERGES_AFTER));
    }
    (line[pos].parents.len() > 1).then_some(crate::i18n::text(MERGE_TARGET))
}

const MERGES_AFTER: &str = "repo-core-2522";
const MERGE_TARGET: &str = "repo-core-2523";
const OFF_BRANCH: &str = "repo-core-2524";

impl Plan {
    pub(crate) fn check_ignored(read: FoldRead) -> Result<(), String> {
        if let Some(file) = read.ignored_in_way {
            return Err(crate::i18n::message(
                "repo-core-2526",
                &[("file", file.into())],
            ));
        }
        Ok(())
    }
    /// The fold `read` describes, unless something there stands in its way.
    pub(crate) fn from_read(read: FoldRead) -> Result<Plan, String> {
        if let Some(why) = blocked(read.staged, read.branch.as_deref(), read.in_progress) {
            return Err(why);
        }
        if read.root && read.shallow {
            return Err(crate::i18n::message(
                "repo-core-2525",
                &[("target", short(&read.target).to_string().into())],
            ));
        }
        if let Some(file) = &read.ignored_in_way {
            return Err(crate::i18n::message(
                "repo-core-2526",
                &[("file", file.to_string().into())],
            ));
        }
        if !read.ancestor {
            return Err(crate::i18n::message(
                "repo-core-2527",
                &[("target", short(&read.target).to_string().into())],
            ));
        }
        // Merges first: they bring in commits of other branches, listed before the target.
        let mut commits = read.commits;
        if commits.iter().any(|c| c.merge && c.hash == read.target) {
            return Err(crate::i18n::text(MERGE_TARGET).to_owned());
        }
        if commits.iter().any(|c| c.merge) {
            return Err(crate::i18n::text(MERGES_AFTER).to_owned());
        }
        if commits.first().is_none_or(|c| c.hash != read.target) {
            return Err(crate::i18n::text(OFF_BRANCH).to_owned());
        }
        let after = commits.split_off(1);
        let Some(target) = commits.pop() else {
            return Err(crate::i18n::text("repo-core-2528").to_owned());
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
            format!("Fold into {}", short(&self.target.hash))
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
    fn guards(&self) -> Vec<Vec<String>> {
        let mut guards = vec![
            expect_symref("HEAD", &self.branch_ref()),
            expect_ref_at("HEAD", &self.head),
        ];
        // Nor anything started since that a commit would finish (a merge would get the
        // fixup's message and become a merge commit).
        for pending in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "REBASE_HEAD",
        ] {
            guards.push(expect_ref_at(pending, ""));
        }
        guards
    }

    /// The staged changes as a commit `fixup! <target's summary>` on top (with the hooks and
    /// signing a commit has).
    fn fixup_commit(&self) -> Vec<String> {
        vec!["commit".to_owned(), format!("--fixup={}", self.target.hash)]
    }

    /// Keep as fixup: the commit only, undone like a commit (its changes staged again).
    pub(crate) fn keep_steps(&self) -> (Vec<Vec<String>>, Undo) {
        let steps = [self.guards(), vec![self.fixup_commit()]].concat();
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
        let check = VERIFY.map(str::to_owned).to_vec();
        let steps = [
            self.guards(),
            vec![
                vec![
                    crate::repo::operations::EXPECT_FOLD_SAFE.into(),
                    self.target.hash.clone(),
                ],
                self.fixup_commit(),
                vec![
                    crate::repo::operations::EXPECT_FOLD_IGNORED.into(),
                    self.target.hash.clone(),
                ],
                rebase,
                check,
            ],
        ]
        .concat();
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

/// Checks, once the rebase is done, that the branch ends where the fixup commit did
/// (`ORIG_HEAD`, which the rebase set to it): staged changes that undo what a later commit does
/// meld into nothing, and that later commit then brings its change back, the rebase succeeding
/// all the same. Failing, the fold is taken back ([`Stopped::put_back_steps`]).
const VERIFY: [&str; 6] = [
    "diff",
    "--quiet",
    "--no-ext-diff",
    "ORIG_HEAD",
    "HEAD",
    "--",
];

/// Whether `error` is the check of [`VERIFY`] failing: the trees differ.
pub(crate) fn lost_in_rebase(error: &str) -> bool {
    error.starts_with(&crate::i18n::message(
        "repo-core-2534",
        &[("VERIFY", VERIFY.join(" ").to_string().into())],
    ))
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

    /// Taking back a fold whose result lost some of the staged changes: the branch, the files
    /// and the index back to the fixup commit (`ORIG_HEAD`, the files changed by the rebase put
    /// back, the other changes kept), then HEAD back to before it, the changes staged again.
    pub(crate) fn put_back_steps(&self) -> Vec<Vec<String>> {
        vec![
            expect_symref("HEAD", &format!("refs/heads/{}", self.branch)),
            ["reset", "-q", "--keep", "ORIG_HEAD"]
                .map(str::to_owned)
                .to_vec(),
            soft_reset(&self.before),
        ]
    }

    /// Aborting: the rebase, back to the fixup commit. Only while the branch is still one
    /// commit (the fixup) past where the fold started; [`Self::unfixup_steps`] then stages its
    /// changes again.
    /// `fixup`: the fixup commit, which the branch and `ORIG_HEAD` (set by the rebase as it
    /// started) are still at while the rebase going on is the fold's; another started in its
    /// place since sets `ORIG_HEAD` to its own branch, and is left alone.
    pub(crate) fn abort_steps(&self, fixup: &str) -> Vec<Vec<String>> {
        vec![
            expect_ref_at(&format!("refs/heads/{}~1", self.branch), &self.before),
            expect_ref_at(&format!("refs/heads/{}", self.branch), fixup),
            expect_ref_at("ORIG_HEAD", fixup),
            vec!["rebase".to_owned(), "--abort".to_owned()],
        ]
    }

    /// After the abort, a step of its own so that its checks see what the abort left: the
    /// fixup commit taken off the branch, its changes staged again, only when that branch is
    /// the one checked out and still the fixup commit on top of where the fold started (an
    /// abort of a rebase that replaced the fold's, on another branch, leaves that one alone).
    pub(crate) fn unfixup_steps(&self) -> Vec<Vec<String>> {
        let branch = format!("refs/heads/{}", self.branch);
        vec![
            expect_symref("HEAD", &branch),
            expect_ref_at(&format!("{branch}~1"), &self.before),
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

impl Stopped {
    /// The undo of the fixup commit `fixup` a fold left on top without melding it in: back to
    /// before it, the changes staged; redo commits them again as it was.
    pub(in crate::repo) fn leftover_undo(&self, fixup: &str) -> UndoEntry {
        let on_branch = expect_symref("HEAD", &format!("refs/heads/{}", self.branch));
        UndoEntry {
            id: 0,
            agent: None,
            label: format!("{} (its fixup! commit)", self.label),
            undo: vec![on_branch.clone(), soft_reset(&self.before)],
            redo: vec![on_branch, soft_reset(fixup)],
            heads: Some((self.before.clone(), fixup.to_owned())),
        }
    }
}

fn soft_reset(to: &str) -> Vec<String> {
    ["reset", "--soft", to].map(str::to_owned).to_vec()
}

#[cfg(test)]
#[path = "plan/tests.rs"]
mod tests;
