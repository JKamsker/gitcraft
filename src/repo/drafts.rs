//! What the tab has typed and not used, as kept by [`crate::drafts`]: taken out of the tab to be
//! saved, and put back when the repository is opened again.

use std::path::PathBuf;

use crate::drafts::{CommitDraft, LineDraft, PrText, RebaseDraft, RebaseStep, RepoDrafts};
use crate::git::RebaseCommit;
use crate::pull_request::DiffSide;

use super::RepoTab;
use super::pr_review::{LineTarget, PrDraft};
use super::rebase::{Entry, RebaseEditor, Step};

/// Which drafts a tab has, cheap to tell each frame: one going away (a commit made, a comment
/// sent, a rebase started or cancelled) is saved at once, not a few seconds later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DraftShape {
    commit: bool,
    rebase: bool,
    /// The boxes with text in them, over all pull requests: one sent while another still has
    /// text counts too.
    pull_requests: usize,
}

impl DraftShape {
    #[cfg(test)]
    pub(crate) fn for_test(commit: bool, rebase: bool, pull_requests: usize) -> Self {
        Self {
            commit,
            rebase,
            pull_requests,
        }
    }

    /// Whether something there in `before` is gone now.
    pub(crate) fn lost_since(self, before: Self) -> bool {
        (before.commit && !self.commit)
            || (before.rebase && !self.rebase)
            || self.pull_requests < before.pull_requests
    }
}

impl RepoTab {
    /// The drafts of this view and of its parked worktrees, by repository.
    pub(crate) fn all_drafts(&self) -> Vec<(PathBuf, RepoDrafts)> {
        std::iter::once(self)
            .chain(self.parked.iter().map(|p| &**p))
            .filter(|tab| tab.drafts_loaded)
            .map(|tab| (tab.path.clone(), tab.drafts()))
            .collect()
    }

    /// The shapes of this view's drafts and of its parked worktrees', by repository.
    pub(crate) fn draft_shapes(&self) -> Vec<(PathBuf, DraftShape)> {
        std::iter::once(self)
            .chain(self.parked.iter().map(|p| &**p))
            .filter(|tab| tab.drafts_loaded)
            .map(|tab| (tab.path.clone(), tab.draft_shape()))
            .collect()
    }

    fn draft_shape(&self) -> DraftShape {
        DraftShape {
            commit: !self.commit_summary.trim().is_empty() || !self.commit_body.trim().is_empty(),
            rebase: self.rebase.is_some(),
            pull_requests: self.pr_drafts.values().map(boxes_with_text).sum(),
        }
    }

    /// What this view has typed and not used.
    pub(crate) fn drafts(&self) -> RepoDrafts {
        let commit = CommitDraft {
            summary: self.commit_summary.clone(),
            body: self.commit_body.clone(),
            amend: self.amend,
        };
        let shown = self.pr_view.as_ref().map(|view| view.number);
        let shown_head = self.pr_view.as_ref().and_then(|view| view.head());
        let pull_requests = self
            .pr_drafts
            .iter()
            .filter_map(|(number, draft)| {
                // A line restored and not looked at since keeps the head it came with.
                let head = shown_head
                    .filter(|_| shown == Some(*number))
                    .or(draft.line_head.as_deref());
                pr_text(draft, head).map(|text| (*number, text))
            })
            .collect();
        RepoDrafts {
            commit: if commit.is_empty() {
                CommitDraft::default()
            } else {
                commit
            },
            pull_requests,
            rebase: self.rebase.as_ref().map(rebase_draft),
        }
    }

    /// Puts back the drafts kept for this repository; says why when they couldn't be read.
    pub(super) fn restore_drafts(&mut self) {
        match crate::drafts::load(&self.path) {
            Ok(drafts) => self.put_back(drafts),
            Err(e) => {
                self.status = Some((
                    format!("What you typed here last time couldn't be restored: {e}"),
                    true,
                ));
            }
        }
    }

