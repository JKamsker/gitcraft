//! Running git commands in the background, taking in how they ended, and undo / redo.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::messages::spawn_mutation;
use super::notice::operation::{Failure, Status as OperationStatus};
use super::toolbar::headline;
use super::{Dialog, Msg, OpKind, PendingOp, Refresh, RepoTab, Undo, UndoEntry, ops};
use crate::git;

/// An operation that takes longer than this gets a notification when it ends.
const EXPECT_HTTPS_ROUTE: &str = "--gitcraft-expect-https-route";
pub(super) const LONG_OP: Duration = Duration::from_secs(4);

impl RepoTab {
    /// Runs git commands one after another (stopping at the first failure), then refreshes.
    pub(super) fn run_ops(
        &mut self,
        label: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        kind: OpKind,
        ctx: &egui::Context,
    ) {
        self.run_ops_ex(label, ops, refresh, kind, false, ctx);
    }

    pub(super) fn run_ops_ex(
        &mut self,
        label: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        kind: OpKind,
        quiet: bool,
        ctx: &egui::Context,
    ) {
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
        if crate::push_preflight::has_push(&ops) {
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

    /// Leaves out of the operation kept to run again after signing in the steps that ran
    /// before step `stopped` failed: they are done, and most would fail a second time (a tag or
    /// remote added again, a branch deleted again), so that the step that needed signing in
    /// would never run.
    pub(super) fn keep_unfinished(&mut self, stopped: usize) {
        if let Some(op) = &mut self.last_op {
            retain_unfinished(&mut op.ops, stopped);
        }
    }

    /// Runs `ops` as an undoable step.
    pub(super) fn run_undoable(
        &mut self,
        label: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        undo: Undo,
        ctx: &egui::Context,
    ) {
        let before = self.refs.head.clone();
        self.run_ops(label, ops, refresh, OpKind::Normal(undo, before), ctx);
    }

    /// Clear all views of this endpoint, including parked worktrees. A cached
    /// transient secret already has an exact scope; fetch URL guesses are not needed.
    pub(crate) fn forget_https_memory(&mut self, path: &std::path::Path, url: &str) {
        if self.path == path
            || self
                .credentials
                .as_ref()
                .is_some_and(|c| c.scope.as_ref().is_some_and(|scope| scope.matches_url(url)))
        {
            let had_signin = self.credentials.is_some()
                || self.https_challenge.is_some()
                || matches!(self.dialog, Some(Dialog::Credentials { .. }));
            self.credentials = None;
            if had_signin {
                self.invalidate_https_challenge();
            }
        }
        for parked in &mut self.parked {
            parked.forget_https_memory(path, url);
        }
    }
    pub(super) fn switch_https(&mut self, url: String, ctx: &egui::Context) {
        if !crate::settings::network_page::valid_https(&url) {
            return;
        }
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::message(
                    "repo-core-2950",
                    &[(
                        "busy",
                        crate::repo::messages::display_operation(busy).into(),
                    )],
                ),
                true,
            )));
            return;
        }
        let challenge = self
            .https_challenge
            .as_ref()
            .filter(|ticket| self.https_ticket_current(ticket))
            .cloned();
        let Some(binding) = challenge.as_ref().and_then(|ticket| ticket.binding.clone()) else {
            self.invalidate_https_challenge();
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2951").into(),
                true,
            )));
            return;
        };
        if !challenge
            .as_ref()
            .is_some_and(|ticket| ticket.authorizes_endpoint(&url))
        {
            self.inspect_unbound_https(vec![url], ctx);
            return;
        }
        let path = self.path.clone();
        self.forget_https_memory(&path, &url);
        crate::settings::network_page::rejected(ctx, path.clone(), url.clone());
        let mut ticket = self.https_ticket(url.clone());
        ticket.binding = Some(binding.clone());
        let reads = self.reads;
        self.then_or(
            ctx,
            true,
            |error| (Err(error), false),
            move || {
                if let Err(error) = binding.verify(&path) {
                    return (Err(error), false);
                }
                let result = git::https_reject(&path, &url);
                let route_current = binding.verify(&path).is_ok();
                (result, route_current)
            },
            move |tab, (result, route_current), _| {
                if tab.reads != reads || !route_current {
                    tab.invalidate_https_challenge();
                }
                tab.https_rejection_done(ticket, result);
            },
        );
    }

    /// Remembers the credentials and runs the operation that needed them again.
    pub(super) fn sign_in(&mut self, creds: git::Credentials, ctx: &egui::Context) {
        let challenge = self.https_challenge.take();
        if !challenge.as_ref().is_some_and(|ticket| {
            self.https_ticket_current(ticket)
                && creds
                    .scope
                    .as_ref()
                    .is_some_and(|scope| ticket.endpoints.iter().any(|url| scope.matches_url(url)))
        }) {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2952").into(),
                true,
            )));
            return;
        }
        let Some(ticket) = challenge else {
            return;
        };
        let Some(binding) = ticket.binding.clone() else {
            self.invalidate_https_challenge();
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2953").into(),
                true,
            )));
            return;
        };
        let selected = ticket.endpoints.iter().find(|url| {
            creds
                .scope
                .as_ref()
                .is_some_and(|scope| scope.matches_url(url))
        });
        if !selected.is_some_and(|url| ticket.authorizes_endpoint(url)) {
            self.inspect_unbound_https(selected.cloned().into_iter().collect(), ctx);
            return;
        }
        self.https_checking = true;
        self.status = Some(crate::repo::Notice::plain((
            crate::i18n::text("repo-core-2954").into(),
            false,
        )));
        let path = self.path.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || binding.verify(&path),
            move |tab, result, ctx| {
                if !tab.https_ticket_current(&ticket) {
                    tab.status = Some(crate::repo::Notice::plain((
                        crate::i18n::text("repo-core-2955").into(),
                        true,
                    )));
                    return;
                }
                tab.https_checking = false;
                if let Err(error) = result {
                    tab.invalidate_https_challenge();
                    tab.status = Some(crate::repo::Notice::plain((error, true)));
                    return;
                }
                if let Some(binding) = ticket.binding
                    && let Some(op) = &mut tab.last_op
                {
                    match serde_json::to_string(&binding) {
                        Ok(json) => op.ops.insert(0, vec![EXPECT_HTTPS_ROUTE.into(), json]),
                        Err(_) => {
                            tab.status = Some(crate::repo::Notice::plain((
                                crate::i18n::text("repo-core-2956").into(),
                                true,
                            )));
                            return;
                        }
                    }
                }
                tab.accept_scoped_signin(creds, ctx);
            },
        );
    }
    fn accept_scoped_signin(&mut self, creds: git::Credentials, ctx: &egui::Context) {
        self.credentials = Some(creds);
        if let Some(busy) = &self.busy_op
            && let Some(op) = &self.last_op
        {
            // It would not start: say so, rather than drop it.
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::message(
                    "repo-core-2957",
                    &[
                        (
                            "label",
                            crate::repo::messages::display_operation(&op.label).into(),
                        ),
                        (
                            "busy",
                            crate::repo::messages::display_operation(busy).into(),
                        ),
                    ],
                ),
                true,
            )));
        } else {
            self.run_last_op_again(ctx);
        }
    }

    /// Runs the last operation started again (what of it didn't run, after a failure).
    pub(super) fn run_last_op_again(&mut self, ctx: &egui::Context) {
        if let Some(op) = self.last_op.take() {
            self.run_ops_ex(&op.label, op.ops, op.refresh, op.kind, op.quiet, ctx);
        }
    }

    /// Takes in how an operation ended: the status line, undo history, sign-in, a notification
    /// for a long one, then re-reads what it changed (and updates the submodules after a
    /// checkout, if asked to).
    pub(super) fn op_done(
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
                    self.open_request = Some(super::OpenRequest::Worktree(path));
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
                if !quiet {
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
    pub(super) fn record_undo(&mut self, label: &str, kind: OpKind, ctx: &egui::Context) -> bool {
        let Some(undo) = undo_entry(label, &kind) else {
            return false;
        };
        self.remember_undo(label, undo, ctx);
        true
    }
}

/// How to undo and redo what an operation of `kind` did, when it can be: a HEAD move is reset
/// back (and forward), only while HEAD is on the branch it moved.
fn undo_entry(label: &str, kind: &OpKind) -> Option<UndoEntry> {
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

/// What each did is kept in `control`, for the activity log.
fn run_step(
    path: &Path,
    args: &[&str],
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
) -> Result<String, String> {
    let Some(control) = control else {
        return if quiet {
            git::run_unattended(Some(path), args, creds)
        } else {
            git::run_naming_hooks(Some(path), args, creds)
        };
    };
    if control.is_cancelled() {
        return Err(git::CANCELLED.to_owned());
    }
    let binding = if git::goes_over_network(args) {
        match git::RouteBinding::capture(path, args) {
            Ok(binding) => Some(binding),
            Err(error) => {
                return Err(crate::i18n::message(
                    "repo-core-2967",
                    &[(
                        "error",
                        crate::diagnostics::crash::scrub(&error).to_string().into(),
                    )],
                ));
            }
        }
    } else {
        None
    };
    let endpoints = binding
        .as_ref()
        .map(|b| b.urls().to_vec())
        .unwrap_or_default();
    // Each call kept for the activity log.
    let (result, recorded) = if git::goes_over_network(args) {
        git::run_watched_recorded(Some(path), args, creds, quiet, control)
    } else {
        git::run_recorded(Some(path), args, creds, quiet)
    };
    if let Err(error) = &result
        && git::is_auth_error(error)
    {
        let mut failure = git::AuthFailure::from_error(error, &endpoints);
        failure.binding = binding;
        control.authentication_failed(failure);
    }
    let mut recorded = recorded;
    recorded.output = super::activity::bounded(&recorded.output);
    control.record(recorded);
    result
}

/// Reviewed dispatch bypasses run_step's normal argv, but retains its original-remote
/// route proof. The runtime private alias must never become the retry authorization.
fn reviewed_operands(op: &[String]) -> Result<(&str, Option<&[String]>), String> {
    match op {
        [marker, id] if marker == crate::push_preflight::APPROVED => Ok((id, None)),
        [marker, id, option, binding]
            if marker == crate::push_preflight::APPROVED
                && option == "-c"
                && binding.starts_with("gitcraft.commandSource=") =>
        {
            Ok((id, Some(&op[2..])))
        }
        _ => Err(crate::i18n::text("repo-core-2969").into()),
    }
}

/// Retrieve the immutable exact argv behind APPROVED; the private marker is
/// still interpreted only here, with its original strict operands.
fn reviewed_push_args(op: &[String]) -> Result<Option<Vec<String>>, String> {
    if op
        .first()
        .is_none_or(|mark| mark != crate::push_preflight::APPROVED)
    {
        return Ok(None);
    }
    let (id, _) = reviewed_operands(op)?;
    let plan = crate::push_preflight::registered(id)?;
    crate::push_preflight::push_args(&plan).map(Some)
}

fn retain_unfinished(ops: &mut Vec<Vec<String>>, stopped: usize) {
    // The checks and opaque source owner stay with the original operation on sign-in retry.
    let mut i = 0;
    ops.retain(|step| {
        i += 1;
        i > stopped || is_expectation(step) || step.first().is_some_and(|m| m == EXPECT_HTTPS_ROUTE)
    });
}

#[cfg(test)]
#[test]
fn merge_pure_reviewed_echo_operands_are_strict_and_survive_retry() -> Result<(), String> {
    let plain = vec![crate::push_preflight::APPROVED.to_owned(), "plan".into()];
    assert_eq!(reviewed_operands(&plain)?, ("plan", None));
    let bound = [
        plain.clone(),
        vec!["-c".into(), "gitcraft.commandSource=owned".into()],
    ]
    .concat();
    let (id, source) = reviewed_operands(&bound)?;
    assert_eq!(id, "plan");
    assert_eq!(source, Some(&bound[2..]));
    for malformed in [
        [plain.clone(), vec!["-c".into()]].concat(),
        [
            plain.clone(),
            vec!["-c".into(), "http.proxy=foreign".into()],
        ]
        .concat(),
        [bound.clone(), vec!["extra".into()]].concat(),
    ] {
        assert!(reviewed_operands(&malformed).is_err());
    }
    // Credential recovery retains internal steps, including their opaque source owner.
    let mut retained = vec![bound.clone()];
    retain_unfinished(&mut retained, 0);
    assert_eq!(retained, vec![bound]);
    Ok(())
}

pub(crate) fn run_reviewed_push(
    original: &[String],
    capture: impl FnOnce(&[&str]) -> Result<git::RouteBinding, String>,
    execute: impl FnOnce() -> Result<String, String>,
    control: Option<&Arc<git::OpControl>>,
) -> Result<String, String> {
    let context = crate::connections::current();
    let args: Vec<_> = original.iter().map(String::as_str).collect();
    let binding = capture(&args).map_err(|error| {
        crate::i18n::message(
            "repo-core-4157",
            &[(
                "error",
                crate::diagnostics::crash::scrub(&error).to_string().into(),
            )],
        )
    })?;
    let result = execute();
    if let Err(error) = &result
        && let Some(failure) = reviewed_auth_failure(error, binding, context)
        && let Some(control) = control
    {
        control.authentication_failed(failure);
    }
    result
}

/// A one-shot reviewed receipt is trusted only inside the captured original route.
/// Never parse the opaque diagnostic as a canonical Git URL or guess a fetch URL.
fn reviewed_auth_failure(
    error: &str,
    binding: git::RouteBinding,
    context: crate::connections::Context,
) -> Option<git::AuthFailure> {
    let endpoint = crate::push_preflight::take_auth_endpoint(error)?;
    if !binding.authorizes_endpoint(&endpoint) {
        return None;
    }
    Some(git::AuthFailure {
        context,
        endpoints: vec![endpoint],
        confirmed: true,
        binding: Some(binding),
        lfs: false,
    })
}

/// Route proofs participate in the same before/after-draft verifier as HEAD and refs.
fn check_https_routes(
    ops: &[Vec<String>],
    mut verify: impl FnMut(&git::RouteBinding) -> Result<(), String>,
) -> Result<(), String> {
    for op in ops {
        if let [marker, json] = op.as_slice()
            && marker == EXPECT_HTTPS_ROUTE
        {
            let binding = serde_json::from_str::<git::RouteBinding>(json)
                .map_err(|_| crate::i18n::text("repo-core-4158").to_owned())?;
            verify(&binding)?;
        }
    }
    Ok(())
}

/// What a cancelled operation says: that it was, with what it may have left behind: the lock
/// files a stopped git couldn't remove (not those `before` it started: they are someone
/// else's), and for a pull, a merge or rebase it had started.
fn cancelled_message(path: &Path, ops: &[Vec<String>], before: &[std::path::PathBuf]) -> String {
    let mut msg = git::CANCELLED.to_owned();
    let locks: Vec<String> = git::leftover_locks(path)
        .into_iter()
        .filter(|l| !before.contains(l))
        .map(|l| {
            l.strip_prefix(path)
                .map_or_else(|_| l.display().to_string(), |r| r.display().to_string())
        })
        .collect();
    if !locks.is_empty() {
        let (them, are) = if locks.len() == 1 {
            ("it", "is")
        } else {
            ("them", "are")
        };
        msg.push_str(&format!(
            ", but git left {} behind\nUntil {them} {are} deleted, git refuses to work in this \
             repository. Delete {them} if no other git program is running here.",
            locks.join(", ")
        ));
    }
    let pulls = ops.iter().any(|op| {
        op.iter().any(|w| w == "pull")
            && git::goes_over_network(&op.iter().map(String::as_str).collect::<Vec<_>>())
    });
    if pulls {
        msg.push_str(
            "\nIf the pull had started merging or rebasing, that is left in progress: Abort in \
             the toolbar goes back to where the branch was.",
        );
    }
    msg
}

/// Why `ops` don't run now: one of them goes on the network, and offline mode is on.
pub(super) fn offline_refusal(ops: &[Vec<String>]) -> Option<String> {
    let pause = crate::network::refused()?;
    let network = ops.iter().any(|op| reaches_a_remote(op));
    network
        .then(|| crate::i18n::message("repo-core-4161", &[("why", pause.why().to_string().into())]))
}

/// Whether the git command `op` reaches a remote: fetch, pull, push, clone, `ls-remote`, the
/// `remote` subcommands that ask it (`update`, `prune`, `show` without `-n`) and LFS's
/// transfers. Not `remote add` / `rename` / `set-url` / `remove`, which only write the config,
/// nor `submodule update`, which reaches one only when a commit is missing (and then says so).
pub(crate) fn reaches_a_remote(op: &[String]) -> bool {
    if op
        .first()
        .is_some_and(|s| s == crate::push_preflight::APPROVED)
    {
        return true;
    }
    let mut words = op.iter().map(String::as_str);
    // Past the options before the subcommand (`-c x=y`).
    let sub = loop {
        match words.next() {
            Some("-c" | "-C") => {
                words.next();
            }
            Some(w) if w.starts_with('-') => {}
            other => break other,
        }
    };
    let rest: Vec<&str> = words.collect();
    match sub {
        Some("fetch" | "pull" | "push" | "clone" | "ls-remote" | git::dvc::PULL_ALIAS) => true,
        Some("remote") => match rest.first().copied() {
            Some("update" | "prune") => true,
            Some("show") => !rest.contains(&"-n"),
            _ => false,
        },
        Some("lfs") => matches!(rest.first().copied(), Some("fetch" | "pull" | "push")),
        _ => false,
    }
}

/// [`run_steps_counted`] without where it stopped.
#[cfg(test)]
pub(super) fn run_steps(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
) -> (Result<String, String>, OpKind) {
    let (result, kind, _) = run_steps_counted(path, ops, kind, creds, false, None);
    (result, kind)
}

/// Native test harnesses must interpret NUL checkout markers through the same
/// guarded worker as the application, without exposing internal operation kinds.
#[cfg(test)]
pub(crate) fn run_live_checkout(
    path: &Path,
    ops: &[Vec<String>],
    creds: Option<&git::Credentials>,
) -> Result<String, String> {
    run_steps(path, ops, OpKind::plain(), creds).0
}

/// Runs git commands one after another, stopping at the first failure; `kind` comes back with
/// what undoing needs, as found on disk. Also says which step it stopped at (`ops.len()` when
/// all ran): the steps before it are done, and running the operation again (after signing in)
/// starts there. A `quiet` one asks nobody for credentials ([`git::run_unattended`]).
pub(super) fn run_steps_counted(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
) -> (Result<String, String>, OpKind, usize) {
    run_steps_guarded(path, ops, kind, creds, quiet, control, None)
}

pub(super) fn run_steps_guarded(
    path: &Path,
    ops: &[Vec<String>],
    kind: OpKind,
    creds: Option<&git::Credentials>,
    quiet: bool,
    control: Option<&Arc<git::OpControl>>,
    fork_scope: Option<&super::forks::guard::Scope>,
) -> (Result<String, String>, OpKind, usize) {
    if let Some(scope) = fork_scope {
        scope.reset_rewrite();
    }
    let saved_label = match &kind {
        OpKind::Undo(entry) | OpKind::Redo(entry) => Some(entry.label.as_str()),
        OpKind::Normal(..) => None,
    };
    let mut checkout = match super::forks::guard::CheckoutReplay::read(ops, saved_label) {
        Ok(checkout) => checkout,
        Err(e) => return (Err(e), kind, 0),
    };
    let mut owned_refs = Vec::new();
    let mut rewrite = None;
    let guarded_fork = ops.iter().any(|op| super::forks::guard::is_guard(op));
    let mut pushed = Vec::new();
    // Undoing a HEAD move resets the branch checked out now: after a checkout or a commit made
    // elsewhere since, that would throw away commits that aren't the operation's.
    let expected = match &kind {
        OpKind::Undo(entry) => entry.heads.as_ref().map(|(_, after)| after),
        OpKind::Redo(entry) => entry.heads.as_ref().map(|(before, _)| before),
        OpKind::Normal(..) => None,
    };
    if let Some(expected) = expected {
        let now = git::head(path);
        if &now != expected {
            return (
                Err(crate::i18n::message(
                    "repo-core-4164",
                    &[
                        ("now", super::short(&now).to_string().into()),
                        ("expected", super::short(expected).to_string().into()),
                    ],
                )),
                kind,
                0,
            );
        }
    }
    // Where HEAD really is now, not where the last reload saw it: a commit made in the
    // terminal since must not be undone along with this.
    let head_before =
        matches!(kind, OpKind::Normal(Undo::HeadMove { .. }, _)).then(|| git::head(path));
    // Any `git stash push`: of everything, of some files, of what is staged.
    let pushes_stash = ops
        .first()
        .is_some_and(|op| op.len() >= 2 && op[0] == "stash" && op[1] == "push");
    let stash_before = pushes_stash.then(|| git::resolve(path, "refs/stash").ok());
    let needs_draft = ops.iter().any(|op| {
        git::subcommand(&op.iter().map(String::as_str).collect::<Vec<_>>())
            .is_some_and(|s| matches!(s, "checkout" | "switch" | "rebase"))
    });
    let approved_head = expected.or(head_before.as_ref());
    let prepared = with_rechecked_preparation(
        || {
            check_https_routes(ops, |binding| binding.verify(path))?;
            check_expected(path, ops, control)?;
            if let Some(expected) = approved_head
                && git::head(path) != *expected
            {
                return Err(crate::i18n::text("repo-core-4166").into());
            }
            Ok(())
        },
        || {
            if !needs_draft {
                return Ok(());
            }
            let active = || {
                !crate::tasks::cancellation().is_cancelled()
                    && !control.is_some_and(|c| c.is_cancelled())
            };
            let snapshot = (|| {
                if git::drafts::enabled_while(path, &active)? {
                    git::drafts::capture(path, crate::i18n::text("repo-core-4167"), &active)?;
                }
                Ok::<_, String>(())
            })();
            snapshot.map_err(|error| {
                crate::i18n::message("repo-core-4168", &[("error", error.to_string().into())])
            })
        },
    );
    if let Err(error) = prepared {
        return (Err(error), kind, 0);
    }
    let mut result = Ok(String::new());
    let mut stopped = ops.len();
    let mut tried = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        #[cfg(test)]
        if !is_expectation(op) {
            super::forks::before_step(op);
        }
        if let Err(e) = fork_scope
            .map_or(Ok(()), |scope| scope.check_remote(path))
            .and_then(|()| super::forks::guard::check(path, ops))
            .and_then(|()| checkout.as_mut().map_or(Ok(()), |c| c.check(path)))
            .and_then(|()| fork_scope.map_or(Ok(()), |scope| scope.check()))
        {
            result = Err(e);
            stopped = i;
            break;
        }
        if let [mark, target] = op.as_slice()
            && mark == EXPECT_FOLD_SAFE
            && let Err(error) =
                git::fold_read(path, target).and_then(super::fold::plan::Plan::from_read)
        {
            result = Err(error);
            stopped = i;
            break;
        }
        if let [mark, target] = op.as_slice()
            && mark == EXPECT_FOLD_IGNORED
            && let Err(error) =
                git::fold_read(path, target).and_then(super::fold::plan::Plan::check_ignored)
        {
            result = Err(error);
            stopped = i;
            break;
        }
        if is_expectation(op) || op.first().is_some_and(|m| m == EXPECT_HTTPS_ROUTE) {
            continue;
        }
        if let [mark, says, args @ ..] = &op[..]
            && mark == TRY
        {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            if let Err(e) = git::run_unattended(Some(path), &args, creds) {
                tried.push(format!("{says} ({})", headline(&e)));
            }
            continue;
        }
        let reviewed = match reviewed_push_args(op) {
            Ok(reviewed) => reviewed,
            Err(e) => {
                result = Err(e);
                stopped = i;
                break;
            }
        };
        let capture_args = reviewed.as_deref().unwrap_or(op);
        let prepared = match super::forks::guard::prepare_push(
            path,
            capture_args,
            guarded_fork,
            rewrite.as_ref(),
        ) {
            Ok(prepared) => prepared,
            Err(e) => {
                result = Err(e);
                stopped = i;
                break;
            }
        };
        // The preflight sender must execute exactly the argv this evidence describes.
        // A resolved APPROVED source that differs is stale, never silently substituted.
        if reviewed.is_some()
            && prepared
                .as_ref()
                .is_some_and(|(args, _)| args != capture_args)
        {
            result = Err("The reviewed fork source changed; reopen push preflight".into());
            stopped = i;
            break;
        }
        let mut actual = prepared
            .as_ref()
            .map_or_else(|| op.clone(), |(args, _)| args.clone());
        if actual.first().is_some_and(|arg| arg == "pull") {
            match ignored_safe_pull(path, &actual) {
                Ok(args) => actual = args,
                Err(error) => {
                    result = Err(error);
                    stopped = i;
                    break;
                }
            }
        }
        // Recheck immediately before invoking rebase. A rejected plan never owns a rewrite.
        let rebasing = (fork_scope.is_some() || guarded_fork)
            && matches!(op.as_slice(), [c, config, rebase, _] if c == "-c" && config == "rebase.updateRefs=false" && rebase == "rebase");
        let rewrite_before = if rebasing {
            if let Err(e) = check_expected(path, ops, control) {
                result = Err(e);
                stopped = i;
                break;
            }
            git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
                .ok()
                .map(|branch| (branch.trim().to_owned(), git::head(path)))
        } else {
            None
        };
        let resetting =
            matches!(op.as_slice(), [reset, mode, _] if reset == "reset" && mode == "--keep");
        let deleting =
            matches!(op.as_slice(), [branch, delete, ..] if branch == "branch" && delete == "-D");
        if guarded_fork
            && (resetting || deleting)
            && let Err(e) = check_local_expected(path, ops, op, control)
        {
            result = Err(e);
            stopped = i;
            break;
        }
        let reset_result = if resetting
            && ops.iter().any(|step| {
                expected_ref(step).is_some_and(|(name, _)| name.starts_with("refs/heads/"))
            })
            && let Ok(branch) = git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
        {
            match git::resolve(path, &op[2]) {
                Ok(tip) => {
                    actual[2] = tip.clone();
                    Some((branch.trim().to_owned(), tip))
                }
                Err(e) => {
                    result = Err(e);
                    stopped = i;
                    break;
                }
            }
        } else {
            None
        };
        let args: Vec<&str> = actual.iter().map(String::as_str).collect();
        if let Some(folder) = crate::agent::added_worktree(op)
            && let Err(e) = if control.is_some_and(|c| c.is_cancelled()) {
                Err(git::CANCELLED.to_owned())
            } else {
                git::prepare_worktree_folder(path, folder)
            }
        {
            result = Err(e);
            stopped = i;
            break;
        }
        result = (|| {
            if let [mark, original @ ..] = op.as_slice()
                && mark == crate::gerrit::AMEND
            {
                let prepared = crate::gerrit::amend_args(path, original)?;
                let args: Vec<&str> = prepared.iter().map(String::as_str).collect();
                return run_step(path, &args, creds, quiet, control);
            }
            if op
                .first()
                .is_some_and(|mark| mark == crate::push_preflight::APPROVED)
            {
                let (id, evidence) = reviewed_operands(op)?;
                let plan = crate::push_preflight::registered(id)?;
                let original = crate::push_preflight::push_args(&plan)?;
                let source = evidence
                    .map(|args| git::CommandSource::reviewed_binding(path, args))
                    .transpose()?;
                return run_reviewed_push(
                    &original,
                    |args| git::RouteBinding::capture(path, args),
                    || match source {
                        Some(source) => crate::push_preflight::execute_controlled_with_guard(
                            path,
                            &plan,
                            creds,
                            control,
                            || source.check_execution(),
                        ),
                        None => {
                            crate::push_preflight::execute_controlled(path, &plan, creds, control)
                        }
                    },
                    control,
                );
            }
            if let [mark, id, fix] = op.as_slice()
                && mark == crate::push_preflight::FIX
            {
                let plan = crate::push_preflight::registered(id)?;
                return crate::push_preflight::fix(path, &plan, fix);
            }
            run_step(path, &args, creds, quiet, control)
        })();
        if let Some((branch, before)) = rewrite_before {
            let done = super::forks::guard::Rewrite {
                tip: git::resolve(path, &branch).unwrap_or_default(),
                branch,
                before,
                conflicts: result
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.contains("CONFLICT") || e.contains("could not apply")),
            };
            if let Some(scope) = fork_scope {
                scope.record_rewrite(done.clone());
            }
            if !done.tip.is_empty() {
                owned_refs.push((done.branch.clone(), done.tip.clone()));
            }
            rewrite = Some(done);
        }
        if result.is_ok() {
            if let Some(done) = reset_result {
                owned_refs.push(done);
            }
            if checkout.is_some()
                && op.first().is_some_and(|s| s == "checkout")
                && let Ok(branch) = git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
            {
                owned_refs.push((branch.trim().to_owned(), git::head(path)));
            }
            // Pin a new remote at its creating step, before marker/config/fetch/checkout.
            if matches!(op.as_slice(), [remote, add, ..] if remote == "remote" && add == "add") {
                result = fork_scope
                    .map_or(Ok(()), |scope| scope.check_remote(path))
                    .and_then(|()| checkout.as_mut().map_or(Ok(()), |c| c.check(path)))
                    .map(|()| String::new());
                if result.is_err() {
                    stopped = i + 1; // remote add completed; a retry must retain its guard, not add it twice.
                    break;
                }
            }
        }
        super::forks::guard::completed_push(&result, prepared, &mut pushed);
        if result.is_err() {
            stopped = i;
            break;
        }
    }
    if result.is_ok() && !tried.is_empty() {
        result = Ok(format!("{TRIED}{}", tried.join("; ")));
    }
    // "No local changes to save" makes no stash: undoing would pop an older one.
    let no_stash = stash_before.is_some_and(|b| b == git::resolve(path, "refs/stash").ok());
    let kind = match kind {
        // For HEAD moves, the new HEAD is what redo goes back to.
        OpKind::Normal(Undo::HeadMove { mode }, before) if result.is_ok() => {
            let before = head_before.filter(|h| !h.is_empty()).unwrap_or(before);
            // And the branch it moved, which undo and redo must find checked out again.
            let branch = git::run_ok_codes(Some(path), &["symbolic-ref", "-q", "HEAD"], &[0, 1])
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            OpKind::Normal(
                Undo::HeadMove { mode },
                format!("{before}\u{0}{}\u{0}{branch}", git::head(path)),
            )
        }
        OpKind::Normal(Undo::Custom { .. }, before) if no_stash => {
            OpKind::Normal(Undo::None, before)
        }
        // The way back expects the refs where this left them.
        OpKind::Normal(Undo::Custom { mut undo, mut redo }, before) if result.is_ok() => {
            fill_expected(path, &mut undo, &owned_refs);
            if let Some(c) = &checkout {
                c.bind(&mut undo);
                c.bind(&mut redo);
            }
            super::forks::guard::bind_leases(&mut undo, &pushed);
            OpKind::Normal(Undo::Custom { undo, redo }, before)
        }
        OpKind::Undo(mut entry) if result.is_ok() => {
            fill_expected(path, &mut entry.redo, &owned_refs);
            if let Some(c) = &checkout {
                c.bind(&mut entry.undo);
                c.bind(&mut entry.redo);
            }
            super::forks::guard::bind_leases(&mut entry.redo, &pushed);
            OpKind::Undo(entry)
        }
        // Redone with nothing to stash, the undo keeps expecting the stash it popped, and so
        // refuses rather than pop an older one.
        OpKind::Redo(mut entry) if result.is_ok() && !no_stash => {
            fill_expected(path, &mut entry.undo, &owned_refs);
            if let Some(c) = &checkout {
                c.bind(&mut entry.undo);
                c.bind(&mut entry.redo);
            }
            super::forks::guard::bind_leases(&mut entry.undo, &pushed);
            OpKind::Redo(entry)
        }
        other => other,
    };
    (result, kind, stopped)
}

