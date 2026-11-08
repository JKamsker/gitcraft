//! The waterfall: one row per job (and per step of the jobs opened), on a time line from the
//! run's start. Queueing is hatched apart from running, a failure carries `!`, a duration the
//! forge didn't record is `?` and never a bar of nothing, the critical path is outlined and
//! tagged `C`, and the earlier run's bar is drawn hollow under each job. The critical path's
//! links and the picked job's dependencies are drawn as arrows, solid when the workflow file
//! says so and dashed when inferred.

use std::collections::{HashMap, HashSet};

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::detail::{queued, ran, seconds, signed};
use super::{Act, act, state_word};
use crate::ci::timing::TimedJob;
use crate::ci::{self, CheckState};
use crate::insights::Evidence;
use crate::insights::ci_path::Explorer;
use crate::repo::Action;
use crate::repo::insights::{common, graph::elide};
use crate::theme::{self, Tokens};

const LABEL_W: f32 = 230.0;
const ROW_H: f32 = 24.0;
const STEP_H: f32 = 18.0;
const AXIS_H: f32 = 20.0;
const BAR_H: f32 = 12.0;

/// What a row shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Job(usize),
    Step(usize, usize),
}

/// When the run started: the time line's zero.
pub(super) fn origin(explorer: &Explorer) -> Option<i64> {
    start_of(&explorer.run.info.started, &explorer.run.jobs)
}

fn start_of(started: &Option<i64>, jobs: &[TimedJob]) -> Option<i64> {
    let first = jobs.iter().filter_map(|j| j.ready().or(j.started)).min();
    match (started, first) {
        (Some(s), Some(f)) => Some((*s).min(f)),
        (s, f) => s.or(f),
    }
}

/// The end of what a job shows: its end, or now while it runs or waits.
fn end_of(job: &TimedJob, now: i64) -> Option<i64> {
    job.finished
        .or_else(|| (job.state == CheckState::Running).then_some(now))
}

/// A tick every so many seconds, at least `min_px` apart.
fn tick(span: i64, width: f32, min_px: f32) -> i64 {
    const STEPS: [i64; 14] = [
        1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200,
    ];
    let per_second = width / span.max(1) as f32;
    STEPS
        .iter()
        .copied()
        .find(|s| *s as f32 * per_second >= min_px)
        .unwrap_or(14_400)
}

