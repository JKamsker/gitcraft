//! Fork conflict recovery, approval and operation settlement.

use super::*;

impl RepoTab {
    /// Once a rebase that stopped on conflicts is continued, asks to push it to the fork; once
    /// it is aborted, forgets it.
    pub(in crate::repo::forks) fn offer_push_after_conflict(&mut self) {
        let Some(scope) = self
            .forks
            .scope
            .as_ref()
            .filter(|s| s.check().is_ok() && s.generation == self.forge_generation)
        else {
            self.forks.after_conflict = None;
            return;
        };
        let id = scope.id;
        let Some(plan) = &self.forks.after_conflict else {
            return;
        };
        if self.work.in_progress.is_some() || self.loading || self.dialog.is_some() {
            return;
        }
        let plan = plan.clone();
        self.forks.after_conflict = None;
        let here = self.refs.current.as_deref() == Some(plan.local.as_str());
        if !here || self.refs.head == plan.head {
            return;
        }
        let tip = self.refs.head.clone();
        self.forks.push_tip = Some(tip.clone());
        let label = format!("Push the rebased {} to {}", plan.local, plan.fork);
        self.dialog = Some(
            Dialog::confirm(
                &label,
                crate::i18n::message(
                    "repo-workflows-forks-offer-push-after-conflict-dcc709d6",
                    &[
                        ("plan-local-1", (plan.local).clone().into()),
                        ("plan-fork-2", (plan.fork).clone().into()),
                        ("plan-branch-3", (plan.branch).clone().into()),
                    ],
                ),
                crate::i18n::text("repo-workflows-forks-offer-push-after-conflict-731ce7ed"),
                plan.push_now(&tip),
                None,
            )
            .instead_ops(crate::repo::Action::Fork(ForkAct::ConfirmPush(id)))
            .undo(Undo::Custom {
                undo: plan.fork_undo_steps(),
                redo: plan.push_now(&tip),
            }),
        );
        self.forks.scope_label = Some(label.clone());
        self.forks.running = Some((label, plan));
    }

    pub(in crate::repo::forks) fn approve_runs(&mut self, number: u64, ctx: &Context) {
        if self.forks.approving {
            return;
        }
        // Chosen once: say what it means. Chosen again: go ahead.
        if self.forks.approve_armed != Some(number) {
            self.forks.approve_armed = Some(number);
            self.status = Some(crate::repo::Notice::plain((
                format!(
                    "Approving runs the contributor's code on your CI: look through #{number}'s \
                     changes first, then choose Approve and run again"
                ),
                false,
            )));
            return;
        }
        self.forks.approve_armed = None;
        let Some(info) = self.forks.of(number) else {
            return;
        };
        let runs = self.forks.waiting(&info.sha).to_vec();
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        if runs.is_empty() {
            return;
        }
        let repo = self.path.clone();
        let Some(primary) = self
            .primary_forge_remote()
            .and_then(|name| self.remotes.iter().find(|r| r.name == name))
            .cloned()
        else {
            return;
        };
        let scope = guard::Scope::new(self.forge_generation, hosting.clone(), primary)
            .with_forge_lifetime(ctx, self.repo_key());
        self.replace_fork_scope(scope.clone());
        let label = format!("Approve #{number}'s workflow runs");
        self.forks.scope_label = Some(label.clone());
        self.busy_op = Some(label);
        self.op_started = Some(Instant::now());
        self.forks.approving = true;
        self.status = Some(crate::repo::Notice::plain((
            format!("Approving #{number}'s workflow runs…"),
            false,
        )));
        let sender = self.forge_operation_sender();
        #[cfg(test)]
        let mut recorded = tests::take_approval_recording();
        crate::repo::messages::spawn_mutation(
            &sender,
            ctx,
            move |e| approval_result(number, Err(e)),
            move || {
                let result: Result<(), String> = (|| {
                    for run in runs {
                        scope.check_remote(&repo)?;
                        #[cfg(test)]
                        if let Some(record) = &mut recorded {
                            record(run)?;
                            continue;
                        }
                        hosting.forge.provider().approve_run(&repo, &hosting, run)?;
                    }
                    Ok(())
                })();
                approval_result(number, result)
            },
        );
    }