/// Preparation may take minutes and observes its own coherent snapshot. It never
/// authorizes a changed operation context: retain the original verifier and run
/// it again after preparation, immediately before command dispatch.
fn with_rechecked_preparation(
    mut verify: impl FnMut() -> Result<(), String>,
    prepare: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    verify()?;
    prepare()?;
    verify()
}

/// Marks a step that runs no git (no argument can hold a NUL).
const EXPECT: &str = "\u{0}expect";

/// A step of an undo (or redo) that runs nothing, but refuses the whole of it unless the ref
/// `name` is still where the operation it undoes left it (it learns where when that runs): so
/// that undoing a stash pops that stash and not one pushed since, and undoing a pull does not
/// put back a branch that got commits since.
pub(super) fn expect_ref(name: &str) -> Vec<String> {
    vec![EXPECT.to_owned(), name.to_owned(), String::new()]
}

/// A step that runs nothing, but refuses the whole operation unless `name` is at `commit`.
pub(crate) fn expect_ref_at(name: &str, commit: &str) -> Vec<String> {
    vec![EXPECT.to_owned(), name.to_owned(), commit.to_owned()]
}

/// Marks a step that runs no git and expects a symbolic ref.
const EXPECT_SYMREF: &str = "\u{0}expect-symref";

/// A step that runs nothing, but refuses the whole operation unless the symbolic ref `name`
/// (`HEAD`) points to the ref `target` (`refs/heads/main`), or is detached when `target` is
/// empty: two branches at the same commit can't be told apart by what they resolve to.
pub(super) fn expect_symref(name: &str, target: &str) -> Vec<String> {
    vec![EXPECT_SYMREF.to_owned(), name.to_owned(), target.to_owned()]
}

