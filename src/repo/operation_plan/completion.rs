use super::*;

impl RepoTab {
    /// Takes in how an operation ended: the status line, undo history, sign-in, a notification
    /// for a long one, then re-reads what it changed (and updates the submodules after a
    /// checkout, if asked to).
    pub(in crate::repo) fn op_done(
        &mut self,
        label: String,
        result: Result<String, String>,
        refresh: Refresh,
        kind: OpKind,
        quiet: bool,
        ctx: &egui::Context,
    ) {
        self.graph_motion_done(result.is_ok());
        self.celebration_done(result.is_ok());
        self.busy_op = None;
        self.detached_op_done(&label, result.is_ok());

        self.dropped_patch_done(&label, &result);
        // Done with it before anything starts after it (updating the submodules).
        let running = self.running.take();
        let cancelled =
            result.is_err() && running.as_ref().is_some_and(|r| r.control.is_cancelled());
        // For the taskbar button, which turns red.
        if result.is_err()
            && !cancelled
            && running.as_ref().is_some_and(|r| r.network && !r.background)
        {
            self.network_failed = true;
        }
        let auth_failure = running.as_ref().and_then(|r| r.control.auth_failure());
        let stack_failure = running.as_ref().and_then(|r| r.control.stack_failure());
        let steps = running.map(|r| r.control.take_steps()).unwrap_or_default();
        let failure = result
            .as_ref()
            .err()
            .map(|error| Failure::capture(error, stack_failure.clone()));
        self.log_activity_presented(&label, &result, cancelled, quiet, steps, failure);
        let long = self
            .op_started
            .take()
            .is_some_and(|t| t.elapsed() > LONG_OP && !cancelled);
        match &result {
            Ok(_) => {
                self.note_reused(None);
                if label == "Commit" || label == "Amend" || label.starts_with("Continue ") {
                    self.commit_summary.clear();
                    self.commit_body.clear();
                    self.amend = false;
                    self.compose.committed();
                    self.ai_committed();
                }
                // Quiet success for staging, which happens all the time.
                if let Ok(out) = &result
                    && let Some(note) = tried_note(out)
                    && !quiet
                {
                    self.status = Some(crate::repo::Notice::operation(OperationStatus::Done {
                        label: label.clone(),
                        note: Some(note.to_owned()),
                    }));
                } else if refresh == Refresh::All && !quiet {
                    self.status = Some(crate::repo::Notice::operation(OperationStatus::Done {
                        label: label.clone(),
                        note: None,
                    }));
                }
                if label.contains("etch") {
                    self.note_fetched();
                    self.load_prs(ctx);
                }
                self.locks_after_push(&label, ctx);
                if let Some((_, path)) = self.open_when_done.take_if(|(l, _)| *l == label) {
                    self.open_request = Some(crate::repo::OpenRequest::Worktree(path));
                }
                self.note_rewrite(&label, &kind, ctx);
            }
            Err(e) if cancelled => {
                self.open_when_done.take_if(|(l, _)| *l == label);
                let left_locks = e.starts_with(&format!("{}, but", git::CANCELLED));
                self.status = Some(crate::repo::Notice::operation(OperationStatus::Cancelled {
                    label: label.clone(),
                    error: Failure::capture(e, None),
                    left_locks,
                }));
            }
            Err(e) if git::is_auth_error(e) && !quiet => {
                self.agent_needs_sign_in(&label);
                // Typed credentials that were refused are forgotten.
                let typed = self.credentials.take().is_some();
                let refused = typed || e.contains("401") || e.contains("403");
                self.status = Some(crate::repo::Notice::operation(OperationStatus::SignIn {
                    label: label.clone(),
                }));
                // Only a URL reported by the failed step, or its captured destinations.
                // An absent endpoint must never be replaced with the default fetch URL.
                if crate::push_preflight::is_reviewed_auth_error(e) && auth_failure.is_none() {
                    self.invalidate_https_challenge();
                    self.status = Some(crate::repo::Notice::operation(
                        OperationStatus::MissingSignInProof,
                    ));
                } else {
                    let failure =
                        auth_failure.unwrap_or_else(|| git::AuthFailure::from_error(e, &[]));
                    self.offer_failed_https_signin(failure, refused, ctx);
                }
            }
            Err(e) => {
                self.note_reused(Some(e));
                self.open_when_done.take_if(|(l, _)| *l == label);
                self.focus_wip = e.contains("CONFLICT") || e.contains("could not apply");
                // A failed undo stays undoable: the timeline stays where it was.
                self.status = Some(crate::repo::Notice::operation(OperationStatus::Failed {
                    label: label.clone(),
                    error: Failure::capture(e, stack_failure.clone()),
                }));
                if !quiet && !self.offer_blocked_checkout(&label, e) {
                    self.explain_failure(e, &label, ctx);
                }
                self.note_stopped_rewrite(&label, &kind, e);
            }
        }
        if !quiet {
            let ok = result.is_ok();
            let mut undo = if ok { undo_entry(&label, &kind) } else { None };
            if let Some(undo) = &mut undo {
                undo.agent = self.operation_agent(&label);
            }
            self.agent_op_done(&label, &result, undo.is_some());
            self.timeline_op_ended(&label, &kind, undo, ok, refresh, ctx);
        }
        if long && !quiet {
            self.finished.push(crate::repo::Notice::operation_finished(
                &self.name,
                &label,
                &result,
                stack_failure,
            ));
        }
        self.fold_op_done(&label, &result, ctx);
        self.dco_op_done(&label, &result, ctx);
        self.forks_op_done(&label, &result, ctx);
        self.hotfix_op_done(&label, &result, ctx);
        self.resolver_op_done(&label, &result);
        match refresh {
            Refresh::Status => self.reload_status(ctx),
            Refresh::All => self.reload(ctx),
        }
        self.follow_checkout_move(result.is_ok(), ctx);
        // Last: it may start the next operation (Sync's push after its pull).
        self.push_op_ended(&label, &result, ctx);
        self.after_op(result.is_ok(), quiet, ctx);
        self.link_status();
        self.after_worktree_add(&label, result.is_ok(), ctx);
        if let Some((_, pending)) = self.preflight_after.take_if(|(after, _)| after == &label)
            && result.is_ok()
        {
            self.run_ops_ex(
                &pending.label,
                pending.ops,
                pending.refresh,
                pending.kind,
                pending.quiet,
                ctx,
            );
        }
    }
}

