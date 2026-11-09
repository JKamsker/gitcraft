//! Rebase before/after ribbon map (#175 item 10): ribbons from each commit of a branch's earlier
//! version to its rewritten one, as `git range-diff` pairs them and as the changed lines they
//! share suggest for squashes and splits. Clicking a ribbon shows how its patch changed;
//! clicking a commit selects it.

mod picture;
#[cfg(test)]
mod tests;

use egui::{RichText, Ui};

use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use crate::git::{self, Series};
use crate::insights::Evidence;
use crate::insights::ribbons::{self, Class, Ribbons, Source};
use crate::repo::versions::VersionsAct;
use crate::repo::{Action, RepoTab, diff, short};
use crate::theme::{self, Tokens};

/// How long the ribbons take to draw in, in seconds.
const DRAW_IN: f64 = 0.6;

/// What is picked in the picture or the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    /// A ribbon, by its place in [`Ribbons::ribbons`].
    Ribbon(usize),
    /// A commit of the old version, by its place.
    Old(usize),
    /// A commit of the new version.
    New(usize),
}

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The rewrites gitcraft kept, read here (the versions view reads them only once a
    /// repository is loaded).
    rewrites: Slot<Vec<git::Rewrite>>,
    /// What is compared.
    source: Option<Source>,
    data: Slot<Ribbons>,
    selected: Option<Pick>,
    /// The patch of what is selected, and what it is of.
    patch: Slot<diff::DiffData>,
    /// When the ribbons were first drawn (egui's clock), for drawing them in.
    shown_at: Option<f64>,
    /// No drawing in: the ribbons as they end.
    still: bool,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Compare this.
    Show(Source),
    Select(Pick),
    Deselect,
    /// Draw the ribbons in, or show them still.
    ToggleStill,
}

impl RepoTab {
    fn ribbons_state(&mut self) -> Option<&mut State> {
        self.insights.as_mut().map(|v| &mut v.ribbons)
    }

    /// Reads what the view shows (again): the rewrites kept, then the comparison shown (the
    /// first there is, the first time).
    pub(super) fn ribbons_load(&mut self, ctx: &egui::Context) {
        let Some(state) = self.ribbons_state() else {
            return;
        };
        let request = state.rewrites.start();
        let path = self.path.clone();
        self.insight_then(
            ctx,
            move || git::rewrites(&path),
            move |tab, list, ctx| {
                let Some(state) = tab.ribbons_state() else {
                    return;
                };
                if !state.rewrites.accept(request, list) {
                    return;
                }
                let shown = state.source.clone();
                let sources = tab.ribbons_sources();
                // A pull request is compared at its head now, which may have moved since.
                let shown = shown.map(|source| match source {
                    Source::Pr { number, .. } => sources
                        .iter()
                        .find(|s| matches!(s, Source::Pr { number: n, .. } if *n == number))
                        .cloned()
                        .unwrap_or(source),
                    other => other,
                });
                match shown.or_else(|| sources.into_iter().next()) {
                    Some(source) => tab.ribbons_read(source, ctx),
                    None => {
                        if let Some(state) = tab.ribbons_state() {
                            state.data = Slot::default();
                        }
                    }
                }
            },
        );
    }

    /// Everything there is to compare now.
    fn ribbons_sources(&self) -> Vec<Source> {
        let rewrites = self
            .insights
            .as_ref()
            .and_then(|v| v.ribbons.rewrites.ready())
            .unwrap_or(&self.versions.rewrites);
        let branches: Vec<ribbons::Tracking> = self
            .refs
            .local
            .iter()
            .map(|b| (b.name.clone(), b.upstream.clone(), b.ahead_behind()))
            .collect();
        let prs: Vec<ribbons::PrSeen> = self
            .prs
            .iter()
            .map(|p| {
                let seen = self.versions.seen_heads.get(&p.number).cloned();
                (
                    p.number,
                    p.title.clone(),
                    p.sha.clone(),
                    p.base.clone(),
                    seen,
                )
            })
            .collect();
        ribbons::sources(rewrites, &branches, self.refs.current.as_deref(), &prs)
    }