/// Marks a step that runs no git and expects a revision to have a tree.
const EXPECT_TREE: &str = "\u{0}expect-tree";

/// A step that runs nothing, but refuses the whole operation unless the revision `rev`
/// (`stash@{0}^2`) has the tree `tree`: unlike [`expect_ref`], what it expects stays as it was
/// made, however often the operation is redone (a patch made from that tree is applied next).
pub(super) fn expect_tree(rev: &str, tree: &str) -> Vec<String> {
    vec![EXPECT_TREE.to_owned(), rev.to_owned(), tree.to_owned()]
}

/// The revision and the tree of an `expect_tree` step.
fn expected_tree(op: &[String]) -> Option<(&str, &str)> {
    match op {
        [mark, rev, tree] if mark == EXPECT_TREE => Some((rev, tree)),
        _ => None,
    }
}

/// [`expect_symref`] in another worktree of the repository: `dir` is its folder as git on the
/// repository's machine knows it.
pub(super) fn expect_symref_in(dir: &str, name: &str, target: &str) -> Vec<String> {
    let mut step = expect_symref(name, target);
    step.push(dir.to_owned());
    step
}

/// The ref and the commit an `expect_ref` step expects.
fn expected_ref(op: &[String]) -> Option<(&str, &str)> {
    match op {
        [mark, name, value] if mark == EXPECT => Some((name, value)),
        _ => None,
    }
}