impl RepoTab {
    pub(in crate::repo) fn record_undo(
        &mut self,
        label: &str,
        kind: OpKind,
        ctx: &egui::Context,
    ) -> bool {
        let Some(undo) = undo_entry(label, &kind) else {
            return false;
        };
        self.remember_undo(label, undo, ctx);
        true
    }
}

/// How to undo and redo what an operation of `kind` did, when it can be: a HEAD move is reset
/// back (and forward), only while HEAD is on the branch it moved.
pub(in crate::repo) fn undo_entry(label: &str, kind: &OpKind) -> Option<UndoEntry> {
    match kind {
        OpKind::Normal(Undo::None, _) | OpKind::Undo(_) | OpKind::Redo(_) => None,
        OpKind::Normal(Undo::HeadMove { mode }, heads) => {
            let mut parts = heads.split('\u{0}');
            let (before, after) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            if before.is_empty() || after.is_empty() || before == after {
                return None;
            }
            // Resetting moves whatever branch is checked out: another one at the same
            // commit passes the check of HEAD's commit, so the branch is checked too.
            let guard: Vec<Vec<String>> = parts
                .next()
                .map(|branch| vec![expect_symref("HEAD", branch)])
                .unwrap_or_default();
            let reset = |to: &str| [guard.clone(), ops(&[&["reset", mode, to]])].concat();
            Some(UndoEntry {
                id: 0,
                agent: None,
                label: label.to_owned(),
                undo: reset(before),
                redo: reset(after),
                heads: Some((before.to_owned(), after.to_owned())),
            })
        }
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            id: 0,
            agent: None,
            label: label.to_owned(),
            undo: undo.clone(),
            redo: redo.clone(),
            heads: None,
        }),
    }
}

#[cfg(test)]
mod settlement_tests {
    use super::*;
    #[test]
    fn successful_mutation_remains_settled_when_the_following_reload_fails() {
        use crate::repo::timeline::{Awaiting, Ended};
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recording-only/mutation-settlement".into(), &ctx);
        tab.life.phase = crate::repo::lifecycle::Phase::Live;
        tab.busy_op = Some("Synthetic mutation".into());
        let kind = OpKind::Normal(
            Undo::Custom {
                undo: ops(&[&["branch", "-D", "created"]]),
                redo: ops(&[&["branch", "created", "tip"]]),
            },
            String::new(),
        );
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.op_done(
                "Synthetic mutation".into(),
                Ok(String::new()),
                Refresh::All,
                kind,
                false,
                &ctx,
            );
        });
        assert!(
            !jobs.is_empty(),
            "reload should be submitted, never executed by this fixture"
        );
        assert!(tab.busy_op.is_none());
        assert!(tab.running.is_none());
        assert!(matches!(
            &tab.timeline.awaiting,
            Awaiting::Ended {
                then: Ended::New { undo: Some(_), .. },
                ..
            }
        ));
        let settled = tab.timeline.awaiting.clone();
        crate::repo::messages::record_jobs(|| {
            tab.loaded(Err("reload failed after mutation".into()), &ctx);
        });
        assert_eq!(tab.timeline.awaiting, settled);
        assert!(tab.busy_op.is_none());
        assert_eq!(
            tab.load_error.as_deref(),
            Some("reload failed after mutation")
        );
    }
}