    pub(in crate::repo) fn stale_fork_operation_finished(&mut self) {
        if self
            .forks
            .scope
            .as_ref()
            .is_some_and(|s| s.check().is_err())
        {
            self.forks.running = None;
            self.forks.scope = None;
            self.forks.scope_label = None;
            self.forks.approving = false;
        }
    }

    /// Takes in how an operation ended: a rebase whose push failed after rewriting the branch
    /// is made undoable here, and signing in pushes it.
    pub(in crate::repo) fn forks_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &Context,
    ) {
        let Some((staged, asked, plan)) = self.forks.take_rebase_completion(label) else {
            if result.is_ok() && self.forks.scope_label.as_deref() == Some(label) {
                self.forks.scope = None;
                self.forks.scope_label = None;
            }
            return;
        };
        // Existing combined-plan recovery remains for already captured work; the
        // new normal confirmation finishes its local-only stage here too.
        if result.is_ok() && !staged {
            return;
        }
        // Even an early failure/cancel has consumed this staged completion.
        // A raw retry of its local-only argv would have no continuation owner.
        if staged && self.last_op.as_ref().is_some_and(|op| op.label == asked) {
            self.last_op = None;
        }
        let scope = self.forks.scope.clone();
        let generation = self.forge_generation;
        if scope.as_ref().is_some_and(|s| s.check().is_err()) {
            self.last_op = None;
            if matches!(self.dialog, Some(Dialog::Credentials { .. })) {
                self.dialog = None;
            }
        }
        let Some(rewrite) = scope.as_ref().and_then(guard::Scope::rewrite) else {
            return;
        };
        if rewrite.branch != format!("refs/heads/{}", plan.local) || rewrite.before != plan.head {
            return;
        }
        let current = scope.as_ref().is_some_and(|s| {
            s.check().is_ok()
                && s.generation == self.forge_generation
                && generation == self.forge_generation
        });
        if rewrite.conflicts && current && result.is_err() {
            self.forks.after_conflict = Some(plan);
            self.status = Some(crate::repo::Notice::plain((
                "The rebase stopped on conflicts: resolve them and continue it, then gitcraft offers to push it (or abort it)".into(), true,
            )));
            return;
        }
        if rewrite.tip.is_empty() || rewrite.tip == plan.head {
            return;
        }
        let (undo, redo) = plan.local_undo(&rewrite.tip);
        self.push_undo(
            UndoEntry {
                id: 0,
                agent: None,
                label: format!("{asked} (here only)"),
                undo,
                redo,
                heads: None,
            },
            ctx,
        );
        if staged
            && plan
                .reviewed_tip(&rewrite, result.is_ok(), current)
                .is_some()
        {
            let pending = plan.push_pending(&rewrite.tip);
            self.forks.scope_label = Some(pending.label.clone());
            self.forks.running = Some((pending.label.clone(), plan));
            self.last_op = None; // A retry must never re-run the completed local rewrite.
            self.preflight_after = Some((asked, pending));
            return;
        }
        self.status = Some(crate::repo::Notice::plain((
            format!(
                "{} is rebased here, but not pushed to {}: Ctrl+Z puts it back as it was",
                plan.local, plan.fork,
            ),
            true,
        )));
        if (staged || !current) && self.last_op.as_ref().is_some_and(|o| o.label == asked) {
            self.last_op = None;
        }
        if !staged
            && result
                .as_ref()
                .is_err_and(|error| crate::git::is_auth_error(error))
            && current
            && let Some(op) = &mut self.last_op
            && op.label == asked
        {
            let label = format!("Push {} to {}", plan.local, plan.fork);
            self.forks.scope_label = Some(label.clone());
            *op = PendingOp {
                label,
                ops: plan.push_now(&rewrite.tip),
                refresh: Refresh::All,
                kind: OpKind::Normal(
                    Undo::Custom {
                        undo: plan.fork_undo_steps(),
                        redo: plan.push_now(&rewrite.tip),
                    },
                    String::new(),
                ),
                quiet: false,
            };
        }
    }
}
