//! The _weakens checks_ badge on a diff's hunks (see [`crate::weakened`]).

use std::collections::HashMap;

use egui::{CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::{DiffLine, Kind};
use crate::theme::{self, Tokens};

/// The original summary and its typed evidence. Only presentation is resolved per paint;
/// the diff's scan, source identity and lifetime do not depend on the language.
pub(in crate::repo) struct HunkSummary {
    raw: String,
    flags: Vec<crate::weakened::Flag>,
}

impl HunkSummary {
    fn new(flags: Vec<crate::weakened::Flag>) -> Self {
        Self {
            raw: crate::weakened::summary(flags.iter()),
            flags,
        }
    }

    fn display_summary(&self) -> String {
        crate::weakened::display::summary(self.flags.iter())
    }
}

impl std::ops::Deref for HunkSummary {
    type Target = str;
    fn deref(&self) -> &str {
        &self.raw
    }
}

impl AsRef<str> for HunkSummary {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}

impl std::fmt::Display for HunkSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

/// The evidence of each hunk that weakens the checks in the one-file diff `text`, by the line
/// of `lines` its header is. What concerns the whole file (a test file deleted, a snapshot
/// rewritten) goes on the first hunk.
pub(super) fn by_hunk_line(text: &str, lines: &[DiffLine]) -> HashMap<usize, HunkSummary> {
    let Some(file) = crate::weakened::scan_patch(text).into_iter().next() else {
        return HashMap::new();
    };
    let headers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == Kind::Hunk)
        .map(|(i, _)| i)
        .collect();
    let mut out = HashMap::new();
    for (place, &line) in headers.iter().enumerate() {
        let flags = file
            .flags
            .iter()
            .filter(|f| f.hunk.unwrap_or(0) == place)
            .cloned()
            .collect();
        let tip = HunkSummary::new(flags);
        if !tip.is_empty() {
            out.insert(line, tip);
        }
    }
    out
}

/// The badge right-aligned at `right` on the header of hunk `i`, `tip` on hover. Opaque, so
/// the header's text under it doesn't show through.
pub(super) fn badge(ui: &mut Ui, right: f32, y: f32, tip: &HunkSummary, i: usize) {
    let t = Tokens::get(ui.ctx());
    let color = crate::repo::widgets::badge_text(&t);
    let galley = ui.painter().layout_no_wrap(
        crate::weakened::display::badge().to_owned(),
        theme::medium(10.0),
        color,
    );
    let size = galley.size() + vec2(10.0, 3.0);
    let rect = Rect::from_min_size(pos2(right - size.x, y - size.y / 2.0), size);
    let painter = ui.painter();
    painter.rect_filled(rect.expand(3.0), CornerRadius::ZERO, t.dock);
    painter.rect_filled(rect, CornerRadius::same(4), t.warning.gamma_multiply(0.18));
    painter.galley(rect.min + vec2(5.0, 1.5), galley, color);
    ui.interact(rect, ui.id().with(("weakened-hunk", i)), Sense::hover())
        .on_hover_text(tip.display_summary());
}

#[cfg(test)]
mod core_guard_paint_tests {
    use super::*;
    use crate::repo::push_preflight::core_guard_paint_tests::{frame, text};

    #[test]
    fn core_guard_paint_cached_hunks_repaint_from_typed_flags_without_rescan() -> Result<(), String>
    {
        let patch = "diff --git a/tests/a.rs b/tests/a.rs\n--- a/tests/a.rs\n+++ b/tests/a.rs\n@@ -12344,1 +12344,2 @@\n #[test]\n+#[ignore]\n";
        let lines = super::super::parse(patch);
        let summaries = by_hunk_line(patch, &lines);
        let (&line, summary) = summaries.iter().next().ok_or("one hunk")?;
        assert_eq!(lines[line].kind, Kind::Hunk);
        let flags = summary.flags.clone();
        let raw = crate::weakened::summary(flags.iter());
        assert_eq!(summary.as_ref(), raw);
        let ctx = egui::Context::default();
        let mut draw = |ui: &mut Ui| {
            ui.allocate_space(vec2(400., 70.));
            badge(ui, 300., 30., summary, line);
        };
        let mut generation = crate::i18n::generation();
        for (locale, label, heading, number) in [
            (
                "en-US",
                "weakens checks",
                "Skips, disables or focuses tests",
                "12,345",
            ),
            (
                "de",
                "schwächt Prüfungen ab",
                "Überspringt, deaktiviert oder fokussiert Tests",
                "12.345",
            ),
            (
                "en-US",
                "weakens checks",
                "Skips, disables or focuses tests",
                "12,345",
            ),
        ] {
            let changed = crate::i18n::set_language(locale);
            assert_eq!(crate::i18n::generation() > generation, changed);
            generation = crate::i18n::generation();
            let pointer = vec![egui::Event::PointerMoved(pos2(290., 30.))];
            let _ = frame(&ctx, pointer.clone(), &mut draw);
            let _ = frame(&ctx, pointer.clone(), &mut draw);
            let shown = text(&frame(&ctx, pointer, &mut draw));
            assert!(shown.contains(label), "{shown}");
            assert!(shown.contains(heading), "{shown}");
            assert!(shown.contains(number), "{shown}");
            assert!(shown.contains("#[ignore]"), "{shown}");
            assert_eq!(summary.flags, flags);
            assert_eq!(summary.as_ref(), raw);
            assert!(summary.starts_with("Skips, disables or focuses tests:"));
        }

        Ok(())
    }
}
