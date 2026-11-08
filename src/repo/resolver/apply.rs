//! Applying a run's files: written into the checkout, then marked resolved, as one operation of
//! the tab's (nothing else runs on the worktree from the first write to the staging). The run
//! says they are applied once they are staged; a file that couldn't be written stays, with the
//! agent's work on it, to apply again.

use super::{Phase, clean_up, done_files};
use crate::repo::{Action, OpKind, Refresh, RepoTab};
use crate::resolver::workspace::Applied;

/// What the tab says it is doing while the files are written.
const APPLYING: &str = "Apply the agent's files";

/// The operation that marks them resolved.
const MARK_RESOLVED: &str = "Mark resolved (agent)";

impl RepoTab {
    pub(super) fn apply_resolved(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some(("Wait for the running operation to finish".to_owned(), true));
            return;
        }
        let Some(run) = self
            .resolver
            .run
            .as_mut()
            .filter(|r| r.phase == Phase::Ready)
        else {
            return;
        };
        let Some(ws) = run.workspace.clone() else {
            return;
        };
        let picked: Vec<String> = run.picked.iter().cloned().collect();
        if picked.is_empty() {
            return;
        }
        run.phase = Phase::Applying;
        let (repo, id) = (self.path.clone(), run.id.clone());
        let done = done_files(&run.progress);
        self.busy_op = Some(APPLYING.to_owned());
        self.then(
            ctx,
            move || ws.apply(&repo, &picked, &done),
            move |tab, applied, ctx| tab.resolved_applied(&id, applied, ctx),
        );
    }

    /// The files are written: those that were are marked resolved; those that couldn't be stay
    /// to apply again.
    fn resolved_applied(&mut self, id: &str, applied: Applied, ctx: &egui::Context) {
        if self.busy_op.as_deref() == Some(APPLYING) {
            self.busy_op = None;
        }
        let Some(run) = self.resolver.run.as_mut().filter(|r| r.id == id) else {
            return;
        };
        let Applied { written, failed } = applied;
        run.applied += written.len();
        run.review = None;
        if failed.is_empty() {
            run.error = None;
            clean_up(run);
        } else {
            let each: Vec<String> = failed.iter().map(|(p, e)| format!("{p}: {e}")).collect();
            run.error = Some(format!("Not applied: {}", each.join("; ")));
            // The agent's work on them is kept: applied again once what stopped it is fixed.
            run.results
                .retain(|(p, _)| failed.iter().any(|(f, _)| f == p));
            run.picked = failed.into_iter().map(|(p, _)| p).collect();
        }
        // Its workspace kept: files that couldn't be written, to apply again.
        let left = run.workspace.is_some();
        if written.is_empty() {
            run.phase = if left { Phase::Ready } else { Phase::Applied };
            if !left {
                self.status = Some((
                    "Nothing was applied: the files changed here since the agent started"
                        .to_owned(),
                    true,
                ));
            }
            return;
        }
        self.close_applied_editor(&written);
        let mut add = ["--literal-pathspecs", "add", "-A", "--"]
            .map(str::to_owned)
            .to_vec();
        add.extend(written);
        self.run_ops(
            MARK_RESOLVED,
            vec![add],
            Refresh::Status,
            OpKind::plain(),
            ctx,
        );
        if self.busy_op.as_deref() != Some(MARK_RESOLVED) {
            // Not started (the status line says why).
            self.resolver_staged(Err("it couldn't start".to_owned()));
        }
    }

    /// The conflict editor of a file just written shows it as it was: it is closed, unless it
    /// holds choices of the user's not saved, which stay (saving them writes over the agent's).
    fn close_applied_editor(&mut self, written: &[String]) {
        let Some(ed) = self
            .conflict
            .as_ref()
            .filter(|ed| written.contains(&ed.path))
        else {
            return;
        };
        if ed.has_draft() {
            self.status = Some((
                format!(
                    "{} has the agent's version now; its conflict editor keeps your choices, \
                     not saved: close it to keep the agent's, or save to write yours over it",
                    ed.path
                ),
                true,
            ));
        } else {
            self.queued.push(Action::CloseDiff);
        }
    }

    /// An operation of the tab's ended: when it marked the agent's files resolved, the run says
    /// how that went.
    pub(in crate::repo) fn resolver_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
    ) {
        if label == MARK_RESOLVED {
            self.resolver_staged(result.as_ref().map(|_| ()).map_err(Clone::clone));
        }
    }

    fn resolver_staged(&mut self, staged: Result<(), String>) {
        let Some(run) = self
            .resolver
            .run
            .as_mut()
            .filter(|r| r.phase == Phase::Applying)
        else {
            return;
        };
        if let Err(e) = staged {
            run.unstaged = true;
            let said = run
                .error
                .take()
                .map(|e| format!("{e}. "))
                .unwrap_or_default();
            run.error = Some(format!(
                "{said}Written, but not marked resolved ({e}): mark them resolved under \
                 CONFLICTED"
            ));
        }
        // Files that couldn't be written are there to apply again (its workspace kept).
        let left = run.workspace.is_some();
        run.phase = if left && !run.unstaged {
            Phase::Ready
        } else {
            Phase::Applied
        };
    }
}
