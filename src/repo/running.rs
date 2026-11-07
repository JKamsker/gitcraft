//! The operation running in a tab: how far it is (git's own progress, as a thin bar under the
//! toolbar), _Cancel_, and the operations asked for meanwhile, which wait for it.

use std::collections::VecDeque;
use std::sync::Arc;

use egui::Ui;

use super::{Action, RepoTab};
use crate::git;
use crate::repo::pull::PullMode;
use crate::settings::Settings;
use crate::theme::Tokens;

/// The most operations that wait for the running one; more are refused, with why.
const MAX_WAITING: usize = 3;

/// What the UI keeps of the operation running.
pub(super) struct Running {
    pub(super) control: Arc<git::OpControl>,
    /// It goes over the network: it reports progress and can be cancelled.
    pub(super) network: bool,
    /// Nobody asked for it (the background fetch): not shown on the taskbar button.
    pub(super) background: bool,
}

/// What can be asked for while an operation runs, to start once it is done. Only these: they
/// work out what they run when they start (Push looks at the branch checked out then), and
/// change nothing another operation of the tab relies on while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Waiter {
    Fetch,
    Pull(PullMode),
    Push,
}

impl Waiter {
    /// The waiter `action` asks for, if it is one.
    pub(super) fn of(action: &Action, ctx: &egui::Context) -> Option<Self> {
        match action {
            Action::Fetch => Some(Self::Fetch),
            Action::Pull => Some(Self::Pull(Settings::get(ctx).pull_mode)),
            Action::PullWith(mode) => Some(Self::Pull(*mode)),
            Action::Push => Some(Self::Push),
            _ => None,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch",
            Self::Pull(mode) => mode.label(),
            Self::Push => "Push",
        }
    }

    fn action(self) -> Action {
        match self {
            Self::Fetch => Action::Fetch,
            Self::Pull(mode) => Action::PullWith(mode),
            Self::Push => Action::Push,
        }
    }
}

/// The operations waiting for the running one, oldest first.
pub(super) type Waiting = VecDeque<Waiter>;

impl RepoTab {
    /// Starts following the operation about to run `ops`: its control, shared with the thread
    /// that runs it.
    pub(super) fn follow_op(
        &mut self,
        ops: &[Vec<String>],
        background: bool,
        ctx: &egui::Context,
    ) -> Arc<git::OpControl> {
        let repaint = ctx.clone();
        let control =
            git::OpControl::scoped(move || repaint.request_repaint(), self.tx.cancel.child());
        let network = ops.iter().any(|op| {
            let args: Vec<&str> = op.iter().map(String::as_str).collect();
            git::goes_over_network(&args)
        });
        self.running = Some(Running {
            control: Arc::clone(&control),
            network,
            background,
        });
        control
    }

