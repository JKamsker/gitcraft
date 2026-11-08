//! Code complexity and churn timeline (#175 item 12): size, churn and complexity of files and folders over the commits read, and rising hotspots.
//!
//! The history of `HEAD` or a branch is read in the background, sample commits spread over it,
//! and at each sample what the rows hold is read (`git ls-tree`, `git cat-file --batch`,
//! within a budget) along with the test lane's verdicts, in the background too. Each row draws
//! its size, churn and complexity as sparklines; a point opens the commits of its window.

mod chart;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{RichText, Ui};

use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use super::evolution::{add_partial, tips};
use crate::insights::Provenance;
use crate::insights::churn::{self as core, Options, Timeline, analyzer};
use crate::insights::evolution::{Axis, bucket, bucket_label};
use crate::insights::history::{self, History};
use crate::repo::file_view::FileTab;
use crate::repo::{Action, DiffKey, RepoTab, short};
use crate::theme::Tokens;

/// The history limits offered, and the one read first.
const LIMITS: [usize; 3] = [200, 500, 2000];
const DEFAULT_LIMIT: usize = 500;
/// The row limits offered.
const ROW_LIMITS: [usize; 3] = [12, 24, 48];
/// The deepest folders offered as rows.
const MAX_DEPTH: usize = 6;

/// A history read and where it came from.
pub(crate) struct Read {
    history: Arc<History>,
    provenance: Provenance,
}

/// A timeline measured, with the history it was planned from.
pub(crate) struct Built {
    history: Arc<History>,
    provenance: Provenance,
    options: Options,
    timeline: Timeline,
}

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The branch read; `None`: `HEAD`.
    rev: Option<String>,
    /// At most so many commits read (0: [`DEFAULT_LIMIT`]).
    limit: usize,
    options: Options,
    read: Slot<Read>,
    built: Slot<Arc<Built>>,
    /// The row and sample selected: the commits of that window are listed.
    selected: Option<(usize, usize)>,
    /// The sample the keyboard is on, in the row focused.
    cursor: usize,
    /// The filter was edited since the timeline was last asked for.
    filter_edited: bool,
}

impl State {
    fn limit(&self) -> usize {
        if self.limit == 0 {
            DEFAULT_LIMIT
        } else {
            self.limit
        }
    }
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Read this branch (`None`: `HEAD`).
    Rev(Option<String>),
    /// Read at most so many commits.
    Limit(usize),
    /// The options changed: plan and measure again.
    Rebuild,
}

impl RepoTab {
    fn churn_state(&mut self) -> Option<&mut State> {
        self.insights.as_deref_mut().map(|v| &mut v.churn)
    }

    /// Reads what the view shows (again).
    pub(super) fn churn_load(&mut self, ctx: &egui::Context) {
        let Some(state) = self.churn_state() else {
            return;
        };
        let request = state.read.start();
        let rev = state.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
        let limit = state.limit();
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                history::read(&path, &rev, limit).map(|h| {
                    let mut provenance = h.provenance();
                    provenance.source =
                        "git log --numstat · git ls-tree · git cat-file · test-lane notes"
                            .to_owned();
                    Read {
                        provenance,
                        history: Arc::new(h),
                    }
                })
            },
            move |tab, result, ctx| {
                let Some(state) = tab.churn_state() else {
                    return;
                };
                if state.read.accept(request, result) {
                    state.selected = None;
                    tab.churn_rebuild(ctx);
                }
            },
        );
    }

    /// Plans the samples of the history read and measures them, in the background.
    fn churn_rebuild(&mut self, ctx: &egui::Context) {
        let Some(state) = self.churn_state() else {
            return;
        };
        let Some(read) = state.read.ready() else {
            return;
        };
        let history = Arc::clone(&read.history);
        let provenance = read.provenance.clone();
        let options = state.options.clone();
        let request = state.built.start();
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let plan = core::plan(&history, &options);
                let measured = core::measure(&path, &plan)?;
                Ok(Arc::new(Built {
                    timeline: core::assemble(plan, measured),
                    history,
                    provenance,
                    options,
                }))
            },
            move |tab, result, _| {
                let Some(state) = tab.churn_state() else {
                    return;
                };
                if state.built.accept(request, result) {
                    state.selected = None;
                }
            },
        );
    }

    pub(super) fn churn_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(state) = self.churn_state() else {
            return;
        };
        match act {
            Act::Rev(rev) => {
                state.rev = rev;
                self.churn_load(ctx);
            }
            Act::Limit(limit) => {
                state.limit = limit;
                self.churn_load(ctx);
            }
            Act::Rebuild => self.churn_rebuild(ctx),
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn churn_back(&mut self) -> bool {
        self.churn_state()
            .is_some_and(|state| state.selected.take().is_some())
    }

    /// Selects sample `k` of row `row`: the commits of its window are listed.
    pub(super) fn churn_select(&mut self, row: usize, k: usize) -> bool {
        let Some(state) = self.churn_state() else {
            return false;
        };
        let Some(built) = state.built.ready() else {
            return false;
        };
        let ok = built
            .timeline
            .rows
            .get(row)
            .is_some_and(|r| k < r.points.len());
        if ok {
            state.selected = Some((row, k));
            state.cursor = k;
        }
        ok
    }

    pub(super) fn churn_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let branches: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let current = tips(&self.refs);
        let Some(view) = self.insights.as_deref_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.churn;
        controls(ui, state, &branches, acts);
        match common::slot_state(ui, &state.read, "the history") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        match common::slot_state(ui, &state.built, "sizes and complexity at the samples") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Churn(Act::Rebuild)));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(built) = state.built.ready().cloned() else {
            return;
        };
        let mut provenance = built.provenance.clone();
        if let Some(current) = &current {
            provenance.check_tips(|name| current(name));
        }
        add_partial(&mut provenance, built.timeline.notes.clone());
        common::provenance(ui, &provenance);
        explain(ui, &built);
        let t = &built.timeline;
        if t.rows.is_empty() {
            let text = if built.options.filter.trim().is_empty() {
                "No file changed in the commits read"
            } else {
                "No path changed in the commits read matches the filter"
            };
            common::empty(ui, text);
            return;
        }
        if let Some((r, k)) = state.selected
            && !window(ui, &built, r, k, acts)
        {
            state.selected = None;
        }
        let picked = if table {
            chart::table(ui, &built, state.selected)
        } else {
            chart::rows(ui, &built, state.selected, &mut state.cursor)
        };
        if let Some((r, k)) = picked {
            state.selected = Some((r, k));
            state.cursor = k;
        }
    }
}