    /// Compares what `source` names, in the background.
    fn ribbons_read(&mut self, source: Source, ctx: &egui::Context) {
        let key = self
            .hosting
            .as_ref()
            .and_then(|h| h.review_repository_key());
        let remotes: Vec<String> = self.remotes.iter().map(|r| r.name.clone()).collect();
        let path = self.path.clone();
        let Some(state) = self.ribbons_state() else {
            return;
        };
        let request = state.data.start();
        state.source = Some(source.clone());
        state.selected = None;
        state.patch = Slot::default();
        state.shown_at = None;
        self.insight_then(
            ctx,
            move || compare(&path, &source, key.as_deref(), &remotes),
            move |tab, result, _| {
                if let Some(state) = tab.ribbons_state() {
                    state.data.accept(request, result);
                }
            },
        );
    }

    pub(super) fn ribbons_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Show(source) => self.ribbons_read(source, ctx),
            Act::Select(pick) => self.ribbons_select(pick, ctx),
            Act::Deselect => {
                if let Some(state) = self.ribbons_state() {
                    state.selected = None;
                    state.patch = Slot::default();
                }
            }
            Act::ToggleStill => {
                if let Some(state) = self.ribbons_state() {
                    state.still = !state.still;
                    state.shown_at = None;
                }
            }
        }
    }

    /// Shows the patch of what `pick` is: how a ribbon's patch changed, or a commit's own
    /// patch. Picking what is picked already lets go of it.
    fn ribbons_select(&mut self, pick: Pick, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(state) = self.ribbons_state() else {
            return;
        };
        if state.selected == Some(pick) {
            state.selected = None;
            state.patch = Slot::default();
            return;
        }
        let Some(data) = state.data.ready() else {
            return;
        };
        enum Read {
            Now(Vec<String>),
            Alone(String, String),
            Lone(String, char),
        }
        let read = match pick {
            Pick::Ribbon(i) => {
                let Some(r) = data.ribbons.get(i) else {
                    return;
                };
                match r.pair.and_then(|p| data.diff.pairs.get(p)) {
                    Some(pair) => Read::Now(pair.body.clone()),
                    None => Read::Alone(
                        data.old[r.old].commit.hash.clone(),
                        data.new[r.new].commit.hash.clone(),
                    ),
                }
            }
            Pick::Old(i) => match data.old.get(i) {
                Some(end) => Read::Lone(end.commit.hash.clone(), '-'),
                None => return,
            },
            Pick::New(i) => match data.new.get(i) {
                Some(end) => Read::Lone(end.commit.hash.clone(), '+'),
                None => return,
            },
        };
        state.selected = Some(pick);
        let request = state.patch.start();
        if let Read::Now(body) = read {
            state.patch.accept(request, Ok(diff::interdiff_data(&body)));
            return;
        }
        self.insight_then(
            ctx,
            move || {
                let body = match read {
                    Read::Alone(old, new) => ribbons::alone_patch(&path, &old, &new),
                    Read::Lone(hash, mark) => git::lone_patch(&path, &hash, mark),
                    Read::Now(body) => Ok(body),
                };
                body.map(|b| diff::interdiff_data(&b))
            },
            move |tab, result, _| {
                if let Some(state) = tab.ribbons_state() {
                    state.patch.accept(request, result);
                }
            },
        );
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn ribbons_back(&mut self) -> bool {
        match self.ribbons_state() {
            Some(state) if state.selected.is_some() => {
                state.selected = None;
                state.patch = Slot::default();
                true
            }
            _ => false,
        }
    }

    /// Where the tip named `name` of what `source` compares is now (`None`: gone), for
    /// telling the comparison stale.
    fn ribbons_tip_now(&self, source: &Source, name: &str, read: &str) -> Option<String> {
        match source {
            // Kept in refs of their own that nothing moves.
            Source::Rewrite(_) => Some(read.to_owned()),
            Source::Upstream { branch, upstream } if name == upstream => self
                .refs
                .remotes
                .iter()
                .flat_map(|(remote, list)| list.iter().map(move |b| (remote, b)))
                .find(|(remote, b)| {
                    b.name == *upstream || format!("{remote}/{}", b.name) == *upstream
                })
                .map(|(_, b)| b.target.clone())
                .or_else(|| (branch == upstream).then(|| read.to_owned())),
            Source::Upstream { branch, .. } => self
                .refs
                .local
                .iter()
                .find(|b| b.name == *branch)
                .map(|b| b.target.clone()),
            Source::Pr { number, .. } if name == "seen" => {
                self.versions.seen_heads.get(number).cloned()
            }
            Source::Pr { number, .. } => self
                .prs
                .iter()
                .find(|p| p.number == *number)
                .map(|p| p.sha.clone()),
        }
    }

    pub(super) fn ribbons_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let sources = self.ribbons_sources();
        let table = self.insights.as_ref().is_some_and(|v| v.table);
        // Stale once a tip moved since it was read.
        let check = self.insights.as_ref().and_then(|v| {
            let state = &v.ribbons;
            let source = state.source.clone()?;
            let tips = state.data.ready()?.provenance.tips.clone();
            Some((source, tips))
        });
        if let Some((source, tips)) = check {
            let now: Vec<(String, Option<String>)> = tips
                .iter()
                .map(|(name, sha)| (name.clone(), self.ribbons_tip_now(&source, name, sha)))
                .collect();
            if let Some(data) = self.ribbons_state().and_then(|s| s.data.ready_mut()) {
                data.provenance.check_tips(|name| {
                    now.iter()
                        .find(|(n, _)| n == name)
                        .and_then(|(_, sha)| sha.clone())
                });
            }
        }
        let reduced = crate::a11y::motion::reduced(ui.ctx());
        let now = ui.input(|i| i.time);
        let Some(state) = self.ribbons_state() else {
            return;
        };
        bar(ui, state, &sources, reduced, acts);
        if state.rewrites.loading && state.data.value.is_none() && !state.data.loading {
            common::loading(ui, "the rewrites kept");
            return;
        }
        let Some(source) = state.source.clone() else {
            match state.rewrites.error() {
                Some(e) if sources.is_empty() => {
                    if common::failed(ui, "the rewrites kept", e) {
                        acts.push(Action::Insights(InsightsAct::Reload));
                    }
                }
                _ if !sources.is_empty() => {
                    common::empty(ui, "Pick what to compare in the menu above.");
                }
                _ => common::unavailable(
                    ui,
                    "Nothing to compare",
                    "No rewrite was kept (gitcraft keeps a branch's commits before and after a \
                     rebase, an amend or a quick edit it makes), no branch has diverged from its \
                     upstream, and no pull request changed since you last looked at it.",
                ),
            }
            return;
        };
        match common::slot_state(ui, &state.data, "the two versions") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Ribbons(Act::Show(source))));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(data) = state.data.ready() else {
            return;
        };
        common::provenance(ui, &data.provenance);
        legend(ui, source.sides());
        if data.old.is_empty() && data.new.is_empty() {
            common::empty(
                ui,
                "Neither version has commits of its own: nothing was rewritten.",
            );
            return;
        }
        let progress = if reduced || state.still {
            1.0
        } else {
            let start = *state.shown_at.get_or_insert(now);
            let x = ((now - start) / DRAW_IN).clamp(0.0, 1.0) as f32;
            if x < 1.0 {
                ui.ctx().request_repaint();
            }
            // Eased out: quick at first, settling at the end.
            1.0 - (1.0 - x).powi(3)
        };
        let selected = state.selected;
        let list_h = if selected.is_some() {
            (ui.available_height() * 0.45).max(120.0)
        } else {
            ui.available_height()
        };
        let picked = if table {
            table_ui(ui, data, selected, source.sides(), list_h)
        } else {
            picture::show(ui, data, selected, progress, source.sides(), list_h)
        };
        if let Some(pick) = picked.pick {
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::Select(pick))));
        }
        if let Some(hash) = picked.commit {
            acts.push(Action::SelectHash(hash));
        }
        if let Some(pick) = selected {
            ui.separator();
            detail(ui, data, pick, &state.patch, &source, acts);
        }
    }
}

