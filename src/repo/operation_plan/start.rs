use super::*;

impl RepoTab {
    /// Runs git commands one after another (stopping at the first failure), then refreshes.
    pub(in crate::repo) fn run_ops(
        &mut self,
        label: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        kind: OpKind,
        ctx: &egui::Context,
    ) {
        self.run_ops_ex(label, ops, refresh, kind, false, ctx);
    }

    pub(in crate::repo) fn run_ops_ex(
        &mut self,
        label: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        kind: OpKind,
        quiet: bool,
        ctx: &egui::Context,
    ) {
        // Also protect direct/background callers that do not enter through apply().
        if self.snapshot_pending() {
            return;
        }
        if quiet && self.echo_push_pending() {
            return;
        }
        if let Some(busy) = &self.busy_op {
            // Fetch, Pull and Push wait for it (see `running::Waiter`); anything else is not
            // started, and says so. A quiet one (the background fetch) just comes again later.
            if !quiet {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-2947",
                        &[
                            (
                                "label",
                                crate::repo::messages::display_operation(label).into(),
                            ),
                            (
                                "busy",
                                crate::repo::messages::display_operation(busy).into(),
                            ),
                        ],
                    ),
                    true,
                )));
            }
            return;
        }
        // A push asked to skip its hooks, this once (set only while that push starts).
        let ops = self.skip_hooks.push_steps(ops);
        if let Some(why) = offline_refusal(&ops) {
            if !quiet {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-2948",
                        &[
                            (
                                "label",
                                crate::repo::messages::display_operation(label).into(),
                            ),
                            ("why", why.to_string().into()),
                        ],
                    ),
                    true,
                )));
            }
            return;
        }
        let ordinary_push = self.push_flow.ordinary_attempt(label)
            && self.preflight_selection.is_none()
            && matches!(kind, OpKind::Normal(..));
        if crate::push_preflight::has_push(&ops) && !ordinary_push {
            if let Some(at) = ops
                .iter()
                .position(|s| crate::push_preflight::has_push(std::slice::from_ref(s)))
                && at > 0
                && ops[..at]
                    .iter()
                    .all(|s| is_expectation(s) || s.first().is_some_and(|s| s == "tag"))
                && ops[..at]
                    .iter()
                    .any(|s| s.first().is_some_and(|s| s == "tag"))
            {
                let follow = PendingOp {
                    label: format!("Push after {label}"),
                    ops: ops[at..].to_vec(),
                    refresh,
                    kind: OpKind::plain(),
                    quiet,
                };
                self.preflight_after = Some((label.to_owned(), follow));
                self.run_ops_ex(label, ops[..at].to_vec(), refresh, kind, quiet, ctx);
                return;
            }
            self.preflight_pending = Some(PendingOp {
                label: label.to_owned(),
                ops: ops.clone(),
                refresh,
                kind,
                quiet,
            });
            self.preflight_timeline = if matches!(
                self.preflight_pending.as_ref().map(|p| &p.kind),
                Some(OpKind::Undo(_) | OpKind::Redo(_))
            ) {
                Some(self.timeline.current)
            } else {
                None
            };
            self.begin_push_preflight(ops, ctx);
            return;
        }
        if self.echo_before_operation(
            PendingOp {
                label: label.to_owned(),
                ops: ops.clone(),
                refresh,
                kind: kind.clone(),
                quiet,
            },
            ctx,
        ) {
            return;
        }
        self.graph_motion_begin(&ops);
        self.celebration_begin(label, &ops, ctx);
        let observe_celebration_head = self.celebration_needs_head();
        self.busy_op = Some(label.to_owned());
        self.op_started = Some(Instant::now());
        let control = self.follow_op(&ops, quiet, ctx);
        let cancellable = self.can_cancel();
        self.timeline_op_started(quiet, refresh);
        self.note_checkout_move(&ops, ctx);
        self.note_worktree_add(label, &ops);
        if !quiet {
            self.status = None;
            // A fix made stays, for its undo, while what it fixed runs again.
            self.error_card.take_if(|card| !card.fixed());
        }
        // A quiet one (the background fetch) never asks to sign in, and must not take the place
        // of the operation that did while its sign-in dialog is open.
        if !quiet {
            self.invalidate_https_challenge();
            self.last_op = Some(PendingOp {
                label: label.to_owned(),
                ops: ops.clone(),
                refresh,
                kind: kind.clone(),
                quiet,
            });
        }
        // Nor does a credential helper on its behalf (`run_steps_counted` runs it unattended): Git
        // Credential Manager would otherwise open its sign-in window every few minutes while a
        // token has expired.
        let path = self.path.clone();
        let fork_scope = self.fork_execution_scope(label);
        let label = label.to_owned();
        let creds = self.credentials.clone();
        // Ended all the same if it panics: the tab stays busy until then.
        let failed = {
            let (label, kind) = (label.clone(), kind.clone());
            move |e| Msg::OpDone {
                label,
                result: Err(e),
                refresh,
                kind,
                quiet,
                stopped: 0,
            }
        };
        let mut operation_tx = self.tx.clone();
        operation_tx.cancel = control.cancellation();
        spawn_mutation(&operation_tx, ctx, failed, move || {
            // The locks there already belong to someone else: only new ones are the cancel's.
            let locks_before = if cancellable {
                git::leftover_locks(&path)
            } else {
                Vec::new()
            };
            let celebration_before = observe_celebration_head.then(|| git::head(&path));
            let (result, kind, stopped) = run_steps_guarded(
                &path,
                &ops,
                kind,
                creds.as_ref(),
                quiet,
                Some(&control),
                fork_scope.as_ref(),
            );
            let result = match result {
                Err(e) if control.is_cancelled() && !e.contains("CONFLICT") => {
                    Err(cancelled_message(&path, &ops, &locks_before))
                }
                result => result,
            };
            // Pin the successful action's target before another queued action
            // can start. This read stays on the owned mutation worker.
            let observed = (result.is_ok() && observe_celebration_head).then(|| git::head(&path));
            let done = Msg::OpDone {
                label,
                result,
                refresh,
                kind,
                quiet,
                stopped,
            };
            if let Some(head) = observed {
                Msg::Then(Box::new(move |tab, ctx| {
                    tab.celebration_observed_heads(celebration_before.unwrap_or_default(), head);
                    tab.handle(done, ctx);
                }))
            } else {
                done
            }
        });
    }
}