/// The branch, the history limit, the samples, the rows and the filter.
fn controls(ui: &mut Ui, state: &mut State, branches: &[String], acts: &mut Vec<Action>) {
    let push =
        |acts: &mut Vec<Action>, act: Act| acts.push(Action::Insights(InsightsAct::Churn(act)));
    ui.horizontal_wrapped(|ui| {
        let shown = state.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
        egui::ComboBox::from_id_salt("insights-churn-rev")
            .selected_text(shown)
            .width(140.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.rev.is_none(), "HEAD (checked out)")
                    .clicked()
                    && state.rev.is_some()
                {
                    push(acts, Act::Rev(None));
                }
                for name in branches {
                    let on = state.rev.as_deref() == Some(name.as_str());
                    if ui.selectable_label(on, name).clicked() && !on {
                        push(acts, Act::Rev(Some(name.clone())));
                    }
                }
            })
            .response
            .on_hover_text("The branch whose history is read");
        let limit = state.limit();
        egui::ComboBox::from_id_salt("insights-churn-limit")
            .selected_text(format!("last {limit} commits"))
            .show_ui(ui, |ui| {
                for n in LIMITS {
                    if ui
                        .selectable_label(n == limit, format!("last {n} commits"))
                        .clicked()
                        && n != limit
                    {
                        push(acts, Act::Limit(n));
                    }
                }
            });
        let mut changed = false;
        let samples = state.options.samples;
        egui::ComboBox::from_id_salt("insights-churn-samples")
            .selected_text(format!("{samples} samples"))
            .show_ui(ui, |ui| {
                for n in core::SAMPLES {
                    if ui
                        .selectable_label(n == samples, format!("{n} samples"))
                        .clicked()
                    {
                        changed |= n != samples;
                        state.options.samples = n;
                    }
                }
            })
            .response
            .on_hover_text("Commits spread evenly over the history read, the tip last");
        ui.separator();
        let group = |g: Option<usize>| match g {
            None => "Files".to_owned(),
            Some(d) => format!("Folders, {d} deep"),
        };
        egui::ComboBox::from_id_salt("insights-churn-group")
            .selected_text(group(state.options.group))
            .show_ui(ui, |ui| {
                for g in std::iter::once(None).chain((1..=MAX_DEPTH).map(Some)) {
                    if ui
                        .selectable_label(state.options.group == g, group(g))
                        .clicked()
                    {
                        changed |= state.options.group != g;
                        state.options.group = g;
                    }
                }
            })
            .response
            .on_hover_text("Rows: files, or the folders (modules) they are in");
        let filter = ui.add(
            egui::TextEdit::singleline(&mut state.options.filter)
                .hint_text("Filter paths")
                .desired_width(150.0),
        );
        // Each change reads the samples again: only once typing stops (Enter or leaving).
        state.filter_edited |= filter.changed();
        if filter.lost_focus() && state.filter_edited {
            state.filter_edited = false;
            changed = true;
        }
        let rows = state.options.max_rows;
        egui::ComboBox::from_id_salt("insights-churn-rows")
            .selected_text(format!("{rows} rows at most"))
            .show_ui(ui, |ui| {
                for n in ROW_LIMITS {
                    if ui
                        .selectable_label(n == rows, format!("{n} rows at most"))
                        .clicked()
                    {
                        changed |= n != rows;
                        state.options.max_rows = n;
                    }
                }
            });
        if state.read.loading || state.built.loading {
            ui.add(crate::spinner::Spinner::new());
        }
        if changed {
            push(acts, Act::Rebuild);
        }
    });
}