/// Reads the comparison `source` names. Nothing is fetched: a pull request's head must be
/// here already.
fn compare(
    repo: &std::path::Path,
    source: &Source,
    key: Option<&str>,
    remotes: &[String],
) -> Result<Ribbons, String> {
    match source {
        Source::Rewrite(r) => {
            let scope = format!(
                "{}: {} → {}",
                source.label(),
                short(&r.old.tip),
                short(&r.new.tip)
            );
            ribbons::read(repo, &r.old, &r.new, ("before", "after"), &scope)
        }
        Source::Upstream { branch, upstream } => {
            let up = git::resolve(repo, &crate::repo::branch_visibility::remote_ref(upstream))?;
            let mine = git::resolve(repo, &crate::repo::branch_visibility::local_ref(branch))?;
            let base = git::merge_base(repo, &up, &mine)?;
            let scope = format!("{upstream} (left) and {branch} (right), from where they forked");
            ribbons::read(
                repo,
                &Series::new(&base, &up),
                &Series::new(&base, &mine),
                (upstream, branch),
                &scope,
            )
        }
        Source::Pr {
            number, head, base, ..
        } => {
            let key = key.ok_or("no primary forge is selected: no pull request was seen")?;
            let base_refs: Vec<String> = remotes
                .iter()
                .map(|r| format!("refs/remotes/{r}/{base}"))
                .collect();
            let (old, new) = ribbons::pr_series(repo, key, *number, head, &base_refs)?;
            let scope = format!("#{number} as last seen in gitcraft and now");
            let now = format!("#{number} head");
            ribbons::read(repo, &old, &new, ("seen", &now), &scope)
        }
    }
}

