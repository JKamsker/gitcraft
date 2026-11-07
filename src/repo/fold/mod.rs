//! Folding the staged changes into an older commit of the checked-out branch: from a commit's
//! menu, or the picker beside _Amend_ in the commit box. A dialog shows the commit and those
//! after it that get rewritten (and which are pushed); then the changes are committed as
//! `fixup! <summary>`, and either kept so or melded into the commit by a rebase without the
//! editor. One undo puts the branch back with the changes staged again; a rebase stopped on a
//! conflict goes on in the usual conflict flow, and aborting it goes back to before the fold.

pub(crate) mod plan;
#[cfg(test)]
mod tests;

use std::borrow::Borrow;

use egui::{RichText, Ui};

use super::quick_edit::first_parents;
use super::{Action, Dialog, OpKind, Refresh, RepoTab, abort_action, short};
use crate::git::{self, Commit, InProgress};
use crate::theme::Tokens;
pub(crate) use plan::Plan;
pub(super) use plan::Stopped;

/// How many commits of the branch the commit box's picker offers.
const PICKER_COMMITS: usize = 15;

/// A fold running or stopped on a conflict.
#[derive(Default)]
pub(super) struct FoldState {
    /// The fold running now, until it ends.
    running: Option<Stopped>,
    /// The fold whose rebase stopped, until it is continued to the end or aborted.
    stopped: Option<Stopped>,
    /// The fold being taken back because its result lost some of the staged changes.
    putting_back: Option<Stopped>,
}

impl RepoTab {
    /// Why nothing can be folded now, whatever the commit, as the last read saw it.
    fn fold_blocked(&self) -> Option<String> {
        if self.refs.head.is_empty() {
            return Some("There are no commits yet".to_owned());
        }
        plan::blocked(
            self.work.staged.len(),
            self.refs.current.as_deref(),
            self.work.in_progress,
        )
    }

    /// Why the staged changes can't go into `hash` now.
    fn fold_refusal(&self, line: &[&Commit], hash: &str) -> Option<String> {
        self.fold_blocked()
            .or_else(|| plan::refusal(line, hash).map(str::to_owned))
    }

    /// _Fold staged changes into this commit_ in a commit's menu, disabled with the reason.
    pub(super) fn fold_item(&self, ui: &mut Ui, hash: &str, idle: bool, acts: &mut Vec<Action>) {
        let line = first_parents(&self.commits, &self.index, &self.refs.head);
        let refused = self.fold_refusal(&line, hash);
        let button = ui
            .add_enabled(
                idle && refused.is_none(),
                egui::Button::new("Fold staged changes into this commit"),
            )
            .on_hover_text(
                "Put the staged changes into this commit, rewriting the commits after it",
            );
        let button = match &refused {
            Some(why) => button.on_disabled_hover_text(why),
            None => button,
        };
        if button.clicked() {
            acts.push(Action::FoldInto(hash.to_owned()));
        }
    }

    /// _Fold…_ beside _Amend last commit_: the latest commits of the branch to fold the staged
    /// changes into. `amending`: that box is ticked, which the fold would not follow.
    pub(super) fn fold_picker(&self, ui: &mut Ui, amending: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let blocked = self.fold_blocked().or_else(|| {
            amending.then(|| "Untick Amend last commit to fold into an older commit".to_owned())
        });
        let button = ui
            .add_enabled(idle && blocked.is_none(), egui::Button::new("Fold…"))
            .on_hover_text("Fold the staged changes into an older commit of this branch");
        let button = match &blocked {
            Some(why) => button.on_disabled_hover_text(why),
            None => button,
        };
        egui::Popup::menu(&button)
            .style(crate::theme::menu_style)
            .show(|ui| {
                // Walked only while the picker is open.
                let line = first_parents(&self.commits, &self.index, &self.refs.head);
                ui.set_max_width(360.0);
                ui.label(
                    RichText::new("Fold the staged changes into:")
                        .color(t.text_faint)
                        .small(),
                );
                for commit in line.iter().take(PICKER_COMMITS) {
                    let refused = self.fold_refusal(&line, &commit.hash);
                    let text = format!("{}  {}", short(&commit.hash), commit.summary);
                    let item =
                        ui.add_enabled(refused.is_none(), egui::Button::new(text).truncate());
                    let item = match &refused {
                        Some(why) => item.on_disabled_hover_text(why),
                        None => item,
                    };
                    if item.clicked() {
                        acts.push(Action::FoldInto(commit.hash.clone()));
                    }
                }
                if line.len() > PICKER_COMMITS {
                    ui.label(
                        RichText::new("Older commits: right-click them in the graph")
                            .color(t.text_faint)
                            .small(),
                    );
                }
            });
    }