/// The symbolic ref, the ref it should point to and the worktree (`None`: this one) of an
/// `expect_symref` step.
fn expected_symref(op: &[String]) -> Option<(&str, &str, Option<&str>)> {
    match op {
        [mark, name, target] if mark == EXPECT_SYMREF => Some((name, target, None)),
        [mark, name, target, dir] if mark == EXPECT_SYMREF => Some((name, target, Some(dir))),
        _ => None,
    }
}

/// Marks a step whose failure doesn't stop the operation.
const TRY: &str = "\u{0}try";
/// Starts what an operation says when it went through but a step tried failed.
const TRIED: &str = "\u{0}tried\u{0}";

/// A step that runs `args` unattended (nobody is asked to sign in) and doesn't stop the
/// operation when it fails: the status says `says` then. A fetch before starting from what it
/// fetches, when what was fetched before will do (offline).
pub(super) fn try_step(args: &[&str], says: &str) -> Vec<String> {
    [TRY, says]
        .into_iter()
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect()
}

/// What the steps tried and failed say, from what an operation that went through gave.
pub(super) fn tried_note(out: &str) -> Option<&str> {
    out.strip_prefix(TRIED)
}

/// Whether `op` is a step that runs nothing, only checks.
pub(super) fn is_expectation(op: &[String]) -> bool {
    super::forks::guard::is_guard(op)
        || expected_ref(op).is_some()
        || expected_symref(op).is_some()
        || expected_tree(op).is_some()
        || matches!(op, [mark, _, _] if mark == crate::gerrit::CHECK)
        || matches!(op, [mark, _] if mark == EXPECT_HTTPS_ROUTE)
        || matches!(op, [mark, _] if mark == EXPECT_FOLD_SAFE)
        || matches!(op, [mark, _] if mark == EXPECT_FOLD_IGNORED)
}

pub(super) const EXPECT_FOLD_SAFE: &str = "\u{0}expect-fold-safe";
pub(super) const EXPECT_FOLD_IGNORED: &str = "\u{0}expect-fold-ignored";

/// Classify the options consumed by Git's builtin/pull.c and parse-options.c.
/// Optional arguments consume only attached text; required arguments also consume
/// the next token, even when it looks like an option. Git permutes operands until `--`.
#[derive(Debug, Default, PartialEq, Eq)]
struct PullMode {
    rebase: Option<bool>,
    ff_only: bool,
}

fn pull_mode(args: &[String]) -> Result<PullMode, String> {
    // (long name, argument: 0 = none, 1 = required, 2 = optional).
    const OPTIONS: &[(&str, u8)] = &[
        ("verbose", 0),
        ("quiet", 0),
        ("progress", 0),
        ("recurse-submodules", 2),
        ("rebase", 2),
        ("stat", 0),
        ("summary", 0),
        ("log", 2),
        ("signoff", 2),
        ("squash", 0),
        ("commit", 0),
        ("edit", 0),
        ("cleanup", 1),
        ("ff", 0),
        ("ff-only", 0),
        ("verify", 0),
        ("verify-signatures", 0),
        ("autostash", 0),
        ("strategy", 1),
        ("strategy-option", 1),
        ("gpg-sign", 2),
        ("allow-unrelated-histories", 0),
        ("all", 0),
        ("append", 0),
        ("upload-pack", 1),
        ("force", 0),
        ("tags", 0),
        ("prune", 0),
        ("jobs", 2),
        ("dry-run", 0),
        ("keep", 0),
        ("depth", 1),
        ("shallow-since", 1),
        ("shallow-exclude", 1),
        ("deepen", 1),
        ("unshallow", 0),
        ("update-shallow", 0),
        ("refmap", 1),
        ("server-option", 1),
        ("ipv4", 0),
        ("ipv6", 0),
        ("negotiation-tip", 1),
        ("show-forced-updates", 0),
        ("set-upstream", 0),
    ];
    fn rebase_value(value: &str) -> Result<bool, String> {
        match value.to_ascii_lowercase().as_str() {
            "" | "false" | "no" | "off" | "0" => Ok(false),
            "true" | "yes" | "on" | "1" => Ok(true),
            _ if matches!(value, "merges" | "m" | "interactive" | "i") => Ok(true),
            _ => Err(format!("Invalid pull rebase value: {value}")),
        }
    }
    let mut mode = PullMode::default();
    let mut tokens = args.iter().skip(1);
    while let Some(token) = tokens.next() {
        if token == "--" {
            break;
        }
        if let Some(long) = token.strip_prefix("--") {
            let (name, value) = long
                .split_once('=')
                .map_or((long, None), |(n, v)| (n, Some(v)));
            let (name, negated) = name
                .strip_prefix("no-")
                .map_or((name, false), |n| (n, true));
            let exact = OPTIONS.iter().find(|(n, _)| *n == name);
            let mut matches = OPTIONS.iter().filter(|(n, _)| n.starts_with(name));
            let option = if let Some(exact) = exact {
                exact
            } else {
                let first = matches
                    .next()
                    .ok_or_else(|| format!("Unknown pull option: {token}"))?;
                if matches.next().is_some() {
                    return Err(format!("Ambiguous pull option: {token}"));
                }
                first
            };
            let (name, arity) = *option;
            if (negated || arity == 0) && value.is_some() {
                return Err(format!("Pull option takes no value: {token}"));
            }
            if negated && matches!(name, "ff-only" | "unshallow" | "refmap") {
                return Err(format!("Pull option cannot be negated: {token}"));
            }
            if !negated && arity == 1 && value.is_none() && tokens.next().is_none() {
                return Err(format!("Pull option requires a value: {token}"));
            }
            match name {
                "rebase" => {
                    mode.rebase = Some(if negated {
                        false
                    } else {
                        value.map_or(Ok(true), rebase_value)?
                    })
                }
                "ff-only" => mode.ff_only = true,
                "ff" => mode.ff_only = false,
                _ => {}
            }
        } else if let Some(short) = token.strip_prefix('-') {
            let letters = short.char_indices();
            for (at, letter) in letters {
                let rest = &short[at + letter.len_utf8()..];
                match letter {
                    'r' => {
                        mode.rebase = Some(if rest.is_empty() {
                            true
                        } else {
                            rebase_value(rest)?
                        });
                        break;
                    }
                    's' | 'X' | 'o' => {
                        if rest.is_empty() && tokens.next().is_none() {
                            return Err(format!("Pull option requires a value: -{letter}"));
                        }
                        break;
                    }
                    'S' | 'j' => break, // optional value is the entire attached rest
                    'v' | 'q' | 'n' | 'a' | 'f' | 't' | 'p' | 'k' | '4' | '6' => {}
                    _ => return Err(format!("Unknown pull option: -{letter}")),
                }
            }
        }
    }
    Ok(mode)
}

/// Keep pull's authentication, submodule and configuration behavior while making its
/// merge refuse to overwrite ignored files. Read the effective options in the worker,
/// after any preceding checkout, and override them only for this command.
fn ignored_safe_pull(path: &Path, args: &[String]) -> Result<Vec<String>, String> {
    ignored_safe_pull_with(args, |args, codes| {
        git::run_ok_codes(Some(path), args, codes)
    })
}

