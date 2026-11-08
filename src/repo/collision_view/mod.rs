//! The collision matrix window: the branches worked on side by side (those of the linked
//! worktrees ticked first) merged pair by pair in memory, and the order to land them in.

mod grid;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use egui::{RichText, Ui};

use super::actions::BranchRef;
use super::{Action, RepoTab, branch_visibility};
use crate::collision::{self, Matrix, Step};
use crate::theme::{self, Tokens};

/// Most branches one matrix takes (105 merges past the target ones).
pub(crate) const MOST: usize = 14;

/// Numbers each computation: the latest one's answer is the one shown, whichever window
/// (closed and opened again meanwhile) asked for it.
static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// What the window holds.
pub(crate) struct CollisionView {
    /// Every local branch but the target, and whether it is in the matrix.
    picks: Vec<(String, bool)>,
    /// The branch they land on (empty: none picked yet).
    target: String,
    run: Option<Running>,
    result: Option<Result<(Matrix, Vec<Step>), String>>,
    /// Said under the buttons (a run stopped).
    note: Option<String>,
    /// Which computation is the latest: an older one finishing late is dropped.
    request: u64,
    /// Put away while a comparison it opened is looked at (Esc, a click beside it); the menu
    /// brings it back as it was.
    hidden: bool,
}

/// A computation going on; dropping it (the window closed, the tab closed) stops it.
struct Running {
    done: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    total: usize,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
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
            note: None,
            request: 0,
            hidden: false,
        }
    }

    fn chosen(&self) -> Vec<String> {
        self.picks
            .iter()
            .filter(|(_, on)| *on)
            .map(|(b, _)| b.clone())
            .collect()
    }

    /// Why Check can't be clicked, if it can't.
    fn cannot_check(&self) -> Option<String> {
        let count = self.chosen().len();
        if self.run.is_some() {
            Some(
                crate::i18n::text("repo-workflows-collision_view-cannot-check-76fb9e14").to_owned(),
            )
        } else if self.target.is_empty() {
            Some(
                crate::i18n::text("repo-workflows-collision_view-cannot-check-42af2e23").to_owned(),
            )
        } else if count == 0 {
            Some(
                crate::i18n::text("repo-workflows-collision_view-cannot-check-8bcfe01d").to_owned(),
            )
        } else if count > MOST {
            Some(crate::i18n::message(
                "repo-workflows-collision_view-cannot-check-c1d9426d",
                &[("MOST", (MOST).clone().into())],
            ))
        } else {
            None
        }
    }
}

