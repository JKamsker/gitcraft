//! Job bar painting, status glyphs, and job tooltips for the CI waterfall.

use super::*;

pub(super) fn state_color(state: CheckState, t: &Tokens) -> Color32 {
    match state {
        CheckState::Passed | CheckState::Running => t.accent,
        CheckState::Failed => t.danger,
        CheckState::Skipped => t.text_faint,
    }
}

/// A glyph just after `bar`.
pub(super) fn glyph_after(ui: &Ui, bar: Rect, text: &'static str, color: Color32) {
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
pub(super) fn unknown(ui: &Ui, painter: &egui::Painter, at: Pos2, t: &Tokens) {
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
pub(super) fn job_bar(
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
pub(super) fn job_tip(ui: &mut Ui, explorer: &Explorer, i: usize) {
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