fn ignored_safe_pull_with(
    args: &[String],
    mut run: impl FnMut(&[&str], &[i32]) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let mode = pull_mode(args)?;
    let branch = run(&["symbolic-ref", "-q", "HEAD"], &[0])?;
    let branch = branch
        .trim()
        .strip_prefix("refs/heads/")
        .ok_or("Pull requires a checked-out local branch")?;
    if !mode.ff_only {
        let rebase = match mode.rebase {
            Some(value) => value,
            None => {
                let local_key = format!("branch.{branch}.rebase");
                let local = run(&["config", "-z", "--get", &local_key], &[0, 1])?;
                let (key, value) = if local.is_empty() {
                    (
                        "pull.rebase",
                        run(&["config", "-z", "--get", "pull.rebase"], &[0, 1])?,
                    )
                } else {
                    (local_key.as_str(), local)
                };
                // A valueless boolean means true, whereas an explicitly empty value
                // means false. Git's typed reader preserves that distinction.
                if !value.is_empty() && value.trim_end_matches('\0').is_empty() {
                    run(&["config", "--bool", "--get", key], &[0])?.trim() == "true"
                } else {
                    !matches!(
                        value
                            .trim_end_matches('\0')
                            .trim()
                            .to_ascii_lowercase()
                            .as_str(),
                        "" | "false" | "no" | "off" | "0"
                    )
                }
            }
        };
        // Rebase's checkout does not honor mergeOptions and offers no equivalent
        // no-overwrite-ignore switch. Refuse before fetch can introduce a collision.
        if rebase
            && !run(
                &[
                    "ls-files",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                    "--directory",
                    "-z",
                ],
                &[0],
            )?
            .is_empty()
        {
            return Err("Rebase pull cannot safely preserve ignored local files. Move them out of the repository or use fast-forward-only or merge pull.".into());
        }
    }
    let key = format!("branch.{branch}.mergeOptions");
    let old = run(&["config", "-z", "--get", &key], &[0, 1])?;
    let old = old.strip_suffix('\0').unwrap_or(&old);
    Ok([
        vec!["-c".into(), format!("{key}={old} --no-overwrite-ignore")],
        args.to_vec(),
    ]
    .concat())
}

/// Captures only the witness from the same validator invocation; no additional Git call.
pub(super) fn check_stack_captured(
    base: &str,
    head: &str,
    control: Option<&Arc<git::OpControl>>,
    run: impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let (result, source) = crate::gerrit::check_stack_presented_with(base, head, run);
    if let Some(control) = control {
        control.stack_checked(source);
    }
    result
}

fn check_expected(
    path: &Path,
    ops: &[Vec<String>],
    control: Option<&Arc<git::OpControl>>,
) -> Result<(), String> {
    for op in ops {
        if let [mark, base, head] = op.as_slice()
            && mark == crate::gerrit::CHECK
        {
            check_stack_captured(base, head, control, |args| {
                if let Some(control) = control {
                    git::run_watched(Some(path), args, None, true, control)
                } else {
                    git::run(Some(path), args)
                }
            })?;
        }
    }
    for (rev, tree) in ops.iter().filter_map(|op| expected_tree(op)) {
        let now = git::run(
            Some(path),
            &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
        )
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
        if now != tree {
            return Err(crate::i18n::message(
                "repo-core-4178",
                &[("rev", rev.to_string().into())],
            ));
        }
    }
    for (name, target, dir) in ops.iter().filter_map(|op| expected_symref(op)) {
        let mut args = Vec::new();
        if let Some(dir) = dir {
            args.extend(["-C", dir]);
        }
        args.extend(["symbolic-ref", "-q", name]);
        // Exit 1, nothing printed: detached.
        let now = git::run_ok_codes(Some(path), &args, &[0, 1]).map(|s| s.trim().to_owned());
        if now.as_deref() != Ok(target) {
            let says = |r: &str| {
                if r.is_empty() {
                    crate::i18n::text("repo-core-4179").to_owned()
                } else {
                    r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()
                }
            };
            let now = now.map_or_else(
                |_| crate::i18n::text("repo-core-4180").to_owned(),
                |r| says(&r),
            );
            let place = dir.map_or(String::new(), |d| {
                crate::i18n::message("repo-core-4181", &[("d", d.to_string().into())])
            });
            return Err(crate::i18n::message(
                "repo-core-4182",
                &[
                    ("now", now.to_string().into()),
                    ("place", place.to_string().into()),
                    ("target", says(target).to_string().into()),
                ],
            ));
        }
    }
    for (name, value) in ops.iter().filter_map(|op| expected_ref(op)) {
        let now = git::resolve(path, name).unwrap_or_default();
        if now != value {
            let what = name.strip_prefix("refs/heads/").unwrap_or(name);
            return Err(if name == "refs/stash" {
                crate::i18n::text("repo-core-4183").to_owned()
            } else if value.is_empty() {
                crate::i18n::message("repo-core-4184", &[("what", what.to_string().into())])
            } else {
                crate::i18n::message(
                    "repo-core-4185",
                    &[
                        ("what", what.to_string().into()),
                        ("now", super::short(&now).to_string().into()),
                        ("value", super::short(value).to_string().into()),
                    ],
                )
            });
        }
    }
    Ok(())
}

fn check_local_expected(
    path: &Path,
    ops: &[Vec<String>],
    mutation: &[String],
    control: Option<&Arc<git::OpControl>>,
) -> Result<(), String> {
    let reset = mutation.first().is_some_and(|s| s == "reset");
    let names: Vec<String> = if reset {
        git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
            .ok()
            .map(|s| vec![s.trim().to_owned()])
            .unwrap_or_default()
    } else {
        mutation
            .iter()
            .skip(2)
            .filter(|s| s.as_str() != "--")
            .map(|name| format!("refs/heads/{name}"))
            .collect()
    };
    let local: Vec<Vec<String>> = ops
        .iter()
        .filter(|op| {
            expected_ref(op).is_some_and(|(name, _)| names.iter().any(|n| n == name))
                || (reset && expected_symref(op).is_some())
        })
        .cloned()
        .collect();
    check_expected(path, &local, control)
}

// A network push may have allowed an unrelated local commit. Prefer the result this
// worker actually produced; disk is only a fallback for refs the operation did not write.
fn fill_expected(path: &Path, ops: &mut [Vec<String>], owned: &[(String, String)]) {
    for op in ops {
        if let Some((name, _)) = expected_ref(op) {
            let now = owned.iter().rev().find(|(r, _)| r == name).map_or_else(
                || git::resolve(path, name).unwrap_or_default(),
                |(_, tip)| tip.clone(),
            );
            if let Some(value) = op.get_mut(2) {
                *value = now;
            }
        }
    }
}