/// The line above the picture: what is compared (a menu of what can be), drawing in or still.
fn bar(ui: &mut Ui, state: &State, sources: &[Source], reduced: bool, acts: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        let shown = state
            .source
            .as_ref()
            .map_or_else(|| "Nothing to compare".to_owned(), Source::label);
        egui::ComboBox::from_id_salt("ribbons-source")
            .selected_text(shown)
            .width(360.0)
            .show_ui(ui, |ui| {
                for source in sources {
                    let on = state.source.as_ref() == Some(source);
                    if ui.selectable_label(on, source.label()).clicked() && !on {
                        acts.push(Action::Insights(InsightsAct::Ribbons(Act::Show(
                            source.clone(),
                        ))));
                    }
                }
                if sources.is_empty() {
                    ui.label("Nothing to compare");
                }
            });
        let mut still = state.still || reduced;
        let toggle = ui.add_enabled(!reduced, egui::Checkbox::new(&mut still, "Static"));
        let toggle = if reduced {
            toggle.on_disabled_hover_text("The system asks for reduced motion: always static")
        } else {
            toggle.on_hover_text("Show the ribbons without drawing them in")
        };
        if toggle.changed() {
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::ToggleStill)));
        }
        if let Some(data) = state.data.ready() {
            let counts: Vec<String> = data
                .counts()
                .iter()
                .map(|(c, n)| format!("{n} {}", c.label()))
                .collect();
            let t = Tokens::get(ui.ctx());
            ui.label(RichText::new(counts.join(", ")).color(t.text_dim));
        }
    });
}

