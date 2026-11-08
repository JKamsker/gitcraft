//! Test coverage relationship map (#175 item 11): what a coverage report says of the lines a
//! change touched (the working tree against HEAD, or a commit against its parent), taken only
//! when the report can be shown to be of that revision (a `<report>.sha` beside it naming the
//! commit). Otherwise what it says stays unknown, never uncovered. Tests appear only when the
//! report records them (lcov `TN:` sections); the test lane's results for the commit appear
//! beside them. A hunk opens its diff, a file the file at the revision, a test what it ran.

mod picture;
mod tables;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{RichText, Ui};

use self::picture::{Laid, NodeRef};
use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use super::graph::{self, Pick};
use crate::insights::Evidence;
use crate::insights::coverage::{self as core, Ask, Coverage, HunkCov, LineCov};
use crate::repo::file_view::FileTab;
use crate::repo::{Action, DiffKey, RepoTab, WIP_HASH, short};
use crate::theme::Tokens;

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The lines looked at, as picked; the working tree when `None`.
    ask: Option<Ask>,
    /// The report picked or typed; the first found in the usual places when `None`.
    report: Option<String>,
    /// What is typed in the report's field.
    typed: String,
    data: Slot<Arc<Coverage>>,
    layout: Slot<Arc<Laid>>,
    selected: Option<Pick>,
    /// The test whose hunks stand out.
    test: Option<usize>,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    Ask(Ask),
    /// Read this report (`None`: the first found).
    Report(Option<String>),
    Select(Option<Pick>),
    Test(Option<usize>),
}

fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Coverage(a))
}

/// Opens a hunk where it is: the working tree's diff against HEAD, or the commit's diff of
/// those lines.
fn open_hunk(data: &Coverage, file: usize, hunk: usize) -> Option<Action> {
    let f = data.files.get(file)?;
    let h = f.hunks.get(hunk)?;
    Some(match data.ask {
        Ask::WorkTree => Action::OpenDiff(DiffKey::Range {
            from: data.sha.clone(),
            to: None,
            path: f.path.clone(),
        }),
        Ask::Commit(_) => Action::OpenDiffOfLines {
            hash: data.sha.clone(),
            path: f.path.clone(),
            lines: vec![h.span()],
        },
    })
}

/// What clicking a node opens or picks.
fn open_node(data: &Coverage, node: NodeRef, test: Option<usize>) -> Vec<Action> {
    match node {
        NodeRef::File(f) => data
            .files
            .get(f)
            .map(|file| Action::FileView {
                path: file.path.clone(),
                rev: match data.ask {
                    Ask::WorkTree => None,
                    Ask::Commit(_) => Some(data.sha.clone()),
                },
                tab: FileTab::File,
            })
            .into_iter()
            .collect(),
        NodeRef::Hunk(f, h) => open_hunk(data, f, h).into_iter().collect(),
        NodeRef::Test(t) => vec![act(Act::Test((test != Some(t)).then_some(t)))],
        NodeRef::Lane(_) => vec![
            Action::SelectHash(data.sha.clone()),
            Action::TestLane(crate::repo::test_lane::LaneAct::ShowOutput(
                data.tree.clone(),
            )),
        ],
    }
}

impl RepoTab {
    /// The lines the map can be about, for its menu.
    fn coverage_subjects(&self) -> Vec<(String, Ask)> {
        let mut list = vec![("Working tree against HEAD".to_owned(), Ask::WorkTree)];
        if let Some(hash) = self.detail_hash.as_deref().filter(|h| *h != WIP_HASH) {
            list.push((
                format!("Selected commit {} against its parent", short(hash)),
                Ask::Commit(hash.to_owned()),
            ));
        }
        list.push((
            "HEAD against its parent".to_owned(),
            Ask::Commit("HEAD".to_owned()),
        ));
        list
    }

    /// Reads what the view shows (again).
    pub(super) fn coverage_load(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.coverage;
        let ask = state.ask.clone().unwrap_or(Ask::WorkTree);
        let report = state.report.clone();
        let request = state.data.start();
        state.selected = None;
        state.test = None;
        self.then(
            ctx,
            move || core::analyze(&path, &ask, report.as_deref()).map(Arc::new),
            move |tab, result, ctx| {
                let Some(view) = tab.insights.as_mut() else {
                    return;
                };
                if view.coverage.data.accept(request, result) {
                    tab.coverage_layout(ctx);
                }
            },
        );
    }