/// How the app stashes everything, untracked files included.
pub(super) fn stash_push_args() -> [&'static str; 3] {
    ["stash", "push", "--include-untracked"]
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::commands::checkout_pull_undo;
    use super::super::{OpKind, Undo, UndoEntry, ops};
    use super::{run_steps, stash_push_args};
    use crate::git;

    mod short_rebase {
        use super::*;

        #[test]
        fn classifier_consumes_git_options_and_uses_last_mode() -> Result<(), String> {
            for (tail, rebase, ff_only) in [
                (vec![], None, false),
                (vec!["-r"], Some(true), false),
                (vec!["-rtrue"], Some(true), false),
                (vec!["-rmerges"], Some(true), false),
                (vec!["-rm"], Some(true), false),
                (vec!["-qr"], Some(true), false),
                (vec!["-rfalse"], Some(false), false),
                (vec!["-qrfalse"], Some(false), false),
                (vec!["-r", "--no-rebase"], Some(false), false),
                (vec!["--no-rebase", "-qr"], Some(true), false),
                (vec!["-rfalse", "--rebase=merges"], Some(true), false),
                (vec!["--rebase=false", "-rm"], Some(true), false),
                (vec!["-r", "--rebase=false"], Some(false), false),
                (vec!["-srecursive"], None, false),
                (vec!["-qs", "--rebase"], None, false),
                (vec!["-X", "-r"], None, false),
                (vec!["-o", "--ff-only", "-r"], Some(true), false),
                (vec!["-Sr", "-jr", "-Xours"], None, false),
                (vec!["--strategy", "--rebase", "-r"], Some(true), false),
                (vec!["--upload-pack", "-r"], None, false),
                (vec!["--strategy-option=-r"], None, false),
                (vec!["--gpg-sign=r", "--jobs=r"], None, false),
                (vec!["--recurse-submodules", "-r"], Some(true), false),
                (vec!["-r", "false"], Some(true), false),
                (vec!["--rebase", "false"], Some(true), false),
                (vec!["origin", "main", "-r"], Some(true), false),
                (vec!["--", "-r", "--ff-only"], None, false),
                (vec!["-r", "--", "--no-rebase"], Some(true), false),
                (vec!["-r", "--ff-only"], Some(true), true),
                (vec!["--ff-only", "--ff", "-r"], Some(true), false),
                (vec!["--ff-only", "--no-ff", "-r"], Some(true), false),
                (vec!["--reb=m"], Some(true), false),
            ] {
                let args = [
                    vec!["pull".to_owned()],
                    tail.iter().map(|s| (*s).to_owned()).collect(),
                ]
                .concat();
                assert_eq!(
                    super::super::pull_mode(&args)?,
                    super::super::PullMode { rebase, ff_only },
                    "{args:?}"
                );
            }
            for option in [
                "cleanup",
                "strategy",
                "strategy-option",
                "upload-pack",
                "depth",
                "shallow-since",
                "shallow-exclude",
                "deepen",
                "refmap",
                "server-option",
                "negotiation-tip",
            ] {
                let args = vec![
                    "pull".into(),
                    format!("--{option}"),
                    "--ff-only".into(),
                    "-r".into(),
                ];
                assert_eq!(
                    super::super::pull_mode(&args)?.rebase,
                    Some(true),
                    "{args:?}"
                );
                assert!(!super::super::pull_mode(&args)?.ff_only, "{args:?}");
            }
            for tail in [
                vec!["-r=false"],
                vec!["-rq"],
                vec!["-qrq"],
                vec!["--rebase=bogus"],
                vec!["--no-rebase=true"],
                vec!["--strategy"],
                vec!["-X"],
                vec!["--str"],
            ] {
                let args = [
                    vec!["pull".to_owned()],
                    tail.iter().map(|s| (*s).to_owned()).collect(),
                ]
                .concat();
                assert!(super::super::pull_mode(&args).is_err(), "{args:?}");
            }
            Ok(())
        }

        #[test]
        fn recording_saved_redo_refuses_before_pull_or_fetch_dispatch() -> Result<(), String> {
            for option in ["-r", "-rtrue", "-rmerges", "-rm", "-qr"] {
                let saved: UndoEntry = serde_json::from_value(serde_json::json!({
                    "label": "Pull", "undo": [], "redo": [["pull", option]], "heads": null
                }))
                .map_err(|e| e.to_string())?;
                let mut calls = Vec::new();
                let result = super::super::ignored_safe_pull_with(&saved.redo[0], |args, _| {
                    calls.push(args.join(" "));
                    match args[0] {
                        "symbolic-ref" => Ok("refs/heads/main\n".into()),
                        "ls-files" => Ok("private\0".into()),
                        _ => Err(format!("Unexpected command: {args:?}")),
                    }
                });
                assert!(
                    result.is_err_and(|e| e.contains("ignored local files")),
                    "{option}"
                );
                assert_eq!(calls.len(), 2);
                assert!(
                    calls
                        .iter()
                        .all(|c| !c.starts_with("pull") && !c.starts_with("fetch"))
                );
            }
            Ok(())
        }

        #[test]
        fn recording_config_fallback_and_overrides_preserve_ignored_files() -> Result<(), String> {
            for (tail, local, global, refused) in [
                (vec![], "", "true\0", true),
                (vec![], "false\0", "true\0", false),
                (vec!["--", "--no-rebase"], "", "true\0", true),
                (vec!["--strategy", "--no-rebase"], "", "true\0", true),
                (vec!["-srecursive"], "", "true\0", true),
                (vec!["-rfalse"], "", "true\0", false),
                (vec!["-qrfalse"], "", "true\0", false),
                (vec!["--no-rebase", "-r"], "false\0", "", true),
                (vec!["-r", "--ff-only"], "", "true\0", false),
                (vec!["--ff-only", "--ff", "-r"], "", "", true),
                (vec![], "", "", false),
            ] {
                let args = [
                    vec!["pull".to_owned()],
                    tail.iter().map(|s| (*s).to_owned()).collect(),
                ]
                .concat();
                let mut calls = Vec::new();
                let result = super::super::ignored_safe_pull_with(&args, |argv, _| {
                    calls.push(argv.join(" "));
                    match argv {
                        ["symbolic-ref", "-q", "HEAD"] => Ok("refs/heads/main\n".into()),
                        ["config", "-z", "--get", "branch.main.rebase"] => Ok(local.into()),
                        ["config", "-z", "--get", "pull.rebase"] => Ok(global.into()),
                        ["config", "-z", "--get", "branch.main.mergeOptions"] => {
                            Ok("--log=3\0".into())
                        }
                        ["ls-files", ..] => Ok("private\0".into()),
                        _ => Err(format!("Unexpected command: {argv:?}")),
                    }
                });
                if refused {
                    assert!(
                        result.is_err_and(|e| e.contains("ignored local files")),
                        "{args:?}"
                    );
                } else {
                    let prepared = result?;
                    assert_eq!(
                        prepared[1],
                        "branch.main.mergeOptions=--log=3 --no-overwrite-ignore"
                    );
                    assert_eq!(prepared[2..], args);
                }
                assert!(
                    calls
                        .iter()
                        .all(|c| !c.starts_with("pull") && !c.starts_with("fetch"))
                );
            }
            Ok(())
        }
    }

    #[test]
    fn selected_continuation_recording_retains_actual_route_receipt_and_expect_retry()
    -> Result<(), String> {
        let route = git::repository_tools::routes::PushRoute {
            branch: "topic".into(),
            commit: "a".repeat(40),
            remote: "publish-host".into(),
            destination: "refs/heads/release".into(),
            urls: vec!["https://publish.invalid/team/repo".into()],
            lease: Some("b".repeat(40)),
        };
        let mut plan = crate::push_preflight::selected_continuation_recording::plan(&route);
        plan.retain_selection(git::repository_tools::routes::PreparedPush {
            snapshot: git::repository_tools::guard::Snapshot::recording(),
            route: route.clone(),
        })?;
        let original = crate::push_preflight::push_args(&plan)?;
        let binding: git::RouteBinding = serde_json::from_value(serde_json::json!({ "args":original, "routes":route.urls, "configuration":"recorded original route" })).map_err(|e| e.to_string())?;
        let control = git::OpControl::new(|| {});
        let receipt = crate::push_preflight::remember_auth_endpoint(&route.urls[0]);
        let failed = super::run_reviewed_push(
            &original,
            |_| Ok(binding.clone()),
            || Err(receipt.clone()),
            Some(&control),
        );
        assert_eq!(failed, Err(receipt));
        let failure = control
            .auth_failure()
            .ok_or("typed reviewed receipt missing")?;
        assert_eq!(failure.binding, Some(binding.clone()));
        assert_eq!(failure.endpoints, route.urls);
        let mut ops = crate::push_preflight::approved_ops(plan);
        let guard = ops[0].clone();
        ops.insert(
            0,
            vec![
                super::EXPECT_HTTPS_ROUTE.into(),
                serde_json::to_string(&binding).map_err(|e| e.to_string())?,
            ],
        );
        super::retain_unfinished(&mut ops, 1);
        assert!(ops.contains(&guard));
        let mut checked = 0;
        super::check_https_routes(&ops, |actual| {
            checked += 1;
            assert_eq!(actual, &binding);
            Ok(())
        })?;
        assert_eq!(checked, 1);
        let approved = ops.last().ok_or("APPROVED continuation missing")?;
        super::reviewed_operands(approved)?;
        Ok(())
    }

    fn recorded_route() -> Result<git::RouteBinding, String> {
        serde_json::from_value(serde_json::json!({
            "args": ["push", "--", "original", "HEAD:refs/heads/main"],
            "routes": ["https://push.invalid/team/repo"],
            "configuration": "captured-routing-config"
        }))
        .map_err(|e| e.to_string())
    }

    #[test]
    fn recording_reviewed_route_is_captured_before_dispatch_and_proves_one_shot_receipt()
    -> Result<(), String> {
        use std::cell::Cell;
        let binding = recorded_route()?;
        let original = binding.args.clone();
        let control = git::OpControl::new(|| {});
        let captured = Cell::new(false);
        let mut context = crate::connections::Context::default();
        context.choices.github.user = "initiating-account".into();
        let receipt =
            crate::push_preflight::remember_auth_endpoint("https://push.invalid/team/repo");
        let result = context.run(|| {
            super::run_reviewed_push(
                &original,
                |args| {
                    assert_eq!(
                        args,
                        original.iter().map(String::as_str).collect::<Vec<_>>()
                    );
                    assert!(!args.contains(&crate::push_preflight::APPROVED));
                    assert!(!args.iter().any(|word| word.contains("private-alias")));
                    captured.set(true);
                    Ok(binding.clone())
                },
                || {
                    assert!(captured.get());
                    Err(receipt.clone())
                },
                Some(&control),
            )
        });
        assert_eq!(result, Err(receipt.clone())); // human diagnostics stay opaque
        let failure = control
            .auth_failure()
            .ok_or("missing authentication failure")?;
        assert_eq!(failure.context, context);
        assert!(failure.confirmed);
        assert_eq!(failure.endpoints, ["https://push.invalid/team/repo"]);
        assert_eq!(failure.binding, Some(binding.clone()));
        assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());
        context.run(|| {
            let ctx = egui::Context::default();
            let mut tab =
                super::RepoTab::new_deferred("/nonexistent/reviewed-signin-recording".into(), &ctx);
            tab.offer_failed_https_signin(failure.clone(), true, &ctx);
            let ticket = tab
                .https_challenge
                .as_ref()
                .ok_or("missing sign-in ticket")?;
            assert!(ticket.authorizes_endpoint("https://push.invalid/team/repo"));
            assert!(!ticket.authorizes_endpoint("https://fetch.invalid/team/repo"));
            assert_eq!(ticket.binding, Some(binding));
            assert!(matches!(
                tab.dialog,
                Some(super::Dialog::Credentials { .. })
            ));
            Ok::<(), String>(())
        })?;
        let mut changed = context.clone();
        changed.choices.github.user = "later-account".into();
        changed.run(|| {
            let ctx = egui::Context::default();
            let mut tab =
                super::RepoTab::new_deferred("/nonexistent/stale-signin-recording".into(), &ctx);
            tab.offer_failed_https_signin(failure, true, &ctx);
            assert!(tab.https_challenge.is_none());
            assert!(tab.dialog.is_none());
        });
        Ok(())
    }

    #[test]
    fn recording_reviewed_auth_missing_foreign_or_unproven_routes_refuse_without_dispatch()
    -> Result<(), String> {
        use std::cell::Cell;
        let binding = recorded_route()?;
        let original = binding.args.clone();
        let dispatched = Cell::new(false);
        let control = git::OpControl::new(|| {});
        let failed = super::run_reviewed_push(
            &original,
            |_| Err("route proof unavailable".into()),
            || {
                dispatched.set(true);
                Ok(String::new())
            },
            Some(&control),
        );
        assert!(failed.is_err());
        assert!(!dispatched.get());
        assert!(control.auth_failure().is_none());
        for endpoint in [
            "https://fetch.invalid/team/repo",
            "https://push.invalid/other/repo",
            "https://push.invalid/team/repo/info/lfs",
            "http://push.invalid/team/repo",
        ] {
            let receipt = crate::push_preflight::remember_auth_endpoint(endpoint);
            let failed = super::run_reviewed_push(
                &original,
                |_| Ok(binding.clone()),
                || Err(receipt.clone()),
                Some(&control),
            );
            assert_eq!(failed, Err(receipt.clone()));
            assert!(control.auth_failure().is_none(), "{endpoint}");
            assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());
        }
        let receipt =
            crate::push_preflight::remember_auth_endpoint("https://push.invalid/team/repo");
        assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_some());
        assert!(
            super::reviewed_auth_failure(&receipt, binding, crate::connections::Context::default())
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn recording_https_route_proof_is_rechecked_after_draft_and_retained_for_retry()
    -> Result<(), String> {
        use std::cell::Cell;
        let binding = recorded_route()?;
        let ops = vec![
            vec![
                super::EXPECT_HTTPS_ROUTE.into(),
                serde_json::to_string(&binding).map_err(|e| e.to_string())?,
            ],
            vec!["checkout".into(), "topic".into()],
        ];
        let current = Cell::new(true);
        let checks = Cell::new(0);
        let result = super::with_rechecked_preparation(
            || {
                super::check_https_routes(&ops, |proof| {
                    assert_eq!(proof, &binding);
                    checks.set(checks.get() + 1);
                    if current.get() {
                        Ok(())
                    } else {
                        Err("push route changed during draft".into())
                    }
                })
            },
            || {
                current.set(false);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(checks.get(), 2);
        let ctx = egui::Context::default();
        let mut tab =
            super::RepoTab::new_deferred("/nonexistent/route-retry-recording".into(), &ctx);
        tab.last_op = Some(super::PendingOp {
            label: "Retry".into(),
            ops: ops.clone(),
            refresh: super::Refresh::All,
            kind: OpKind::plain(),
            quiet: false,
        });
        tab.keep_unfinished(1);
        assert_eq!(
            tab.last_op.as_ref().ok_or("missing retry operation")?.ops,
            ops
        );
        let malformed = vec![vec![
            super::EXPECT_HTTPS_ROUTE.into(),
            "invalid-json".into(),
        ]];
        let called = Cell::new(false);
        assert!(
            super::check_https_routes(&malformed, |_| {
                called.set(true);
                Err("invalid proof dispatched".into())
            })
            .is_err()
        );
        assert!(!called.get(), "invalid proof dispatched");
        Ok(())
    }

    #[test]
    fn recording_safety_draft_cannot_authorize_an_operation_after_its_context_moves() {
        use std::cell::{Cell, RefCell};
        let head = Cell::new("approved-head");
        let branch = Cell::new("approved-branch");
        let checks = RefCell::new(Vec::new());
        for change_branch in [false, true] {
            head.set("approved-head");
            branch.set("approved-branch");
            checks.borrow_mut().clear();
            let mut dispatched = false;
            let result = super::with_rechecked_preparation(
                || {
                    checks.borrow_mut().push((head.get(), branch.get()));
                    if head.get() == "approved-head" && branch.get() == "approved-branch" {
                        Ok(())
                    } else {
                        Err("original operation context moved".into())
                    }
                },
                || {
                    if change_branch {
                        branch.set("another-branch");
                    } else {
                        head.set("later-coherent-draft-head");
                    }
                    Ok(()) // a coherent successful snapshot of the *new* state
                },
            );
            if result.is_ok() {
                dispatched = true;
            }
            assert!(result.is_err());
            assert!(!dispatched);
            assert_eq!(checks.borrow().len(), 2);
        }
    }

    fn repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gitcraft-ops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
        ] {
            assert!(git::run(Some(&dir), args).is_ok());
        }
        dir
    }

    #[test]
    fn pull_preserves_merge_options_and_refuses_ignored_file_overwrite() -> Result<(), String> {
        let upstream = repo("pull-ignored-upstream");
        let clone = tempfile::tempdir().map_err(|e| e.to_string())?;
        let clone_path = clone.path().to_str().ok_or("clone path")?;
        git::run(Some(&upstream), &["clone", "-q", ".", clone_path])?;
        let before = git::head(clone.path());
        std::fs::write(upstream.join("secret.env"), "upstream").map_err(|e| e.to_string())?;
        git::run(Some(&upstream), &["add", "secret.env"])?;
        git::run(Some(&upstream), &["commit", "-qm", "add secret path"])?;
        std::fs::write(clone.path().join(".git/info/exclude"), "secret.env\n")
            .map_err(|e| e.to_string())?;
        std::fs::write(clone.path().join("secret.env"), "private").map_err(|e| e.to_string())?;
        let key = "branch.main.mergeOptions";
        let old = "--log=3 --no-stat";
        // --short would report heads/main when a tag uses the same name.
        git::run(Some(clone.path()), &["tag", "main"])?;
        git::run(Some(clone.path()), &["config", key, old])?;
        let pull = ops(&[&["pull", "--ff-only"]]);
        let argv = super::ignored_safe_pull(clone.path(), &pull[0])?;
        assert_eq!(argv[1], format!("{key}={old} --no-overwrite-ignore"));
        let (result, _) = run_steps(clone.path(), &pull, OpKind::plain(), None);
        assert!(result.is_err(), "{result:?}");
        assert_eq!(git::head(clone.path()), before);
        assert_eq!(
            std::fs::read(clone.path().join("secret.env")).map_err(|e| e.to_string())?,
            b"private"
        );
        assert_eq!(
            git::run(Some(clone.path()), &["config", "--get", key])?.trim(),
            old
        );
        std::fs::remove_file(clone.path().join("secret.env")).map_err(|e| e.to_string())?;
        let (result, _) = run_steps(clone.path(), &pull, OpKind::plain(), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(git::head(clone.path()), git::head(&upstream));
        let _ = std::fs::remove_dir_all(upstream);
        Ok(())
    }

    #[test]
    fn rebase_pull_refuses_ignored_files_before_fetch_and_honors_explicit_modes()
    -> Result<(), String> {
        let dir = repo("rebase-pull-ignored");
        std::fs::write(dir.join(".git/info/exclude"), "private\n").map_err(|e| e.to_string())?;
        std::fs::write(dir.join("private"), "precious").map_err(|e| e.to_string())?;
        let before = git::head(&dir);
        let rebase = ops(&[&["pull", "--rebase", "--autostash"]]);
        let (result, _) = run_steps(&dir, &rebase, OpKind::plain(), None);
        assert!(result.is_err_and(|e| e.contains("ignored local files")));
        assert_eq!(git::head(&dir), before);
        assert!(!dir.join(".git/FETCH_HEAD").exists());
        assert_eq!(
            std::fs::read(dir.join("private")).map_err(|e| e.to_string())?,
            b"precious"
        );
        git::run(Some(&dir), &["config", "pull.rebase", "true"])?;
        assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_err());
        for args in [&["pull", "--ff-only"][..], &["pull", "--no-rebase"][..]] {
            assert!(super::ignored_safe_pull(&dir, &ops(&[args])[0]).is_ok());
        }
        git::run(Some(&dir), &["config", "branch.main.rebase", "false"])?;
        assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_ok());
        git::run(Some(&dir), &["config", "--unset", "branch.main.rebase"])?;
        let config = dir.join(".git/config");
        let mut text = std::fs::read_to_string(&config).map_err(|e| e.to_string())?;
        text.push_str("\n[branch \"main\"]\n rebase\n");
        std::fs::write(&config, &text).map_err(|e| e.to_string())?;
        assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_err());
        git::run(Some(&dir), &["config", "branch.main.rebase", ""])?;
        assert!(super::ignored_safe_pull(&dir, &ops(&[&["pull"]])[0]).is_ok());
        std::fs::remove_file(dir.join("private")).map_err(|e| e.to_string())?;
        assert!(super::ignored_safe_pull(&dir, &rebase[0]).is_ok());
        let _ = std::fs::remove_dir_all(dir);
        Ok(())
    }

    /// A quiet operation (the background fetch) runs its git steps, and only checks its
    /// expectations: git is not given them.
    #[test]
    fn a_quiet_operation_runs_its_steps_but_not_its_expectations() {
        let dir = repo("quiet-steps");
        let before = git::head(&dir);
        let steps: Vec<Vec<String>> = [
            super::expect_symref("HEAD", "refs/heads/main"),
            super::expect_ref_at("HEAD", &before),
        ]
        .into_iter()
        .chain(ops(&[&["commit", "-q", "--allow-empty", "-m", "quiet"]]))
        .collect();
        let (result, _, stopped) =
            super::run_steps_counted(&dir, &steps, OpKind::plain(), None, true, None);
        let after = git::head(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(stopped, steps.len());
        assert_ne!(after, before);
    }

    fn entry(kind: OpKind) -> Option<UndoEntry> {
        match kind {
            OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
                id: 0,
                agent: None,
                label: "op".to_owned(),
                undo,
                redo,
                heads: None,
            }),
            _ => None,
        }
    }

    fn stash(dir: &Path, file: &str) -> Option<UndoEntry> {
        let _ = std::fs::write(dir.join(file), "x\n");
        let push = ops(&[&stash_push_args()]);
        let undo = Undo::Custom {
            undo: [super::expect_ref("refs/stash")]
                .into_iter()
                .chain(ops(&[&["stash", "pop", "--index"]]))
                .collect(),
            redo: push.clone(),
        };
        let (result, kind) = run_steps(dir, &push, OpKind::Normal(undo, String::new()), None);
        assert!(result.is_ok());
        entry(kind)
    }

    /// Undoing a stash pops that stash, not one pushed since (in the terminal).
    #[test]
    fn undoing_a_stash_refuses_once_another_was_pushed() {
        let dir = repo("stash-newer");
        let first = stash(&dir, "a.txt");
        let _ = std::fs::write(dir.join("b.txt"), "y\n");
        let other = git::run(Some(&dir), &["stash", "push", "-u"]);
        let refused = first
            .as_ref()
            .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None).0);
        let count = git::run(Some(&dir), &["stash", "list"]).map(|s| s.lines().count());
        // Popped by hand: now the newest is this one again, and undoing goes through.
        let popped = git::run(Some(&dir), &["stash", "pop"]);
        let _ = std::fs::remove_file(dir.join("b.txt"));
        let undone = first
            .as_ref()
            .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None));
        let restored = dir.join("a.txt").exists();
        // Redone, the undo expects the new stash.
        let redone = undone.and_then(|(r, kind)| match kind {
            OpKind::Undo(e) if r.is_ok() => {
                Some(run_steps(&dir, &e.redo.clone(), OpKind::Redo(e), None))
            }
            _ => None,
        });
        let again = redone.and_then(|(r, kind)| match kind {
            OpKind::Redo(e) if r.is_ok() => {
                Some(run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None).0)
            }
            _ => None,
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(other.is_ok());
        assert!(matches!(refused, Some(Err(_))));
        assert_eq!(count, Ok(2));
        assert!(popped.is_ok());
        assert!(restored);
        assert!(matches!(again, Some(Ok(_))));
    }

    /// Redoing a stash with nothing left to stash: undoing it then pops no older stash.
    #[test]
    fn a_redone_stash_that_saved_nothing_is_not_popped() {
        let dir = repo("stash-redo-nothing");
        let _ = std::fs::write(dir.join("old.txt"), "o\n");
        let older = git::run(Some(&dir), &["stash", "push", "-u"]);
        let first = stash(&dir, "a.txt");
        let undone = first
            .map(|e| run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None))
            .and_then(|(r, kind)| r.ok().and(Some(kind)));
        // The changes popped back go elsewhere: redoing stashes nothing.
        let _ = std::fs::remove_file(dir.join("a.txt"));
        let redone = undone.and_then(|kind| match kind {
            OpKind::Undo(e) => Some(run_steps(&dir, &e.redo.clone(), OpKind::Redo(e), None)),
            _ => None,
        });
        let again = redone.and_then(|(r, kind)| match kind {
            OpKind::Redo(e) if r.is_ok() => {
                Some(run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None).0)
            }
            _ => None,
        });
        let count = git::run(Some(&dir), &["stash", "list"]).map(|s| s.lines().count());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(older.is_ok());
        assert!(matches!(again, Some(Err(_))));
        assert_eq!(count, Ok(1));
    }

    /// A quick edit's rebase does not start once HEAD moved past the commits it read.
    #[test]
    fn an_expected_head_that_moved_stops_the_operation() {
        let dir = repo("expect-head");
        let read = git::head(&dir);
        let made = git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "since"],
        );
        let since = git::head(&dir);
        let step = ops(&[&["commit", "-q", "--allow-empty", "-m", "op"]]);
        let guarded = |at: &str| {
            [super::expect_ref_at("HEAD", at)]
                .into_iter()
                .chain(step.clone())
                .collect::<Vec<_>>()
        };
        let (refused, _) = run_steps(&dir, &guarded(&read), OpKind::plain(), None);
        let still = git::head(&dir);
        let (ran, _) = run_steps(&dir, &guarded(&since), OpKind::plain(), None);
        let after = git::head(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(made.is_ok());
        assert!(refused.is_err());
        assert_eq!(still, since);
        assert!(ran.is_ok());
        assert_ne!(after, since);
    }

    #[test]
    fn checkout_pull_and_redo_refuse_an_externally_advanced_cached_target() -> Result<(), String> {
        let dir = repo("checkout-pull-stale-target");
        let was = git::head(&dir);
        let steps = crate::repo::commands::checkout_pull_steps("target", &was);
        let redo = match checkout_pull_undo("main", "target", &was, &steps) {
            Undo::Custom { redo, .. } => redo,
            _ => return Err("missing checkout pull undo".into()),
        };
        assert_eq!(steps, redo);
        git::run(Some(&dir), &["checkout", "-qb", "target"])?;
        git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "external advance"],
        )?;
        let advanced = git::head(&dir);
        git::run(Some(&dir), &["checkout", "-q", "main"])?;
        for run in [&steps, &redo] {
            let (result, _) = run_steps(&dir, run, OpKind::plain(), None);
            assert!(result.is_err());
            assert_eq!(git::head(&dir), was);
            assert_eq!(git::resolve(&dir, "refs/heads/target")?, advanced);
            assert_eq!(
                git::run(Some(&dir), &["symbolic-ref", "--short", "HEAD"])?.trim(),
                "main"
            );
        }
        let _ = std::fs::remove_dir_all(dir);
        Ok(())
    }

    /// Undoing "Checkout and pull" does not put the branch back over commits made on it since.
    #[test]
    fn undoing_checkout_and_pull_keeps_commits_made_since() {
        let dir = repo("pull-since");
        let was = git::head(&dir);
        let steps = [
            &["checkout", "-q", "-b", "side"][..],
            &["commit", "-q", "--allow-empty", "-m", "pulled"],
        ];
        for args in steps {
            assert!(git::run(Some(&dir), args).is_ok());
        }
        // As if `main` had been checked out and pulled to `pulled` from `side`.
        let pulled = git::head(&dir);
        let reset = git::run(Some(&dir), &["branch", "-f", "main", &pulled]);
        let checkout = ops(&[&["checkout", "main", "--"]]);
        let undo = checkout_pull_undo("side", "main", &was, &checkout);
        let (result, kind) = run_steps(&dir, &checkout, OpKind::Normal(undo, String::new()), None);
        let entry = entry(kind);
        // Then a commit on main.
        let more = git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "mine"]);
        let mine = git::head(&dir);
        let refused = entry
            .as_ref()
            .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None).0);
        let main_now = git::resolve(&dir, "refs/heads/main");
        let head_now = git::head(&dir);
        // Without it, undoing goes through.
        let back = git::run(Some(&dir), &["reset", "-q", "--hard", &pulled]);
        let undone = entry
            .as_ref()
            .map(|e| run_steps(&dir, &e.undo, OpKind::Undo(e.clone()), None).0);
        let main_after = git::resolve(&dir, "refs/heads/main");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(reset.is_ok() && result.is_ok() && more.is_ok() && back.is_ok());
        assert!(matches!(refused, Some(Err(_))));
        assert_eq!(main_now, Ok(mine.clone()));
        assert_eq!(head_now, mine);
        assert!(matches!(undone, Some(Ok(_))));
        assert_eq!(main_after, Ok(was));
    }
}

