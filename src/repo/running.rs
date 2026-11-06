//! The operation running in a tab: how far it is (git's own progress, as a thin bar under the
//! toolbar), _Cancel_, and the operations asked for meanwhile, which wait for it.

use std::collections::VecDeque;
use std::sync::Arc;

use egui::Ui;

use super::{PendingOp, RepoTab};
use crate::git;
use crate::theme::Tokens;

/// The most operations that wait for the running one; more are refused, with why.
const MAX_WAITING: usize = 3;

/// What the UI keeps of the operation running.
pub(super) struct Running {
    pub(super) control: Arc<git::OpControl>,
    /// It goes over the network: it reports progress and can be cancelled.
    pub(super) network: bool,
}

/// The operations waiting for the running one, oldest first.
pub(super) type Waiting = VecDeque<PendingOp>;

impl RepoTab {
    /// Starts following the operation about to run `ops`: its control, shared with the thread
    /// that runs it.
    pub(super) fn follow_op(
        &mut self,
        ops: &[Vec<String>],
        ctx: &egui::Context,
    ) -> Arc<git::OpControl> {
        let repaint = ctx.clone();
        let control = git::OpControl::new(move || repaint.request_repaint());
        let network = ops.iter().any(|op| {
            let args: Vec<&str> = op.iter().map(String::as_str).collect();
            git::goes_over_network(&args)
        });
        self.running = Some(Running {
            control: Arc::clone(&control),
            network,
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
    /// no later step runs; nothing waiting for it runs either.
    pub(super) fn cancel_op(&mut self) {
        if !self.can_cancel() {
            return;
        }
        if let Some(running) = &self.running {
            running.control.cancel();
        }
        self.waiting.clear();
    }

    /// Whether the operation that just ended was cancelled.
    pub(super) fn was_cancelled(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|r| r.control.is_cancelled())
    }

    /// Keeps `op` to run once the operation running now ends, and says so; `false` when there
    /// is no room left for it (it is then dropped, and the status says why).
    pub(super) fn wait_for_running(&mut self, op: PendingOp) -> bool {
        let busy = self.busy_op.clone().unwrap_or_default();
        // Clicked twice: once is enough.
        if self
            .waiting
            .iter()
            .any(|w| w.label == op.label && w.ops == op.ops)
        {
            return true;
        }
        if self.waiting.len() >= MAX_WAITING {
            self.status = Some((
                format!(
                    "{}: not started, {MAX_WAITING} operations are already waiting for {busy}",
                    op.label
                ),
                true,
            ));
            return false;
        }
        self.status = Some((format!("{} waits for {busy} to finish", op.label), false));
        self.waiting.push_back(op);
        true
    }

    /// Once an operation ended: runs the next one waiting when it went well, or else drops
    /// them all, saying so (they were asked for on top of what failed).
    pub(super) fn after_op(&mut self, ok: bool, ctx: &egui::Context) {
        self.running = None;
        if !ok {
            let dropped = std::mem::take(&mut self.waiting);
            if !dropped.is_empty()
                && let Some((msg, _)) = &mut self.status
            {
                let names: Vec<&str> = dropped.iter().map(|op| op.label.as_str()).collect();
                msg.push_str(&format!("\n(not run: {})", names.join(", ")));
            }
            return;
        }
        if let Some(op) = self.waiting.pop_front() {
            self.run_ops_ex(&op.label, op.ops, op.refresh, op.kind, op.quiet, ctx);
        }
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
                ui.ctx().request_repaint();
                egui::Rect::from_min_size(
                    egui::pos2(left, rect.top()),
                    egui::vec2(width, rect.height()),
                )
            }
        };
        painter.rect_filled(filled, 0.0, t.accent);
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::super::{Action, OpKind, Refresh, RepoTab, Undo, UndoEntry, ops};
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

    /// Cancel stops the fetch, drops what waited for it, leaves undo as it was and the tab idle,
    /// ready for the next operation.
    #[cfg(unix)]
    #[test]
    fn cancelling_stops_the_operation_and_cleans_up() {
        let dir = repo("cancel");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let entry = UndoEntry {
            label: "Commit".to_owned(),
            undo: ops(&[&["status"]]),
            redo: ops(&[&["status"]]),
            heads: None,
        };
        tab.undo_stack.push(entry.clone());
        tab.run_ops(
            "Fetch",
            hanging_fetch(&dir),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        assert!(tab.can_cancel());
        // Asked for meanwhile: waits.
        tab.run_ops(
            "Tag",
            ops(&[&["tag", "after-cancel"]]),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        assert_eq!(tab.waiting.len(), 1);
        let pid_file = dir.join("pid");
        let start = Instant::now();
        while !pid_file.exists() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        tab.apply(Action::CancelOp, &ctx);
        assert!(tab.waiting.is_empty());
        until(&mut tab, &ctx, |t| t.busy_op.is_none());
        let tagged = git::run(Some(&dir), &["tag", "--list", "after-cancel"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(tab.busy_op.is_none());
        assert!(tab.running.is_none());
        assert!(!tab.can_cancel());
        assert_eq!(
            tab.status.as_ref().map(|(s, e)| (s.as_str(), *e)),
            Some(("Fetch: cancelled", false))
        );
        assert_eq!(tab.undo_stack, vec![entry]);
        assert_eq!(tagged.as_deref(), Ok(""));
        assert!(tab.finished.is_empty(), "a cancel is no notification");
    }

    /// What is asked for while an operation runs runs after it, in order; twice the same is
    /// once, and past the limit it is refused with why.
    #[test]
    fn operations_asked_for_meanwhile_run_after_in_order() {
        let dir = repo("waiting");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let tag = |name: &str| ops(&[&["tag", name]]);
        tab.run_ops("Tag a", tag("a"), Refresh::Status, OpKind::plain(), &ctx);
        for name in ["b", "b", "c", "d"] {
            tab.run_ops(
                &format!("Tag {name}"),
                tag(name),
                Refresh::Status,
                OpKind::plain(),
                &ctx,
            );
        }
        let waiting: Vec<String> = tab.waiting.iter().map(|w| w.label.clone()).collect();
        tab.run_ops("Tag e", tag("e"), Refresh::Status, OpKind::plain(), &ctx);
        let refused_e = tab.status.clone();
        until(&mut tab, &ctx, |t| {
            t.busy_op.is_none() && t.waiting.is_empty()
        });
        let tags = git::run(Some(&dir), &["tag", "--sort=creatordate"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(waiting, ["Tag b", "Tag c", "Tag d"]);
        assert!(refused_e.is_some_and(|(s, err)| err && s.contains("Tag e: not started")));
        assert_eq!(
            tags.as_deref()
                .map(str::lines)
                .map(Iterator::collect::<Vec<_>>),
            Ok(vec!["a", "b", "c", "d"])
        );
    }

    /// When the running one fails, those waiting don't run, and the status says which.
    #[test]
    fn a_failure_drops_what_waited() {
        let dir = repo("waiting-fails");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let undo = Undo::None;
        tab.run_ops(
            "Checkout",
            ops(&[&["checkout", "no-such-branch"]]),
            Refresh::Status,
            OpKind::Normal(undo, String::new()),
            &ctx,
        );
        tab.run_ops(
            "Tag x",
            ops(&[&["tag", "x"]]),
            Refresh::Status,
            OpKind::plain(),
            &ctx,
        );
        until(&mut tab, &ctx, |t| t.busy_op.is_none());
        let tags = git::run(Some(&dir), &["tag"]);
        let _ = std::fs::remove_dir_all(&dir);
        let status = tab.status.map(|(s, _)| s).unwrap_or_default();
        assert!(status.starts_with("Checkout failed"), "{status}");
        assert!(status.ends_with("(not run: Tag x)"), "{status}");
        assert_eq!(tags.as_deref(), Ok(""));
    }
}