/// The colour a class is drawn in; its letter says the same.
pub(super) fn color(class: Class, t: &Tokens) -> egui::Color32 {
    match class {
        Class::Unchanged => t.text_faint,
        Class::Reordered => t.accent,
        Class::Reworded => t.lanes[4],
        Class::Modified => t.warning,
        Class::Squashed => t.lanes[6],
        Class::Split => t.lanes[8],
        Class::Dropped => t.danger,
        Class::Added => t.success,
    }
}

/// What a class is called where `sides` (two branches, neither older) makes added and dropped
/// "only on the right / left".
pub(super) fn label(class: Class, sides: bool) -> &'static str {
    match class {
        Class::Dropped if sides => "only on the left",
        Class::Added if sides => "only on the right",
        _ => class.label(),
    }
}

fn legend(ui: &mut Ui, sides: bool) {
    let t = Tokens::get(ui.ctx());
    let entries: Vec<(Mark, &str)> = Class::ALL
        .iter()
        .map(|&c| (Mark::Glyph(c.glyph(), color(c, &t)), label(c, sides)))
        .chain([
            (
                common::evidence_line(Evidence::Observed, t.text_dim),
                "exact: the same patch-id",
            ),
            (
                common::evidence_line(Evidence::Inferred, t.text_dim),
                "inferred: similar, or sharing changed lines",
            ),
        ])
        .collect();
    common::legend(ui, &entries);
}

/// What was clicked: something to show, a commit to select.
#[derive(Default)]
pub(super) struct Picked {
    pub(super) pick: Option<Pick>,
    pub(super) commit: Option<String>,
}

/// A commit as a row names it.
fn named(c: &git::SeriesCommit) -> String {
    let empty = if c.empty { " (changes nothing)" } else { "" };
    format!("{}. {} {}{empty}", c.index, short(&c.hash), c.summary)
}

/// The ribbons and the commits without one, as a table.
fn table_ui(ui: &mut Ui, data: &Ribbons, selected: Option<Pick>, sides: bool, h: f32) -> Picked {
    let mut picked = Picked::default();
    let widths = [100.0, 80.0, 240.0, 240.0];
    let head = ["What", "Evidence", "Old", "New", "Why"].map(str::to_owned);
    common::table_row(ui, &widths, &head, true, false);
    let mut rows: Vec<(Pick, [String; 5])> = Vec::new();
    let mut order: Vec<usize> = (0..data.ribbons.len()).collect();
    order.sort_by_key(|&i| (data.ribbons[i].new, data.ribbons[i].old));
    for i in order {
        let r = &data.ribbons[i];
        let moved = if r.moved { ", moved" } else { "" };
        rows.push((
            Pick::Ribbon(i),
            [
                format!("{}{moved}", label(r.class, sides)),
                r.evidence.label().to_owned(),
                named(&data.old[r.old].commit),
                named(&data.new[r.new].commit),
                r.why.clone(),
            ],
        ));
    }
    let unlinked = |old: bool, at: usize| data.ribbons_of(old, at).next().is_none();
    for (i, end) in data
        .old
        .iter()
        .enumerate()
        .filter(|(i, _)| unlinked(true, *i))
    {
        rows.push((
            Pick::Old(i),
            [
                label(end.class, sides).to_owned(),
                Evidence::Observed.label().to_owned(),
                named(&end.commit),
                "—".to_owned(),
                "range-diff paired it with nothing, nor do its changed lines match".to_owned(),
            ],
        ));
    }
    for (i, end) in data
        .new
        .iter()
        .enumerate()
        .filter(|(i, _)| unlinked(false, *i))
    {
        rows.push((
            Pick::New(i),
            [
                label(end.class, sides).to_owned(),
                Evidence::Observed.label().to_owned(),
                "—".to_owned(),
                named(&end.commit),
                "range-diff paired it with nothing, nor do its changed lines match".to_owned(),
            ],
        ));
    }
    egui::ScrollArea::vertical()
        .id_salt("ribbons-table")
        .max_height(h)
        .auto_shrink([false, true])
        .show_rows(ui, 22.0, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (pick, cells) in &rows[range] {
                let on = selected == Some(*pick);
                if common::table_row(ui, &widths, cells, false, on).clicked() {
                    picked.pick = Some(*pick);
                }
            }
        });
    picked
}

