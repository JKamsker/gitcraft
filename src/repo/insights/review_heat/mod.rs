//! PR review heatmap (#175 item 7): a pull request's files, and their hunks once opened, by
//! what the version seen in gitcraft already showed, what changed since, where the open review
//! threads are, and what isn't known (the forge's "viewed" marks, coverage). A cell opens the
//! file's diff at the pull request's revisions, scrolled to its hunk or thread.

mod presentation;
pub(super) use presentation::seen_mark;
use presentation::{bar, legend, seen_line, threads};

mod grid;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use egui::{RichText, Ui};

use super::common::{self, Mark, Slot, SlotState};
use super::{InsightsAct, Kind};
use crate::insights::review_heat::{self as heat, Heat, Seen, SeenMark};
use crate::repo::{Action, DiffKey, RepoTab, short};
use crate::theme::Tokens;

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

    /// The pull request to show when none was picked: the one whose page the insights were
    /// opened over, else the checked-out branch's.
    fn review_heat_default(&self) -> Option<u64> {
        let shown = self.pr_view.as_ref().map(|v| v.number);
        shown
            .or(self.insights.as_ref().and_then(|v| v.pr))
            .or_else(|| {
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
        self.insight_forge_then(
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
            move |tab, result, _| tab.review_heat_reply(request, result),
        );
    }

    fn review_heat_reply(&mut self, request: u64, result: Result<Heat, String>) {
        let Some(state) = self.review_heat_state() else {
            return;
        };
        let paths =
            |h: &Heat| -> Vec<String> { h.files.iter().map(|f| f.file.path.clone()).collect() };
        let before = state.data.ready().map(paths);
        if state.data.accept(request, result) && state.data.ready().map(paths) != before {
            // The files opened and picked were by place: places of other files now.
            state.expanded.clear();
            state.selected = None;
        }
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
                // Another line of the file shown only scrolls: opening it again would close it.
                let shown = self
                    .diff
                    .as_ref()
                    .is_some_and(|d| d.key == key && d.lines.is_none());
                if !shown || at.is_none() {
                    self.open_diff(key.clone(), ctx);
                }
                if let Some(at) = at {
                    self.review_heat_jump(key, at);
                }
            }
        }
    }

    /// Scrolls the diff of `key` to line `at` (old side or not, number), now or once it is
    /// read.
    fn review_heat_jump(&mut self, key: DiffKey, at: (bool, u32)) {
        let Some(view) = self.diff.as_mut().filter(|d| d.key == key) else {
            return;
        };
        match &view.data {
            Some(Ok(data)) => view.jump_line = crate::repo::state::line_at(&data.lines, at),
            Some(Err(_)) => {}
            None => view.pending_line = Some(at),
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