    /// Whether the running operation can be cancelled (it goes over the network).
    pub(super) fn can_cancel(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|r| r.network && !r.control.is_cancelled())
    }

    /// Stops the running operation: the command running is stopped, with what it started, and
    /// no later step runs. What waited for it doesn't run either (see [`Self::after_op`]).
    pub(super) fn cancel_op(&mut self) {
        if !self.can_cancel() {
            return;
        }
        if let Some(running) = &self.running {
            running.control.cancel();
        }
    }

    /// Whether the running operation is being cancelled.
    pub(super) fn was_cancelled(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|r| r.control.is_cancelled())
    }

    /// Keeps `waiter` to start once the operation running now ends (twice is once); refused,
    /// with why, past [`MAX_WAITING`].
    pub(super) fn wait_for_running(&mut self, waiter: Waiter) {
        let busy = self.busy_op.clone().unwrap_or_default();
        if self.waiting.contains(&waiter) {
            return;
        }
        if self.waiting.len() >= MAX_WAITING {
            self.status = Some((
                format!(
                    "{}: not started, {MAX_WAITING} operations already wait for {busy}",
                    waiter.label()
                ),
                true,
            ));
            return;
        }
        self.waiting.push_back(waiter);
    }

    /// Once an operation ended (and whatever it started itself, like updating the submodules,
    /// is running): starts the next one waiting. After one the user asked for failed or was
    /// cancelled, they are dropped instead, and the status says so: they were asked for on top
    /// of it. A background fetch's failure doesn't drop them.
    pub(super) fn after_op(&mut self, ok: bool, quiet: bool, ctx: &egui::Context) {
        if !ok && !quiet {
            let dropped = std::mem::take(&mut self.waiting);
            if !dropped.is_empty()
                && let Some((msg, _)) = &mut self.status
            {
                *msg = with_not_run(msg, &dropped);
            }
            return;
        }
        if self.busy_op.is_none()
            && let Some(waiter) = self.waiting.pop_front()
        {
            self.apply(waiter.action(), ctx);
        }
    }

    /// For the taskbar button: how far the network operation the user asked for is (`None`
    /// inside: not known), when one runs.
    pub fn taskbar_progress(&self) -> Option<Option<f32>> {
        let running = self
            .running
            .as_ref()
            .filter(|r| r.network && !r.background)?;
        Some(running.control.progress().and_then(|p| p.fraction()))
    }

    /// Whether a network operation the user asked for failed since this was last asked (for the
    /// taskbar button, which turns red).
    pub fn take_network_failure(&mut self) -> bool {
        std::mem::take(&mut self.network_failed)
    }

    /// What the running operation says it is doing ("Receiving objects: 45%").
    pub(super) fn op_progress(&self) -> Option<git::progress::Progress> {
        self.running
            .as_ref()
            .filter(|r| r.network)
            .and_then(|r| r.control.progress())
    }

    /// The thin bar under the toolbar while an operation goes over the network: how far its
    /// current phase is, or a stripe going back and forth while git hasn't said.
    pub(super) fn progress_bar(&self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, t.separator);
        let filled = match self.op_progress().and_then(|p| p.fraction()) {
            Some(f) => {
                egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * f, rect.height()))
            }
            None => {
                let time = ui.input(|i| i.time) as f32;
                let width = rect.width() * 0.25;
                let swing = (time * 1.4).sin() * 0.5 + 0.5;
                let left = rect.left() + (rect.width() - width) * swing;
                // As often as the spinner beside it, not every frame.
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(50));
                egui::Rect::from_min_size(
                    egui::pos2(left, rect.top()),
                    egui::vec2(width, rect.height()),
                )
            }
        };
        painter.rect_filled(filled, 0.0, t.accent);
    }
}