pub(super) fn show(
    ui: &mut Ui,
    explorer: &Explorer,
    selected: Option<usize>,
    expanded: &HashSet<usize>,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let jobs = &explorer.run.jobs;
    if jobs.is_empty() {
        common::empty(ui, "The forge lists no jobs for this run.");
        return;
    }
    let now = explorer.now;
    let Some(t0) = origin(explorer) else {
        common::empty(
            ui,
            "The forge recorded no times for this run's jobs: see the table for what it did record.",
        );
        return;
    };
    let compared = explorer.compared.as_ref();
    let before_t0 = compared.and_then(|c| start_of(&c.run.info.started, &c.run.jobs));
    let deltas: HashMap<usize, &crate::insights::ci_path::compare::JobDelta> = compared
        .map(|c| {
            c.comparison
                .jobs
                .iter()
                .filter_map(|d| d.now.map(|i| (i, d)))
                .collect()
        })
        .unwrap_or_default();
    let mut t1 = jobs
        .iter()
        .filter_map(|j| end_of(j, now))
        .max()
        .unwrap_or(t0 + 1);
    if let (Some(c), Some(b0)) = (compared, before_t0)
        && let Some(end) = c.run.jobs.iter().filter_map(|j| j.finished).max()
    {
        t1 = t1.max(t0 + (end - b0));
    }
    let span = (t1 - t0).max(1);

    let mut rows = Vec::new();
    for (i, job) in jobs.iter().enumerate() {
        rows.push(Row::Job(i));
        if expanded.contains(&i) {
            rows.extend((0..job.steps.len()).map(|s| Row::Step(i, s)));
        }
    }
    let height = AXIS_H
        + rows
            .iter()
            .map(|r| match r {
                Row::Job(_) => ROW_H,
                Row::Step(..) => STEP_H,
            })
            .sum::<f32>();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, t.radius, t.dock);
    let plot = Rect::from_min_max(
        pos2(rect.left() + LABEL_W, rect.top() + AXIS_H),
        pos2(rect.right() - 60.0, rect.bottom()),
    );
    let scale = plot.width().max(10.0) / span as f32;
    let x = |time: i64| plot.left() + (time - t0) as f32 * scale;

    // The time axis.
    let step = tick(span, plot.width(), 70.0);
    let mut at = 0;
    while at <= span {
        let px = plot.left() + at as f32 * scale;
        painter.vline(
            px,
            plot.top()..=plot.bottom(),
            Stroke::new(1.0, t.separator.gamma_multiply(0.6)),
        );
        painter.text(
            pos2(px, rect.top() + 4.0),
            Align2::CENTER_TOP,
            format!("+{}", ci::duration_label(at)),
            FontId::proportional(10.5),
            t.text_faint,
        );
        at += step;
    }

    let id = ui.id().with("ci-path-waterfall");
    let mut y = plot.top();
    let mut centers: HashMap<usize, f32> = HashMap::new();
    for row in &rows {
        let h = match row {
            Row::Job(_) => ROW_H,
            Row::Step(..) => STEP_H,
        };
        let row_rect = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), h));
        y += h;
        let cy = row_rect.center().y;
        match *row {
            Row::Job(i) => {
                let job = &jobs[i];
                centers.insert(i, cy);
                let on_path = explorer.critical.contains(i);
                let resp = ui.interact(row_rect, id.with(("job", i)), Sense::click());
                crate::a11y::a11y_selected(
                    &resp,
                    || {
                        format!(
                            "{}: {}, queued {}, ran {}{}",
                            job.name,
                            state_word(job.state),
                            seconds(queued(job)),
                            seconds(ran(job)),
                            if on_path {
                                ", on the critical path"
                            } else {
                                ""
                            }
                        )
                    },
                    selected == Some(i),
                );
                if selected == Some(i) {
                    painter.rect_filled(row_rect, 2.0, t.row_selected);
                } else if resp.hovered() || resp.has_focus() {
                    painter.rect_filled(row_rect, 2.0, t.hover);
                }
                if resp.has_focus() {
                    painter.rect_stroke(
                        row_rect.shrink(1.0),
                        2.0,
                        Stroke::new(1.0, t.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                let mut lx = row_rect.left() + 6.0;
                if !job.steps.is_empty() {
                    let toggle = Rect::from_center_size(pos2(lx + 6.0, cy), vec2(14.0, h));
                    let open = expanded.contains(&i);
                    let tr = ui
                        .interact(toggle, id.with(("expand", i)), Sense::click())
                        .on_hover_text(if open {
                            "Hide its steps"
                        } else {
                            "Show its steps"
                        });
                    painter.text(
                        toggle.center(),
                        Align2::CENTER_CENTER,
                        if open { "v" } else { ">" },
                        FontId::monospace(11.0),
                        t.text_dim,
                    );
                    if tr.clicked() {
                        acts.push(act(Act::Expand(i)));
                    }
                }
                lx += 16.0;
                if on_path {
                    common::paint_mark(
                        ui,
                        Rect::from_center_size(pos2(lx + 6.0, cy), vec2(14.0, 12.0)),
                        common::Mark::Glyph("C", t.text),
                    );
                }
                lx += 18.0;
                painter.text(
                    pos2(lx, cy),
                    Align2::LEFT_CENTER,
                    elide(&job.name, 30),
                    if on_path {
                        theme::medium(12.0)
                    } else {
                        FontId::proportional(12.0)
                    },
                    t.text,
                );
                let delta = deltas.get(&i).copied();
                job_bar(ui, &painter, job, cy, &x, now, on_path, &t);
                if let (Some(d), Some(c), Some(b0)) = (delta, compared, before_t0)
                    && let Some(before) = d.before.and_then(|b| c.run.jobs.get(b))
                    && let (Some(s), Some(f)) = (before.started, before.finished)
                {
                    let ghost = Rect::from_min_max(
                        pos2(x(t0 + (s - b0)), cy + BAR_H / 2.0 + 1.0),
                        pos2(
                            x(t0 + (f - b0)).max(x(t0 + (s - b0)) + 2.0),
                            cy + BAR_H / 2.0 + 5.0,
                        ),
                    );
                    painter.rect_stroke(
                        ghost,
                        1.0,
                        Stroke::new(1.0, t.text_faint),
                        egui::StrokeKind::Inside,
                    );
                }
                if let Some(d) = delta.filter(|d| d.slower)
                    && let Some(end) = end_of(job, now)
                {
                    let at = pos2(x(end) + 4.0, cy);
                    common::paint_mark(
                        ui,
                        Rect::from_min_size(at - vec2(0.0, 6.0), vec2(12.0, 12.0)),
                        common::Mark::Glyph("+", t.warning),
                    );
                    painter.text(
                        at + vec2(15.0, 0.0),
                        Align2::LEFT_CENTER,
                        d.delta().map_or("?".to_owned(), signed),
                        FontId::proportional(10.5),
                        t.warning,
                    );
                }
                let resp = resp.on_hover_ui(|ui| job_tip(ui, explorer, i));
                if resp.double_clicked() {
                    acts.push(Action::OpenCi {
                        sha: explorer.run.info.sha.clone(),
                        run: Some(explorer.run.info.id),
                    });
                } else if resp.clicked() {
                    let pick = if selected == Some(i) { None } else { Some(i) };
                    acts.push(act(Act::Select(pick)));
                }
            }
            Row::Step(i, s) => {
                let job = &jobs[i];
                let step = &job.steps[s];
                let resp = ui.interact(row_rect, id.with(("step", i, s)), Sense::click());
                crate::a11y::a11y_selected(
                    &resp,
                    || {
                        format!(
                            "Step {} of {}: {}",
                            step.name,
                            job.name,
                            state_word(step.state)
                        )
                    },
                    false,
                );
                if resp.hovered() || resp.has_focus() {
                    painter.rect_filled(row_rect, 2.0, t.hover);
                }
                painter.text(
                    pos2(row_rect.left() + 48.0, cy),
                    Align2::LEFT_CENTER,
                    elide(&step.name, 28),
                    FontId::proportional(11.0),
                    t.text_dim,
                );
                let end = step
                    .finished
                    .or_else(|| (step.state == CheckState::Running).then_some(now));
                match step.started.zip(end) {
                    Some((a, b)) => {
                        let bar = Rect::from_min_max(
                            pos2(x(a), cy - 4.0),
                            pos2(x(b).max(x(a) + 2.0), cy + 4.0),
                        );
                        let color = state_color(step.state, &t).gamma_multiply(0.7);
                        painter.rect_filled(bar, 1.0, color);
                        if step.state == CheckState::Failed {
                            glyph_after(ui, bar, "!", t.danger);
                        }
                    }
                    None => unknown(ui, &painter, pos2(plot.left(), cy), &t),
                }
                let took = step.started.zip(step.finished).map(|(a, b)| (b - a).max(0));
                let resp = resp.on_hover_text(format!(
                    "{}: {}, ran {}\nClick for its log on the forge",
                    step.name,
                    state_word(step.state),
                    seconds(took)
                ));
                if resp.clicked() {
                    acts.push(Action::OpenUrl(job.step_url(step)));
                }
            }
        }
    }

    // The critical path's links, then the picked job's dependencies.
    let mut links: Vec<(usize, usize, Evidence, Color32)> = explorer
        .critical
        .jobs
        .windows(2)
        .filter_map(|w| {
            let dep = explorer
                .deps
                .edges
                .iter()
                .find(|d| d.from == w[0] && d.to == w[1])?;
            Some((w[0], w[1], dep.evidence, t.text))
        })
        .collect();
    if let Some(sel) = selected {
        for d in explorer.deps.before(sel) {
            if !explorer.critical.link(d.from, sel) {
                links.push((d.from, sel, d.evidence, t.text_dim));
            }
        }
    }
    for (from, to, evidence, color) in links {
        let (Some(&ya), Some(&yb)) = (centers.get(&from), centers.get(&to)) else {
            continue;
        };
        let (Some(end), Some(start)) = (
            end_of(&jobs[from], now),
            jobs[to].ready().or(jobs[to].started),
        ) else {
            continue;
        };
        let a = pos2(x(end), ya);
        let b = pos2(x(start).max(a.x + 4.0), yb);
        let knee = a.x + 4.0;
        let points = [a, pos2(knee, ya), pos2(knee, yb), b];
        common::evidence_segment(&painter, &points, evidence, Stroke::new(1.5, color));
        painter.add(Shape::convex_polygon(
            vec![b, b + vec2(-5.0, -3.5), b + vec2(-5.0, 3.5)],
            color,
            Stroke::NONE,
        ));
    }
}

fn state_color(state: CheckState, t: &Tokens) -> Color32 {
    match state {
        CheckState::Passed | CheckState::Running => t.accent,
        CheckState::Failed => t.danger,
        CheckState::Skipped => t.text_faint,
    }
}

/// A glyph just after `bar`.
fn glyph_after(ui: &Ui, bar: Rect, text: &'static str, color: Color32) {
    common::paint_mark(
        ui,
        Rect::from_min_size(
            pos2(bar.right() + 3.0, bar.center().y - 6.0),
            vec2(12.0, 12.0),
        ),
        common::Mark::Glyph(text, color),
    );
}

/// A duration the forge didn't record: `?` and a dotted line, never a bar of nothing.
fn unknown(ui: &Ui, painter: &egui::Painter, at: Pos2, t: &Tokens) {
    common::paint_mark(
        ui,
        Rect::from_min_size(at - vec2(0.0, 6.0), vec2(12.0, 12.0)),
        common::Mark::Glyph("?", t.text_dim),
    );
    painter.extend(Shape::dotted_line(
        &[at + vec2(16.0, 0.0), at + vec2(70.0, 0.0)],
        t.text_faint,
        4.0,
        1.0,
    ));
    painter.text(
        at + vec2(76.0, 0.0),
        Align2::LEFT_CENTER,
        "duration unknown",
        FontId::proportional(10.5),
        t.text_faint,
    );
}

/// A job's bar: queued (hatched) then running (filled), outlined on the critical path.
#[allow(clippy::too_many_arguments)]
fn job_bar(
    ui: &Ui,
    painter: &egui::Painter,
    job: &TimedJob,
    cy: f32,
    x: &impl Fn(i64) -> f32,
    now: i64,
    on_path: bool,
    t: &Tokens,
) {
    let (top, bottom) = (cy - BAR_H / 2.0, cy + BAR_H / 2.0);
    let ready = job.ready();
    // Waiting, not started: the wait is known up to now.
    if job.started.is_none() && job.state == CheckState::Running {
        if let Some(r) = ready {
            let wait = Rect::from_min_max(pos2(x(r), top), pos2(x(now).max(x(r) + 2.0), bottom));
            common::paint_mark(ui, wait, common::Mark::Hatched(t.warning));
            painter.text(
                pos2(wait.right() + 4.0, cy),
                Align2::LEFT_CENTER,
                "waiting",
                FontId::proportional(10.5),
                t.text_faint,
            );
        } else {
            unknown(ui, painter, pos2(x(now), cy), t);
        }
        return;
    }
    let Some((s, e)) = job.started.zip(end_of(job, now)) else {
        unknown(ui, painter, pos2(x(ready.unwrap_or(now)), cy), t);
        return;
    };
    if let Some(r) = ready.filter(|r| *r < s) {
        let wait = Rect::from_min_max(pos2(x(r), top), pos2(x(s), bottom));
        common::paint_mark(ui, wait, common::Mark::Hatched(t.warning));
    }
    let bar = Rect::from_min_max(pos2(x(s), top), pos2(x(e).max(x(s) + 2.0), bottom));
    let color = state_color(job.state, t);
    match job.state {
        CheckState::Skipped => common::paint_mark(ui, bar, common::Mark::Hollow(color)),
        _ => {
            painter.rect_filled(bar, 2.0, color);
        }
    }
    if on_path {
        painter.rect_stroke(
            bar.expand(1.5),
            2.0,
            Stroke::new(2.0, t.text),
            egui::StrokeKind::Outside,
        );
    }
    match job.state {
        CheckState::Failed => glyph_after(ui, bar, "!", t.danger),
        CheckState::Running => {
            painter.text(
                pos2(bar.right() + 4.0, cy),
                Align2::LEFT_CENTER,
                "running",
                FontId::proportional(10.5),
                t.text_faint,
            );
        }
        _ => {}
    }
}

/// What hovering a job says.
fn job_tip(ui: &mut Ui, explorer: &Explorer, i: usize) {
    let job = &explorer.run.jobs[i];
    ui.label(egui::RichText::new(&job.name).font(theme::medium(12.0)));
    let mut lines = vec![format!(
        "{} · queued {} · ran {}",
        state_word(job.state),
        seconds(queued(job)),
        seconds(ran(job))
    )];
    if explorer.critical.contains(i) {
        lines.push("On the critical path".to_owned());
    }
    let waits: Vec<String> = explorer
        .deps
        .before(i)
        .map(|d| {
            format!(
                "{} ({})",
                explorer.run.jobs[d.from].name,
                d.evidence.label()
            )
        })
        .collect();
    if !waits.is_empty() {
        lines.push(format!("Waits for {}", waits.join(", ")));
    }
    if let Some(d) = explorer
        .compared
        .as_ref()
        .and_then(|c| c.comparison.jobs.iter().find(|d| d.now == Some(i)))
    {
        lines.push(match d.before {
            None => "Not in the earlier run".to_owned(),
            Some(_) => format!(
                "Earlier run: {} ({})",
                seconds(d.took_before),
                d.delta().map_or("?".to_owned(), signed)
            ),
        });
    }
    lines.push("Click to pick, double-click to open the run in the CI view".to_owned());
    ui.label(lines.join("\n"));
}
