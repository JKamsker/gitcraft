//! Restacking from the tab: the preview of each layer's new base, running it in the
//! background, taking in a stop on conflicts (continued or aborted from STACKS), and the one
//! undo that puts every branch it moved back.

use std::path::Path;

use egui::RichText;

use super::{StackAct, dependents, to_restack};
use crate::git::stack::restack::{self, Moved, Outcome, Place, Preview};
use crate::git::stack::{BASE_KEY, Layer, config_key, local_ref, set_op};
use crate::repo::messages::{Msg, spawn_or};
use crate::repo::operations::{expect_ref_at, expect_symref, expect_symref_in};
use crate::repo::{Action, OpKind, Refresh, RepoTab, UndoEntry, short, widgets};
use crate::theme::{self, Tokens};

/// A restack being previewed.
pub(crate) struct RestackPreview {
    pub layers: Vec<Layer>,
    /// Each step's new base and commits: `None` while they are worked out.
    pub previews: Option<Result<Vec<Preview>, String>>,
    /// Which preview this is: what is worked out for one closed since isn't taken.
    id: u64,
}

impl RepoTab {
    /// Previews restacking the layers on `from`, or every layer that needs it.
    pub(super) fn open_restack(&mut self, from: Option<&str>, ctx: &egui::Context) {
        let layers = match from {
            Some(branch) => dependents(&self.stacks.layers, branch),
            None => to_restack(&self.stacks.layers),
        };
        if layers.is_empty() {
            self.status = Some((
                "Nothing to restack: every layer is on its parent's tip.".to_owned(),
                false,
            ));
            return;
        }
        static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let list: Vec<(String, String, Option<String>)> = layers
            .iter()
            .map(|l| (l.branch.clone(), l.parent.clone(), l.base.clone()))
            .collect();
        self.stacks.preview = Some(RestackPreview {
            layers,
            previews: None,
            id,
        });
        let path = self.path.clone();
        self.then(
            ctx,
            move || restack::prepare(&path, &list),
            move |tab, found, _| {
                if let Some(p) = tab.stacks.preview.as_mut().filter(|p| p.id == id) {
                    p.previews = Some(found);
                }
            },
        );
    }

    /// Restacks as previewed.
    pub(super) fn start_restack(&mut self, ctx: &egui::Context) {
        let Some(RestackPreview {
            previews: Some(Ok(previews)),
            ..
        }) = self.stacks.preview.take()
        else {
            return;
        };
        let steps = previews.into_iter().map(|p| p.step).collect();
        self.run_restack("Restack", ctx, move |path| restack::start(path, steps));
    }

    pub(in crate::repo) fn resume_restack(&mut self, ctx: &egui::Context) {
        self.run_restack("Restack", ctx, restack::resume);
    }

    pub(in crate::repo) fn abort_restack(&mut self, ctx: &egui::Context) {
        self.run_restack("Abort restack", ctx, |path| {
            restack::abort(path).map(Outcome::Aborted)
        });
    }

    /// Runs `work` (start, resume or abort) in the background as the tab's operation.
    fn run_restack(
        &mut self,
        label: &str,
        ctx: &egui::Context,
        work: impl FnOnce(&Path) -> Result<Outcome, String> + Send + 'static,
    ) {
        if let Some(busy) = &self.busy_op {
            self.status = Some((format!("{label}: run it again once {busy} is done"), true));
            return;
        }
        self.busy_op = Some(label.to_owned());
        self.op_started = Some(std::time::Instant::now());
        // Signing in after this fails must not run some earlier operation again.
        self.last_op = None;
        self.status = None;
        let path = self.path.clone();
        let label = label.to_owned();
        let failed = {
            let label = label.clone();
            move |e| failed_msg(label, e)
        };
        spawn_or(&self.tx, ctx, failed, move || match work(&path) {
            Ok(outcome) => Msg::Then(Box::new(move |tab, ctx| {
                tab.restack_done(outcome, ctx);
            })),
            Err(e) => failed_msg(label, e),
        });
    }