    /// Reads what folding into `hash` rewrites (in the background), then asks with the dialog.
    pub(super) fn fold_into(&mut self, hash: String, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::fold_read(&path, &hash).and_then(Plan::from_read),
            |tab, plan, _| match plan {
                Ok(plan) if tab.dialog.is_none() => {
                    tab.dialog = Some(Dialog::Fold(Box::new(plan)));
                }
                Ok(_) => {}
                Err(e) => tab.status = Some((format!("Fold staged changes: {e}"), true)),
            },
        );
    }

    /// Runs the fold the dialog confirmed.
    pub(super) fn run_fold(&mut self, plan: Plan, ctx: &egui::Context) {
        if let Some(busy) = &self.busy_op {
            self.status = Some((
                format!(
                    "{}: {busy} is still running, try again when it's done",
                    plan.label()
                ),
                true,
            ));
            return;
        }
        // What is going on now, not when the dialog read it: a `git am` stopped since (which
        // no guard step can see) would take the fixup commit as its own.
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::in_progress(&path),
            move |tab, state, ctx| match state {
                Ok(None) => tab.run_fold_now(plan, ctx),
                Ok(Some(state)) => {
                    tab.status = Some((
                        format!(
                            "{}: a {} is in progress now: finish or abort it first",
                            plan.label(),
                            state.label().to_lowercase()
                        ),
                        true,
                    ));
                }
                Err(e) => tab.status = Some((format!("{}: {e}", plan.label()), true)),
            },
        );
    }

    /// Runs the fold, nothing being in progress.
    fn run_fold_now(&mut self, plan: Plan, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some((
                format!("{}: something else started, try again", plan.label()),
                true,
            ));
            return;
        }
        let label = plan.label();
        if plan.keep_fixup {
            let (steps, undo) = plan.keep_steps();
            self.run_undoable(&label, steps, Refresh::All, undo, ctx);
            return;
        }
        match plan.editor() {
            Ok(editor) => self.with_rebase_command(editor, ctx, move |tab, command, ctx| {
                tab.fold_ready(&plan, command, ctx);
            }),
            Err(e) => self.status = Some((format!("{label}: {e}"), true)),
        }
    }

    /// Runs the fold once its rebase's todo list is written (on another machine, in the
    /// background).
    fn fold_ready(
        &mut self,
        plan: &Plan,
        command: Result<(Vec<String>, String), String>,
        ctx: &egui::Context,
    ) {
        let label = plan.label();
        match command {
            // Started while something else ran: say so rather than drop it.
            Ok(_) if self.busy_op.is_some() => {
                self.status = Some((
                    format!(
                        "{label}: {} is still running, try again when it's done",
                        self.busy_op.as_deref().unwrap_or_default()
                    ),
                    true,
                ));
            }
            Ok((rebase, _)) => {
                let (steps, undo) = plan.fold_steps(rebase);
                self.fold.running = Some(plan.stopped());
                self.run_undoable(&label, steps, Refresh::All, undo, ctx);
            }
            Err(e) => self.status = Some((format!("{label}: {e}"), true)),
        }
    }

    /// Takes in how an operation ended, for a fold: one that failed may have stopped in its
    /// rebase, to be continued or aborted; continued to the end or aborted, it is over.
    /// Called before the reload after the operation starts.
    pub(super) fn fold_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &egui::Context,
    ) {
        let ok = result.is_ok();
        if let Some(back) = self
            .fold
            .putting_back
            .take_if(|r| put_back_label(r) == label)
        {
            self.status = Some(match result {
                Ok(_) => (
                    format!(
                        "{}: nothing was changed. The staged changes undo what a later commit \
                         does, so they would have been lost in the commit; they are staged as \
                         before.",
                        back.label
                    ),
                    true,
                ),
                Err(e) => (
                    format!(
                        "{}: the result lost some of the staged changes, and going back failed \
                         ({e}). They are all in ORIG_HEAD, the fixup! commit.",
                        back.label
                    ),
                    true,
                ),
            });
            return;
        }
        if let Some(mut running) = self.fold.running.take_if(|r| r.label == label) {
            if let Err(e) = result
                && plan::lost_in_rebase(e)
            {
                // Done, but not as staged: taken back at once, before anything else runs.
                let steps = running.put_back_steps();
                let back = put_back_label(&running);
                self.fold.putting_back = Some(running);
                self.run_ops(&back, steps, Refresh::All, OpKind::plain(), ctx);
                return;
            }
            if let Err(e) = result {
                // The changes are in the fixup commit the rebase stopped on, not lost: say how
                // to go on rather than only that it failed.
                if e.contains("CONFLICT") || e.contains("could not apply") {
                    self.status = Some((
                        format!(
                            "{label} stopped on a conflict: resolve it and Continue rebase, or \
                             Abort to go back with the changes staged"
                        ),
                        true,
                    ));
                }
                // The status read the reload after this starts tells whether it stopped.
                running.read = Some(self.reads + 1);
                self.fold.stopped = Some(running);
            }
            return;
        }
        if ok && let Some(stopped) = self.fold.stopped.take_if(|s| label == abort_label(s)) {
            // Aborted: the changes staged again, checked again now that the abort is done.
            let steps = stopped.unfixup_steps();
            self.run_ops(
                &unfixup_label(&stopped),
                steps,
                Refresh::All,
                OpKind::plain(),
                ctx,
            );
            return;
        }
        let over = |s: &Stopped| label == s.label;
        if ok && self.fold.stopped.as_ref().is_some_and(over) {
            self.fold.stopped = None;
            // As after any rebase continued: what the box held was the rebase's.
            self.commit_summary.clear();
            self.commit_body.clear();
            self.amend = false;
        }
    }

    /// Takes in the repository read numbered `read` (its refs and history already in), with
    /// `state` in progress: a stopped fold whose rebase isn't going on any more (finished or
    /// aborted in the terminal, or never started) is forgotten. When it left its fixup commit
    /// on top (the rebase failed before it began, or was aborted elsewhere), that commit gets
    /// an undo of its own, which stages the changes again.
    pub(super) fn fold_loaded(&mut self, read: u64, state: Option<InProgress>, ctx: &egui::Context) {
        if !self
            .fold
            .stopped
            .as_ref()
            .is_some_and(|s| s.over(read, state))
        {
            return;
        }
        let Some(stopped) = self.fold.stopped.take() else {
            return;
        };
        let head = self.refs.head.clone();
        let on_top = self
            .history
            .iter()
            .find(|c| c.hash == head)
            .is_some_and(|c| c.parents.first() == Some(&stopped.before));
        if !on_top || self.refs.current.as_deref() != Some(stopped.branch.as_str()) {
            return;
        }
        self.remember_undo(&stopped.label, stopped.leftover_undo(&head), ctx);
        self.status = Some((
            format!(
                "{}: the rebase didn't run, so the changes are in a commit on top. Ctrl+Z \
                 stages them again.",
                stopped.label
            ),
            true,
        ));
    }

    /// The fold stopped in the rebase going on now: the branch is still the fixup commit on top
    /// of where the fold started (and not a rebase started since in the terminal).
    pub(super) fn fold_stopped_here(&self, state: InProgress) -> Option<&Stopped> {
        let stopped = self.fold.stopped.as_ref()?;
        let tip = self.refs.local.iter().find(|b| b.name == stopped.branch)?;
        let parent = self
            .index
            .get(&tip.target)
            .and_then(|&i| self.commits.get(i))
            .and_then(|c| Borrow::<Commit>::borrow(c).parents.first().cloned());
        (state == InProgress::Rebase && parent.as_deref() == Some(stopped.before.as_str()))
            .then_some(stopped)
    }

    /// Abort, asked first: for a fold stopped on a conflict, back to before the fold with the
    /// changes staged again.
    pub(super) fn abort_for(&self, state: InProgress) -> Action {
        match &self.fold.stopped {
            Some(_) if state == InProgress::Rebase => Action::AbortFold,
            _ => abort_action(state),
        }
    }

    /// Abort with a fold stopped: checks first (in the background) that the rebase going on is
    /// the fold's, on its branch, with the fixup commit right on top of where it started; a
    /// rebase started in its place (in the terminal) is aborted the ordinary way, and no other
    /// branch is reset.
    pub(super) fn abort_fold(&mut self, ctx: &egui::Context) {
        let Some(stopped) = self.fold.stopped.clone() else {
            return;
        };
        let path = self.path.clone();
        // The fixup commit, when the rebase going on is the fold's.
        let theirs = move || -> Result<Option<String>, String> {
            let branch = format!("refs/heads/{}", stopped.branch);
            let rebasing = git::rebasing_branch(&path)?;
            let parent = git::resolve(&path, &format!("{branch}~1")).unwrap_or_default();
            let ours = rebasing.as_deref() == Some(branch.as_str()) && parent == stopped.before;
            Ok(ours.then(|| git::resolve(&path, &branch).ok()).flatten())
        };
        self.then(ctx, theirs, |tab, ours, _| {
            if tab.dialog.is_some() {
                return;
            }
            let action = match (ours, &tab.fold.stopped) {
                (Ok(Some(fixup)), Some(stopped)) => fold_abort_dialog(stopped, &fixup),
                (Err(e), _) => {
                    tab.status = Some((format!("Abort rebase: {e}"), true));
                    return;
                }
                _ => abort_action(InProgress::Rebase),
            };
            if let Action::Dialog(dialog) = action {
                tab.dialog = Some(dialog);
            }
        });
    }
}

