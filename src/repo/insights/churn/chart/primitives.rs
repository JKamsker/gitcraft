//! Sparkline painting and sample labels shared by churn rows and tables.

use super::*;

pub(super) fn verdict_mark(verdict: Option<Verdict>, t: &Tokens) -> Mark {
    match verdict {
        Some(Verdict::Good) => Mark::Glyph("+", t.success),
        Some(Verdict::Bad) => Mark::Glyph("x", t.danger),
        None => Mark::Glyph("?", t.text_faint),
    }
}

pub(super) fn verdict_text(verdict: Option<Verdict>) -> &'static str {
    match verdict {
        Some(Verdict::Good) => "tests passed (test lane)",
        Some(Verdict::Bad) => "tests failed (test lane)",
        None => "no test result recorded (unknown, not failing)",
    }
}

/// A sample, in a line.
pub(super) fn sample_text(s: &Tested) -> String {
    format!(
        "{} · {} · {}\n{}",
        short(&s.sample.hash),
        day(s.sample.time),
        s.sample.subject,
        verdict_text(s.verdict)
    )
}

pub(super) fn size_text(p: &Point) -> String {
    match p.size {
        None if p.files > 0 && p.binary == p.files => "binary only, no lines".to_owned(),
        None if p.files > 0 => format!("not read ({} files over the budget)", p.files),
        None => "not there at this sample".to_owned(),
        Some(n) if p.complete => format!("{n} lines"),
        Some(n) => format!("at least {n} lines ({} of {} files read)", p.read, p.files),
    }
}

fn complexity_text(p: &Point) -> String {
    match p.complexity {
        None => "not measured (no file of a language the heuristic reads was read)".to_owned(),
        Some(n) => {
            let bound = if p.complete { "" } else { "at least " };
            let of = if p.measured < p.files {
                format!(", {} of its {} files measured", p.measured, p.files)
            } else {
                String::new()
            };
            format!("{bound}{n} branch points{of}")
        }
    }
}

/// What a point's tooltip says.
pub(super) fn point_text(built: &Built, series: &Series, k: usize) -> String {
    let tl = &built.timeline;
    let (Some(p), Some(s)) = (series.points.get(k), tl.samples.get(k)) else {
        return String::new();
    };
    let commits = series.commits_in(k).len();
    format!(
        "{}\nSize (factual): {}\nChurn (factual): {} lines in {} of the window's {} commits\n\
         Complexity ({}): {}\nClick: list the window's commits",
        sample_text(s),
        size_text(p),
        p.churn,
        commits,
        tl.windows.get(k).map_or(0, Vec::len),
        analyzer::LABEL,
        complexity_text(p),
    )
}

/// `n` short: `1.2k`.
fn compact(n: u64) -> String {
    match n {
        0..1000 => n.to_string(),
        1000..1_000_000 => format!("{:.1}k", n as f64 / 1000.0),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

/// A sparkline of `values` in `rect`: a line through the points there, filled where read in
/// full and hollow where a lower bound, a dashed tick at the bottom where there is none.
pub(super) fn line(
    painter: &Painter,
    rect: Rect,
    values: &[(Option<u64>, bool)],
    color: Color32,
    t: &Tokens,
) {
    let xs = xs(rect, values.len());
    let max = values.iter().filter_map(|v| v.0).max().unwrap_or(0).max(1) as f32;
    let y = |v: u64| rect.bottom() - 3.0 - v as f32 / max * (rect.height() - 6.0);
    let step = if xs.len() > 1 { xs[1] - xs[0] } else { 8.0 };
    for (k, w) in values.windows(2).enumerate() {
        if let (Some(a), Some(b)) = (w[0].0, w[1].0) {
            painter.line_segment(
                [pos2(xs[k], y(a)), pos2(xs[k + 1], y(b))],
                Stroke::new(1.5, color),
            );
        }
    }
    for (k, &(v, complete)) in values.iter().enumerate() {
        match v {
            Some(v) if complete => {
                painter.circle_filled(pos2(xs[k], y(v)), 2.2, color);
            }
            Some(v) => {
                painter.circle_stroke(pos2(xs[k], y(v)), 2.6, Stroke::new(1.2, color));
            }
            None => {
                let half = (step / 2.0 - 1.0).max(1.5);
                painter.extend(Shape::dashed_line(
                    &[
                        pos2(xs[k] - half, rect.bottom() - 2.0),
                        pos2(xs[k] + half, rect.bottom() - 2.0),
                    ],
                    Stroke::new(1.0, t.text_faint),
                    2.0,
                    2.0,
                ));
            }
        }
    }
    max_label(painter, rect, max as u64, t);
}

/// Bars of `values` in `rect`, one per window.
pub(super) fn bars(painter: &Painter, rect: Rect, values: &[u64], color: Color32, t: &Tokens) {
    let xs = xs(rect, values.len());
    let max = values.iter().copied().max().unwrap_or(0).max(1) as f32;
    let step = if xs.len() > 1 { xs[1] - xs[0] } else { 10.0 };
    let w = (step * 0.6).clamp(2.0, 10.0);
    for (k, &v) in values.iter().enumerate() {
        let h = if v == 0 {
            1.0
        } else {
            (v as f32 / max * (rect.height() - 6.0)).max(2.0)
        };
        let bar = Rect::from_min_max(
            pos2(xs[k] - w / 2.0, rect.bottom() - 2.0 - h),
            pos2(xs[k] + w / 2.0, rect.bottom() - 2.0),
        );
        let fill = if v == 0 { t.text_faint } else { color };
        painter.rect_filled(bar, 1.0, fill);
    }
    max_label(painter, rect, max as u64, t);
}

pub(super) fn max_label(painter: &Painter, rect: Rect, max: u64, t: &Tokens) {
    painter.text(
        rect.right_top(),
        Align2::RIGHT_TOP,
        compact(max),
        egui::FontId::proportional(9.5),
        t.text_faint,
    );
}
