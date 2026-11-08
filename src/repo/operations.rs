//! Running git commands in the background, taking in how they ended, and undo / redo.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::messages::spawn_mutation;
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
                self.status = Some((
                    format!("{label}: not started, {busy} is still running"),
                    true,
                ));
            }
            return;
        }
        // A push asked to skip its hooks, this once (set only while that push starts).
        let ops = self.skip_hooks.push_steps(ops);
        if let Some(why) = offline_refusal(&ops) {
            if !quiet {
                self.status = Some((format!("{label}: not started. {why}"), true));
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
            let (result, kind, stopped) =
                run_steps_counted(&path, &ops, kind, creds.as_ref(), quiet, Some(&control));
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
            self.status = Some((
                format!("Wait until {busy} finishes before switching sign-ins"),
                true,
            ));
            return;
        }
        let challenge = self
            .https_challenge
            .as_ref()
            .filter(|ticket| self.https_ticket_current(ticket))
            .cloned();
        let Some(binding) = challenge.as_ref().and_then(|ticket| ticket.binding.clone()) else {
            self.invalidate_https_challenge();
            self.status=Some(("This sign-in has no verified operation route. Run the operation again before switching accounts.".into(),true));
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
            self.status = Some(("The sign-in destination or account context changed. Run the operation again before signing in.".into(), true));
            return;
        }
        let Some(ticket) = challenge else {
            return;
        };
        let Some(binding) = ticket.binding.clone() else {
            self.invalidate_https_challenge();
            self.status = Some((
                "This sign-in has no verified destination snapshot. Run the operation again."
                    .into(),
                true,
            ));
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
        self.status = Some(("Checking the sign-in destination…".into(), false));
        let path = self.path.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || binding.verify(&path),
            move |tab, result, ctx| {
                if !tab.https_ticket_current(&ticket) {
                    tab.status = Some((
                        "Sign-in was discarded because the operation or connection changed.".into(),
                        true,
                    ));
                    return;
                }
                tab.https_checking = false;
                if let Err(error) = result {
                    tab.invalidate_https_challenge();
                    tab.status = Some((error, true));
                    return;
                }
                if let Some(binding) = ticket.binding
                    && let Some(op) = &mut tab.last_op
                {
                    match serde_json::to_string(&binding) {
                        Ok(json) => op.ops.insert(0, vec![EXPECT_HTTPS_ROUTE.into(), json]),
                        Err(_) => {
                            tab.status = Some((
                                "Could not preserve the sign-in route. Run the operation again."
                                    .into(),
                                true,
                            ));
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
            self.status = Some((
                format!("{}: run it again once {busy} is done", op.label),
                true,
            ));
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
        let steps = running.map(|r| r.control.take_steps()).unwrap_or_default();
        self.log_activity(&label, &result, cancelled, quiet, steps);
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
                    self.status = Some((format!("{label}: done, but {note}"), true));
                } else if refresh == Refresh::All && !quiet {
                    self.status = Some((format!("{label}: done"), false));
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
                self.status = Some((format!("{label}: {e}"), left_locks));
            }
            Err(e) if git::is_auth_error(e) && !quiet => {
                self.agent_needs_sign_in(&label);
                // Typed credentials that were refused are forgotten.
                let typed = self.credentials.take().is_some();
                let refused = typed || e.contains("401") || e.contains("403");
                self.status = Some((format!("{label}: sign-in needed"), true));
                // Only a URL reported by the failed step, or its captured destinations.
                // An absent endpoint must never be replaced with the default fetch URL.
                if crate::push_preflight::is_reviewed_auth_error(e) && auth_failure.is_none() {
                    self.invalidate_https_challenge();
                    self.status = Some((
                        "The reviewed push's sign-in proof is unavailable. Reopen push preflight before signing in.".into(),
                        true,
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
                self.status = Some((format!("{label} failed: {e}"), true));
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
            self.finished.push(match &result {
                Ok(_) => (format!("{} · {label}: done", self.name), false),
                Err(e) => (
                    format!("{} · {label} failed: {}", self.name, headline(e)),
                    true,
                ),
            });
        }
        self.fold_op_done(&label, &result, ctx);
        self.dco_op_done(&label, &result, ctx);
        self.forks_op_done(&label, &result, ctx);
        self.hotfix_op_done(&label, &result, ctx);
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
                return Err(format!(
                    "Could not verify the network operation's destinations; command not started: {}",
                    crate::diagnostics::crash::scrub(&error)
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
        _ => Err("Invalid reviewed push execution evidence".into()),
    }
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

fn run_reviewed_push(
    original: &[String],
    capture: impl FnOnce(&[&str]) -> Result<git::RouteBinding, String>,
    execute: impl FnOnce() -> Result<String, String>,
    control: Option<&Arc<git::OpControl>>,
) -> Result<String, String> {
    let context = crate::connections::current();
    let args: Vec<_> = original.iter().map(String::as_str).collect();
    let binding = capture(&args).map_err(|error| {
        format!(
            "Could not verify the reviewed push's destinations; command not started: {}",
            crate::diagnostics::crash::scrub(&error)
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
                .map_err(|_| "Invalid sign-in route proof".to_owned())?;
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
    network.then(|| {
        format!(
            "{}: turn it off in the toolbar or Settings → Git.",
            pause.why()
        )
    })
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
                Err(format!(
                    "HEAD has moved since (it is at {}, not {}): resetting it would lose what \
                     was done after",
                    super::short(&now),
                    super::short(expected)
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
                return Err("HEAD moved while preparing this operation; reopen its preview. No operation was run.".into());
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
                    git::drafts::capture(path, "Before checkout / rebase", &active)?;
                }
                Ok::<_, String>(())
            })();
            snapshot.map_err(|error| format!("Working draft safety check: {error}"))
        },
    );
    if let Err(error) = prepared {
        return (Err(error), kind, 0);
    }
    let mut result = Ok(String::new());
    let mut stopped = ops.len();
    let mut tried = Vec::new();
    for (i, op) in ops.iter().enumerate() {
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
        let args: Vec<&str> = op.iter().map(String::as_str).collect();
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
        OpKind::Normal(Undo::Custom { mut undo, redo }, before) if result.is_ok() => {
            fill_expected(path, &mut undo);
            OpKind::Normal(Undo::Custom { undo, redo }, before)
        }
        OpKind::Undo(mut entry) if result.is_ok() => {
            fill_expected(path, &mut entry.redo);
            OpKind::Undo(entry)
        }
        // Redone with nothing to stash, the undo keeps expecting the stash it popped, and so
        // refuses rather than pop an older one.
        OpKind::Redo(mut entry) if result.is_ok() && !no_stash => {
            fill_expected(path, &mut entry.undo);
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
    expected_ref(op).is_some()
        || expected_symref(op).is_some()
        || expected_tree(op).is_some()
        || matches!(op, [mark, _, _] if mark == crate::gerrit::CHECK)
        || matches!(op, [mark, _] if mark == EXPECT_HTTPS_ROUTE)
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
            crate::gerrit::check_stack_with(base, head, |args| {
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
            return Err(format!(
                "{rev} isn't what this made any more (it was made again since, with other \
                 changes): going ahead would lose those"
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
                    "a detached HEAD".to_owned()
                } else {
                    r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()
                }
            };
            let now = now.map_or_else(|_| "something else".to_owned(), |r| says(&r));
            let place = dir.map_or(String::new(), |d| format!(" in the worktree at {d}"));
            return Err(format!(
                "{now} is checked out{place} now, not {}: going ahead would change it instead",
                says(target)
            ));
        }
    }
    for (name, value) in ops.iter().filter_map(|op| expected_ref(op)) {
        let now = git::resolve(path, name).unwrap_or_default();
        if now != value {
            let what = name.strip_prefix("refs/heads/").unwrap_or(name);
            return Err(if name == "refs/stash" {
                "The newest stash is not the one this made any more".to_owned()
            } else if value.is_empty() {
                format!("{what} has commits now: going ahead would go on top of them")
            } else {
                format!(
                    "{what} has moved since (it is at {}, not {}): going ahead would lose \
                     what was done after",
                    super::short(&now),
                    super::short(value)
                )
            });
        }
    }
    Ok(())
}

fn fill_expected(path: &Path, ops: &mut [Vec<String>]) {
    for op in ops {
        if let Some((name, _)) = expected_ref(op) {
            let now = git::resolve(path, name).unwrap_or_default();
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
