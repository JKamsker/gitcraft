//! The card a background run shows over the tab's big area, bottom right: the agent's plan as
//! a bar of steps and a list, what it is doing now, the time gone and left, and once it is
//! done, the files to apply. Folded, it is a pill that keeps counting.

mod outcome;
use outcome::*;
pub(super) use outcome::{bring_back_button, name_pick};
mod progress;
use progress::*;
use std::time::Instant;

use egui::{Align2, Color32, RichText, Sense, Stroke, Ui, vec2};

use super::{Phase, ResolverAct, Run};
use crate::icons;
use crate::repo::{Action, RepoTab, widgets};
use crate::resolver::agent::Activity;
use crate::resolver::report::{Step, StepStatus, clock, left_words};
use crate::resolver::workspace::FileResult;
use crate::theme::{self, Tokens};

/// The card's width.
const WIDTH: f32 = 380.0;
/// How far it keeps from the big area's edges.
const MARGIN: f32 = 14.0;
/// The most recent things the agent did that are shown.
const RECENT: usize = 4;

impl RepoTab {
    /// The card (or the pill), over `area`, the tab's big area.
    pub(in crate::repo) fn resolver_card(
        &self,
        ctx: &egui::Context,
        area: egui::Rect,
        acts: &mut Vec<Action>,
    ) {
        // Under review, the big area shows the run itself.
        let Some(run) = self.resolver.run.as_ref().filter(|r| r.review.is_none()) else {
            return;
        };
        // A merge put aside: its card goes with its row, selected (elsewhere the row's pill
        // says how far along it is).
        if run.parked.is_some()
            && self.selected_parked().map(|p| &p.id) != run.parked.as_ref().map(|p| &p.id)
        {
            return;
        }
        let t = Tokens::get(ctx);
        let folded = self.resolver.folded;
        egui::Area::new(egui::Id::new(("gitcraft-resolver", &self.path)))
            .order(egui::Order::Middle)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(area.right_bottom() - vec2(MARGIN, MARGIN))
            .constrain_to(area)
            .show(ctx, |ui| {
                let frame = egui::Frame::new()
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.field_border))
                    .corner_radius(t.radius_lg)
                    .shadow(ctx.global_style().visuals.window_shadow);
                if folded {
                    frame
                        .inner_margin(egui::Margin::symmetric(12, 7))
                        .show(ui, |ui| pill(ui, &t, run, acts));
                } else {
                    // No taller than the big area: what doesn't fit scrolls.
                    let tallest = (area.height() - 2.0 * MARGIN - 28.0).max(120.0);
                    frame.inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                        ui.set_width(WIDTH);
                        egui::ScrollArea::vertical()
                            .id_salt("resolver-card")
                            .max_height(tallest)
                            // An area's room is its last size: up to this, whatever it was.
                            .min_scrolled_height(tallest)
                            .show(ui, |ui| card(ui, &t, run, acts));
                    });
                }
            });
    }
}

impl RepoTab {
    /// _Resolve all with agent_, on the banner of the merge (rebase…) in progress; while a run
    /// is on, what brings its card back.
    pub(in crate::repo) fn resolve_all_button(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if self.resolver.busy() {
            if ui
                .button("Show agent")
                .on_hover_text("The agent resolving the conflicts in the background")
                .clicked()
            {
                acts.push(Action::Resolver(ResolverAct::Fold(false)));
            }
            return;
        }
        let engine = crate::resolver::agent::Engine::from_settings(
            crate::settings::Settings::get(ui.ctx()).ai.tool,
        );
        let n = self.work.conflicted.len();
        let local = !crate::remote::is_remote(&self.path);
        let r = ui
            .add_enabled(local, egui::Button::new("Resolve all with agent"))
            .on_hover_text(format!(
                "{} resolves the {n} conflicted file{} in the background, in a private copy of \
                 this checkout. gitcraft then tries to put the merge aside, like a stash \
                 (conflicts and staged files kept as they are, the worktree reset to its \
                 branch): check out and work on anything. When the agent is done, bring it back \
                 from the graph and review what it did. One that can't be put aside (a rebase) \
                 stays in place: resolve it there rather than switch branches. (The agent is \
                 picked in Settings → AI.)",
                engine.label(),
                if n == 1 { "" } else { "s" },
            ))
            .on_disabled_hover_text("For repositories on this machine only, for now");
        if r.clicked() {
            acts.push(Action::Resolver(ResolverAct::Start(None)));
        }
    }
}

/// "Resolving 3 files", "Resolved 3 files"…
fn title(run: &Run) -> String {
    let n = run.files.len();
    let files = if n == 1 {
        run.files[0]
            .rsplit('/')
            .next()
            .unwrap_or(&run.files[0])
            .to_owned()
    } else {
        format!("{n} files")
    };
    match run.phase {
        Phase::Preparing => format!("Getting ready to resolve {files}"),
        Phase::Working if run.stopping => "Stopping…".to_owned(),
        Phase::Working | Phase::Checking => format!("Resolving {files}"),
        Phase::Ready | Phase::Applying => {
            let ready = ready(run);
            let partly =
                ready < run.results.len() || run.progress.finished.as_ref().is_some_and(|f| !f.ok);
            match () {
                _ if ready == 0 && run.stopping => "Stopped".to_owned(),
                _ if ready == 0 => format!("Didn't resolve {files}"),
                _ if partly => format!("Partly resolved {files}"),
                _ if run.stopping => "Stopped".to_owned(),
                _ => format!("Resolved {files}"),
            }
        }
        Phase::Applied => "Applied".to_owned(),
        Phase::Failed => "Couldn't resolve".to_owned(),
    }
}

