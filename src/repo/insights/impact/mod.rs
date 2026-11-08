//! Change impact graph (#175 item 1): what refers to the symbols a change touched, at the base
//! and head revisions compared. The change is the selected commit against its parent, the
//! checked-out branch since it left the default branch, or a pull request. Its Rust items are
//! observed; what refers to them is a name match (inferred); files in other languages are
//! unknown. Clicking a node opens the definition or reference at the revision it was read at,
//! never the working tree; clicking an edge lists the lines behind it.

mod picture;
mod tables;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{RichText, Sense, Stroke, Ui, vec2};

use self::picture::{Filter, Laid, NodeRef, Scope};
use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use super::graph::{self, Pick};
use crate::insights::impact::{self as core, Ask, Impact, Reference};
use crate::repo::{Action, DiffKey, RepoTab, WIP_HASH, short};
use crate::theme::Tokens;

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// What the graph is about, as picked; the selected commit (or else the branch) when
    /// `None`.
    ask: Option<Ask>,
    data: Slot<Arc<Impact>>,
    layout: Slot<Arc<Laid>>,
    filter: Filter,
    /// Laid out by force rather than in columns.
    force: bool,
    selected: Option<Pick>,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Read the impact of another change.
    Ask(Ask),
    Filter(Filter),
    Force(bool),
    Select(Option<Pick>),
}

fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Impact(a))
}

/// Opens a reference's line at the revision it was read at.
fn open_ref(r: &Reference) -> Action {
    Action::BlameAt {
        path: r.path.clone(),
        rev: Some(r.rev.clone()),
        line: r.line as usize,
    }
}

/// What clicking a node opens: a symbol's definition or a site at the revision it was read
/// at, or the diff of a file no analyzer reads.
fn open_node(data: &Impact, node: NodeRef) -> Option<Action> {
    match node {
        NodeRef::Symbol(i) => {
            let span = data.symbols.get(i)?.at()?;
            Some(Action::BlameAt {
                path: span.path.clone(),
                rev: Some(span.rev.clone()),
                line: span.line as usize,
            })
        }
        NodeRef::Site(i) => {
            let site = data.sites.get(i)?;
            Some(Action::BlameAt {
                path: site.path.clone(),
                rev: Some(site.rev.clone()),
                line: site.line as usize,
            })
        }
        NodeRef::Other(i) => Some(file_diff(data, &data.other.get(i)?.path)),
    }
}

/// The change's diff of `path`, from its base to its head.
fn file_diff(data: &Impact, path: &str) -> Action {
    Action::OpenDiff(DiffKey::Range {
        from: data.range.base.clone(),
        to: Some(data.range.head.clone()),
        path: path.to_owned(),
    })
}

impl RepoTab {
    /// What the graph is about unless something else is picked: the pull request the page was
    /// opened over, else the selected commit, else the checked-out branch.
    fn impact_default_ask(&self) -> Ask {
        let opened_over = self.insights.as_ref().and_then(|v| v.pr);
        if let Some(pr) = opened_over.and_then(|n| self.prs.iter().find(|p| p.number == n)) {
            return Ask::Pr {
                number: pr.number,
                head: pr.sha.clone(),
                base: pr.base.clone(),
            };
        }
        match self.detail_hash.as_deref() {
            Some(hash) if hash != WIP_HASH => Ask::Commit(hash.to_owned()),
            _ => Ask::Branch {
                base: self.base_branch_name(),
            },
        }
    }

    /// The changes the graph can be about, for its menu.
    fn impact_subjects(&self) -> Vec<(String, Ask)> {
        let mut list = Vec::new();
        if let Some(hash) = self.detail_hash.as_deref().filter(|h| *h != WIP_HASH) {
            list.push((
                format!("Selected commit {} against its parent", short(hash)),
                Ask::Commit(hash.to_owned()),
            ));
        }
        let base = self.base_branch_name();
        list.push((
            format!(
                "Checked-out branch since it left {}",
                base.as_deref().unwrap_or("the default branch")
            ),
            Ask::Branch { base },
        ));
        for pr in self.prs.iter().take(30) {
            list.push((
                format!("#{} {}", pr.number, graph::elide(&pr.title, 48)),
                Ask::Pr {
                    number: pr.number,
                    head: pr.sha.clone(),
                    base: pr.base.clone(),
                },
            ));
        }
        list
    }