#[cfg(test)]
mod signout_tests {
    use super::*;
    #[test]
    fn signout_clears_nested_parked_credentials_before_a_view_is_reopened() -> Result<(), String> {
        let ctx = egui::Context::default();
        let url = "https://push-a.invalid/team/repo.git";
        let mut main = RepoTab::new_deferred("/nonexistent/signout-main".into(), &ctx);
        let mut parked = RepoTab::new_deferred("/nonexistent/signout-worktree".into(), &ctx);
        let mut nested = RepoTab::new_deferred("/nonexistent/signout-nested".into(), &ctx);
        let unrelated = git::Credentials::for_url(
            "https://push-b.invalid/team/repo.git",
            "other".into(),
            "DUMMY_OTHER".into(),
        )?;
        main.credentials = Some(unrelated);
        parked.credentials = Some(git::Credentials::for_url(
            url,
            "old-account".into(),
            "DUMMY_OLD".into(),
        )?);
        nested.credentials = parked.credentials.clone();
        parked.parked.push(Box::new(nested));
        let path = parked.path.clone();
        main.parked.push(Box::new(parked));
        main.forget_https_memory(std::path::Path::new("/nonexistent/another-tab"), url);
        assert!(main.credentials.is_some());
        let reopened = main.take_parked(&path).ok_or("Parked view missing")?;
        assert!(reopened.credentials.is_none());
        assert!(reopened.parked[0].credentials.is_none());
        Ok(())
    }
}

