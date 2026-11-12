//! What a repository tab's keys (Fetch, Pull, Push, Stash, New branch, Terminal, Stage all,
//! Commit; see [`crate::keymap`]) do, and why they do nothing when they can't be used now.

use super::{Action, Dialog, Refresh, RepoTab, WIP_HASH, ops};
use crate::keymap::Command;
use crate::repo::terminals::TerminalAct;
use crate::settings::Settings;

impl RepoTab {
    pub(crate) fn command_refusal(&self, command: Command) -> Option<String> {
        match command.scope() {
            crate::keymap::Scope::Graph if self.commits.is_empty() => {
                Some(crate::i18n::text("repo-core-2857").into())
            }
            crate::keymap::Scope::Rebase if self.rebase.is_none() => {
                Some(crate::i18n::text("repo-core-2858").into())
            }
            crate::keymap::Scope::Rebase
                if self.rebase.as_ref().is_some_and(|editor| editor.starting) =>
            {
                Some(crate::i18n::text("repo-core-2859").into())
            }
            _ => self.key_action(command).err(),
        }
    }

    /// The action the key of `command` stands for here, or why it can't be done now, as the
    /// toolbar would say it. `None` for the window's keys, which aren't the tab's.
    pub(super) fn key_action(&self, command: Command) -> Result<Option<Action>, String> {
        if let Some(op) = &self.busy_op
            && !matches!(
                command,
                Command::Terminal
                    | Command::Timeline
                    | Command::Search
                    | Command::Back
                    | Command::Forward
                    | Command::NextChange
                    | Command::PreviousChange
                    | Command::NextChangeAlt
                    | Command::PreviousChangeAlt
                    | Command::GraphCopyHash
                    | Command::GraphCopyMessage
                    | Command::GraphCopyBranch
            )
        {
            return Err(crate::i18n::message(
                "repo-core-2860",
                &[("op", crate::repo::messages::display_operation(op).into())],
            ));
        }
        let no_remote = || crate::i18n::text("repo-core-2861").to_owned();
        let action = match command {
            Command::Fetch | Command::Pull | Command::Push if self.remotes.is_empty() => {
                return Err(no_remote());
            }
            Command::Undo if self.timeline.current_entry().is_none() => {
                return Err(crate::i18n::text("repo-core-2862").into());
            }
            Command::Redo if self.timeline.redo_target().is_none() => {
                return Err(crate::i18n::text("repo-core-2863").into());
            }
            Command::Undo => Action::Undo,
            Command::Redo => Action::Redo,
            Command::Timeline => Action::ToggleTimeline,
            Command::Reload => Action::Reload,
            Command::GraphCopyHash | Command::GraphCopyMessage | Command::GraphCopyBranch => {
                let text = self
                    .selected
                    .and_then(|i| self.graph_copy_text(command, i))
                    .ok_or_else(|| crate::i18n::text("repo-core-2864").to_owned())?;
                Action::CopyText(text)
            }
            Command::Search => Action::FocusSearch,
            Command::Back if self.nav.peek(-1).is_none() => {
                return Err(crate::i18n::text("repo-core-2865").into());
            }
            Command::Forward if self.nav.peek(1).is_none() => {
                return Err(crate::i18n::text("repo-core-2866").into());
            }
            Command::Back => Action::Navigate(-1),
            Command::Forward => Action::Navigate(1),
            Command::NextChange | Command::NextChangeAlt => Action::DiffStep(1),
            Command::PreviousChange | Command::PreviousChangeAlt => Action::DiffStep(-1),
            Command::Fetch => Action::Fetch,
            Command::Pull => Action::Pull,
            Command::Push => Action::Push,
            Command::Stash if self.work.is_clean() => {
                return Err(crate::i18n::text("repo-core-2867").into());
            }
            Command::Stash => Action::Dialog(Dialog::stash()),
            Command::NewBranch if self.refs.head.is_empty() => {
                return Err(crate::i18n::text("repo-core-2868").into());
            }
            Command::NewBranch => Action::Dialog(Dialog::create_branch(&self.refs.head, "HEAD")),
            Command::Terminal => Action::Terminal(TerminalAct::Toggle),
            Command::StageAll if self.work.unstaged.is_empty() => {
                return Err(crate::i18n::text("repo-core-0838").into());
            }
            // `add -A` would mark them resolved, conflict markers and all.
            Command::StageAll if !self.work.conflicted.is_empty() => {
                return Err(crate::i18n::text("repo-core-0351").into());
            }
            Command::StageAll => Action::Ops {
                label: "Stage all".into(),
                ops: ops(&[&["add", "-A"]]),
                refresh: Refresh::Status,
            },
            Command::Commit => return self.commit_key().map(Some),
            Command::FocusSidebar => Action::FocusPane(super::keyboard::Pane::Sidebar),
            Command::FocusGraph => Action::FocusPane(super::keyboard::Pane::Graph),
            Command::FocusDetail => Action::FocusPane(super::keyboard::Pane::Detail),
            Command::FileNext
            | Command::FilePrevious
            | Command::FileToggleStage
            | Command::FileOpen
            | Command::HunkStage
            | Command::HunkUnstage
            | Command::HunkDiscard => return self.keyboard_command(command).map(Some),
            _ => return Ok(None),
        };
        Ok(Some(action))
    }

    /// The commit key: commits as the button would, or, with no summary written yet, goes to
    /// the message.
    fn commit_key(&self) -> Result<Action, String> {
        if !self.work.conflicted.is_empty() {
            return Err(crate::i18n::text("repo-core-0351").into());
        }
        // A rebase or patch stopped on conflicts: the commit continues it, with git's message.
        if self.work.in_progress.is_some_and(|s| s.continues()) {
            return Ok(Action::Commit);
        }
        // A merge or cherry-pick whose conflicts were resolved back to HEAD's content leaves no
        // changes, and committing still concludes it.
        let concluding = self.work.in_progress.is_some();
        if self.work.is_clean() && !self.amend && !concluding {
            return Err(crate::i18n::text("repo-core-2869").into());
        }
        if self.commit_summary.trim().is_empty() {
            // The summary is in the changes' commit box: without the changes row, nowhere.
            return if self.index.contains_key(WIP_HASH) {
                Ok(Action::FocusCommit)
            } else {
                Err(crate::i18n::text("repo-core-0355").into())
            };
        }
        Ok(Action::Commit)
    }

    /// Does what the key of `command` does, or says in the toolbar why it can't.
    pub(super) fn run_key(&mut self, command: Command, ctx: &egui::Context) {
        match self.key_action(command) {
            Ok(Some(action)) => self.apply(action, ctx),
            Ok(None) => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::text("repo-core-2870").into(),
                    true,
                )))
            }
            Err(why) => self.status = Some(crate::repo::Notice::plain((why, true))),
        }
    }

    /// Shows the changes and puts the cursor in the commit summary.
    pub(super) fn focus_commit(&mut self, ctx: &egui::Context) {
        // No changes row (read again since the key): no commit box to put the cursor in.
        let Some(&row) = self.index.get(WIP_HASH) else {
            return;
        };
        if self.selected != Some(row) {
            self.select(row, ctx);
        }
        if !Settings::get(ctx).show_detail {
            Settings::set_show_detail(ctx, true);
        }
        self.focus_commit_summary = true;
    }
}

#[cfg(test)]
#[path = "direct_tests/keys_tests.rs"]
mod tests;