    /// Why what is shown is stale: the branch or pull request it was read from points
    /// elsewhere now.
    fn impact_stale(&self) -> Option<String> {
        let data = self.insights.as_ref()?.impact.data.ready()?;
        let (name, sha) = data.range.tip.as_ref()?;
        let now = if let Some(number) = name.strip_prefix('#') {
            let number: u64 = number.parse().ok()?;
            self.prs.iter().find(|p| p.number == number)?.sha.clone()
        } else if name == "HEAD" {
            self.refs.head.clone()
        } else {
            self.refs
                .local
                .iter()
                .find(|b| &b.name == name)?
                .target
                .clone()
        };
        (!now.is_empty() && &now != sha).then(|| format!("{name} is at {} now", short(&now)))
    }

    /// Reads what the view shows (again).
    pub(super) fn impact_load(&mut self, ctx: &egui::Context) {
        let fallback = self.impact_default_ask();
        let path = self.path.clone();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.impact;
        let ask = state.ask.clone().unwrap_or(fallback);
        let request = state.data.start();
        state.selected = None;
        self.insight_then(
            ctx,
            move || {
                core::resolve(&path, &ask)
                    .and_then(|range| core::analyze(&path, &range))
                    .map(Arc::new)
            },
            move |tab, result, ctx| {
                let Some(view) = tab.insights.as_mut() else {
                    return;
                };
                if view.impact.data.accept(request, result) {
                    tab.impact_layout(ctx);
                }
            },
        );
    }

    /// Builds the graph of what was read, as the filters say, in the background.
    fn impact_layout(&mut self, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.impact;
        let Some(data) = state.data.ready().cloned() else {
            return;
        };
        let (filter, force) = (state.filter.clone(), state.force);
        let request = state.layout.start();
        self.insight_then(
            ctx,
            move || -> Result<Arc<Laid>, String> {
                Ok(Arc::new(picture::build(&data, &filter, force, &t)))
            },
            move |tab, laid, _| {
                if let Some(view) = tab.insights.as_mut() {
                    view.impact.layout.accept(request, laid);
                }
            },
        );
    }

