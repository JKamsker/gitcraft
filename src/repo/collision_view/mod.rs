//! The collision matrix window: the branches worked on side by side (those of the linked
//! worktrees ticked first) merged pair by pair in memory, and the order to land them in.

mod grid;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{RichText, Ui};

use super::actions::BranchRef;
use super::{Action, RepoTab, branch_visibility};
use crate::collision::{self, Matrix, Step};
use crate::theme::{self, Tokens};

/// Most branches one matrix takes (105 merges past the target ones).
pub(crate) const MOST: usize = 14;

/// What the window holds.
pub(crate) struct CollisionView {
    /// Every local branch, and whether it is in the matrix.
    picks: Vec<(String, bool)>,
    /// The branch they land on.
    target: String,
    run: Option<Running>,
    result: Option<Result<(Matrix, Vec<Step>), String>>,
    /// Which computation is the latest: an older one finishing late is dropped.
    request: u64,
}

/// A computation going on.
struct Running {
    done: Arc<AtomicUsize>,
    cancel: Arc<AtomicUsize>,
    total: usize,
}

impl CollisionView {
    /// The window for `branches`, those in `ticked` in the matrix, landing on `target`.
    pub(crate) fn new(branches: &[String], ticked: &[String], target: &str) -> Self {
        let picks = branches
            .iter()
            .filter(|b| *b != target)
            .map(|b| (b.clone(), ticked.contains(b)))
            .collect();
        Self {
            picks,
            target: target.to_owned(),
            run: None,
            result: None,
            request: 0,
        }
    }

    fn chosen(&self) -> Vec<String> {
        self.picks
            .iter()
            .filter(|(_, on)| *on)
            .map(|(b, _)| b.clone())
            .collect()
    }
}

impl RepoTab {
    /// Opens the window with the linked worktrees' branches ticked, landing on the main
    /// worktree's branch (or the checked-out one).
    pub(crate) fn open_collisions(&mut self) {
        let branches: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let target = self
            .worktrees
            .iter()
            .find(|w| w.main)
            .and_then(|w| w.branch.clone())
            .or_else(|| self.refs.current.clone())
            .unwrap_or_default();
        let ticked: Vec<String> = self
            .worktrees
            .iter()
            .filter(|w| !w.main && w.prunable.is_none())
            .filter_map(|w| w.branch.clone())
            .filter(|b| *b != target)
            .take(MOST)
            .collect();
        self.collisions = Some(CollisionView::new(&branches, &ticked, &target));
    }

    /// Merges the chosen branches pair by pair in the background.
    fn check_collisions(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.collisions else {
            return;
        };
        let names = view.chosen();
        let full: Vec<String> = names
            .iter()
            .map(|b| branch_visibility::local_ref(b))
            .collect();
        let target = branch_visibility::local_ref(&view.target);
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicUsize::new(0));
        if let Some(old) = view.run.take() {
            old.cancel.store(1, Ordering::Relaxed);
        }
        view.run = Some(Running {
            done: done.clone(),
            cancel: cancel.clone(),
            total: collision::merges(full.len()),
        });
        view.request += 1;
        let request = view.request;
        let shown_target = view.target.clone();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                collision::compute(&repo, &full, &target, &done, &cancel).map(|mut m| {
                    // Shown by their names; given to git by their full refs.
                    m.branches = names;
                    m.target = shown_target;
                    let order = collision::landing_order(&m);
                    (m, order)
                })
            },
            move |tab, found, _| {
                if let Some(view) = &mut tab.collisions
                    && view.request == request
                {
                    view.run = None;
                    view.result = Some(found);
                }
            },
        );
    }

    /// The window, while it is open.
    pub(crate) fn collision_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if self.collisions.is_none() {
            return;
        }
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut check = false;
        let mut close = false;
        // A cell clicked opens a comparison, which the window would hide.
        let before = acts.len();
        let response = egui::Modal::new(egui::Id::new("gitcraft-collisions"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(620.0);
                if let Some(view) = &mut self.collisions {
                    (check, close) = body(ui, view, acts);
                }
            });
        if response.should_close() || close || acts.len() > before {
            if let Some(run) = self.collisions.take().and_then(|v| v.run) {
                run.cancel.store(1, Ordering::Relaxed);
            }
        } else if check {
            self.check_collisions(ctx);
        }
        if self.collisions.as_ref().is_some_and(|v| v.run.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// The window's content; whether to start the check, whether to close.
fn body(ui: &mut Ui, view: &mut CollisionView, acts: &mut Vec<Action>) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("Collision matrix").font(theme::semibold(15.0)));
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "Each pair of branches merged in memory (git merge-tree), and each with {}: nothing is \
             checked out or changed. Branches of the linked worktrees are ticked.",
            view.target
        ))
        .size(12.0)
        .color(t.text_dim),
    );
    ui.add_space(8.0);
    let running = view.run.is_some();
    ui.add_enabled_ui(!running, |ui| pick_branches(ui, view));
    ui.add_space(10.0);
    let count = view.chosen().len();
    let mut check = false;
    let mut close = false;
    ui.horizontal(|ui| {
        let label = format!("Check {count} branch{}", if count == 1 { "" } else { "es" });
        let can = !running && (1..=MOST).contains(&count) && !view.target.is_empty();
        let reason = if count > MOST {
            format!("At most {MOST} branches at a time")
        } else {
            "Tick the branches to check".to_owned()
        };
        if super::primary_button(ui, &label, can, 130.0)
            .on_disabled_hover_text(reason)
            .clicked()
        {
            check = true;
        }
        if let Some(run) = &view.run {
            ui.add(crate::spinner::Spinner::new());
            let done = run.done.load(Ordering::Relaxed);
            ui.label(format!("Merging in memory… {done} of {}", run.total));
            if ui.button("Stop").clicked() {
                run.cancel.store(1, Ordering::Relaxed);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = ui.button("Close").clicked();
        });
    });
    match &view.result {
        Some(Ok((m, order))) => {
            ui.add_space(12.0);
            grid::results(ui, m, order, acts);
        }
        Some(Err(e)) => {
            ui.add_space(10.0);
            ui.colored_label(t.danger, e);
        }
        None => {}
    }
    (check, close)
}

/// The target and the branches to tick.
fn pick_branches(ui: &mut Ui, view: &mut CollisionView) {
    ui.horizontal(|ui| {
        ui.label("Lands on");
        let mut target = view.target.clone();
        egui::ComboBox::from_id_salt("collision-target")
            .selected_text(target.clone())
            .show_ui(ui, |ui| {
                let names: Vec<String> = std::iter::once(view.target.clone())
                    .chain(view.picks.iter().map(|(b, _)| b.clone()))
                    .collect();
                for name in names {
                    ui.selectable_value(&mut target, name.clone(), name);
                }
            });
        if target != view.target {
            let old = std::mem::replace(&mut view.target, target.clone());
            view.picks.retain(|(b, _)| *b != target);
            view.picks.push((old, false));
            view.picks.sort_by(|a, b| a.0.cmp(&b.0));
            view.result = None;
        }
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("collision-branches")
        .max_height(150.0)
        .show(ui, |ui| {
            if view.picks.is_empty() {
                ui.label("There is no other local branch.");
            }
            for (name, on) in &mut view.picks {
                ui.checkbox(on, name.as_str());
            }
        });
}

/// Compares two branches of the matrix: what `b` adds since it left `a`.
fn compare(a: &str, b: &str) -> Action {
    super::actions::compare_branches_action(BranchRef::local(a), BranchRef::local(b), true)
}