    /// Takes in how a restack (or the part of it run now) ended.
    fn restack_done(&mut self, outcome: Outcome, ctx: &egui::Context) {
        self.busy_op = None;
        let long = self
            .op_started
            .take()
            .is_some_and(|t| t.elapsed() > crate::repo::operations::LONG_OP);
        match outcome {
            Outcome::Done(moved) => {
                let n = moved.len();
                self.status = Some((
                    match n {
                        0 => "Restack: every branch was already on its parent's tip".to_owned(),
                        1 => "Restack: 1 branch moved · Ctrl+Z puts it back".to_owned(),
                        n => format!("Restack: {n} branches moved · Ctrl+Z puts them back"),
                    },
                    false,
                ));
                if n > 0 {
                    let (undo, redo) = undo_steps(&moved);
                    self.push_undo(
                        UndoEntry {
                            id: 0,
                            agent: None,
                            label: "Restack".to_owned(),
                            undo,
                            redo,
                            heads: None,
                        },
                        ctx,
                    );
                }
            }
            Outcome::Aborted(kept) => {
                self.status = Some(if kept.is_empty() {
                    (
                        "Restack aborted: every branch is back where it was".to_owned(),
                        false,
                    )
                } else {
                    (
                        format!(
                            "Restack aborted: every branch is back where it was but {}, which \
                             got commits after it was restacked and is left as it is",
                            kept.join(", ")
                        ),
                        true,
                    )
                });
            }
            Outcome::Stopped {
                branch, worktree, ..
            } => {
                let place = match &worktree {
                    Some(wt) => format!(" in its worktree at {}", wt.display()),
                    None => String::new(),
                };
                self.status = Some((
                    format!(
                        "Restack stopped at {branch} on conflicts: resolve them{place}, then \
                         Continue restack (or Abort restack)"
                    ),
                    true,
                ));
                self.focus_wip = worktree.is_none();
            }
        }
        if long && let Some((message, failed)) = self.status.clone() {
            self.finished
                .push((format!("{} · {message}", self.name), failed));
        }
        self.reload(ctx);
    }

    /// The preview, when it is open.
    pub(in crate::repo) fn restack_dialog(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(preview) = &self.stacks.preview else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let idle = self.busy_op.is_none();
        let response = egui::Modal::new(egui::Id::new("gitcraft-stack-restack"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(460.0);
                let n = preview.layers.len();
                ui.label(
                    RichText::new(format!(
                        "Restack {n} branch{}",
                        if n == 1 { "" } else { "es" }
                    ))
                    .font(theme::semibold(15.0)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Each branch's own commits are replayed onto its parent's new tip, \
                         parents first. A conflict stops the restack: resolve it, then Continue \
                         restack. Ctrl+Z afterwards puts every branch back.",
                    )
                    .color(t.text_dim),
                );
                ui.add_space(10.0);
                let mut can = idle;
                match &preview.previews {
                    None => {
                        can = false;
                        ui.horizontal(|ui| {
                            ui.add(crate::spinner::Spinner::new());
                            ui.label(
                                RichText::new("Working out the new bases…").color(t.text_faint),
                            );
                        });
                    }
                    Some(Err(e)) => {
                        can = false;
                        ui.colored_label(t.danger, format!("Can't restack: {e}"));
                    }
                    Some(Ok(list)) => {
                        for (i, p) in list.iter().enumerate() {
                            // A parent restacked first lands somewhere new.
                            let parent_moves =
                                list[..i].iter().any(|q| q.step.branch == p.step.parent);
                            preview_row(ui, &t, p, parent_moves);
                        }
                        let here = list.iter().any(|p| p.worktree.is_none());
                        if here && !self.work.is_clean() {
                            can = false;
                            ui.add_space(6.0);
                            ui.colored_label(
                                t.warning,
                                "Commit or stash your changes first: restacking checks the \
                                 branches out here.",
                            );
                        }
                        if self.work.in_progress.is_some() {
                            can = false;
                            ui.add_space(6.0);
                            ui.colored_label(
                                t.warning,
                                "Finish or abort the operation in progress first.",
                            );
                        }
                    }
                }
                if let Some(busy) = &self.busy_op {
                    ui.add_space(6.0);
                    ui.label(RichText::new(format!("Waiting for {busy}…")).color(t.text_faint));
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can, egui::Button::new("Replay in isolated worktrees…"))
                        .clicked()
                        && let Some(Ok(previews)) = &preview.previews
                    {
                        acts.push(Action::RepositoryTools(
                            crate::repo::repository_tools::Act::IsolatedStack(previews.clone()),
                        ));
                    }
                    if widgets::primary_button(ui, "Restack", can, 90.0).clicked() {
                        acts.push(Action::Stack(StackAct::StartRestack));
                    }
                    if ui
                        .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
                        .clicked()
                    {
                        acts.push(Action::Stack(StackAct::CloseRestack));
                    }
                });
            });
        if response.should_close() {
            acts.push(Action::Stack(StackAct::CloseRestack));
        }
    }
}

