//! What a repository tab's keys (Fetch, Pull, Push, Stash, New branch, Terminal, Stage all,
//! Commit; see [`crate::keymap`]) do, and why they do nothing when they can't be used now.

use super::{Action, Dialog, Refresh, RepoTab, WIP_HASH, ops};
use crate::keymap::Command;
use crate::settings::Settings;

impl RepoTab {
    /// The action the key of `command` stands for here, or why it can't be done now, as the
    /// toolbar would say it. `None` for the window's keys, which aren't the tab's.
    pub(super) fn key_action(&self, command: Command) -> Result<Option<Action>, String> {
        if let Some(op) = &self.busy_op
            && command != Command::Terminal
        {
            return Err(format!("Wait for {op} to finish"));
        }
        let no_remote = || "This repository has no remote".to_owned();
        let action = match command {
            Command::Fetch | Command::Pull | Command::Push if self.remotes.is_empty() => {
                return Err(no_remote());
            }
            Command::Fetch => Action::Fetch,
            Command::Pull => Action::Pull,
            Command::Push => Action::Push,
            Command::Stash if self.work.is_clean() => return Err("No changes to stash".into()),
            Command::Stash => Action::Dialog(Dialog::stash()),
            Command::NewBranch if self.refs.head.is_empty() => {
                return Err("Make a first commit to branch from".into());
            }
            Command::NewBranch => Action::Dialog(Dialog::create_branch(&self.refs.head, "HEAD")),
            Command::Terminal => Action::ToggleTerminal,
            Command::StageAll if self.work.unstaged.is_empty() => {
                return Err("Nothing to stage".into());
            }
            Command::StageAll => Action::Ops {
                label: "Stage all".into(),
                ops: ops(&[&["add", "-A"]]),
                refresh: Refresh::Status,
            },
            Command::Commit => return self.commit_key().map(Some),
            Command::NewTab
            | Command::CloseTab
            | Command::ReopenTab
            | Command::NextTab
            | Command::PreviousTab
            | Command::GoToTab(_)
            | Command::LastTab
            | Command::Palette
            | Command::Settings
            | Command::Reload
            | Command::Manual => return Ok(None),
        };
        Ok(Some(action))
    }

    /// The commit key: commits as the button would, or, with no summary written yet, goes to
    /// the message.
    fn commit_key(&self) -> Result<Action, String> {
        if !self.work.conflicted.is_empty() {
            return Err("Resolve the conflicted files first".into());
        }
        // A rebase or patch stopped on conflicts: the commit continues it, with git's message.
        if self.work.in_progress.is_some_and(|s| s.continues()) {
            return Ok(Action::Commit);
        }
        if self.work.is_clean() && !self.amend {
            return Err("Nothing to commit".into());
        }
        if self.commit_summary.trim().is_empty() {
            return Ok(Action::FocusCommit);
        }
        Ok(Action::Commit)
    }

    /// Does what the key of `command` does, or says in the toolbar why it can't.
    pub(super) fn run_key(&mut self, command: Command, ctx: &egui::Context) {
        match self.key_action(command) {
            Ok(Some(action)) => self.apply(action, ctx),
            Ok(None) => {}
            Err(why) => self.status = Some((why, true)),
        }
    }

    /// Shows the changes and puts the cursor in the commit summary.
    pub(super) fn focus_commit(&mut self, ctx: &egui::Context) {
        if let Some(&row) = self.index.get(WIP_HASH)
            && self.selected != Some(row)
        {
            self.select(row, ctx);
        }
        let mut settings = Settings::get(ctx);
        if !settings.show_detail {
            settings.show_detail = true;
            settings.set(ctx);
        }
        self.focus_commit_summary = true;
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use egui::{Event, Key, Modifiers, RawInput};

    use super::super::{Action, RepoTab, WIP_HASH};
    use crate::git;
    use crate::keymap::Command;

    /// A repository with one commit in a fresh folder with spaces and accents in its name.
    fn repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft keys ü {name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgsign", "false"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
        ] {
            assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
        }
        dir
    }