    /// Builds the map of what was read, in the background.
    fn coverage_layout(&mut self, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.coverage;
        let Some(data) = state.data.ready().cloned() else {
            return;
        };
        let test = state.test;
        let request = state.layout.start();
        self.then(
            ctx,
            move || -> Result<Arc<Laid>, String> { Ok(Arc::new(picture::build(&data, test, &t))) },
            move |tab, laid, _| {
                if let Some(view) = tab.insights.as_mut() {
                    view.coverage.layout.accept(request, laid);
                }
            },
        );
    }

    pub(super) fn coverage_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.coverage;
        match act {
            Act::Ask(ask) => {
                state.ask = Some(ask);
                self.coverage_load(ctx);
            }
            Act::Report(report) => {
                state.report = report;
                self.coverage_load(ctx);
            }
            Act::Select(pick) => state.selected = pick,
            Act::Test(test) => {
                state.test = test;
                self.coverage_layout(ctx);
            }
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn coverage_back(&mut self) -> bool {
        let Some(view) = self.insights.as_mut() else {
            return false;
        };
        let state = &mut view.coverage;
        if state.selected.take().is_some() {
            return true;
        }
        if state.test.take().is_some() {
            return true;
        }
        false
    }

    pub(super) fn coverage_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let subjects = self.coverage_subjects();
        let Some(view) = self.insights.as_deref_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.coverage;
        controls(ui, state, &subjects, acts);
        let state = &view.coverage;
        match common::slot_state(ui, &state.data, "the changed lines and the coverage report") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(data) = state.data.ready() else {
            return;
        };
        common::provenance(ui, &data.provenance());
        trust(ui, data);
        legend(ui);
        let counts = data.counts();
        let t = Tokens::get(ui.ctx());
        if data.files.is_empty() {
            common::empty(
                ui,
                "No changed lines here: pick a commit above (HEAD against its parent).",
            );
            return;
        }
        ui.label(
            RichText::new(format!("Changed lines: {}", picture::counts_text(counts)))
                .color(t.text_dim),
        );
        uncovered(ui, data, acts);
        lane(ui, data, acts);
        if table {
            tables::show(ui, data, state.test, acts);
            return;
        }
        let Some(laid) = state.layout.ready() else {
            common::loading(ui, "the map");
            return;
        };
        if let Some(pick) = state.selected {
            details(ui, data, laid, pick, acts);
        }
        if laid.hidden > 0 {
            ui.label(
                RichText::new(format!(
                    "{} more hunks are left out of the picture; the table lists them",
                    laid.hidden
                ))
                .size(11.5)
                .color(t.text_faint),
            );
        }
        let picked = graph::show(
            ui,
            "coverage",
            &laid.graph,
            &laid.positions,
            state.selected,
            300.0,
        );
        if let Some(pick) = picked.clicked {
            acts.push(act(Act::Select(Some(pick))));
            if let Pick::Node(i) = pick
                && let Some(&node) = laid.nodes.get(i)
            {
                acts.extend(open_node(data, node, state.test));
            }
        }
    }
}

/// The lines looked at, the report read, and a report's path to type.
fn controls(ui: &mut Ui, state: &mut State, subjects: &[(String, Ask)], acts: &mut Vec<Action>) {
    let asked = state.ask.clone().unwrap_or(Ask::WorkTree);
    ui.horizontal_wrapped(|ui| {
        let shown = subjects
            .iter()
            .find(|(_, a)| *a == asked)
            .map_or_else(|| "The change".to_owned(), |(l, _)| l.clone());
        egui::ComboBox::from_id_salt("coverage-subject")
            .selected_text(shown)
            .width(260.0)
            .show_ui(ui, |ui| {
                for (label, ask) in subjects {
                    if ui.selectable_label(*ask == asked, label).clicked() && *ask != asked {
                        acts.push(act(Act::Ask(ask.clone())));
                    }
                }
            });
        if let Some(data) = state.data.ready() {
            let current = data.report.as_ref().map(|r| r.path.clone());
            if current.is_some() || !data.others.is_empty() {
                egui::ComboBox::from_id_salt("coverage-report")
                    .selected_text(current.clone().unwrap_or_else(|| "No report".to_owned()))
                    .show_ui(ui, |ui| {
                        for other in current.iter().chain(&data.others) {
                            if ui
                                .selectable_label(Some(other) == current.as_ref(), other)
                                .clicked()
                                && Some(other) != current.as_ref()
                            {
                                acts.push(act(Act::Report(Some(other.clone()))));
                            }
                        }
                    });
            }
        }
        let field = ui.add(
            egui::TextEdit::singleline(&mut state.typed)
                .hint_text("Another report: path/to/lcov.info")
                .desired_width(220.0),
        );
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Read").clicked() || entered) && !state.typed.trim().is_empty() {
            acts.push(act(Act::Report(Some(state.typed.trim().to_owned()))));
        }
        if state.report.is_some() && ui.small_button("Usual places").clicked() {
            acts.push(act(Act::Report(None)));
        }
    });
}