/// Asks before aborting the fold `stopped`: back to before it, the changes staged again.
fn fold_abort_dialog(stopped: &Stopped, fixup: &str) -> Action {
    Action::Dialog(Dialog::confirm(
        &abort_label(stopped),
        "Stop the fold and go back to where it started? The branch is put back as it was, \
         with the changes staged again. Conflicts resolved so far are lost, and can't be \
         brought back."
            .to_owned(),
        "Abort",
        stopped.abort_steps(fixup),
        None,
    ))
}

impl RepoTab {
    /// Continuing the rebase of a fold stopped on a conflict: its label and how it is undone
    /// (as the whole fold).
    pub(super) fn fold_continue(&self, state: InProgress) -> Option<(String, OpKind)> {
        self.fold_stopped_here(state)
            .map(|s| (s.label.clone(), s.continue_kind()))
    }
}

fn unfixup_label(stopped: &Stopped) -> String {
    format!(
        "Stage the changes of {} again",
        stopped.label.replacen("Fold", "fold", 1)
    )
}

fn put_back_label(stopped: &Stopped) -> String {
    format!("Take back {}", stopped.label.replacen("Fold", "fold", 1))
}

fn abort_label(stopped: &Stopped) -> String {
    format!("Abort {}", stopped.label.replacen("Fold", "fold", 1))
}
//! Files better read folded: generated ones (`linguist-generated`, `-diff` in
//! `.gitattributes`), lockfiles, minified files and the patterns chosen in the settings. They
//! start folded in file lists and diffs, behind a line like _+1,204 −880, lockfile_.