/// One step of the preview: the branch, its new base, how many commits move, where.
/// `parent_moves`: its parent is restacked first, so its new base is that one's new tip.
fn preview_row(ui: &mut egui::Ui, t: &Tokens, p: &Preview, parent_moves: bool) {
    let commits = |n: u32| format!("{n} commit{}", if n == 1 { "" } else { "s" });
    let what = if parent_moves {
        format!(
            "{} onto {}'s new tip · base {} → its new tip",
            commits(p.commits),
            p.step.parent,
            short(&p.step.old_base)
        )
    } else if p.step.old_base == p.new_base {
        format!("already on {}'s tip: nothing to replay", p.step.parent)
    } else {
        format!(
            "{} onto {} · base {} → {}",
            commits(p.commits),
            p.step.parent,
            short(&p.step.old_base),
            short(&p.new_base),
        )
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&p.step.branch).font(theme::medium(12.5)));
        ui.label(RichText::new(what).color(t.text_dim));
        if let Some(wt) = &p.worktree {
            ui.label(
                RichText::new(format!("· in its worktree at {}", wt.display())).color(t.text_faint),
            );
        }
    });
}

/// An operation that failed, as `op_done` takes it in.
fn failed_msg(label: String, e: String) -> Msg {
    Msg::OpDone {
        label,
        result: Err(e),
        refresh: Refresh::All,
        kind: OpKind::plain(),
        quiet: false,
        stopped: 0,
    }
}

/// The steps that put the branches a restack moved back (undo), and forward again (redo).
/// Each refuses unless every branch is where the other left it; a branch checked out is
/// reset there (`--keep`, so changes made since stay or the step refuses), another moved as
/// a ref alone.
pub(crate) fn undo_steps(moved: &[Moved]) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let way = |back: bool| {
        let mut steps = Vec::new();
        for m in moved {
            let (from, _) = ends(m, back);
            steps.push(expect_ref_at(&local_ref(&m.branch), from));
            match &m.place {
                Place::Here => steps.push(expect_symref("HEAD", &local_ref(&m.branch))),
                // Still that branch's worktree: another branch checked out there since would
                // be the one reset.
                Place::Worktree(path) => {
                    steps.push(expect_symref_in(
                        &there(path),
                        "HEAD",
                        &local_ref(&m.branch),
                    ));
                }
                Place::Nowhere => {}
            }
        }
        // The resets first, which refuse over changes they would lose: refused, nothing has
        // moved yet. A branch not checked out goes last (`branch -f`, which refuses one
        // checked out somewhere since).
        let mut order: Vec<&Moved> = moved.iter().collect();
        order.sort_by_key(|m| match m.place {
            Place::Here => 0,
            Place::Worktree(_) => 1,
            Place::Nowhere => 2,
        });
        for m in order {
            let (from, to) = ends(m, back);
            steps.push(move_step(m, from, to));
            let key = config_key(&m.branch, BASE_KEY);
            let base = if back {
                m.base_before.as_deref()
            } else {
                Some(m.base_after.as_str())
            };
            match base.filter(|b| !b.is_empty()) {
                Some(base) => steps.push(set_op(&key, base)),
                None => steps.extend(crate::git::stack::unset_ops(&key)),
            }
        }
        steps
    };
    (way(true), way(false))
}

/// Where a branch is (`from`) and goes (`to`) when undoing (`back`) or redoing.
fn ends(m: &Moved, back: bool) -> (&str, &str) {
    if back {
        (&m.after, &m.before)
    } else {
        (&m.before, &m.after)
    }
}

/// The git command that moves `m`'s branch from `from` to `to`, where it is checked out.
/// (Its `expect_ref_at` checks it is still at `from`.)
fn move_step(m: &Moved, _from: &str, to: &str) -> Vec<String> {
    let mut step: Vec<String> = Vec::new();
    if let Place::Worktree(path) = &m.place {
        step.extend(["-C".to_owned(), there(path)]);
    }
    let words: Vec<&str> = match &m.place {
        Place::Here | Place::Worktree(_) => vec!["reset", "-q", "--keep", to],
        // git refuses to force a branch checked out in any worktree: one checked out since
        // isn't moved under its files.
        Place::Nowhere => vec!["branch", "-f", "--", &m.branch, to],
    };
    step.extend(words.into_iter().map(str::to_owned));
    step
}

/// A worktree's folder as git on the repository's machine knows it.
fn there(path: &std::path::Path) -> String {
    crate::remote::parse(path)
        .map(|t| t.path)
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