/// What is selected: what it is, where to go from it, and its patch.
fn detail(
    ui: &mut Ui,
    data: &Ribbons,
    pick: Pick,
    patch: &Slot<diff::DiffData>,
    source: &Source,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let sides = source.sides();
    let (title, line, commits, what) = match pick {
        Pick::Ribbon(i) => {
            let Some(r) = data.ribbons.get(i) else {
                return;
            };
            let (old, new) = (&data.old[r.old].commit, &data.new[r.new].commit);
            let moved = if r.moved { ", in another place" } else { "" };
            (
                format!("{} → {}", named(old), named(new)),
                format!(
                    "{}{moved}: {} · {}: {}",
                    label(r.class, sides),
                    r.class.meaning(),
                    r.evidence.label(),
                    r.why
                ),
                vec![
                    ("Old commit", old.hash.clone()),
                    ("New commit", new.hash.clone()),
                ],
                if r.pair.is_some() {
                    "How its patch changed, as range-diff shows it"
                } else {
                    "How the two patches differ (range-diff didn't pair them)"
                },
            )
        }
        Pick::Old(i) | Pick::New(i) => {
            let old = matches!(pick, Pick::Old(_));
            let list = if old { &data.old } else { &data.new };
            let Some(end) = list.get(i) else {
                return;
            };
            (
                named(&end.commit),
                format!("{}: {}", label(end.class, sides), end.class.meaning()),
                vec![("Select the commit", end.commit.hash.clone())],
                if old {
                    "Its patch, in the old version"
                } else {
                    "Its patch, in the new version"
                },
            )
        }
    };
    ui.label(RichText::new(title).font(theme::medium(12.5)).color(t.text));
    ui.add(egui::Label::new(RichText::new(line).color(t.text_dim)).wrap());
    ui.horizontal_wrapped(|ui| {
        for (label, hash) in commits {
            if ui.button(label).on_hover_text(hash.clone()).clicked() {
                acts.push(Action::SelectHash(hash));
            }
        }
        let (act, tip) = match source {
            Source::Rewrite(r) => (
                VersionsAct::Rewrite(r.clone()),
                "Every pair in the comparison of versions".to_owned(),
            ),
            Source::Upstream { branch, upstream } => (
                VersionsAct::Branches {
                    old: crate::repo::branch_visibility::remote_ref(upstream),
                    old_label: upstream.clone(),
                    new: crate::repo::branch_visibility::local_ref(branch),
                    new_label: branch.clone(),
                },
                "Every pair in the comparison of versions".to_owned(),
            ),
            Source::Pr { number, .. } => (
                VersionsAct::PrSince(*number),
                format!(
                    "The comparison of versions, which then takes this version of #{number} as \
                     seen, as it always does"
                ),
            ),
        };
        if ui
            .button("Open in the versions view")
            .on_hover_text(tip)
            .clicked()
        {
            acts.push(Action::Versions(act));
        }
        if ui.button("Close").on_hover_text("Esc").clicked() {
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::Deselect)));
        }
    });
    ui.label(RichText::new(what).color(t.text_faint));
    match common::slot_state(ui, patch, "the patch") {
        SlotState::Retry => {
            // Picking it again would let go of it: let go first, then pick.
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::Deselect)));
            acts.push(Action::Insights(InsightsAct::Ribbons(Act::Select(pick))));
        }
        SlotState::Waiting => {}
        SlotState::Ready => {
            let Some(found) = patch.ready() else {
                return;
            };
            if found.lines.is_empty() {
                common::empty(
                    ui,
                    "Nothing changed: the same message and the same changes.",
                );
                return;
            }
            let h = ui.available_height().max(160.0);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            diff::interdiff_rows(&mut inner, found, ("ribbons-patch", format!("{pick:?}")));
        }
    }
}