mod banner;
mod list;
mod rules;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(super) use banner::folded_diff;
pub(super) use list::split;
pub(super) use rules::{FoldReason, Rules, stats_line};

use crate::git::{FoldAttr, WorkStatus};

/// While a `.gitattributes` has changes, how often its files' attributes are read again (it
/// may have been edited since, and its status letter wouldn't say).
const REREAD_EVERY: Duration = Duration::from_secs(30);

/// What a tab knows of its files' attributes, asked of git in the background.
#[derive(Default)]
pub(crate) struct Folding {
    /// What the attributes said when last read, by path; kept while they are read again, so
    /// files don't jump out of their fold and back meanwhile.
    attrs: HashMap<String, FoldAttr>,
    /// Paths read (or given up on) since the attributes were last forgotten.
    fresh: HashSet<String>,
    /// Paths asked about, and when (by `epoch`): not asked again then.
    asked: HashMap<String, u64>,
    /// Changes each time answers come in or are forgotten: lists work out again what they fold.
    pub(super) generation: u64,
    /// Changes each time the attributes are forgotten. Shared with the reads under way, which
    /// stop when it moves on.
    epoch: Arc<AtomicU64>,
    /// What a `.gitattributes` looked like at the last status read (path and status letter of
    /// those changed) and the commit checked out: when either changes, the attributes may have.
    seen: (Vec<(String, char)>, String),
    /// When they were last forgotten.
    forgot: Option<Instant>,
    /// When to forget them next: a read that failed is tried again, an edit seen too soon
    /// after the last read is read once the interval is over.
    reread_at: Option<Instant>,
}

