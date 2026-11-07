//! The card a background run shows over the tab's big area, bottom right: the agent's plan as
//! a bar of steps and a list, what it is doing now, the time gone and left, and once it is
//! done, the files to apply. Folded, it is a pill that keeps counting.

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
                    frame.inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                        ui.set_width(WIDTH);
                        card(ui, &t, run, acts);
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
                 this checkout. The merge is put aside meanwhile, like a stash (conflicts and \
                 staged files kept as they are): check out and work on anything. When the \
                 agent is done, bring it back from the graph and review what it did. (The \
                 agent is picked in Settings → AI.)",
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
        Phase::Ready | Phase::Applying => match &run.progress.finished {
            Some(f) if !f.ok => format!("Partly resolved {files}"),
            _ if run.stopping => "Stopped".to_owned(),
            _ => format!("Resolved {files}"),
        },
        Phase::Applied => "Applied".to_owned(),
        Phase::Failed => "Couldn't resolve".to_owned(),
    }
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
    let ok = run.error.is_none() && run.progress.finished.as_ref().is_none_or(|f| f.ok);
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

fn step_color(t: &Tokens, status: StepStatus) -> Color32 {
    match status {
        StepStatus::Pending => t.field,
        StepStatus::Running => t.accent,
        StepStatus::Done => t.success,
        StepStatus::Failed => t.danger,
        StepStatus::Skipped => t.text_faint,
    }
}

/// The plan as a bar: a segment per step, coloured by how it stands.
fn step_bar(ui: &mut Ui, t: &Tokens, run: &Run) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let steps = &run.progress.steps;
    if steps.is_empty() {
        painter.rect_filled(rect, 3.0, t.field);
        return;
    }
    let gap = 3.0;
    let n = steps.len() as f32;
    let each = ((rect.width() - gap * (n - 1.0)) / n).max(2.0);
    for (i, step) in steps.iter().enumerate() {
        let x = rect.left() + i as f32 * (each + gap);
        let seg = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(each, rect.height()));
        painter.rect_filled(seg, 3.0, step_color(t, step.status));
        if step.status == StepStatus::Running {
            // Part way: a lighter band over the second half.
            let rest = egui::Rect::from_min_max(seg.center_top(), seg.right_bottom());
            painter.rect_filled(rest, 3.0, t.accent.gamma_multiply(0.35));
        }
    }
}

fn step_icon(ui: &mut Ui, t: &Tokens, status: StepStatus) {
    let size = vec2(16.0, 16.0);
    match status {
        StepStatus::Running => {
            ui.add(crate::spinner::Spinner::new().size(14.0).color(t.accent));
        }
        StepStatus::Done => icons::inline(ui, "check", 15.0, t.success),
        StepStatus::Failed => icons::inline(ui, "x", 15.0, t.danger),
        StepStatus::Skipped => icons::inline(ui, "minus", 15.0, t.text_faint),
        StepStatus::Pending => {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter()
                .circle_stroke(rect.center(), 5.0, Stroke::new(1.5, t.text_faint));
        }
    }
}

fn steps(ui: &mut Ui, t: &Tokens, steps: &[Step], height: f32) {
    egui::ScrollArea::vertical()
        .id_salt("resolver-steps")
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for step in steps {
                ui.horizontal_top(|ui| {
                    step_icon(ui, t, step.status);
                    ui.vertical(|ui| {
                        let color = match step.status {
                            StepStatus::Pending | StepStatus::Skipped => t.text_dim,
                            _ => t.text,
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&step.title).color(color));
                            if let (Some(a), Some(b)) = (step.started, step.ended) {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let took = b.saturating_duration_since(a);
                                        ui.label(
                                            RichText::new(clock(took)).color(t.text_faint).small(),
                                        );
                                    },
                                );
                            }
                        });
                        if let Some(file) = step.file.as_deref().filter(|f| *f != step.title) {
                            ui.label(
                                RichText::new(file)
                                    .font(theme::mono(11.0))
                                    .color(t.text_faint),
                            );
                        }
                        if let Some(note) = &step.note {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(note).color(t.text_dim).small().italics(),
                                )
                                .wrap(),
                            );
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
}

/// The last few things the agent did, the newest brightest.
fn recent(ui: &mut Ui, t: &Tokens, run: &Run) {
    let Some(agent) = &run.agent else { return };
    let lines: Vec<String> = agent.with_feed(|f| {
        f.activities
            .iter()
            .rev()
            .filter_map(|a| match a {
                Activity::Tool(s) => Some(format!("› {s}")),
                Activity::Said(s) => Some(s.clone()),
                Activity::Ended { .. } => None,
            })
            .take(RECENT)
            .collect()
    });
    if lines.is_empty() {
        return;
    }
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(t.field)
        .corner_radius(t.radius_lg)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (i, line) in lines.iter().enumerate().rev() {
                let color = if i == 0 { t.text_dim } else { t.text_faint };
                ui.add(
                    egui::Label::new(RichText::new(line).font(theme::mono(11.0)).color(color))
                        .truncate(),
                );
            }
        });
}

