//! _Skip hooks_: one commit or one push run with `--no-verify`, so that the hooks it would run
//! (`pre-commit`, `commit-msg`, `pre-push`) don't. Ticked for that one run, then off again.

use egui::{Response, Ui};

use super::{Action, RepoTab};

/// Which of the next commit and the next push skip their hooks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SkipHooks {
    pub commit: bool,
    pub push: bool,
}

impl SkipHooks {
    /// The commit box's commit, `ops`, with `--no-verify` on its commit step when it skips
    /// hooks; unticked once used. Only for that commit: a reword or a squash, also `git
    /// commit`, runs its hooks whatever the box says.
    pub fn commit_steps(&mut self, ops: Vec<Vec<String>>) -> Vec<Vec<String>> {
        let mut only = SkipHooks {
            commit: self.commit,
            push: false,
        };
        self.commit = false;
        only.apply(ops)
    }

    /// The push `ops` (see [`RepoTab::push_skipping_hooks`]), with `--no-verify` on its push
    /// step when it skips hooks.
    pub fn push_steps(&mut self, ops: Vec<Vec<String>>) -> Vec<Vec<String>> {
        let mut only = SkipHooks {
            commit: false,
            push: self.push,
        };
        self.push = false;
        only.apply(ops)
    }

    /// `ops` with `--no-verify` on their commit steps when the commit skips hooks,
    /// and on their push steps when the push does; each of the two is off again once used.
    fn apply(&mut self, mut ops: Vec<Vec<String>>) -> Vec<Vec<String>> {
        let mut used = SkipHooks::default();
        for step in &mut ops {
            let Some(at) = subcommand_at(step) else {
                continue;
            };
            let (ticked, used) = match step[at].as_str() {
                "commit" => (self.commit, &mut used.commit),
                "push" => (self.push, &mut used.push),
                _ => continue,
            };
            if !ticked {
                continue;
            }
            *used = true;
            if !step.iter().any(|a| a == "--no-verify") {
                step.insert(at + 1, "--no-verify".to_owned());
            }
        }
        self.commit &= !used.commit;
        self.push &= !used.push;
        ops
    }
}

/// Where the subcommand of the git step `step` is (after `-c x=y` and the like).
fn subcommand_at(step: &[String]) -> Option<usize> {
    let offset = usize::from(
        step.first()
            .is_some_and(|word| word == crate::gerrit::AMEND),
    );
    let words: Vec<&str> = step[offset..].iter().map(String::as_str).collect();
    let sub = crate::git::subcommand(&words)?;
    let mut i = 0;
    while i < words.len() {
        match words[i] {
            "-c" | "-C" => i += 2,
            w if w == sub => return Some(i + offset),
            _ => i += 1,
        }
    }
    None
}

/// The Push button's right-click menu: push once without the `pre-push` hook.
pub(super) fn push_menu(push: &Response, acts: &mut Vec<Action>) {
    egui::Popup::context_menu(push)
        .style(crate::theme::menu_style)
        .show(|ui: &mut Ui| {
            if ui
                .button("Push, skipping hooks")
                .on_hover_text(
                    "Pushes once without running the pre-push hook (git push --no-verify)",
                )
                .clicked()
            {
                acts.push(Action::PushSkippingHooks);
            }
        });
}