    /// Polls `tab` until `done`, or gives up after ten seconds.
    fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
        for _ in 0..400 {
            tab.poll(ctx);
            if done(tab) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// The tab on `dir`, read through.
    fn loaded(dir: &Path, ctx: &egui::Context) -> RepoTab {
        let mut tab = RepoTab::new_deferred(dir.to_owned(), ctx);
        tab.start_deferred(ctx);
        wait(&mut tab, ctx, |t| !t.opening() && !t.loading);
        tab
    }

    /// Runs the key of `command` and waits for what it started, and the read after it.
    fn press(tab: &mut RepoTab, ctx: &egui::Context, command: Command) {
        tab.run_key(command, ctx);
        wait(tab, ctx, |t| t.busy_op.is_none() && !t.loading);
    }

    fn status(dir: &Path) -> String {
        git::run(Some(dir), &["status", "--porcelain"]).unwrap_or_default()
    }

    /// What can't be done now says why, as the toolbar greys it out; the window's keys aren't
    /// the tab's.
    #[test]
    fn keys_that_cannot_be_used_now_say_why() {
        let dir = repo("refused");
        let ctx = egui::Context::default();
        let tab = loaded(&dir, &ctx);
        let why = |c| tab.key_action(c).err().unwrap_or_default();
        let fetch = why(Command::Fetch);
        let push = why(Command::Push);
        let stash = why(Command::Stash);
        let stage = why(Command::StageAll);
        let commit = why(Command::Commit);
        let branch = matches!(
            tab.key_action(Command::NewBranch),
            Ok(Some(Action::Dialog(_)))
        );
        let terminal = matches!(
            tab.key_action(Command::Terminal),
            Ok(Some(Action::ToggleTerminal))
        );
        let window = tab.key_action(Command::NextTab);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(fetch, "This repository has no remote");
        assert_eq!(push, "This repository has no remote");
        assert_eq!(stash, "No changes to stash");
        assert_eq!(stage, "Nothing to stage");
        assert_eq!(commit, "Nothing to commit");
        assert!(branch);
        assert!(terminal);
        assert!(matches!(window, Ok(None)));
    }

    /// Every key of the tab is turned into an action or a reason, and, while an operation
    /// runs, only the terminal's key still does something.
    #[test]
    fn every_key_resolves_and_waits_for_a_running_operation() {
        let dir = repo("busy");
        let ctx = egui::Context::default();
        let mut tab = loaded(&dir, &ctx);
        for b in crate::keymap::BINDINGS {
            let resolved = tab.key_action(b.command);
            let tabs = b.command.scope() == crate::keymap::Scope::Repository;
            match resolved {
                Ok(Some(_)) => assert!(tabs, "{:?}", b.command),
                Ok(None) => assert!(!tabs, "{:?}", b.command),
                Err(why) => assert!(tabs && !why.is_empty(), "{:?}", b.command),
            }
        }
        tab.busy_op = Some("Push".to_owned());
        let fetch = tab.key_action(Command::Fetch).err();
        let commit = tab.key_action(Command::Commit).err();
        let terminal = tab.key_action(Command::Terminal);
        tab.run_key(Command::StageAll, &ctx);
        let shown = tab.status.clone();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(fetch.as_deref(), Some("Wait for Push to finish"));
        assert_eq!(commit.as_deref(), Some("Wait for Push to finish"));
        assert!(matches!(terminal, Ok(Some(Action::ToggleTerminal))));
        assert_eq!(shown, Some(("Wait for Push to finish".to_owned(), true)));
    }

    /// Stage all, then commit: with no summary the key goes to the message; with one it
    /// commits what is staged, names and messages in any script.
    #[test]
    fn stage_all_and_commit_from_the_keys() {
        let dir = repo("commit");
        let ctx = egui::Context::default();
        let name = "ünï cødé 文件 with spaces.txt";
        let _ = std::fs::write(dir.join(name), "x\n");
        let mut tab = loaded(&dir, &ctx);
        wait(&mut tab, &ctx, |t| !t.work.unstaged.is_empty());

        press(&mut tab, &ctx, Command::StageAll);
        let staged = status(&dir);

        tab.commit_summary = "   ".to_owned();
        press(&mut tab, &ctx, Command::Commit);
        let focused = tab.focus_commit_summary;
        let on_changes = tab.selected == tab.index.get(WIP_HASH).copied();

        let summary = format!("feat: ✓ ünïcode {}", "long ".repeat(2000));
        tab.commit_summary = summary.clone();
        press(&mut tab, &ctx, Command::Commit);
        let message = git::run(Some(&dir), &["log", "-1", "--format=%s"]).unwrap_or_default();
        let after = status(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(staged.starts_with("A "), "{staged}");
        assert!(focused);
        assert!(on_changes);
        assert_eq!(message.trim(), summary.trim());
        assert!(after.is_empty(), "{after}");
    }

    /// Fetch, pull and push from the keys, against a remote in a folder with spaces.
    #[test]
    fn fetch_pull_and_push_from_the_keys() {
        let dir = repo("remote");
        let bare = dir.with_file_name(format!(
            "{} remote.git",
            dir.file_name().unwrap_or_default().to_string_lossy()
        ));
        let _ = std::fs::remove_dir_all(&bare);
        let bare_s = bare.to_string_lossy().into_owned();
        assert!(git::run(None, &["init", "-q", "--bare", &bare_s]).is_ok());
        assert!(git::run(Some(&dir), &["remote", "add", "origin", &bare_s]).is_ok());
        let ctx = egui::Context::default();
        let mut tab = loaded(&dir, &ctx);

        press(&mut tab, &ctx, Command::Push);
        let pushed = git::run(Some(&bare), &["log", "-1", "--format=%s"]).unwrap_or_default();
        press(&mut tab, &ctx, Command::Fetch);
        let fetch_status = tab.status.clone();
        press(&mut tab, &ctx, Command::Pull);
        let pull_status = tab.status.clone();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&bare);
        assert_eq!(pushed.trim(), "base");
        assert!(!fetch_status.is_some_and(|(_, err)| err));
        assert!(!pull_status.is_some_and(|(_, err)| err));
    }

    /// One frame of `tab`'s keys with `events`, with a text field focused or not.
    fn keys_in_frame(tab: &mut RepoTab, ctx: &egui::Context, events: Vec<Event>) -> Vec<Action> {
        let mut acts = Vec::new();
        let input = RawInput {
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |_| tab.shortcuts(ctx, &mut acts))
            .textures_delta
            .clear();
        acts
    }

    /// Typing in a text field (the commit message, the branch filter) never runs a key's action.
    #[test]
    fn a_focused_text_field_keeps_the_keys() {
        let dir = repo("focus");
        let ctx = egui::Context::default();
        let mut tab = loaded(&dir, &ctx);
        let press = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        let free = keys_in_frame(&mut tab, &ctx, vec![press(Key::J)]);

        // A text field takes the keyboard, then the key comes.
        let mut text = String::new();
        let mut field = |ctx: &egui::Context, events| {
            let input = RawInput {
                events,
                ..Default::default()
            };
            let mut acts = Vec::new();
            ctx.run_ui(input, |ui| {
                let r = ui.text_edit_singleline(&mut text);
                r.request_focus();
                tab.shortcuts(ctx, &mut acts);
            })
            .textures_delta
            .clear();
            acts
        };
        let _ = field(&ctx, Vec::new());
        let typing = field(&ctx, vec![press(Key::J), press(Key::Enter)]);
        // The palette or the manual is open over the tab.
        let ctx = egui::Context::default();
        tab.keys_blocked = true;
        let blocked = keys_in_frame(&mut tab, &ctx, vec![press(Key::J)]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(free.as_slice(), [Action::Key(Command::Terminal)]));
        assert!(typing.iter().all(|a| !matches!(a, Action::Key(_))));
        assert!(blocked.is_empty());
    }
}
