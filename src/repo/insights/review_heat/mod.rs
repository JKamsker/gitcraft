//! PR review heatmap (#175 item 7): a pull request's files, and their hunks once opened, by
//! what the version seen in gitcraft already showed, what changed since, where the open review
//! threads are, and what isn't known (the forge's "viewed" marks, coverage). A cell opens the
//! file's diff at the pull request's revisions, scrolled to its hunk or thread.

mod grid;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use egui::{RichText, Ui};

use super::common::{self, Mark, Slot, SlotState};
use super::{InsightsAct, Kind};
use crate::insights::review_heat::{self as heat, Heat, Seen, SeenMark};
use crate::repo::{Action, DiffKey, RepoTab, short};
use crate::theme::Tokens;

/// How many times a diff opened at a line is waited for, a frame or so apart, before the line
/// is given up on.
const JUMP_TRIES: u32 = 400;

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The pull request shown.
    number: Option<u64>,
    data: Slot<Heat>,
    /// The files whose hunks are shown, by place.
    expanded: HashSet<usize>,
    /// The cell last opened.
    selected: Option<(usize, usize)>,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Show the pull request numbered so (again).
    Pick(u64),
    /// Show or hide a file's hunks.
    Toggle(usize),
    /// Every file's hunks shown (or none).
    ExpandAll(bool),
    /// Open a diff, scrolled to a line (old side or not, its number) once it is read.
    Open {
        key: DiffKey,
        at: Option<(bool, u32)>,
        cell: Option<(usize, usize)>,
    },
}

impl RepoTab {
    fn review_heat_state(&mut self) -> Option<&mut State> {
        self.insights.as_mut().map(|v| &mut v.review_heat)
    }

    /// The pull request to show when none was picked: the one whose page is open, else the
    /// checked-out branch's.
    fn review_heat_default(&self) -> Option<u64> {
        self.pr_view.as_ref().map(|v| v.number).or_else(|| {
            let current = self.refs.current.as_deref()?;
            self.prs
                .iter()
                .find(|p| p.branch == current && !p.from_fork)
                .map(|p| p.number)
        })
    }

    /// Reads what the view shows (again).
    pub(super) fn review_heat_load(&mut self, ctx: &egui::Context) {
        let fallback = self.review_heat_default();
        let Some(state) = self.review_heat_state() else {
            return;
        };
        let Some(number) = state.number.or(fallback) else {
            return;
        };
        self.review_heat_read(number, ctx);
    }

    /// Reads pull request `number` from the forge, then what the repository says of it.
    fn review_heat_read(&mut self, number: u64, ctx: &egui::Context) {
        let hosting = self.hosting.clone();
        let key = hosting.as_ref().and_then(|h| h.review_repository_key());
        let path = self.path.clone();
        let Some(state) = self.review_heat_state() else {
            return;
        };
        if state.number != Some(number) {
            state.expanded.clear();
            state.selected = None;
        }
        state.number = Some(number);
        let Some(hosting) = hosting else {
            state.data = Slot::default();
            return;
        };
        let request = state.data.start();
        self.forge_then(
            ctx,
            move || {
                let pages = hosting.forge.provider().pages().ok_or_else(|| {
                    format!(
                        "gitcraft can't read {}s of this forge",
                        hosting.forge.noun()
                    )
                })?;
                let details = pages.details(&path, &hosting, number)?;
                heat::analyze(&path, &details, key.as_deref())
            },
            move |tab, result, _| {
                if let Some(state) = tab.review_heat_state() {
                    state.data.accept(request, result);
                }
            },
        );
    }

    pub(super) fn review_heat_act(&mut self, act: Act, ctx: &egui::Context) {
        match act {
            Act::Pick(number) => self.review_heat_read(number, ctx),
            Act::Toggle(file) => {
                if let Some(state) = self.review_heat_state()
                    && !state.expanded.remove(&file)
                {
                    state.expanded.insert(file);
                }
            }
            Act::ExpandAll(on) => {
                if let Some(state) = self.review_heat_state() {
                    let files = state.data.ready().map_or(0, |d| d.files.len());
                    state.expanded = if on {
                        (0..files).collect()
                    } else {
                        HashSet::new()
                    };
                }
            }
            Act::Open { key, at, cell } => {
                if let Some(state) = self.review_heat_state() {
                    state.selected = cell;
                }
                self.open_diff(key.clone(), ctx);
                if let Some(at) = at {
                    self.review_heat_jump(key, at, 0, ctx);
                }
            }
        }
    }