    pub(super) fn impact_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.impact;
        match act {
            Act::Ask(ask) => {
                state.ask = Some(ask);
                state.filter = Filter::default();
                self.impact_load(ctx);
            }
            Act::Filter(filter) => {
                state.filter = filter;
                state.selected = None;
                self.impact_layout(ctx);
            }
            Act::Force(force) => {
                state.force = force;
                state.selected = None;
                self.impact_layout(ctx);
            }
            Act::Select(pick) => state.selected = pick,
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn impact_back(&mut self) -> bool {
        let Some(view) = self.insights.as_mut() else {
            return false;
        };
        view.impact.selected.take().is_some()
    }

    pub(super) fn impact_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let subjects = self.impact_subjects();
        let stale = self.impact_stale();
        let default_ask = self.impact_default_ask();
        let Some(view) = self.insights.as_deref() else {
            return;
        };
        let (state, table) = (&view.impact, view.table);
        let asked = state.ask.clone().unwrap_or(default_ask);
        controls(ui, state, &subjects, &asked, acts);
        match common::slot_state(ui, &state.data, "the change and what refers to it") {
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
        let mut p = data.provenance();
        p.stale = stale;
        common::provenance(ui, &p);
        legend(ui);
        notes(ui, data);
        if data.symbols.is_empty() && data.other.is_empty() {
            common::empty(ui, "The change touches no files.");
            return;
        }
        if data.symbols.is_empty() {
            common::unavailable(
                ui,
                "No Rust items changed",
                "Only Rust is analyzed so far; the files below are shown as unknown.",
            );
        }
        if table {
            tables::show(ui, data, &state.filter, acts);
            return;
        }
        let Some(laid) = state.layout.ready() else {
            common::loading(ui, "the graph");
            return;
        };
        if let Some(pick) = state.selected {
            details(ui, data, laid, pick, &state.filter, acts);
        }
        if laid.hidden > 0 {
            let t = Tokens::get(ui.ctx());
            ui.label(
                RichText::new(format!(
                    "{} less referred-to sites are left out of the picture; the table lists \
                     them",
                    laid.hidden
                ))
                .size(11.5)
                .color(t.text_faint),
            );
        }
        let picked = graph::show(
            ui,
            "impact",
            &laid.graph,
            &laid.positions,
            state.selected,
            320.0,
        );
        if let Some(pick) = picked.clicked {
            acts.push(act(Act::Select(Some(pick))));
            if let Pick::Node(i) = pick
                && let Some(&node) = laid.nodes.get(i)
            {
                acts.extend(open_node(data, node));
            }
        }
    }
}

/// The change picked, the filters and the layout.
fn controls(
    ui: &mut Ui,
    state: &State,
    subjects: &[(String, Ask)],
    asked: &Ask,
    acts: &mut Vec<Action>,
) {
    ui.horizontal_wrapped(|ui| {
        let shown = state
            .data
            .ready()
            .map(|d| d.range.label.clone())
            .or_else(|| {
                subjects
                    .iter()
                    .find(|(_, a)| a == asked)
                    .map(|(l, _)| l.clone())
            })
            .unwrap_or_else(|| "The change".to_owned());
        egui::ComboBox::from_id_salt("impact-subject")
            .selected_text(graph::elide(&shown, 56))
            .width(300.0)
            .show_ui(ui, |ui| {
                for (label, ask) in subjects {
                    if ui.selectable_label(ask == asked, label).clicked() && ask != asked {
                        acts.push(act(Act::Ask(ask.clone())));
                    }
                }
            });
        let mut filter = state.filter.clone();
        if ui
            .checkbox(&mut filter.public_only, "Public API only")
            .on_hover_text("Only items declared `pub` (not `pub(crate)` or narrower)")
            .changed()
        {
            acts.push(act(Act::Filter(filter.clone())));
        }
        egui::ComboBox::from_id_salt("impact-scope")
            .selected_text(state.filter.scope.label())
            .show_ui(ui, |ui| {
                for scope in Scope::ALL {
                    if ui
                        .selectable_label(scope == state.filter.scope, scope.label())
                        .clicked()
                    {
                        acts.push(act(Act::Filter(Filter {
                            scope,
                            ..state.filter.clone()
                        })));
                    }
                }
            });
        if let Some(data) = state.data.ready()
            && !data.symbols.is_empty()
        {
            let symbol_label = |i: Option<usize>| match i.and_then(|i| data.symbols.get(i)) {
                Some(s) => graph::elide(&s.path, 40),
                None => "Every changed symbol".to_owned(),
            };
            egui::ComboBox::from_id_salt("impact-symbol")
                .selected_text(symbol_label(state.filter.symbol))
                .show_ui(ui, |ui| {
                    let choices = std::iter::once(None).chain((0..data.symbols.len()).map(Some));
                    for choice in choices {
                        if ui
                            .selectable_label(state.filter.symbol == choice, symbol_label(choice))
                            .clicked()
                        {
                            acts.push(act(Act::Filter(Filter {
                                symbol: choice,
                                ..state.filter.clone()
                            })));
                        }
                    }
                });
        }
        if ui
            .selectable_label(!state.force, "Columns")
            .on_hover_text("Changed symbols, the code referring to them, the tests")
            .clicked()
            && state.force
        {
            acts.push(act(Act::Force(false)));
        }
        if ui
            .selectable_label(state.force, "Force")
            .on_hover_text("Referring code pulled towards what it refers to")
            .clicked()
            && !state.force
        {
            acts.push(act(Act::Force(true)));
        }
    });
}

/// What the marks, lines and shapes mean.
fn legend(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    common::legend(
        ui,
        &[
            (
                Mark::Line(t.text_dim, false),
                "solid: in the diff (observed)",
            ),
            (
                Mark::Line(t.text_dim, true),
                "dashed: a name match (inferred; may be another item of that name)",
            ),
            (Mark::Dotted(t.text_dim), "dotted: unknown (no analyzer)"),
            (Mark::Glyph("+", t.success), "added"),
            (Mark::Glyph("-", t.danger), "removed"),
            (Mark::Glyph("~", t.accent), "modified"),
            (Mark::Glyph("T", t.text_dim), "test"),
        ],
    );
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (diamond, color, label) in [
            (false, t.text_dim, "code in a changed file"),
            (
                true,
                t.warning,
                "code in a file the change left alone (affected, unchanged)",
            ),
        ] {
            let (rect, _) = ui.allocate_exact_size(vec2(16.0, 12.0), Sense::hover());
            let c = rect.center();
            let stroke = Stroke::new(1.5, color);
            if diamond {
                let r = 5.5;
                let points = vec![
                    c + vec2(0.0, -r),
                    c + vec2(r, 0.0),
                    c + vec2(0.0, r),
                    c + vec2(-r, 0.0),
                ];
                ui.painter().add(egui::Shape::closed_line(points, stroke));
            } else {
                ui.painter().circle_stroke(c, 5.0, stroke);
            }
            ui.label(RichText::new(label).size(11.5).color(t.text_dim));
            ui.add_space(8.0);
        }
    });
}