/// `msg` ("Fetch failed: …") with the operations that waited for it named in its first part:
/// "Fetch failed (Pull not run): …", which is what the toolbar shows of it.
pub(super) fn with_not_run(msg: &str, dropped: &Waiting) -> String {
    let names: Vec<&str> = dropped.iter().map(|w| w.label()).collect();
    let note = format!(" ({} not run)", names.join(", "));
    match msg.split_once(": ") {
        Some((head, rest)) => format!("{head}{note}: {rest}"),
        None => match msg.split_once('\n') {
            Some((head, rest)) => format!("{head}{note}\n{rest}"),
            None => format!("{msg}{note}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::super::{Action, OpKind, Refresh, RepoTab, UndoEntry, ops};
    use super::{Waiter, Waiting, with_not_run};
    use crate::git;

    fn repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-running {name} {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
        ] {
            assert!(git::run(Some(&dir), args).is_ok());
        }
        dir
    }

    /// Polls `tab` until `done` holds (or a while has passed).
    fn until(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
        let start = Instant::now();
        while !done(tab) && start.elapsed() < Duration::from_secs(20) {
            tab.poll(ctx);
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A fetch whose "ssh" hangs until it is stopped, and leaves `pid` behind when it starts.
    #[cfg(unix)]
    fn hanging_fetch(dir: &Path) -> Vec<Vec<String>> {
        use std::os::unix::fs::PermissionsExt;
        let ssh = dir.join("hang.sh");
        let pid = dir.join("pid");
        let _ = std::fs::write(
            &ssh,
            format!("#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n", pid.display()),
        );
        let _ = std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755));
        let config = format!("core.sshCommand='{}'", ssh.display());
        ops(&[&["-c", &config, "fetch", "ssh://example.invalid/r.git"]])
    }

    /// Cancel stops the fetch, drops what waited for it (saying so), leaves undo as it was and
    /// the tab idle, ready for the next operation.
    #[cfg(unix)]
    #[test]
    fn cancelling_stops_the_operation_and_cleans_up() {
        let dir = repo("cancel");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let entry = UndoEntry {
            id: 0,
            agent: None,
            label: "Commit".to_owned(),
            undo: ops(&[&["status"]]),
            redo: ops(&[&["status"]]),
            heads: None,
        };
        tab.remember_undo(&entry.label, entry.clone(), &ctx);
        let entry = tab.timeline.current_entry().and_then(|e| e.undo.clone());
        tab.run_ops(
            "Fetch",
            hanging_fetch(&dir),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        assert!(tab.can_cancel());
        // Asked for meanwhile: waits.
        tab.apply(Action::Push, &ctx);
        assert_eq!(tab.waiting, [Waiter::Push]);
        // Anything else is not started, and says why.
        tab.run_ops(
            "Tag",
            ops(&[&["tag", "x"]]),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        let refused = tab.status.clone();
        let pid_file = dir.join("pid");
        let start = Instant::now();
        while !pid_file.exists() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        tab.apply(Action::CancelOp, &ctx);
        until(&mut tab, &ctx, |t| t.busy_op.is_none());
        let tags = git::run(Some(&dir), &["tag"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            refused,
            Some(("Tag: not started, Fetch is still running".to_owned(), true))
        );
        assert!(tab.busy_op.is_none());
        assert!(tab.running.is_none());
        assert!(tab.waiting.is_empty());
        assert!(!tab.can_cancel());
        assert_eq!(
            tab.status.as_ref().map(|(s, e)| (s.as_str(), *e)),
            Some(("Fetch (Push not run): cancelled", false))
        );
        assert_eq!(
            tab.timeline.current_entry().and_then(|e| e.undo.clone()),
            entry
        );
        assert_eq!(tags.as_deref(), Ok(""));
        assert!(tab.finished.is_empty(), "a cancel is no notification");
    }

    /// Fetch, Pull and Push asked for meanwhile wait, in order, twice the same once; past the
    /// limit one is refused with why. They then run one after the other.
    #[test]
    fn fetch_pull_and_push_asked_for_meanwhile_run_after_in_order() {
        use crate::repo::pull::PullMode;
        let dir = repo("waiting");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        tab.run_ops(
            "Tag a",
            ops(&[&["tag", "a"]]),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        let modes = PullMode::ALL;
        for action in [
            Action::Fetch,
            Action::Fetch,
            Action::PullWith(modes[0]),
            Action::Push,
            Action::PullWith(modes[1]),
        ] {
            tab.apply(action, &ctx);
        }
        let waiting = tab.waiting.clone();
        let refused = tab.status.clone();
        let mut ran = Vec::new();
        let start = Instant::now();
        while (tab.busy_op.is_some() || !tab.waiting.is_empty())
            && start.elapsed() < Duration::from_secs(20)
        {
            tab.poll(&ctx);
            if let Some(op) = &tab.busy_op
                && ran.last() != Some(op)
            {
                ran.push(op.clone());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            waiting,
            [Waiter::Fetch, Waiter::Pull(modes[0]), Waiter::Push]
        );
        assert!(refused.is_some_and(|(s, err)| err && s.contains("not started, 3 operations")));
        // Fetch (no remotes: nothing to do) went well, then the pull ran (and failed, with
        // nothing to pull from), and the push it was asked before is dropped.
        assert_eq!(ran[..2], ["Tag a".to_owned(), "Fetch".to_owned()]);
        assert_eq!(ran.get(2).map(String::as_str), Some(modes[0].label()));
        let status = tab.status.map(|(s, _)| s).unwrap_or_default();
        assert!(status.contains("(Push not run)"), "{status}");
    }

    /// A background fetch that fails doesn't drop what the user asked for meanwhile.
    #[test]
    fn a_failed_background_fetch_still_runs_what_waited() {
        let dir = repo("waiting-quiet");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let fails = ops(&[&["fetch", "no-such-remote"]]);
        tab.run_ops_ex(
            "Background fetch",
            fails,
            Refresh::Status,
            OpKind::plain(),
            true,
            &ctx,
        );
        tab.apply(Action::Fetch, &ctx);
        until(&mut tab, &ctx, |t| {
            t.busy_op.is_none() && t.waiting.is_empty()
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(tab.status, Some(("Fetch: done".to_owned(), false)));
    }

    #[test]
    fn what_was_not_run_is_named_in_the_first_part() {
        let dropped: Waiting = [Waiter::Fetch, Waiter::Push].into_iter().collect();
        assert_eq!(
            with_not_run("Pull failed: fatal: x\nmore", &dropped),
            "Pull failed (Fetch, Push not run): fatal: x\nmore"
        );
        assert_eq!(with_not_run("Odd", &dropped), "Odd (Fetch, Push not run)");
    }
}
