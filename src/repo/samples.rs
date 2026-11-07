//! For the tests that drive the window: actions a user takes, made from what a tab shows.

use super::bisect::BisectAct;
use super::file_view::FileTab;
use super::reflog::{ReflogAct, Source};
use super::partial_stash::StashRequest;
use super::timeline::view::TimelineAct;
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
                // What a merge resolved: read in the background, never from the window's thread.
                acts.push(Action::ShowRemerge(true));
                acts.push(Action::LoadMore);
                acts.push(Action::ToggleSearch);
                acts.push(Action::ToggleSearch);
                // The reflog view: HEAD's, an entry shown, a branch's, the scan.
                acts.push(Action::Reflog(ReflogAct::Show(Source::head())));
                acts.push(Action::Reflog(ReflogAct::Select(0)));
                acts.push(Action::Reflog(ReflogAct::Show(Source::branch("main"))));
                acts.push(Action::Reflog(ReflogAct::FindLost));
                acts.push(Action::CloseReflog);
                if let Some(file) = self.work.unstaged.first() {
                    acts.push(Action::OpenDiff(DiffKey::Work(file.clone())));
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
                // What a hard reset to HEAD would change, read in the background.
                let target = self.refs.current.as_deref().unwrap_or("HEAD");
                let head = &self.refs.head;
                acts.push(Action::Dialog(
                    Dialog::confirm("Hard reset", String::new(), "Reset", Vec::new(), None)
                        .preview(
                            crate::git::PreviewAsk::reset(target, head, head, None, true),
                            String::new(),
                        ),
                ));
                acts.push(Action::Dialog(Dialog::add_worktree(
                    &self.path, None, "HEAD",
                )));
                acts.push(Action::Fetch);
                acts.push(Action::Sync);
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
                }
                acts.push(Action::RunRebase);
                acts.push(Action::Stash(StashRequest::everything("sample")));
                // A bisect from start to end, a run in it, and the dialog that starts one.
                acts.push(Action::Dialog(self.start_bisect_dialog(&self.refs.head)));
                acts.push(Action::Bisect(BisectAct::Start {
                    bad: "HEAD".to_owned(),
                    good: Some("HEAD~1".to_owned()),
                }));
                acts.push(Action::Bisect(BisectAct::Run("true".to_owned())));
                acts.push(Action::Bisect(BisectAct::StopRun));
                acts.push(Action::Bisect(BisectAct::Mark {
                    mark: crate::git::Mark::Skip,
                    rev: None,
                }));
                acts.push(Action::Bisect(BisectAct::Reset));
                // The timeline, an entry picked and a jump asked about (not made).
                acts.push(Action::ToggleTimeline);
                acts.push(Action::Timeline(TimelineAct::Select(self.timeline.current)));
                acts.push(Action::Timeline(TimelineAct::AskJump(None)));
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