    /// Puts `drafts` back into this view.
    pub(super) fn put_back(&mut self, drafts: RepoDrafts) {
        self.drafts_loaded = true;
        // "Amend" isn't ticked again: the last commit may be another one by now (one made in
        // a terminal), which a click would then rewrite.
        if !drafts.commit.is_empty() {
            self.commit_summary = drafts.commit.summary;
            self.commit_body = drafts.commit.body;
        }
        for (number, text) in drafts.pull_requests {
            let draft = self.pr_drafts.entry(number).or_default();
            draft.comment = text.comment;
            draft.review = text.review;
            draft.line_text = text.line_text;
            if let Some(line) = text.line {
                draft.target = Some(LineTarget {
                    path: line.path,
                    line: line.line,
                    side: if line.new_side {
                        DiffSide::Right
                    } else {
                        DiffSide::Left
                    },
                });
                draft.line_head = Some(line.head);
            }
        }
        match drafts.rebase.map(rebase_editor) {
            Some(Some(editor)) => {
                self.rebase = Some(editor);
                self.status = Some((
                    "The interactive rebase you were setting up is back: start it, or cancel it"
                        .to_owned(),
                    false,
                ));
            }
            Some(None) => {
                self.status = Some((
                    "The interactive rebase you were setting up was saved by another version of \
                     gitcraft and couldn't be restored"
                        .to_owned(),
                    true,
                ));
            }
            None => {}
        }
    }
}

/// How many of `draft`'s boxes have text in them.
fn boxes_with_text(draft: &PrDraft) -> usize {
    [&draft.comment, &draft.review, &draft.line_text]
        .iter()
        .filter(|text| !text.trim().is_empty())
        .count()
}

/// What `draft` has typed, if anything. `head` is the pull request's latest commit when its
/// page is open: a line picked is kept only with it (it is let go of when the page is left).
fn pr_text(draft: &PrDraft, head: Option<&str>) -> Option<PrText> {
    if boxes_with_text(draft) == 0 {
        return None;
    }
    let line = draft
        .target
        .as_ref()
        .zip(head)
        .filter(|_| !draft.line_text.trim().is_empty())
        .map(|(target, head)| LineDraft {
            path: target.path.clone(),
            line: target.line,
            new_side: target.side == DiffSide::Right,
            head: head.to_owned(),
        });
    Some(PrText {
        comment: draft.comment.clone(),
        review: draft.review.clone(),
        line_text: draft.line_text.clone(),
        line,
    })
}

fn rebase_draft(editor: &RebaseEditor) -> RebaseDraft {
    RebaseDraft {
        onto: editor.onto.clone(),
        onto_label: editor.onto_label.clone(),
        head: editor.head.clone(),
        entries: editor
            .entries
            .iter()
            .map(|entry| RebaseStep {
                hash: entry.commit.hash.clone(),
                author: entry.commit.author.clone(),
                email: entry.commit.email.clone(),
                summary: entry.commit.summary.clone(),
                original: entry.commit.message.clone(),
                merge: entry.commit.merge,
                step: step_name(entry.step).to_owned(),
                message: entry.message.clone(),
            })
            .collect(),
    }
}

/// The editor `draft` was saved from. Its HEAD is the one the commits were read at, so
/// starting it once the branch has moved is refused, as it was before. `None` when a step is
/// one this version doesn't know: guessing it would rebase other than what was set up.
fn rebase_editor(draft: RebaseDraft) -> Option<RebaseEditor> {
    let mut editor = RebaseEditor::new(draft.onto, draft.onto_label, Vec::new());
    editor.head = draft.head;
    editor.entries = draft
        .entries
        .into_iter()
        .map(|saved| {
            Some(Entry {
                step: step_named(&saved.step)?,
                message: saved.message,
                commit: RebaseCommit {
                    hash: saved.hash,
                    author: saved.author,
                    email: saved.email,
                    summary: saved.summary,
                    message: saved.original,
                    merge: saved.merge,
                },
            })
        })
        .collect::<Option<_>>()?;
    Some(editor)
}

fn step_name(step: Step) -> &'static str {
    match step {
        Step::Pick => "pick",
        Step::Reword => "reword",
        Step::Squash => "squash",
        Step::Fixup => "fixup",
        Step::Drop => "drop",
        Step::Edit => "edit",
    }
}

/// The step saved as `name`, if this version knows it.
fn step_named(name: &str) -> Option<Step> {
    Some(match name {
        "pick" => Step::Pick,
        "reword" => Step::Reword,
        "squash" => Step::Squash,
        "fixup" => Step::Fixup,
        "drop" => Step::Drop,
        "edit" => Step::Edit,
        _ => return None,
    })
}
