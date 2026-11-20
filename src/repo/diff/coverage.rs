//! Test coverage in the gutter of the diff (#173 item 8), opt-in: each changed line of the new
//! side marked covered, uncovered or unknown, as the repository's coverage report says. What
//! the report says is read and judged in the background by the Insights coverage map's own
//! logic (`repo::insights::coverage`); here it is only shown. A report that isn't of the
//! diff's revision is never applied: its lines show unknown, under a conspicuous chip that
//! says why. No report: no marks and no chip.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{DiffData, Kind};
use crate::repo::{Action, DiffKey, DiffView, RepoTab, widgets};
use crate::settings::Settings;
use crate::theme::Tokens;

/// What the coverage gutter is asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoverageAct {
    /// Show the marks in diffs, or stop (the setting).
    Toggle,
    /// Read what the report says of the diff shown.
    Load,
}

/// What is known of a changed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Mark {
    /// It ran, so many times.
    Covered(u64),
    Uncovered,
    /// No current evidence: the report doesn't match the revision, or doesn't have the file.
    Unknown,
}

/// How far the report can be trusted for the diff's revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Trust {
    /// Its `.sha` records the revision shown (for uncommitted changes: HEAD, edits older).
    Current,
    /// Its `.sha` records another commit.
    Other(String),
    /// Of HEAD, but files changed after it was written.
    EditedAfter,
    /// Of HEAD; whether it includes the uncommitted edits can't be told.
    Undetermined,
    /// The staged file isn't the working tree's, which is what was measured.
    IndexDiffers,
    /// No `.sha` beside it records the commit tested.
    NoRevision,
}

/// The marks of a diff and what they rest on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) struct Gutter {
    /// The report, as found (relative to the repository).
    pub(in crate::repo) report: String,
    /// The revision the diff's new side is of (HEAD for uncommitted changes).
    pub(in crate::repo) sha: String,
    pub(in crate::repo) trust: Trust,
    /// By line number on the new side; a changed line not here isn't executable.
    pub(in crate::repo) marks: HashMap<u32, Mark>,
}

/// The gutter of the diff open: asked for once per reading of the diff.
#[derive(Default)]
pub(in crate::repo) struct Overlay {
    /// The reading (its source hash) it was asked for.
    pub(in crate::repo) asked: Option<u64>,
    /// The answer: `Ok(None)` when there is no report.
    pub(in crate::repo) gutter: Option<Result<Option<Arc<Gutter>>, String>>,
}

impl Overlay {
    /// The gutter to paint, when the setting is on and it is read.
    pub(super) fn shown(&self, on: bool) -> Option<&Gutter> {
        match &self.gutter {
            Some(Ok(Some(g))) if on => Some(g),
            _ => None,
        }
    }
}

/// The diffs whose new side a report can be about: not a merge's own resolution, not a
/// conflict (it has no single new side).
pub(in crate::repo) fn applies(key: &DiffKey) -> bool {
    match key {
        DiffKey::Commit { .. } | DiffKey::Range { .. } => true,
        DiffKey::Work(f) => f.area != crate::git::Area::Conflicted,
        DiffKey::Remerge { .. } => false,
    }
}

/// Asks for the gutter when it is on and not read for this reading of the diff yet.
pub(super) fn request(view: &DiffView, data: &DiffData, on: bool, acts: &mut Vec<Action>) {
    if on && applies(&view.key) && view.coverage.asked != Some(data.source) {
        acts.push(Action::DiffCoverage(CoverageAct::Load));
    }
}

impl RepoTab {
    /// Turns the coverage marks on or off in every diff.
    pub(in crate::repo) fn toggle_diff_coverage(&mut self, ctx: &egui::Context) {
        let mut settings = Settings::get(ctx);
        settings.diff_coverage = !settings.diff_coverage;
        settings.set(ctx);
        if let Some(view) = &mut self.diff {
            // Read again when next shown: the report may have changed meanwhile.
            view.coverage = Overlay::default();
        }
    }
}

/// The palette's entry turning the marks on or off.
pub(in crate::repo) fn palette_item() -> (String, String, Action) {
    (
        crate::i18n::text("diff-coverage-toggle").to_owned(),
        "coverage lcov cobertura gutter tests".to_owned(),
        Action::DiffCoverage(CoverageAct::Toggle),
    )
}