impl RepoTab {
    /// Pushes the way Push does, without the `pre-push` hook this once.
    pub(super) fn push_skipping_hooks(&mut self, ctx: &egui::Context) {
        if let Some(mut dialog) = self.review_dialog() {
            if let super::Dialog::GerritReview { skip_hooks, .. } = &mut dialog {
                *skip_hooks = true;
            }
            self.dialog = Some(dialog);
            return;
        }
        self.skip_hooks.push = true;
        self.push(ctx);
        // Push may not have run (no branch, no remote, busy): the next one runs the hook again.
        self.skip_hooks.push = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::ops;

    #[test]
    fn the_commit_step_gets_no_verify_once() {
        let mut skip = SkipHooks {
            commit: true,
            push: false,
        };
        let steps = skip.apply(ops(&[&["add", "-A"], &["commit", "-m", "ünï --x"]]));
        assert_eq!(
            steps,
            ops(&[&["add", "-A"], &["commit", "--no-verify", "-m", "ünï --x"]])
        );
        assert!(!skip.commit);
        // The next commit runs its hooks again.
        let again = skip.apply(ops(&[&["commit", "-m", "x"]]));
        assert_eq!(again, ops(&[&["commit", "-m", "x"]]));
    }

    #[test]
    fn a_push_after_options_gets_no_verify_and_other_steps_dont() {
        let mut skip = SkipHooks {
            commit: false,
            push: true,
        };
        let push = ops(&[&["-c", "push.default=simple", "push", "-u", "origin", "main"]]);
        assert_eq!(
            skip.apply(push),
            ops(&[&[
                "-c",
                "push.default=simple",
                "push",
                "--no-verify",
                "-u",
                "origin",
                "main"
            ]])
        );
        assert!(!skip.push);
        // A commit isn't a push: it keeps its hooks, and a ticked push waits for its push.
        let mut waiting = SkipHooks {
            commit: false,
            push: true,
        };
        let commit = waiting.apply(ops(&[&["commit", "-m", "push"]]));
        assert_eq!(commit, ops(&[&["commit", "-m", "push"]]));
        assert!(waiting.push);
    }

    /// A reword (`commit --amend --only`) runs like any operation: the commit box's tick is
    /// not for it, and stays for the commit it was ticked for.
    #[test]
    fn another_commit_step_neither_skips_nor_uses_the_tick() {
        let mut skip = SkipHooks {
            commit: true,
            push: false,
        };
        let reword = ops(&[&["commit", "--amend", "--only", "-m", "new"]]);
        assert_eq!(skip.push_steps(reword.clone()), reword);
        assert!(skip.commit);
        let commit = skip.commit_steps(ops(&[&["commit", "-m", "x"]]));
        assert_eq!(commit, ops(&[&["commit", "--no-verify", "-m", "x"]]));
        assert!(!skip.commit);
    }

    #[test]
    fn nothing_changes_without_skipping_and_no_verify_is_never_doubled() {
        let mut none = SkipHooks::default();
        let steps = ops(&[&["commit", "-m", "x"], &["push"]]);
        assert_eq!(none.apply(steps.clone()), steps);
        let mut skip = SkipHooks {
            commit: true,
            push: true,
        };
        let already = ops(&[&["commit", "--no-verify", "-m", "x"]]);
        assert_eq!(skip.apply(already.clone()), already);
        assert!(skip.apply(Vec::new()).is_empty());
        assert!(skip.apply(vec![Vec::new()]).iter().all(Vec::is_empty));
    }

    /// Through the tab, on real repositories: a failing hook named in the error, and skipped
    /// once.
    mod on_repositories {
        use std::path::{Path, PathBuf};
        use std::time::Duration;

        use crate::git;
        use crate::repo::{Action, RepoTab};

        fn git_ok(dir: &Path, args: &[&str]) {
            let ran = git::run(Some(dir), args);
            assert!(ran.is_ok(), "{args:?}: {ran:?}");
        }

        /// A hook named `name` in `dir` that says `says` and exits with 3.
        fn failing_hook(dir: &Path, name: &str, says: &str) {
            let hooks = dir.join(".git").join("hooks");
            let _ = std::fs::create_dir_all(&hooks);
            let path = hooks.join(name);
            let _ = std::fs::write(&path, format!("#!/bin/sh\necho '{says}' >&2\nexit 3\n"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
            }
        }

        /// A repository with one commit on `main`, its folder named with a space and accents.
        fn repo(name: &str) -> PathBuf {
            let dir = std::env::temp_dir()
                .join(format!("gitcraft-hooks ünï {name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::create_dir_all(&dir);
            for args in [
                &["init", "-q", "-b", "main"][..],
                &["config", "user.name", "t"],
                &["config", "user.email", "t@t"],
                &["commit", "-q", "--allow-empty", "-m", "base"],
            ] {
                git_ok(&dir, args);
            }
            dir
        }

        fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
            for _ in 0..400 {
                tab.poll(ctx);
                if done(tab) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }

        fn opened(dir: &Path, ctx: &egui::Context) -> RepoTab {
            let mut tab = RepoTab::new(dir.to_owned(), ctx);
            wait(&mut tab, ctx, |t| !t.loading && !t.refs.head.is_empty());
            tab
        }

        /// Runs `act` and waits for it, and the reload after it, to end: what the status says.
        fn run(tab: &mut RepoTab, ctx: &egui::Context, act: Action) -> String {
            tab.apply(act, ctx);
            wait(tab, ctx, |t| t.busy_op.is_none() && !t.loading);
            tab.status.clone().map(|(s, _)| s).unwrap_or_default()
        }

        #[test]
        fn a_failing_pre_commit_hook_is_named_and_skip_hooks_commits_once() {
            let dir = repo("commit");
            failing_hook(&dir, "pre-commit", "lint: 2 problems ✗");
            let _ = std::fs::write(dir.join("a file.txt"), "x\n");
            let ctx = egui::Context::default();
            let mut tab = opened(&dir, &ctx);
            let before = git::head(&dir);
            tab.commit_summary = "Add a file ✓".to_owned();
            let failed = run(&mut tab, &ctx, Action::Commit);
            let still = git::head(&dir);
            tab.skip_hooks.commit = true;
            let skipped = run(&mut tab, &ctx, Action::Commit);
            let committed = git::head(&dir);
            let message = git::head_message(&dir).unwrap_or_default();
            let ticked = tab.skip_hooks.commit;
            // The next commit runs the hook again.
            let _ = std::fs::write(dir.join("a file.txt"), "y\n");
            tab.commit_summary = "Again".to_owned();
            let again = run(&mut tab, &ctx, Action::Commit);
            let _ = std::fs::remove_dir_all(&dir);
            assert!(
                failed.contains("the pre-commit hook stopped it (exit code 3)\nlint: 2 problems ✗"),
                "{failed}"
            );
            assert!(failed.contains("Tick Skip hooks"), "{failed}");
            assert_eq!(still, before);
            assert_eq!(skipped, "Commit: done");
            assert_ne!(committed, before);
            assert_eq!(message.trim(), "Add a file ✓");
            assert!(!ticked);
            assert!(again.contains("pre-commit hook stopped it"), "{again}");
        }

        /// A `commit-msg` hook is told apart from `pre-commit` (both run on a commit).
        #[test]
        fn the_hook_that_failed_is_the_one_named() {
            let dir = repo("commit-msg");
            let hooks = dir.join(".git").join("hooks");
            let _ = std::fs::create_dir_all(&hooks);
            let pass = hooks.join("pre-commit");
            let _ = std::fs::write(&pass, "#!/bin/sh\nexit 0\n");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&pass, std::fs::Permissions::from_mode(0o755));
            }
            failing_hook(&dir, "commit-msg", "");
            let err =
                git::run_naming_hooks(Some(&dir), &["commit", "--allow-empty", "-m", "x"], None);
            let plain = git::run_naming_hooks(Some(&dir), &["commit", "-m", "nothing"], None);
            let _ = std::fs::remove_dir_all(&dir);
            let err = err.err().unwrap_or_default();
            assert!(
                err.starts_with("the commit-msg hook stopped it (exit code 3)"),
                "{err}"
            );
            // Nothing to commit is git's own failure: no hook is blamed.
            let plain = plain.err().unwrap_or_default();
            assert!(!plain.contains(" hook "), "{plain}");
        }

        #[test]
        fn a_failing_pre_push_hook_is_named_and_skip_hooks_pushes_once() {
            let dir = repo("push");
            let remote =
                dir.with_file_name(format!("gitcraft-hooks remote-{}.git", std::process::id()));
            let _ = std::fs::remove_dir_all(&remote);
            let bare = remote.to_string_lossy().into_owned();
            git_ok(&dir, &["init", "-q", "--bare", &bare]);
            git_ok(&dir, &["remote", "add", "origin", &bare]);
            git_ok(&dir, &["push", "-q", "-u", "origin", "main"]);
            git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "to push"]);
            failing_hook(&dir, "pre-push", "tests failed");
            let ctx = egui::Context::default();
            let mut tab = opened(&dir, &ctx);
            let failed = run(&mut tab, &ctx, Action::Push);
            let not_pushed = git::resolve(&remote, "refs/heads/main");
            let skipped = run(&mut tab, &ctx, Action::PushSkippingHooks);
            let pushed = git::resolve(&remote, "refs/heads/main");
            let head = git::head(&dir);
            let ticked = tab.skip_hooks.push;
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::remove_dir_all(&remote);
            assert!(
                failed.contains("the pre-push hook stopped it (exit code 3)\ntests failed"),
                "{failed}"
            );
            assert_ne!(not_pushed.as_deref(), Ok(head.as_str()));
            assert_eq!(skipped, "Push: done");
            assert_eq!(pushed, Ok(head));
            assert!(!ticked);
        }
    }
}