/// What each measure is and how it is known, the hotspot rule, and the legend.
fn explain(ui: &mut Ui, built: &Built) {
    let t = Tokens::get(ui.ctx());
    let faint = |s: String| RichText::new(s).size(11.5).color(t.text_dim);
    ui.add(
        egui::Label::new(faint(format!(
            "Size: lines at the sample (factual) · Churn: lines changed in the window of commits \
             up to it (factual) · Complexity: branch points, {} — Rust and C-like files only, \
             others not measured · Tests: the test lane's verdict on the sample's tree, when \
             one was recorded",
            analyzer::LABEL
        )))
        .wrap(),
    );
    ui.add(
        egui::Label::new(faint(format!(
            "{}. Files over {} KiB aren't read, nor more than {} files or {} MiB in all: what \
             that leaves out is said above, and its points are lower bounds.",
            core::HOTSPOT_RULE,
            core::MAX_BLOB >> 10,
            core::MAX_BLOBS,
            core::MAX_BYTES >> 20
        )))
        .wrap(),
    );
    if let Some(e) = &built.timeline.tests_error {
        ui.label(
            RichText::new(format!(
                "Test results couldn't be read ({e}): shown as unknown"
            ))
            .size(11.5)
            .color(t.warning),
        );
    }
    common::legend(
        ui,
        &[
            (Mark::Glyph("!", t.warning), "hotspot"),
            (Mark::Fill(t.accent), "read in full"),
            (Mark::Hollow(t.accent), "lower bound: not all files read"),
            (Mark::Line(t.text_dim, true), "not there at that sample"),
            (Mark::Glyph("+", t.success), "tests passed"),
            (Mark::Glyph("x", t.danger), "tests failed"),
            (Mark::Glyph("?", t.text_faint), "no test result (unknown)"),
        ],
    );
}

/// The calendar day of `time`, UTC.
fn day(time: i64) -> String {
    bucket_label(bucket(time, Axis::Day), Axis::Day)
}

/// The commits of window `k` of row `r` that changed it: each selects its commit, each of its
/// paths opens that file's diff. `false` when it was closed (or is gone).
fn window(ui: &mut Ui, built: &Built, r: usize, k: usize, acts: &mut Vec<Action>) -> bool {
    let tl = &built.timeline;
    let (Some(row), Some(sample)) = (tl.rows.get(r), tl.samples.get(k)) else {
        return false;
    };
    let commits = row.commits_in(k);
    let window_len = tl.windows.get(k).map_or(0, Vec::len);
    let mut open = true;
    ui.horizontal_wrapped(|ui| {
        common::heading(
            ui,
            &format!(
                "{} · window {} of {} up to {} ({}): {} of its {} changed it",
                row.shown(),
                k + 1,
                tl.samples.len(),
                short(&sample.sample.hash),
                day(sample.sample.time),
                plural(commits.len(), "commit"),
                window_len,
            ),
        );
        if !row.folder
            && row.points.get(k).is_some_and(|p| p.size.is_some())
            && let Some(path) = row.paths.get(k).cloned()
            && ui
                .small_button("File at this sample")
                .on_hover_text("Open the file as it was at the sample commit")
                .clicked()
        {
            acts.push(Action::FileView {
                path,
                rev: Some(sample.sample.hash.clone()),
                tab: FileTab::File,
            });
        }
        if ui.small_button("Close").on_hover_text("Esc").clicked() {
            open = false;
        }
    });
    if commits.is_empty() {
        common::empty(ui, "No commit of this window changed it");
        return open;
    }
    egui::ScrollArea::vertical()
        .id_salt("insights-churn-window")
        .max_height(150.0)
        .show(ui, |ui| {
            for (c, paths) in &commits {
                let Some(commit) = built.history.commits.get(*c) else {
                    continue;
                };
                let lines: u64 = commit
                    .files
                    .iter()
                    .filter(|f| paths.contains(&f.path))
                    .map(|f| u64::from(f.lines()))
                    .sum();
                let resp = common::table_row(
                    ui,
                    &[90.0, 90.0, 80.0],
                    &[
                        short(&commit.hash).to_owned(),
                        day(commit.time),
                        format!("{lines} lines"),
                        commit.subject.clone(),
                    ],
                    false,
                    false,
                )
                .on_hover_text("Select the commit");
                if resp.clicked() {
                    acts.push(Action::SelectHash(commit.hash.clone()));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(96.0);
                    for path in paths {
                        if ui
                            .small_button(path)
                            .on_hover_text("Open this file's diff in the commit")
                            .clicked()
                        {
                            acts.push(Action::OpenDiff(DiffKey::Commit {
                                hash: commit.hash.clone(),
                                path: path.clone(),
                            }));
                        }
                    }
                });
            }
        });
    open
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}