/// How far the report is trusted for these lines, and what follows from it.
fn trust(ui: &mut Ui, data: &Coverage) {
    let t = Tokens::get(ui.ctx());
    let Some(report) = &data.report else {
        common::unavailable(
            ui,
            "No coverage report found",
            &format!(
                "Looked for {}. Type a report's path above, or make one (cargo llvm-cov \
                 --lcov --output-path lcov.info) and write the commit it tested to \
                 lcov.info.sha beside it. Every changed line is unknown until then.",
                data.searched.join(", ")
            ),
        );
        return;
    };
    let (word, color) = match report.freshness.evidence {
        Evidence::Observed => ("Matches the revision", t.success),
        Evidence::Inferred => ("Likely matches", t.warning),
        Evidence::Unknown => ("Revision not matched", t.text_faint),
    };
    let mut lines = vec![format!("{word}: {}.", report.freshness.said)];
    if report.freshness.evidence == Evidence::Unknown {
        lines.push(format!(
            "What it says of the changed lines isn't taken: they show as unknown. Write the \
             tested commit's id to {}{} to match it.",
            report.path,
            core::SIDECAR
        ));
    }
    if report.per_test && report.freshness.evidence == Evidence::Unknown {
        lines.push(
            "It names tests, but which ran what isn't shown for an unmatched report.".to_owned(),
        );
    } else if !report.per_test {
        lines.push(format!(
            "Aggregate coverage only: {} records no tests per line{}.",
            report.format.label(),
            if report.tests.len() == 1 {
                " (it names one, for everything)"
            } else {
                ""
            }
        ));
    }
    for (i, line) in lines.into_iter().enumerate() {
        let text = RichText::new(line).size(12.0);
        ui.add(
            egui::Label::new(if i == 0 {
                text.color(color)
            } else {
                text.color(t.text_faint)
            })
            .wrap(),
        );
    }
}

fn legend(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    common::legend(
        ui,
        &[
            (Mark::Glyph("C", t.success), "covered"),
            (Mark::Glyph("!", t.danger), "uncovered lines"),
            (Mark::Glyph("-", t.text_dim), "not instrumented"),
            (
                Mark::Glyph("?", t.text_faint),
                "unknown (no matching report)",
            ),
            (Mark::Glyph("T", t.accent), "a test the report names"),
            (
                Mark::Line(t.text_dim, false),
                "solid: matched by .sha (observed)",
            ),
            (Mark::Line(t.text_dim, true), "dashed: likely (inferred)"),
            (Mark::Dotted(t.text_dim), "dotted: unknown"),
        ],
    );
}

/// The changed lines a trusted report says no test ran, each range opening its hunk.
fn uncovered(ui: &mut Ui, data: &Coverage, acts: &mut Vec<Action>) {
    let mut rows: Vec<(usize, usize, String)> = Vec::new();
    for (fi, f) in data.files.iter().enumerate() {
        for (hi, h) in f.hunks.iter().enumerate() {
            for (a, b) in uncovered_ranges(h) {
                let lines = if a == b {
                    format!("line {a}")
                } else {
                    format!("lines {a}-{b}")
                };
                rows.push((fi, hi, format!("{}: {lines}", f.path)));
            }
        }
    }
    if rows.is_empty() {
        return;
    }
    common::heading(
        ui,
        &format!("Uncovered changed lines ({} ranges)", rows.len()),
    );
    egui::ScrollArea::vertical()
        .id_salt("coverage-uncovered")
        .max_height(110.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (fi, hi, text) in rows {
                if common::table_row(ui, &[], &[text], false, false)
                    .on_hover_text("Open its diff")
                    .clicked()
                {
                    acts.extend(open_hunk(data, fi, hi));
                }
            }
        });
}