/// The diff header's switch, laid out right to left.
pub(super) fn control(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Action>) {
    let on = Settings::get(ui.ctx()).diff_coverage;
    let b = crate::icons::small_button(
        ui,
        "flask-conical",
        crate::i18n::text(if on {
            "diff-coverage-hide"
        } else {
            "diff-coverage-show"
        }),
    );
    if on {
        widgets::active_underline(ui, b.rect, t.accent);
    }
    if b.clicked() {
        acts.push(Action::DiffCoverage(CoverageAct::Toggle));
    }
}

/// The mark of line `i` of the diff, painted on the new side.
pub(super) fn mark_of(gutter: &Gutter, data: &DiffData, i: usize) -> Option<Mark> {
    let line = data.lines.get(i)?;
    if line.kind != Kind::Add {
        return None;
    }
    gutter.marks.get(&line.new?).copied()
}

/// What a mark says, in words: its tooltip and accessible name.
pub(in crate::repo) fn mark_text(mark: Mark) -> String {
    match mark {
        Mark::Covered(hits) => crate::i18n::message(
            "diff-coverage-mark-covered",
            &[("hits", crate::i18n::Value::from(hits))],
        ),
        Mark::Uncovered => crate::i18n::text("diff-coverage-mark-uncovered").to_owned(),
        Mark::Unknown => crate::i18n::text("diff-coverage-mark-unknown").to_owned(),
    }
}

/// Paints `mark` as a bar ending at `right` (the edge of the numbers) on the line `rect`:
/// filled green when covered, filled red when not, hollow when unknown.
pub(super) fn paint_mark(ui: &mut Ui, rect: Rect, right: f32, mark: Mark, i: usize) {
    let t = Tokens::get(ui.ctx());
    let bar = Rect::from_min_size(
        pos2(right - 5.0, rect.top() + 2.0),
        vec2(3.0, rect.height() - 4.0),
    );
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    match mark {
        Mark::Covered(_) => {
            painter.rect_filled(bar, 1.0, t.success);
        }
        Mark::Uncovered => {
            painter.rect_filled(bar, 1.0, t.danger);
        }
        Mark::Unknown => {
            painter.rect_stroke(
                bar,
                1.0,
                Stroke::new(1.0, t.text_faint),
                egui::StrokeKind::Inside,
            );
        }
    }
    let resp = ui
        .interact(
            bar.expand2(vec2(3.0, 0.0)),
            ui.id().with(("diff_coverage", i)),
            Sense::hover(),
        )
        .on_hover_text(mark_text(mark));
    crate::a11y::a11y(&resp, egui::WidgetType::Label, || mark_text(mark));
}

/// The line over the diff saying which report the marks come from, or, conspicuously, why
/// they say nothing.
pub(super) fn banner(ui: &mut Ui, overlay: &Overlay, on: bool) {
    if !on {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let (text, tip, warn) = match &overlay.gutter {
        Some(Ok(Some(g))) => banner_text(g),
        Some(Err(e)) => (
            crate::i18n::text("diff-coverage-error").to_owned(),
            e.clone(),
            true,
        ),
        // Not read yet, or no report: nothing to say.
        _ => return,
    };
    egui::Frame::new()
        .fill(if warn {
            t.warning.gamma_multiply(0.14)
        } else {
            t.chrome
        })
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let color = if warn { t.warning } else { t.text_dim };
            ui.label(RichText::new(text).color(color).small())
                .on_hover_text(tip);
        });
}

/// What the banner says of `g` and its tip; `true` when the marks aren't applied.
pub(in crate::repo) fn banner_text(g: &Gutter) -> (String, String, bool) {
    let short = |s: &str| s.chars().take(8).collect::<String>();
    let args = |id: &str, sha: &str| {
        crate::i18n::message(
            id,
            &[
                ("report", g.report.clone().into()),
                ("sha", short(sha).into()),
            ],
        )
    };
    let (id, sha) = match &g.trust {
        Trust::Current => {
            return (
                args("diff-coverage-current", &g.sha),
                crate::i18n::text("diff-coverage-legend").to_owned(),
                false,
            );
        }
        Trust::Other(recorded) => ("diff-coverage-stale-other", recorded.as_str()),
        Trust::EditedAfter => ("diff-coverage-stale-edited", g.sha.as_str()),
        Trust::Undetermined => ("diff-coverage-undetermined", g.sha.as_str()),
        Trust::IndexDiffers => ("diff-coverage-stale-index", g.sha.as_str()),
        Trust::NoRevision => ("diff-coverage-no-revision", g.sha.as_str()),
    };
    (args(id, sha), args("diff-coverage-stale-tip", &g.sha), true)
}