    /// Scrolls the diff of `key`, once it is read, to line `at` (old side or not, number).
    /// Until then it is looked at again in a moment.
    fn review_heat_jump(&mut self, key: DiffKey, at: (bool, u32), tries: u32, ctx: &egui::Context) {
        let Some(view) = self.diff.as_mut().filter(|d| d.key == key) else {
            return;
        };
        match &view.data {
            Some(Ok(data)) => {
                view.jump_line = line_index(&data.lines, at);
            }
            Some(Err(_)) => {}
            None if tries < JUMP_TRIES => {
                self.then(
                    ctx,
                    || std::thread::sleep(std::time::Duration::from_millis(15)),
                    move |tab, (), ctx| tab.review_heat_jump(key, at, tries + 1, ctx),
                );
            }
            None => {}
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn review_heat_back(&mut self) -> bool {
        match self.review_heat_state() {
            Some(state) if state.selected.is_some() => {
                state.selected = None;
                true
            }
            _ => false,
        }
    }

    pub(super) fn review_heat_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let table = self.insights.as_ref().is_some_and(|v| v.table);
        // Stale once the head or the version seen moved since it was read.
        let number = self.insights.as_ref().and_then(|v| v.review_heat.number);
        if let Some(number) = number {
            let head = self
                .prs
                .iter()
                .find(|p| p.number == number)
                .map(|p| p.sha.clone());
            let seen = self.versions.seen_heads.get(&number).cloned();
            if let Some(data) = self.review_heat_state().and_then(|s| s.data.ready_mut()) {
                let read = data.provenance.tips.clone();
                let was = |name: &str| read.iter().find(|(n, _)| n == name).map(|(_, s)| s.clone());
                data.provenance.check_tips(|name| {
                    if name == heat::head_name(number) {
                        head.clone().or_else(|| was(name))
                    } else {
                        seen.clone().or_else(|| was(name))
                    }
                });
            }
        }
        let prs: Vec<(u64, String)> = self
            .prs
            .iter()
            .map(|p| (p.number, p.title.clone()))
            .collect();
        let pr = number
            .and_then(|n| self.prs.iter().find(|p| p.number == n))
            .cloned();
        let hosting = self.hosting.clone();
        let Some(state) = self.insights.as_ref().map(|v| &v.review_heat) else {
            return;
        };
        bar(ui, state, &prs, acts);
        let Some(hosting) = hosting else {
            common::unavailable(
                ui,
                "No forge",
                "The review heatmap reads a pull request and its review threads from the \
                 repository's forge (GitHub, GitLab, Forgejo, Gitea…), and no primary forge is \
                 selected for this repository.",
            );
            return;
        };
        if hosting.forge.provider().pages().is_none() {
            common::unavailable(
                ui,
                "Not for this forge",
                &format!(
                    "gitcraft can't read the {}s of this forge, so it has nothing to show here.",
                    hosting.forge.noun()
                ),
            );
            return;
        }
        let Some(number) = state.number else {
            if prs.is_empty() {
                common::unavailable(
                    ui,
                    "No pull request",
                    "The list of this repository's pull requests is empty, or wasn't read yet.",
                );
            } else {
                common::empty(ui, "Pick a pull request above.");
            }
            return;
        };
        match common::slot_state(ui, &state.data, &format!("#{number}")) {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::Pick(number))));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(data) = state.data.ready() else {
            return;
        };
        common::provenance(ui, &data.provenance);
        seen_line(ui, data, pr.as_ref(), acts);
        legend(ui);
        if data.files.is_empty() {
            common::empty(ui, "The pull request changes no file.");
            return;
        }
        let rows = grid::rows(data, &state.expanded);
        let h = (ui.available_height() * 0.62).max(160.0);
        if table {
            grid::table(ui, data, &rows, h, acts);
        } else {
            grid::show(ui, data, &rows, state.selected, h, acts);
        }
        threads(ui, data, acts);
    }
}