/// What was left out or can't be known, in a few lines.
fn notes(ui: &mut Ui, data: &Impact) {
    let t = Tokens::get(ui.ctx());
    let mut lines = Vec::new();
    if !data.other.is_empty() {
        let mut why: Vec<&str> = data.other.iter().map(|o| o.why.as_str()).collect();
        why.sort();
        why.dedup();
        lines.push(format!(
            "{} changed file{} in other languages: what they affect is unknown ({})",
            data.other.len(),
            if data.other.len() == 1 { "" } else { "s" },
            why.join(", ")
        ));
    }
    let same: u32 = data.symbols.iter().map(|s| s.same_named).sum();
    if same > 0 {
        lines.push(format!(
            "{same} other definition{} share a changed symbol's name: some references may be \
             to them",
            if same == 1 { "" } else { "s" }
        ));
    }
    if data.in_comments > 0 {
        lines.push(format!(
            "{} name match{} in comments or strings left out",
            data.in_comments,
            if data.in_comments == 1 { "" } else { "es" }
        ));
    }
    let outside: u32 = data.rust_files.iter().map(|(_, n)| n).sum();
    if outside > 0 {
        lines.push(format!(
            "{outside} changed line{} of Rust files are in no item (imports, blank lines)",
            if outside == 1 { "" } else { "s" }
        ));
    }
    for line in lines {
        ui.add(egui::Label::new(RichText::new(line).size(11.5).color(t.text_faint)).wrap());
    }
}

/// What is selected in the picture and the lines behind it, each opening at its revision.
fn details(
    ui: &mut Ui,
    data: &Impact,
    laid: &Laid,
    pick: Pick,
    filter: &Filter,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let shown = filter.refs(data);
    let (title, refs, buttons): (String, Vec<usize>, Vec<(String, Action)>) = match pick {
        Pick::Edge(i) => {
            let Some(edge) = laid.edges.get(i) else {
                return;
            };
            (
                format!(
                    "{} refers to {} by name (inferred): the lines",
                    data.sites[edge.site].label, data.symbols[edge.symbol].name
                ),
                edge.refs.clone(),
                Vec::new(),
            )
        }
        Pick::Node(i) => match laid.nodes.get(i) {
            Some(&NodeRef::Symbol(s)) => {
                let symbol = &data.symbols[s];
                let mut buttons = Vec::new();
                for (side, span) in [("head", &symbol.head), ("base", &symbol.base)] {
                    if let Some(span) = span {
                        buttons.push((
                            format!("Definition at {side} {}", short(&span.rev)),
                            Action::BlameAt {
                                path: span.path.clone(),
                                rev: Some(span.rev.clone()),
                                line: span.line as usize,
                            },
                        ));
                    }
                }
                if let Some(span) = symbol.at() {
                    buttons.push(("Diff of the file".to_owned(), file_diff(data, &span.path)));
                }
                let mut title = format!(
                    "{} {} ({}): what refers to it by name",
                    symbol.kind.word(),
                    symbol.path,
                    symbol.change.word()
                );
                if symbol.refs_cut {
                    title.push_str(&format!(" (only the first {} kept)", core::MAX_REFS));
                }
                let refs = shown
                    .iter()
                    .copied()
                    .filter(|&r| data.refs[r].symbol == s)
                    .collect();
                (title, refs, buttons)
            }
            Some(&NodeRef::Site(s)) => (
                format!("{}: what it refers to", data.sites[s].label),
                shown
                    .iter()
                    .copied()
                    .filter(|&r| data.refs[r].site == s)
                    .collect(),
                Vec::new(),
            ),
            Some(&NodeRef::Other(o)) => {
                let other = &data.other[o];
                (
                    format!("{}: {}", other.path, other.why),
                    Vec::new(),
                    vec![("Diff of the file".to_owned(), file_diff(data, &other.path))],
                )
            }
            None => return,
        },
    };
    egui::Frame::new()
        .fill(t.card)
        .corner_radius(t.radius)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(title).strong());
                for (label, action) in buttons {
                    if ui.button(label).clicked() {
                        acts.push(action);
                    }
                }
                if ui.small_button("Clear").on_hover_text("Esc").clicked() {
                    acts.push(act(Act::Select(None)));
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("impact-details")
                .max_height(140.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for &r in &refs {
                        let reference = &data.refs[r];
                        let cells = [
                            format!(
                                "{}:{} @ {}",
                                reference.path,
                                reference.line,
                                short(&reference.rev)
                            ),
                            reference.text.clone(),
                        ];
                        if common::table_row(ui, &[300.0], &cells, false, false)
                            .on_hover_text("Open this line at that revision")
                            .clicked()
                        {
                            acts.push(open_ref(reference));
                        }
                    }
                });
        });
    ui.add_space(4.0);
}
