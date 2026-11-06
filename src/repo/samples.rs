//! For the tests that drive the window: actions a user takes, made from what a tab shows.

use super::file_view::FileTab;
use super::{Action, Dialog, DiffKey, DiffView, RepoTab, diff};

impl RepoTab {
    /// Actions of every kind a user takes on this tab as it is now: reading, staging,
    /// dialogs, rewriting, files. What `phase` 1 opened (a diff, the rebase editor), `phase` 2
    /// acts on.
    pub(crate) fn sample_actions(&self, phase: u8) -> Vec<Action> {
        let mut acts = Vec::new();
        match phase {
            1 => {
                acts.push(Action::Reload);
                acts.push(Action::Select(1));
                acts.push(Action::Select(0));
                acts.push(Action::LoadMore);
                acts.push(Action::ToggleSearch);
                acts.push(Action::ToggleSearch);
                if let Some(file) = self.work.unstaged.first() {
                    acts.push(Action::OpenDiff(DiffKey::Work(file.clone())));
                    // Read again as prose (a.txt), both sides read on that machine.
                    acts.push(Action::DiffRich(Some(diff::rich::RichKind::Prose)));
                }
                acts.push(Action::FileView {
                    path: "a.txt".to_owned(),
                    rev: None,
                    tab: FileTab::History,
                });
                acts.push(Action::FileView {
                    path: "a.txt".to_owned(),
                    rev: None,
                    tab: FileTab::Blame,
                });
                acts.push(Action::CloseFileView);
                acts.push(Action::Ignore {
                    pattern: "/ignored.txt".to_owned(),
                    path: "ignored.txt".to_owned(),
                });
                if let Some(tag) = self.refs.tags.first() {
                    acts.push(Action::DeleteTag {
                        name: tag.name.clone(),
                        target: tag.target.clone(),
                    });
                }
                acts.push(Action::Dialog(Dialog::add_worktree(
                    &self.path, None, "HEAD",
                )));
                acts.push(Action::Fetch);
                acts.push(Action::InteractiveRebase {
                    onto: "HEAD~1".to_owned(),
                    label: "HEAD~1".to_owned(),
                });
            }
            _ => {
                // The lines of the diff opened, staged.
                if let Some(DiffView {
                    key: DiffKey::Work(_),
                    data: Some(Ok(data)),
                    ..
                }) = &self.diff
                {
                    acts.push(Action::PickLines {
                        pick: diff::Pick::Stage,
                        lines: diff::hunk_changes(&data.lines, 0),
                    });
                    // After the pick, whose lines it numbers.
                    acts.push(Action::DiffUnfold {
                        gap: 0,
                        how: diff::Unfold::All,
                    });
                }
                // Back from the prose view opened in phase 1, to a line of the diff.
                acts.push(Action::DiffGoToLine {
                    old: None,
                    new: Some(1),
                });
                acts.push(Action::RunRebase);
                acts.push(Action::StashPush("sample".to_owned()));
            }
        }
        acts
    }

    /// Lets go of the dialog shown, if any.
    pub(crate) fn close_dialog(&mut self) {
        self.dialog = None;
    }

    /// Whether the tab has finished reading what was asked (no read or operation running).
    pub(crate) fn settled(&self) -> bool {
        !self.loading && self.busy_op.is_none()
    }
}