/// The diff line (by its place) showing line `n` of the old side (`old`) or the new one; the
/// nearest when the diff doesn't show it.
fn line_index(lines: &[crate::repo::diff::DiffLine], (old, n): (bool, u32)) -> Option<usize> {
    use crate::repo::diff::Kind;
    let number = |l: &crate::repo::diff::DiffLine| {
        if old {
            l.old
        } else if l.kind == Kind::Del {
            None
        } else {
            l.new
        }
    };
    lines.iter().position(|l| number(l) == Some(n)).or_else(|| {
        lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| Some((number(l)?.abs_diff(n), i)))
            .min()
            .map(|(_, i)| i)
    })
}

/// The line above the grid: which pull request (a menu of the others), opening it, and every
/// file's hunks shown or hidden.
fn bar(ui: &mut Ui, state: &State, prs: &[(u64, String)], acts: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        let shown = state
            .number
            .map(|n| {
                prs.iter()
                    .find(|(m, _)| *m == n)
                    .map_or_else(|| format!("#{n}"), |(_, title)| format!("#{n} {title}"))
            })
            .unwrap_or_else(|| "Pick a pull request".to_owned());
        egui::ComboBox::from_id_salt("review-heat-pr")
            .selected_text(shown)
            .width(360.0)
            .show_ui(ui, |ui| {
                for (n, title) in prs {
                    let on = state.number == Some(*n);
                    if ui.selectable_label(on, format!("#{n} {title}")).clicked() && !on {
                        acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::Pick(*n))));
                    }
                }
                if prs.is_empty() {
                    ui.label("No pull requests listed");
                }
            });
        if let Some(n) = state.number {
            if ui
                .button(format!("Open #{n}"))
                .on_hover_text("Its page: description, checks, conversation")
                .clicked()
            {
                acts.push(Action::OpenPr(n));
            }
            if state.data.ready().is_some() {
                if ui
                    .button("Expand all")
                    .on_hover_text("Every file's hunks")
                    .clicked()
                {
                    acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::ExpandAll(
                        true,
                    ))));
                }
                if ui.button("Collapse all").clicked() {
                    acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::ExpandAll(
                        false,
                    ))));
                }
            }
        }
    });
}

/// What the version seen in gitcraft is, and when it is stale, why, with the way to compare.
fn seen_line(
    ui: &mut Ui,
    data: &Heat,
    pr: Option<&crate::forge::PullRequest>,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let text = match &data.seen {
        Some(Seen::Fresh { tip, .. }) if *tip == data.head => format!(
            "Seen in gitcraft at {}, the head now: nothing changed since.",
            short(tip)
        ),
        Some(Seen::Fresh { tip, since }) => format!(
            "Seen in gitcraft at {}; {} file(s) changed since. gitcraft remembers the version \
             looked at, not which lines were read.",
            short(tip),
            since.len()
        ),
        Some(Seen::Never) => "Not seen in gitcraft yet: opening its page, comparing its versions \
                              or marking it as seen keeps a version to count changes from."
            .to_owned(),
        Some(Seen::Unknown(why)) => format!("What was seen in gitcraft is unknown: {why}."),
        Some(Seen::Stale { .. }) | None => String::new(),
    };
    if !text.is_empty() {
        ui.add(egui::Label::new(RichText::new(text).color(t.text_dim)).wrap());
    }
    let Some(why) = &data.stale_review else {
        return;
    };
    egui::Frame::new()
        .fill(t.warning.gamma_multiply(0.14))
        .stroke(egui::Stroke::new(1.0, t.warning.gamma_multiply(0.6)))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                crate::repo::widgets::chip(ui, "stale review", t.warning);
                ui.add(egui::Label::new(RichText::new(why).color(t.text)).wrap());
                if let Some(pr) = pr
                    && ui
                        .button("Compare the versions")
                        .on_hover_text(
                            "The commits seen and the commits now, paired up, in Rebase ribbons",
                        )
                        .clicked()
                {
                    let source = crate::insights::ribbons::Source::Pr {
                        number: pr.number,
                        title: pr.title.clone(),
                        head: pr.sha.clone(),
                        base: pr.base.clone(),
                    };
                    acts.push(Action::Insights(InsightsAct::Open(Kind::Ribbons)));
                    acts.push(Action::Insights(InsightsAct::Ribbons(
                        super::ribbons::Act::Show(source),
                    )));
                }
            });
        });
}