/// How many of its files came back resolved (or deleted, as one side did).
fn ready(run: &Run) -> usize {
    run.results.iter().filter(|(_, r)| r.applies()).count()
}

fn running(run: &Run) -> bool {
    matches!(
        run.phase,
        Phase::Preparing | Phase::Working | Phase::Checking
    )
}

/// The time gone (frozen once it ended).
fn elapsed(run: &Run) -> std::time::Duration {
    run.ended
        .unwrap_or_else(Instant::now)
        .saturating_duration_since(run.started)
}

fn pill(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    let r = ui
        .horizontal(|ui| {
            status_mark(ui, t, run);
            let (over, total) = run.progress.counts();
            let mut text = title(run);
            if total > 0 && running(run) {
                text.push_str(&format!(" · {over}/{total}"));
            }
            text.push_str(&format!(" · {}", clock(elapsed(run))));
            ui.label(RichText::new(text).font(theme::medium(12.5)));
            if icons::small_button(ui, "chevron-up", "Show the agent's progress").clicked() {
                acts.push(Action::Resolver(ResolverAct::Fold(false)));
            }
        })
        .response
        .interact(Sense::click());
    if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        acts.push(Action::Resolver(ResolverAct::Fold(false)));
    }
}

/// A spinner while it works, else how it ended.
fn status_mark(ui: &mut Ui, t: &Tokens, run: &Run) {
    if running(run) || run.phase == Phase::Applying {
        ui.add(crate::spinner::Spinner::new().size(14.0).color(t.accent));
        return;
    }
    // Done with all of its files, as they were checked: not merely an agent that exited well.
    let all = run.phase != Phase::Ready || (ready(run) > 0 && ready(run) == run.results.len());
    let ok = all && run.error.is_none() && run.progress.finished.as_ref().is_none_or(|f| f.ok);
    match run.phase {
        Phase::Failed => icons::inline(ui, "circle-alert", 15.0, t.danger),
        _ if ok => icons::inline(ui, "check", 15.0, t.success),
        _ => icons::inline(ui, "circle-alert", 15.0, t.warning),
    }
}

fn card(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    header(ui, t, run, acts);
    ui.add_space(10.0);
    step_bar(ui, t, run);
    ui.add_space(10.0);
    if run.progress.steps.is_empty() && running(run) {
        ui.horizontal(|ui| {
            ui.add(
                crate::spinner::Spinner::new()
                    .size(12.0)
                    .color(t.text_faint),
            );
            let what = if run.phase == Phase::Preparing {
                "Making a private copy of the checkout for the agent…"
            } else {
                "Waiting for the agent's plan…"
            };
            ui.label(RichText::new(what).color(t.text_dim).small());
        });
    } else {
        // Once it is done, the files to apply come first.
        let height = if running(run) { 240.0 } else { 130.0 };
        steps(ui, t, &run.progress.steps, height);
    }
    if running(run) {
        recent(ui, t, run);
    }
    match run.phase {
        Phase::Ready | Phase::Applying => outcome(ui, t, run, acts),
        Phase::Applied => applied(ui, t, run, acts),
        Phase::Failed => failed(ui, t, run, acts),
        _ => {}
    }
}

fn header(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        egui::Frame::new()
            .fill(t.accent.gamma_multiply(0.16))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| ui.add(icons::image("bot", 18.0, t.accent)));
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(title(run)).font(theme::semibold(14.0)));
            });
            let mut sub = vec![run.engine.label().to_owned(), clock(elapsed(run))];
            if let Some(left) = run
                .progress
                .remaining(Instant::now())
                .filter(|_| running(run))
            {
                sub.push(left_words(left));
            }
            if let Some(cost) = run.cost_usd {
                sub.push(format!("${cost:.2}"));
            }
            ui.label(RichText::new(sub.join(" · ")).color(t.text_dim).small());
            if let Some(parked) = &run.parked {
                ui.label(
                    RichText::new(format!(
                        "The {} is put aside: work on anything meanwhile",
                        parked.what()
                    ))
                    .color(t.accent)
                    .small(),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            if icons::small_button(ui, "chevron-down", "Fold: it goes on in the background")
                .clicked()
            {
                acts.push(Action::Resolver(ResolverAct::Fold(true)));
            }
            if running(run)
                && !run.stopping
                && ui
                    .small_button("Stop")
                    .on_hover_text("Stop the agent. What it resolved so far can still be applied")
                    .clicked()
            {
                acts.push(Action::Resolver(ResolverAct::Stop));
            }
        });
    });
}

#[cfg(test)]
mod tests;