impl RepoTab {
    /// Opens the window (or brings back the one put away) with the linked worktrees'
    /// branches ticked, landing on the main worktree's branch (or the checked-out one).
    pub(crate) fn open_collisions(&mut self) {
        if let Some(view) = &mut self.collisions {
            view.hidden = false;
            return;
        }
        let branches: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let target = self
            .worktrees
            .iter()
            .find(|w| w.main)
            .and_then(|w| w.branch.clone())
            .or_else(|| self.refs.current.clone())
            .filter(|t| branches.contains(t))
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
        let branch = |name: &str| collision::Branch {
            name: name.to_owned(),
            git: branch_visibility::local_ref(name),
        };
        let branches: Vec<collision::Branch> = view.chosen().iter().map(|b| branch(b)).collect();
        let target = branch(&view.target);
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        view.run = Some(Running {
            done: done.clone(),
            cancel: cancel.clone(),
            total: collision::merges(branches.len()),
        });
        view.note = None;
        view.request = REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
        let request = view.request;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                collision::compute(&repo, &branches, &target, &done, &cancel).map(|m| {
                    let order = collision::landing_order(&m);
                    (m, order)
                })
            },
            move |tab, found, _| {
                let Some(view) = &mut tab.collisions else {
                    return;
                };
                if view.request != request {
                    return;
                }
                view.run = None;
                match found {
                    Err(e) if e == collision::STOPPED => {
                        view.note = Some(
                            crate::i18n::text(
                                "repo-workflows-collision_view-check-collisions-96d2bcac",
                            )
                            .to_owned(),
                        );
                    }
                    found => view.result = Some(found),
                }
            },
        );
    }

    /// The window, while it is open and not put away.
    pub(crate) fn collision_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if self.collisions.as_ref().is_none_or(|v| v.hidden) {
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
        if close {
            // Dropping the run stops it.
            self.collisions = None;
        } else if let Some(view) = &mut self.collisions {
            if response.should_close() || acts.len() > before {
                view.hidden = true;
            } else if check {
                self.check_collisions(ctx);
            }
        }
        if self.collisions.as_ref().is_some_and(|v| v.run.is_some()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// The window's content; whether to start the check, whether to close.
fn body(ui: &mut Ui, view: &mut CollisionView, acts: &mut Vec<Action>) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-collision_view-body-cf642735",
        ))
        .font(theme::semibold(15.0)),
    );
    ui.add_space(4.0);
    let onto = if view.target.is_empty() {
        crate::i18n::text("repo-workflows-collision_view-body-5c35d043").to_owned()
    } else {
        view.target.clone()
    };
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-collision_view-body-282052b1",
            &[("onto", (onto).clone().into())],
        ))
        .size(12.0)
        .color(t.text_dim),
    );
    ui.add_space(8.0);
    let running = view.run.is_some();
    let ticks = view.chosen();
    ui.add_enabled_ui(!running, |ui| pick_branches(ui, view));
    // What is shown no longer is what is ticked: check again.
    if view.chosen() != ticks {
        view.result = None;
    }
    ui.add_space(10.0);
    let count = view.chosen().len();
    let mut check = false;
    let mut close = false;
    ui.horizontal(|ui| {
        let label = crate::i18n::message(
            "repo-workflows-collision-check",
            &[("count", (count).into())],
        );
        let reason = view.cannot_check();
        if super::primary_button(ui, &label, reason.is_none(), 130.0)
            .on_disabled_hover_text(reason.unwrap_or_default())
            .clicked()
        {
            check = true;
        }
        if let Some(run) = &view.run {
            ui.add(crate::spinner::Spinner::new());
            let done = run.done.load(Ordering::Relaxed);
            ui.label(crate::i18n::message(
                "repo-workflows-collision_view-body-94846973",
                &[
                    ("done", (done).clone().into()),
                    ("run-total-1", (run.total).clone().into()),
                ],
            ));
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-collision_view-body-cae7d57b",
                ))
                .clicked()
            {
                run.cancel.store(true, Ordering::Relaxed);
            }
        } else if let Some(note) = &view.note {
            ui.label(RichText::new(note).size(12.0).color(t.text_faint));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = ui
                .button(crate::i18n::text(
                    "repo-workflows-collision_view-body-7d9eb7ac",
                ))
                .clicked();
        });
    });
    match &view.result {
        Some(Ok((m, order))) => {
            ui.add_space(12.0);
            grid::results(ui, m, order, acts);
        }
        Some(Err(e)) => {
            ui.add_space(10.0);
            ui.colored_label(t.danger, collision::display::diagnostic(e));
        }
        None => {}
    }
    (check, close)
}

/// The target and the branches to tick.
fn pick_branches(ui: &mut Ui, view: &mut CollisionView) {
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-collision_view-pick-branches-3bc09abe",
        ));
        let mut target = view.target.clone();
        let shown = if target.is_empty() {
            crate::i18n::text("repo-workflows-collision_view-pick-branches-fb40a427")
        } else {
            target.as_str()
        }
        .to_owned();
        egui::ComboBox::from_id_salt("collision-target")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                let names: Vec<String> = std::iter::once(view.target.clone())
                    .filter(|t| !t.is_empty())
                    .chain(view.picks.iter().map(|(b, _)| b.clone()))
                    .collect();
                for name in names {
                    ui.selectable_value(&mut target, name.clone(), name);
                }
            });
        if target != view.target {
            let old = std::mem::replace(&mut view.target, target.clone());
            view.picks.retain(|(b, _)| *b != target);
            if !old.is_empty() {
                view.picks.push((old, false));
            }
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
                ui.label(crate::i18n::text(
                    "repo-workflows-collision_view-pick-branches-6de3bf71",
                ));
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

#[cfg(test)]
mod fluent_guard_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_collision_actual_error_paint_keeps_raw_result() -> Result<(), String> {
        let raw = "The collision matrix needs git 2.38 or later (git merge-tree --write-tree)";
        let mut view = CollisionView::new(&["feature/ß".into()], &[], "main");
        view.result = Some(Err(raw.into()));
        let mut initialized = false;
        let mut h = Harness::new_ui_state(
            |ui, state: &mut CollisionView| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    initialized = true;
                    ui.ctx().request_discard("install supplied fonts");
                    return;
                }
                body(ui, state, &mut vec![]);
            },
            view,
        );
        for lang in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(lang);
            h.run();
            h.get_by_label(&collision::display::diagnostic(raw));
            assert!(matches!(&h.state().result,Some(Err(e)) if e==raw && e.contains("needs git")));
            assert_ne!(raw, collision::STOPPED);
        }
        Ok(())
    }
}