fn result_line(t: &Tokens, result: &FileResult) -> (&'static str, Color32, &'static str) {
    match result {
        FileResult::Resolved => ("check", t.success, "resolved"),
        FileResult::Deleted => ("minus", t.success, "deleted, as one side did"),
        FileResult::StillConflicted => ("circle-alert", t.warning, "still has conflicts"),
        FileResult::ChangedMeanwhile => (
            "circle-alert",
            t.warning,
            "you changed it meanwhile: kept yours",
        ),
        FileResult::Untouched => ("circle-alert", t.warning, "not decided: yours to choose"),
        FileResult::NotPlain => ("x", t.danger, "a link, not a file: not applied"),
    }
}

fn outcome(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    ui.add_space(8.0);
    let summary = run
        .progress
        .finished
        .as_ref()
        .map(|f| f.summary.clone())
        .or_else(|| run.last_words.clone())
        .filter(|s| !s.is_empty());
    if let Some(summary) = summary {
        egui::Frame::new()
            .fill(t.accent.gamma_multiply(0.08))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(summary).color(t.text)).wrap());
            });
        ui.add_space(6.0);
    }
    if let Some(e) = &run.error {
        ui.add(egui::Label::new(RichText::new(e).color(t.warning).small()).wrap());
    }
    for (i, (path, result)) in run.results.iter().enumerate() {
        let (icon, color, words) = result_line(t, result);
        ui.horizontal(|ui| {
            let mut on = run.picked.contains(path);
            let usable = result.applies() && run.phase == Phase::Ready;
            if ui
                .add_enabled(usable, egui::Checkbox::without_text(&mut on))
                .changed()
            {
                acts.push(Action::Resolver(ResolverAct::Pick(path.clone(), on)));
            }
            icons::inline(ui, icon, 13.0, color);
            let name = ui
                .add(
                    egui::Label::new(RichText::new(path).font(theme::mono(11.5)))
                        .sense(Sense::click()),
                )
                .on_hover_text("See what the agent did to it")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.label(RichText::new(words).color(t.text_faint).small());
            let review = ui.add_enabled(
                run.phase == Phase::Ready,
                egui::Button::new(RichText::new("Review").small()).small(),
            );
            if name.clicked() || review.clicked() {
                acts.push(Action::Resolver(ResolverAct::Review(i)));
            }
        });
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                run.phase == Phase::Ready && !run.results.is_empty(),
                egui::Button::new("Review changes"),
            )
            .on_hover_text("Each conflict with its two sides and what the agent wrote in its place")
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::Review(0)));
        }
        // The merge put aside: brought back first, then the files are applied to it.
        if let Some(parked) = &run.parked {
            bring_back_button(ui, parked, acts);
            return;
        }
        let n = run.picked.len();
        let label = match n {
            0 => "Nothing to apply".to_owned(),
            1 => "Apply 1 file".to_owned(),
            n => format!("Apply {n} files"),
        };
        let can = n > 0 && run.phase == Phase::Ready;
        if widgets::primary_button(ui, &label, can, 0.0)
            .on_hover_text(
                "Writes the agent's version of each file picked here and marks it resolved",
            )
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::Apply));
        }
        if run.phase == Phase::Applying {
            ui.add(crate::spinner::Spinner::new().size(14.0));
        }
        if ui
            .add_enabled(run.phase == Phase::Ready, egui::Button::new("Discard"))
            .on_hover_text("Forget the agent's work; your checkout stays as it is")
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::Dismiss));
        }
    });
}

/// _Bring it back_: the merge put aside, back in the worktree as it was, to apply the agent's
/// work to.
pub(super) fn bring_back_button(
    ui: &mut Ui,
    parked: &crate::resolver::park::Parked,
    acts: &mut Vec<Action>,
) {
    if widgets::primary_button(ui, "Bring it back", true, 0.0)
        .on_hover_text(format!(
            "Checks out {} and puts the {} back as it was, conflicts and staged files \
             included; then the agent's work is there to review and apply",
            parked.branch.as_deref().unwrap_or("its commit"),
            parked.what()
        ))
        .clicked()
    {
        acts.push(Action::Resolver(ResolverAct::BringBack(parked.id.clone())));
    }
}

fn applied(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    ui.add_space(8.0);
    let text = match run.applied {
        0 => "Nothing was applied.".to_owned(),
        1 => "Applied 1 file and marked it resolved.".to_owned(),
        n => format!("Applied {n} files and marked them resolved."),
    };
    ui.label(RichText::new(text).color(t.text));
    if let Some(e) = &run.error {
        ui.add(egui::Label::new(RichText::new(e).color(t.warning).small()).wrap());
    }
    ui.label(
        RichText::new("Check the result in the staged changes before you commit.")
            .color(t.text_dim)
            .small(),
    );
    ui.add_space(6.0);
    if ui.button("Close").clicked() {
        acts.push(Action::Resolver(ResolverAct::Dismiss));
    }
}

fn failed(ui: &mut Ui, t: &Tokens, run: &Run, acts: &mut Vec<Action>) {
    ui.add_space(8.0);
    let e = run
        .error
        .as_deref()
        .unwrap_or("The agent ended without resolving anything.");
    ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        // Its merge put aside: brought back before anything else (it is never dropped).
        if let Some(parked) = &run.parked {
            bring_back_button(ui, parked, acts);
            return;
        }
        if ui.button("Try again").clicked() {
            acts.push(Action::Resolver(ResolverAct::Dismiss));
            acts.push(Action::Resolver(ResolverAct::Start(Some(
                run.files.clone(),
            ))));
        }
        if ui.button("Close").clicked() {
            acts.push(Action::Resolver(ResolverAct::Dismiss));
        }
    });
}