fn legend(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    common::legend(
        ui,
        &[
            (Mark::Glyph("=", t.success), "as seen in gitcraft"),
            (Mark::Glyph("~", t.warning), "changed since seen"),
            (
                Mark::Hollow(t.success),
                "same content after a rewrite (inferred)",
            ),
            (Mark::Glyph("!", t.danger), "seen version rewritten: stale"),
            (Mark::Glyph("·", t.text_faint), "not seen here"),
            (Mark::Fill(t.accent), "lines changed since (darker: more)"),
            (Mark::Hatched(t.warning), "open review threads"),
            (Mark::Glyph("?", t.text_faint), "unknown"),
        ],
    );
}

/// How many threads are listed at most.
const MAX_THREADS: usize = 60;

/// The review threads, open ones first: each opens where it is, or its comment on the forge.
fn threads(ui: &mut Ui, data: &Heat, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let Some(all) = &data.threads else {
        common::heading(ui, "Review threads");
        ui.label(
            RichText::new("The review threads couldn't be read: unknown, not none.")
                .color(t.text_faint),
        );
        return;
    };
    let open = all.iter().filter(|th| !th.resolved).count();
    common::heading(ui, &format!("Review threads: {open} open of {}", all.len()));
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by_key(|&i| (all[i].resolved, all[i].path.clone(), all[i].line));
    egui::ScrollArea::vertical()
        .id_salt("review-heat-threads")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for &i in order.iter().take(MAX_THREADS) {
                let th = &all[i];
                ui.horizontal(|ui| {
                    let place = match th.line {
                        Some(n) => format!("{}:{n}", th.path),
                        None => th.path.clone(),
                    };
                    let changed = data.files.iter().any(|f| f.file.path == th.path);
                    let link = ui
                        .add_enabled(changed, egui::Link::new(place))
                        .on_hover_text(
                            "Open the file's diff at the pull request's revisions, at this line",
                        )
                        .on_disabled_hover_text("The pull request doesn't change this file now");
                    if link.clicked() {
                        let at = th.line.map(|n| (th.left, n.min(u32::MAX as u64) as u32));
                        acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::Open {
                            key: DiffKey::Range {
                                from: data.from.clone(),
                                to: Some(data.head.clone()),
                                path: th.path.clone(),
                            },
                            at,
                            cell: None,
                        })));
                    }
                    let state = match (th.resolved, th.outdated) {
                        (true, _) => "resolved",
                        (false, true) => "open, outdated",
                        (false, false) => "open",
                    };
                    ui.label(RichText::new(state).color(if th.resolved {
                        t.text_faint
                    } else {
                        t.warning
                    }));
                    if crate::pull_request::is_web_url(&th.url)
                        && ui
                            .small_button("on the forge")
                            .on_hover_text(th.url.clone())
                            .clicked()
                    {
                        acts.push(Action::OpenUrl(th.url.clone()));
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{}: {}", th.author, th.text)).color(t.text_dim),
                        )
                        .truncate(),
                    );
                });
            }
            if all.len() > MAX_THREADS {
                ui.label(
                    RichText::new(format!("… and {} more", all.len() - MAX_THREADS))
                        .color(t.text_faint),
                );
            }
        });
}

/// What the mark of a file or hunk's state seen says, for cells and tooltips.
pub(super) fn seen_mark(mark: SeenMark, t: &Tokens) -> (egui::Color32, Mark) {
    match mark {
        SeenMark::Same => (common::tint(t.success, 0.3, t), Mark::Glyph("=", t.success)),
        SeenMark::Changed => (
            common::tint(t.warning, 0.35, t),
            Mark::Glyph("~", t.warning),
        ),
        SeenMark::SameAfterRewrite => (t.field, Mark::Hollow(t.success)),
        SeenMark::Stale => (common::tint(t.danger, 0.25, t), Mark::Glyph("!", t.danger)),
        SeenMark::NotSeen => (t.field, Mark::Glyph("·", t.text_faint)),
        SeenMark::Unknown => (t.field, Mark::Glyph("?", t.text_faint)),
    }
}