impl Folding {
    /// Why `path` is folded, if it is (`rules`: the settings'). While its attributes aren't
    /// known yet, only its name decides.
    pub(super) fn reason(&self, rules: &Rules, path: &str) -> Option<FoldReason> {
        rules.reason(path, self.attrs.get(path).copied())
    }

    /// Whether anything is known of the attributes of `path` yet.
    #[cfg(test)]
    pub(super) fn knows(&self, path: &str) -> bool {
        self.attrs.contains_key(path)
    }

    fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Relaxed)
    }

    /// Of `paths`, those not read since the attributes were last forgotten and not asked
    /// about since either.
    pub(super) fn unknown<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let epoch = self.epoch();
        let mut seen = HashSet::new();
        paths
            .into_iter()
            .filter(|p| !self.fresh.contains(*p) && self.asked.get(*p) != Some(&epoch))
            .filter(|p| seen.insert(*p))
            .map(str::to_owned)
            .collect()
    }

    /// `paths` are being asked about: not again until they are forgotten. Returns the epoch
    /// the answer is handed back with, and what tells the read whether it is still wanted.
    pub(super) fn asking(&mut self, paths: &[String]) -> (u64, Arc<AtomicU64>) {
        let epoch = self.epoch();
        for path in paths {
            self.asked.insert(path.clone(), epoch);
        }
        (epoch, self.epoch.clone())
    }

    /// The answer about `paths`, asked at `epoch`: what was `found`, and the error that left
    /// the others unread. Those keep their last answer (or their names decide) and are asked
    /// again after [`REREAD_EVERY`]. An answer asked before the attributes were forgotten only
    /// fills in paths not read since, and leaves them to be asked again.
    pub(super) fn arrived(
        &mut self,
        epoch: u64,
        paths: Vec<String>,
        mut found: HashMap<String, FoldAttr>,
        error: Option<String>,
    ) {
        let current = epoch == self.epoch();
        if current && error.is_some() {
            self.reread_at = Some(Instant::now() + REREAD_EVERY);
        }
        for path in paths {
            if self.asked.get(&path) == Some(&epoch) {
                self.asked.remove(&path);
            }
            if self.fresh.contains(&path) {
                continue;
            }
            match found.remove(&path) {
                Some(attr) => {
                    self.attrs.insert(path.clone(), attr);
                }
                None if current && error.is_none() => {
                    self.attrs.insert(path.clone(), FoldAttr::Unspecified);
                }
                None if current => {
                    // Unread: not asked again until it is retried.
                    self.attrs
                        .entry(path.clone())
                        .or_insert(FoldAttr::Unspecified);
                    self.asked.insert(path, epoch);
                    continue;
                }
                None => {
                    self.attrs
                        .entry(path.clone())
                        .or_insert(FoldAttr::Unspecified);
                }
            }
            if current {
                self.fresh.insert(path);
            }
        }
        self.generation += 1;
    }

    /// After a status read (at `now`): forgets the attributes when a `.gitattributes` changed
    /// or another commit is checked out, and every [`REREAD_EVERY`] while one has changes.
    pub(super) fn saw_status(&mut self, work: &WorkStatus, head: &str, now: Instant) {
        let changed: Vec<(String, char)> = work
            .staged
            .iter()
            .chain(&work.unstaged)
            .chain(&work.conflicted)
            // A `.gitattributes` renamed away counts too.
            .flat_map(|f| {
                let paths = [Some(&f.path), f.old_path.as_ref()];
                paths.into_iter().flatten().map(move |p| (p, f.code))
            })
            .filter(|(path, _)| is_attributes_file(path))
            .map(|(path, code)| (path.clone(), code))
            .collect();
        let seen = (changed, head.to_owned());
        let due = self
            .forgot
            .is_none_or(|at| now.saturating_duration_since(at) >= REREAD_EVERY);
        if seen != self.seen || (!seen.0.is_empty() && due) {
            self.seen = seen;
            self.forget(now);
        } else if !seen.0.is_empty() {
            // Edited again, perhaps, too soon after the last read: read once the interval is
            // over, whether or not anything else happens by then.
            let at = self.forgot.map_or(now, |at| at + REREAD_EVERY);
            self.reread_at = Some(self.reread_at.map_or(at, |due| due.min(at)));
        }
    }

    /// Each frame (at `now`): forgets the attributes when reading them again is due. Returns
    /// how long until then, to wake the window for it.
    pub(super) fn tick(&mut self, now: Instant) -> Option<Duration> {
        let at = self.reread_at?;
        if now < at {
            return Some(at - now);
        }
        self.forget(now);
        Some(Duration::ZERO)
    }

    /// Forgets what was read: the files of the lists shown are asked about again (their last
    /// answers apply meanwhile), and reads under way stop.
    fn forget(&mut self, now: Instant) {
        self.forgot = Some(now);
        self.reread_at = None;
        if self.fresh.is_empty() && self.asked.is_empty() {
            return;
        }
        self.epoch.fetch_add(1, Ordering::Relaxed);
        self.fresh.clear();
        self.asked.clear();
        self.generation += 1;
    }
}

/// Whether `path` is a `.gitattributes` file, at the top or in a folder.
fn is_attributes_file(path: &str) -> bool {
    path == ".gitattributes" || path.ends_with("/.gitattributes")
}

impl super::RepoTab {
    /// Asks git, in the background, what `.gitattributes` says of those of `paths` not known
    /// yet; lists and diffs fold them once the answer is in.
    pub(super) fn ask_fold_attributes(&mut self, paths: &[String], ctx: &egui::Context) {
        let paths = self.folding.unknown(paths.iter().map(String::as_str));
        if paths.is_empty() {
            return;
        }
        let (epoch, now) = self.folding.asking(&paths);
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let wanted = || now.load(Ordering::Relaxed) == epoch;
                let found = crate::git::fold_attributes(&repo, &paths, &wanted);
                (paths, found)
            },
            move |tab, (paths, (found, error)), _| {
                tab.folding.arrived(epoch, paths, found, error);
            },
        );
    }
}
