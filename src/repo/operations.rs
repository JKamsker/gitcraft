//! Running git commands in the background, taking in how they ended, and undo / redo.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::messages::spawn_or;
use super::toolbar::headline;
use super::{Dialog, Msg, OpKind, PendingOp, Refresh, RepoTab, Undo, UndoEntry, ops};
use crate::git;

/// An operation that takes longer than this gets a notification when it ends.
const LONG_OP: Duration = Duration::from_secs(4);

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
        self.busy_op = Some(label.to_owned());
        self.op_started = Some(Instant::now());
        let control = self.follow_op(&ops, ctx);
        let cancellable = self.can_cancel();
        self.note_checkout_move(&ops, ctx);
        if !quiet {
            self.status = None;
            // A fix made stays, for its undo, while what it fixed runs again.
            self.error_card.take_if(|card| !card.fixed());
        }
        // A quiet one (the background fetch) never asks to sign in, and must not take the place
        // of the operation that did while its sign-in dialog is open.
        if !quiet {
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
        spawn_or(&self.tx, ctx, failed, move || {
            // The locks there already belong to someone else: only new ones are the cancel's.
            let locks_before = if cancellable {
                git::leftover_locks(&path)
            } else {
                Vec::new()
            };
            let (result, kind, stopped) =
                run_steps_counted(&path, &ops, kind, creds.as_ref(), quiet, Some(&control));
            let result = match result {
                Err(e) if control.is_cancelled() && !e.contains("CONFLICT") => {
                    Err(cancelled_message(&path, &ops, &locks_before))
                }
                result => result,
            };
            Msg::OpDone {
                label,
                result,
                refresh,
                kind,
                quiet,
                stopped,
            }
        });
    }

    /// Leaves out of the operation kept to run again after signing in the steps that ran
    /// before step `stopped` failed: they are done, and most would fail a second time (a tag or
    /// remote added again, a branch deleted again), so that the step that needed signing in
    /// would never run.
    pub(super) fn keep_unfinished(&mut self, stopped: usize) {
        if let Some(op) = &mut self.last_op {
            // The checks stay: what they guard must still hold when it runs after signing in.
            let mut i = 0;
            op.ops.retain(|step| {
                i += 1;
                i > stopped || is_expectation(step)
            });
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

    /// Remembers the credentials and runs the operation that needed them again.
    pub(super) fn sign_in(&mut self, creds: git::Credentials, ctx: &egui::Context) {
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
            self.take_back(&op.kind);
            self.run_ops_ex(&op.label, op.ops, op.refresh, op.kind, op.quiet, ctx);
        }
    }

    /// Takes the undo or redo of `kind` off its stack again, to run it: it went back there when
    /// it needed signing in, so that cancelling the sign-in keeps it (see `op_done`).
    fn take_back(&mut self, kind: &OpKind) {
        let (stack, entry) = match kind {
            OpKind::Undo(entry) => (&mut self.undo_stack, entry),
            OpKind::Redo(entry) => (&mut self.redo_stack, entry),
            OpKind::Normal(..) => return,
        };
        if let Some(i) = stack.iter().rposition(|e| e == entry) {
            stack.remove(i);
        }
    }

    pub(super) fn undo(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        if let Some(entry) = self.undo_stack.pop() {
            let label = format!("Undo {}", entry.label);
            let steps = entry.undo.clone();
            self.run_ops(&label, steps, Refresh::All, OpKind::Undo(entry), ctx);
        }
    }

    pub(super) fn redo(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        if let Some(entry) = self.redo_stack.pop() {
            let label = format!("Redo {}", entry.label);
            let steps = entry.redo.clone();
            self.run_ops(&label, steps, Refresh::All, OpKind::Redo(entry), ctx);
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
        self.busy_op = None;
        self.detached_op_done(&label, result.is_ok());
        let mut undoable = false;
        self.dropped_patch_done(&label, &result);
        // Done with it before anything starts after it (updating the submodules).
        let running = self.running.take();
        let cancelled =
            result.is_err() && running.as_ref().is_some_and(|r| r.control.is_cancelled());
        let steps = running.map(|r| r.control.take_steps()).unwrap_or_default();
        self.log_activity(&label, &result, cancelled, quiet, steps);
        let long = self
            .op_started
            .take()
            .is_some_and(|t| t.elapsed() > LONG_OP && !cancelled);
        match &result {
            Ok(_) => {
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
                undoable = self.record_undo(&label, kind);
            }
            Err(e) if cancelled => {
                self.open_when_done.take_if(|(l, _)| *l == label);
                // Red only when it left something to clear up.
                let left_locks = e.starts_with(&format!("{}, but", git::CANCELLED));
                self.status = Some((format!("{label}: {e}"), left_locks));
                // Not done: what would undo it stays where it was.
                match kind {
                    OpKind::Undo(entry) => self.undo_stack.push(entry),
                    OpKind::Redo(entry) => self.redo_stack.push(entry),
                    OpKind::Normal(..) => {}
                }
            }
            Err(e) if git::is_auth_error(e) && !quiet => {
                self.agent_needs_sign_in(&label);
                // Typed credentials that were refused are forgotten.
                let refused = self.credentials.take().is_some();
                self.status = Some((format!("{label}: sign-in needed"), true));
                let url = self
                    .default_remote()
                    .and_then(|r| self.remotes.iter().find(|x| x.name == r))
                    .map(|r| r.url.clone())
                    .unwrap_or_default();
                self.dialog = Some(Dialog::credentials(&url, refused));
                // Back on its stack until signing in runs it again: the sign-in can be
                // cancelled, and the undo must not be lost then.
                match kind {
                    OpKind::Undo(entry) => self.undo_stack.push(entry),
                    OpKind::Redo(entry) => self.redo_stack.push(entry),
                    OpKind::Normal(..) => {}
                }
            }
            Err(e) => {
                self.open_when_done.take_if(|(l, _)| *l == label);
                self.focus_wip = e.contains("CONFLICT") || e.contains("could not apply");
                self.status = Some((format!("{label} failed: {e}"), true));
                if !quiet {
                    self.explain_failure(e, &label, ctx);
                }
                self.note_stopped_rewrite(&label, &kind, e);
                // A failed undo stays undoable.
                match kind {
                    OpKind::Undo(entry) => self.undo_stack.push(entry),
                    OpKind::Redo(entry) => self.redo_stack.push(entry),
                    OpKind::Normal(..) => {}
                }
            }
        }
        self.agent_op_done(&label, &result, undoable);
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
        match refresh {
            Refresh::Status => self.reload_status(ctx),
            Refresh::All => self.reload(ctx),
        }
        self.follow_checkout_move(result.is_ok(), ctx);
        // Last: it may start the next operation (Sync's push after its pull).
        self.push_op_ended(&label, &result, ctx);
        self.after_op(result.is_ok(), quiet, ctx);
        self.link_status();
    }

    /// Puts what `kind` did on the undo (or redo) stack. Returns whether a new step went on
    /// the undo list.
    pub(super) fn record_undo(&mut self, label: &str, kind: OpKind) -> bool {
        let mut added = false;
        match kind {
            OpKind::Normal(Undo::None, _) => {}
            OpKind::Normal(Undo::HeadMove { mode }, heads) => {
                let mut parts = heads.split('\u{0}');
                let (before, after) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                if before.is_empty() || after.is_empty() || before == after {
                    // Nothing to undo (a first commit), but HEAD moved: what was undone
                    // before is not redone on top of it.
                    if before != after {
                        self.redo_stack.clear();
                    }
                    return false;
                }
                // Resetting moves whatever branch is checked out: another one at the same
                // commit passes the check of HEAD's commit, so the branch is checked too.
                let guard: Vec<Vec<String>> = parts
                    .next()
                    .map(|branch| vec![expect_symref("HEAD", branch)])
                    .unwrap_or_default();
                let reset = |to: &str| [guard.clone(), ops(&[&["reset", mode, to]])].concat();
                self.undo_stack.push(UndoEntry {
                    label: label.to_owned(),
                    undo: reset(before),
                    redo: reset(after),
                    heads: Some((before.to_owned(), after.to_owned())),
                    agent: None,
                });
                self.redo_stack.clear();
                added = true;
            }
            OpKind::Normal(Undo::Custom { undo, redo }, _) => {
                self.undo_stack.push(UndoEntry {
                    label: label.to_owned(),
                    undo,
                    redo,
                    heads: None,
                    agent: None,
                });
                self.redo_stack.clear();
                added = true;
            }
            OpKind::Undo(entry) => self.redo_stack.push(entry),
            OpKind::Redo(entry) => self.undo_stack.push(entry),
        }
        // Plenty for a session.
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        added
    }
}

/// Runs one step of an operation: one that goes over the network followed through `control`
/// (its progress shown, and stopped on a cancel), any other as it is; none once cancelled.
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
    // Each call kept for the activity log.
    let (result, recorded) = if git::goes_over_network(args) {
        git::run_watched_recorded(Some(path), args, creds, quiet, control)
    } else {
        git::run_recorded(Some(path), args, creds, quiet)
    };
    let mut recorded = recorded;
    recorded.output = super::activity::bounded(&recorded.output);
    control.record(recorded);
    result
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
pub(super) fn reaches_a_remote(op: &[String]) -> bool {
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
        Some("fetch" | "pull" | "push" | "clone" | "ls-remote") => true,
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
    // Checked before anything runs, so a refused undo changes nothing.
    if let Err(e) = check_expected(path, ops) {
        return (Err(e), kind, 0);
    }
    let mut result = Ok(String::new());
    let mut stopped = ops.len();
    let mut tried = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        if is_expectation(op) {
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
        result = run_step(path, &args, creds, quiet, control);
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
pub(super) fn expect_ref_at(name: &str, commit: &str) -> Vec<String> {
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

/// The ref and the commit an `expect_ref` step expects.
fn expected_ref(op: &[String]) -> Option<(&str, &str)> {
    match op {
        [mark, name, value] if mark == EXPECT => Some((name, value)),
        _ => None,
    }
}

/// The symbolic ref and the ref it should point to of an `expect_symref` step.
fn expected_symref(op: &[String]) -> Option<(&str, &str)> {
    match op {
        [mark, name, target] if mark == EXPECT_SYMREF => Some((name, target)),
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
    expected_ref(op).is_some() || expected_symref(op).is_some() || expected_tree(op).is_some()
}

fn check_expected(path: &Path, ops: &[Vec<String>]) -> Result<(), String> {
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
    for (name, target) in ops.iter().filter_map(|op| expected_symref(op)) {
        // Exit 1, nothing printed: detached.
        let now = git::run_ok_codes(Some(path), &["symbolic-ref", "-q", name], &[0, 1])
            .map(|s| s.trim().to_owned());
        if now.as_deref() != Ok(target) {
            let says = |r: &str| {
                if r.is_empty() {
                    "a detached HEAD".to_owned()
                } else {
                    r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()
                }
            };
            let now = now.map_or_else(|_| "something else".to_owned(), |r| says(&r));
            return Err(format!(
                "{now} is checked out now, not {}: going ahead would change it instead",
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
                label: "op".to_owned(),
                undo,
                redo,
                heads: None,
                agent: None,
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