#[cfg(test)]
mod core_guard_paint_tests {
    use super::*;
    use crate::repo::push_preflight::core_guard_paint_tests::paint;

    #[test]
    fn core_guard_paint_reviewed_auth_extracts_original_proof_before_display() -> Result<(), String>
    {
        let binding: git::RouteBinding = serde_json::from_value(serde_json::json!({
            "args": ["push", "--", "remote-12345", "HEAD:refs/heads/topic-12345"],
            "routes": ["https://push.invalid/team/12345"],
            "configuration": "opaque-configuration-12345"
        }))
        .map_err(|error| error.to_string())?;
        let original = binding.args.clone();
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            let receipt =
                crate::push_preflight::remember_auth_endpoint("https://push.invalid/team/12345");
            let before = receipt.clone();
            let control = git::OpControl::new(|| {});
            let captured = std::cell::Cell::new(false);
            let result = run_reviewed_push(
                &original,
                |args| {
                    assert_eq!(
                        args,
                        original.iter().map(String::as_str).collect::<Vec<_>>()
                    );
                    captured.set(true);
                    Ok(binding.clone())
                },
                || {
                    assert!(captured.get());
                    Err(receipt.clone())
                },
                Some(&control),
            );
            assert_eq!(result, Err(before.clone()));
            let failure = control.auth_failure().ok_or("typed authentication proof")?;
            assert!(failure.confirmed);
            assert_eq!(failure.binding, Some(binding.clone()));
            assert_eq!(failure.endpoints, ["https://push.invalid/team/12345"]);
            assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());
            // Presentation follows actual one-shot extraction; no receipt is sent to a translator.
            let shown = paint(&egui::Context::default(), |ui| {
                ui.label(crate::push_preflight::display::reviewed_auth_failure());
            });
            assert!(shown.contains(if locale == "de" {
                "Authentifizierung"
            } else {
                "Authentication"
            }));
            assert!(!shown.contains("gitcraft-reviewed-auth"));
            assert!(crate::push_preflight::is_reviewed_auth_error(&receipt));
            assert_eq!(receipt, before);
            assert_eq!(crate::push_preflight::APPROVED, "\0push-preflight");
            assert_eq!(crate::push_preflight::FIX, "\0push-preflight-fix");
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn recording_approved_fork_capture_binds_only_successful_exact_pushed_tips() -> Result<(), String> {
    use super::forks::guard;
    const UNBOUND: &str = "\0fork-pushed-tip";
    let expected = "a".repeat(40);
    let tip = "b".repeat(40);
    let route = git::repository_tools::routes::PushRoute {
        branch: "topic".into(),
        commit: tip.clone(),
        remote: "fork".into(),
        destination: "refs/heads/topic".into(),
        urls: vec!["https://forge.invalid/team/fork".into()],
        lease: Some(expected.clone()),
    };
    let plan = crate::push_preflight::selected_continuation_recording::plan(&route);
    let original = crate::push_preflight::push_args(&plan)?;
    let approved = crate::push_preflight::approved_ops(plan);
    let marker = approved.last().ok_or("missing APPROVED marker")?;
    assert_eq!(marker[0], crate::push_preflight::APPROVED);
    let captured = reviewed_push_args(marker)?.ok_or("missing APPROVED capture")?;
    assert_eq!(captured, original);
    assert!(captured.contains(&format!("--force-with-lease=refs/heads/topic:{expected}")));
    let prepared = guard::prepare_push_with(&captured, true, None, |source| {
        assert_eq!(source, tip);
        Ok(tip.clone())
    })?
    .ok_or("reviewed fork source evidence missing")?;
    assert_eq!(prepared.0, original);
    for lease in [
        "--force-with-lease=refs/heads/topic:",
        "--force-with-lease=refs/heads/topic",
    ] {
        let mut unsupported_fork = captured.clone();
        let at = unsupported_fork
            .iter()
            .position(|arg| arg.starts_with("--force-with-lease="))
            .ok_or("missing fixed lease")?;
        unsupported_fork[at] = lease.into();
        let resolved = std::cell::Cell::new(false);
        assert!(
            guard::prepare_push_with(&unsupported_fork, true, None, |_| {
                resolved.set(true);
                Err("unexpected resolution".into())
            })
            .is_err()
        );
        assert!(
            !resolved.get(),
            "fork lease must remain fixed before resolving"
        );
    }
    let inverse = vec![vec![
        "push".into(),
        format!("--force-with-lease=refs/heads/topic:{}", UNBOUND),
        "--".into(),
        "fork".into(),
        format!("{expected}:refs/heads/topic"),
    ]];
    for result in [
        Ok(String::new()),
        Err("auth failure".into()),
        Err(git::CANCELLED.into()),
        Err("stale guard".into()),
    ] {
        let mut pushed = Vec::new();
        guard::completed_push(&result, Some(prepared.clone()), &mut pushed);
        let mut undo = inverse.clone();
        guard::bind_leases(&mut undo, &pushed);
        assert_eq!(pushed.len(), usize::from(result.is_ok()));
        let lease = if result.is_ok() {
            tip.as_str()
        } else {
            UNBOUND
        };
        assert_eq!(
            undo[0][1],
            format!("--force-with-lease=refs/heads/topic:{lease}")
        );
        // An error never turns the unresolved inverse into an executable push.
        if result.is_err() {
            let resolved = std::cell::Cell::new(false);
            assert!(
                guard::prepare_push_with(&undo[0], true, None, |_| {
                    resolved.set(true);
                    Err("unexpected resolution".into())
                })
                .is_err()
            );
            assert!(!resolved.get(), "invalid lease must not resolve");
        }
    }
    let mut other = inverse.clone();
    other[0][3] = "another-fork".into();
    let mut pushed = Vec::new();
    guard::completed_push(&Ok(String::new()), Some(prepared), &mut pushed);
    guard::bind_leases(&mut other, &pushed);
    assert_eq!(other[0][1], inverse[0][1]);
    let mut malformed = marker.clone();
    malformed.push("extra".into());
    assert!(reviewed_push_args(&malformed).is_err());
    Ok(())
}