/// The runs of uncovered lines in a hunk.
fn uncovered_ranges(h: &HunkCov) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for l in h.lines.iter().filter(|l| l.cov == LineCov::Uncovered) {
        match out.last_mut() {
            Some((_, end)) if *end + 1 == l.line => *end = l.line,
            _ => out.push((l.line, l.line)),
        }
    }
    out
}

/// The test lane's results for the commit tested.
fn lane(ui: &mut Ui, data: &Coverage, acts: &mut Vec<Action>) {
    if data.lane.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Test lane at {}:", short(&data.sha))).color(t.text_dim));
        for (i, v) in data.lane.iter().enumerate() {
            if ui
                .button(format!("{} {}", v.name, v.verdict.word()))
                .on_hover_text("Show the commit, and what its test printed if it ran here")
                .clicked()
            {
                acts.extend(open_node(data, NodeRef::Lane(i), None));
            }
        }
    });
}

/// What is selected in the picture, line by line.
fn details(ui: &mut Ui, data: &Coverage, laid: &Laid, pick: Pick, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let Pick::Node(i) = pick else {
        return;
    };
    let Some(&node) = laid.nodes.get(i) else {
        return;
    };
    let tests: &[String] = data.report.as_ref().map_or(&[], |r| &r.tests);
    egui::Frame::new()
        .fill(t.card)
        .corner_radius(t.radius)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let title = match node {
                    NodeRef::File(f) => data.files[f].path.clone(),
                    NodeRef::Hunk(f, h) => {
                        format!(
                            "{} {}",
                            data.files[f].path,
                            picture::hunk_label(&data.files[f].hunks[h])
                        )
                    }
                    NodeRef::Test(x) => format!(
                        "{}: the hunks it ran",
                        tests.get(x).map_or("", String::as_str)
                    ),
                    NodeRef::Lane(x) => format!("Test lane: {}", data.lane[x].name),
                };
                ui.label(RichText::new(title).strong());
                if let NodeRef::Hunk(f, h) = node
                    && ui.button("Open the diff").clicked()
                {
                    acts.extend(open_hunk(data, f, h));
                }
                if ui.small_button("Clear").on_hover_text("Esc").clicked() {
                    acts.push(act(Act::Select(None)));
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("coverage-details")
                .max_height(130.0)
                .auto_shrink([false, true])
                .show(ui, |ui| match node {
                    NodeRef::Hunk(f, h) => {
                        for l in &data.files[f].hunks[h].lines {
                            let said = match (l.cov, l.hits) {
                                (LineCov::Covered(n), _) => format!("covered, ran {n} times"),
                                (LineCov::Unknown, Some(n)) => {
                                    format!("unknown (the unmatched report says {n} hits)")
                                }
                                (cov, _) => cov.label().to_owned(),
                            };
                            ui.label(format!("line {}: {said}", l.line));
                        }
                    }
                    NodeRef::Test(x) => {
                        for (fi, f) in data.files.iter().enumerate() {
                            for (hi, h) in f.hunks.iter().enumerate() {
                                if h.tests.contains(&x)
                                    && common::table_row(
                                        ui,
                                        &[],
                                        &[format!("{} {}", f.path, picture::hunk_label(h))],
                                        false,
                                        false,
                                    )
                                    .clicked()
                                {
                                    acts.extend(open_hunk(data, fi, hi));
                                }
                            }
                        }
                    }
                    NodeRef::File(f) => {
                        for h in &data.files[f].hunks {
                            ui.label(format!(
                                "{}: {}",
                                picture::hunk_label(h),
                                picture::counts_text(h.counts())
                            ));
                        }
                    }
                    NodeRef::Lane(x) => {
                        ui.label(format!(
                            "{} at {}",
                            data.lane[x].verdict.word(),
                            short(&data.sha)
                        ));
                    }
                });
        });
    ui.add_space(4.0);
}
