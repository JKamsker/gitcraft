//! How a parallel bisect shows on the graph: the commits being tested, those tested (green,
//! red, faint for skipped), the first bad one, and the commits that may still be it, which
//! shrink round after round.

use egui::{Color32, Painter, Rect, vec2};

use super::banner::last_lines;
use crate::parallel_bisect::Verdict;
use crate::repo::RepoTab;
use crate::repo::graph_view::{ROW_GAP, ROW_H};
use crate::theme::Tokens;

/// How much of a test's output a row's hover shows.
const OUTPUT_LINES: usize = 12;

/// What the parallel bisect says of a commit, for its row.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::repo) enum RowMark {
    Testing,
    Tested(Verdict),
    FirstBad,
    /// It may still be the first bad commit.
    Left,
}

impl RepoTab {
    /// What the parallel bisect shown says of `hash`.
    pub(in crate::repo) fn parallel_row(&self, hash: &str) -> Option<RowMark> {
        let view = self.parallel.view()?;
        if view.culprits().iter().any(|h| h == hash) {
            return Some(RowMark::FirstBad);
        }
        if view.testing.contains(hash) {
            return Some(RowMark::Testing);
        }
        if let Some(tested) = view.results.get(hash) {
            return Some(RowMark::Tested(tested.verdict));
        }
        (view.running() && view.remaining.contains(hash)).then_some(RowMark::Left)
    }

    /// Tints the row of `hash` as the parallel bisect says, and gives what it says, to show on
    /// hover.
    pub(in crate::repo) fn parallel_mark(
        &self,
        painter: &Painter,
        t: &Tokens,
        rect: Rect,
        hash: &str,
    ) -> Option<String> {
        let mark = self.parallel_row(hash)?;
        let view = self.parallel.view()?;
        let (color, tint, said) = match &mark {
            RowMark::FirstBad => (
                t.danger,
                0.16,
                "The first bad commit, as the parallel bisect found".to_owned(),
            ),
            RowMark::Testing => (t.accent, 0.10, format!("Being tested on {}…", view.machine)),
            RowMark::Tested(verdict) => {
                let result = self.parallel_result(hash);
                let how = if result.is_some_and(|r| r.cached) {
                    "its tree was tested before"
                } else {
                    "tested"
                };
                let mut said =
                    format!("{} in the parallel bisect ({how})", capital(verdict.word()));
                if let Some(output) = result.map(|r| last_lines(&r.output, OUTPUT_LINES))
                    && !output.is_empty()
                {
                    said.push_str("\n\n");
                    said.push_str(&output);
                }
                (verdict_color(t, *verdict), 0.08, said)
            }
            RowMark::Left => (
                t.accent,
                0.0,
                "May still be the first bad commit".to_owned(),
            ),
        };
        let band = rect.shrink2(vec2(0.0, ROW_GAP / 2.0));
        if tint > 0.0 {
            painter.rect_filled(band, 0.0, color.gamma_multiply(tint));
        }
        // What is left: a thin line, the range it draws shrinking; the rest a strip.
        let width = if mark == RowMark::Left { 1.5 } else { 3.0 };
        painter.rect_filled(
            Rect::from_min_size(band.min, vec2(width, ROW_H)),
            0.0,
            color,
        );
        Some(said)
    }
}

/// The colour of a commit tested so: the theme's own for success and danger, faint text for
/// one skipped, warning for one that ran out of time.
fn verdict_color(t: &Tokens, verdict: Verdict) -> Color32 {
    match verdict {
        Verdict::Good => t.success,
        Verdict::Bad => t.danger,
        Verdict::Skip => t.text_faint,
        Verdict::TimedOut => t.warning,
    }
}

/// "good" → "Good".
fn capital(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
