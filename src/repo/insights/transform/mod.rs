//! Repeated-edit transformation matrix (#175 item 8): the same edit made across files, and where it was made otherwise.

mod parts;
#[cfg(test)]
mod tests;

use egui::Ui;

use super::InsightsAct;
use super::common::{self, Slot, SlotState};
use super::heat_grid::{self, Cell, Layout};
use crate::insights::transform::{self, Ask, SELECTED, Shape, Transform};
use crate::repo::{Action, DiffKey, RepoTab};
use crate::theme::Tokens;
use parts::{cell_details, controls, legend, patterns_table, summary};

/// What the edits are read of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Of {
    /// The commit selected (HEAD when none is), against its first parent.
    #[default]
    Commit,
    /// HEAD since its merge base with the default branch.
    Branch,
    /// A pull request since its merge base with its base.
    Pr(u64),
}

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    of: Of,
    read: Slot<Transform>,
    /// What the last read was asked for; for a commit, the commit selected then.
    asked: Option<(Of, Option<String>)>,
    /// The cell shown: (pattern, file).
    selected: Option<(usize, usize)>,
    /// Patterns of one edit hidden (the count of them is still said).
    hide_once: bool,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Read the edits of this.
    Of(Of),
    /// Show a cell: (pattern, file), or none.
    Select(Option<(usize, usize)>),
    /// Hide the patterns made once, or show them again.
    HideOnce(bool),
}

pub(super) fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Transform(a))
}

/// What opens the edits `edits` with their context: the commit's diff at their lines, or the
/// range's diff of their file.
pub(super) fn open(t: &Transform, edits: &[usize]) -> Option<Action> {
    let first = t.edits.get(*edits.first()?)?;
    let path = t.files.get(first.file)?.clone();
    Some(match &t.subject.commit {
        Some(hash) => Action::OpenDiffOfLines {
            hash: hash.clone(),
            path,
            lines: edits
                .iter()
                .filter_map(|&e| t.edits.get(e))
                .map(|e| e.lines())
                .collect(),
        },
        None => Action::OpenDiff(DiffKey::Range {
            from: t.subject.base.clone(),
            to: Some(t.subject.head.clone()),
            path,
        }),
    })
}

/// The patterns shown as rows, indices into [`Transform::patterns`].
fn rows(t: &Transform, hide_once: bool) -> Vec<usize> {
    (0..t.patterns.len())
        .filter(|&p| {
            let pattern = &t.patterns[p];
            !hide_once || pattern.edits.len() > 1 || pattern.deviates_from.is_some()
        })
        .collect()
}

/// A row's label: what the pattern does, how often, and what it deviates from.
fn row_label(t: &Transform, p: usize) -> String {
    let pattern = &t.patterns[p];
    let n = pattern.edits.len();
    match pattern.deviates_from {
        Some(of) => format!("! {} ×{n}, deviates from #{}", pattern.label(), of + 1),
        None if pattern.deviating > 0 => format!(
            "#{} {} ×{n}, {} deviating",
            p + 1,
            pattern.label(),
            pattern.deviating
        ),
        None => format!("#{} {} ×{n}", p + 1, pattern.label()),
    }
}

impl RepoTab {
    /// What the edits are asked of now, and the commit selected when it is a commit.
    fn transform_ask(&self, of: Of) -> Result<(Ask, Option<String>), String> {
        match of {
            Of::Commit => {
                let hash = self.detail_hash.clone();
                let rev = hash.clone().unwrap_or_else(|| "HEAD".to_owned());
                Ok((Ask::Commit(rev), hash))
            }
            Of::Branch => {
                let base = self
                    .base_ref()
                    .ok_or("no default branch (main, master or develop) to compare with")?;
                Ok((Ask::Branch { base: base.name }, None))
            }
            Of::Pr(number) => {
                let pr = self
                    .prs
                    .iter()
                    .find(|p| p.number == number)
                    .ok_or_else(|| format!("#{number} isn't among the pull requests listed"))?;
                let mut bases = Vec::new();
                if let Some(remote) = self.default_remote() {
                    bases.push(format!("{remote}/{}", pr.base));
                }
                bases.push(pr.base.clone());
                Ok((
                    Ask::Pr {
                        number,
                        head: pr.sha.clone(),
                        bases,
                    },
                    None,
                ))
            }
        }
    }

    /// Reads what the view shows (again).
    pub(super) fn transform_load(&mut self, ctx: &egui::Context) {
        // Opened over a pull request, the edits are first those of that pull request.
        let opened_over = self.insights.as_ref().and_then(|v| v.pr);
        let listed = opened_over.filter(|n| self.prs.iter().any(|p| p.number == *n));
        if let Some(view) = self.insights.as_mut()
            && let Some(number) = listed
            && view.transform.asked.is_none()
        {
            view.transform.of = Of::Pr(number);
        }
        let Some(of) = self.insights.as_ref().map(|v| v.transform.of) else {
            return;
        };
        let ask = self.transform_ask(of);
        let Some(state) = self.insights.as_mut().map(|v| &mut v.transform) else {
            return;
        };
        let request = state.read.start();
        let (ask, commit) = match ask {
            Ok(found) => found,
            Err(e) => {
                state.asked = Some((of, None));
                state.read.accept(request, Err(e));
                return;
            }
        };
        state.asked = Some((of, commit));
        let repo = self.path.clone();
        self.insight_then(
            ctx,
            move || {
                let subject = transform::resolve(&repo, &ask)?;
                transform::read(&repo, subject, &transform::Limits::default())
            },
            move |tab, found, _| {
                if let Some(state) = tab.insights.as_mut().map(|v| &mut v.transform)
                    && state.read.accept(request, found)
                {
                    state.selected = None;
                }
            },
        );
    }

    pub(super) fn transform_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.transform) else {
            return;
        };
        match act {
            Act::Of(of) => {
                state.of = of;
                self.transform_load(ctx);
            }
            Act::Select(cell) => state.selected = cell,
            Act::HideOnce(hide) => state.hide_once = hide,
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn transform_back(&mut self) -> bool {
        self.insights
            .as_mut()
            .is_some_and(|v| v.transform.selected.take().is_some())
    }

    /// Where a tip the edits were read at is now.
    fn transform_tip(&self, name: &str) -> Option<String> {
        let head = (!self.refs.head.is_empty()).then(|| self.refs.head.clone());
        if name == SELECTED {
            return self.detail_hash.clone().or(head);
        }
        if name == "HEAD" {
            return head;
        }
        if let Some(n) = name
            .strip_prefix("PR #")
            .and_then(|n| n.parse::<u64>().ok())
        {
            return self
                .prs
                .iter()
                .find(|p| p.number == n)
                .map(|p| p.sha.clone());
        }
        (self.refs.local.iter())
            .chain(self.refs.remotes.iter().flat_map(|(_, list)| list))
            .find(|b| b.name == name)
            .map(|b| b.target.clone())
    }

    pub(super) fn transform_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        // The commit selected changed: its edits are read, as they are shown by default.
        if let Some(view) = self.insights.as_ref()
            && view.transform.of == Of::Commit
            && !view.transform.read.loading
            && view
                .transform
                .asked
                .as_ref()
                .is_some_and(|(of, commit)| *of == Of::Commit && *commit != self.detail_hash)
        {
            self.transform_load(&ctx);
        }
        let known = !self.refs.head.is_empty();
        let tips: Vec<(String, Option<String>)> = self
            .insights
            .as_ref()
            .and_then(|v| v.transform.read.ready())
            .map(|t| {
                t.provenance
                    .tips
                    .iter()
                    .map(|(name, _)| (name.clone(), self.transform_tip(name)))
                    .collect()
            })
            .unwrap_or_default();
        let prs: Vec<(u64, String)> = self
            .prs
            .iter()
            .map(|p| (p.number, p.title.clone()))
            .collect();
        let base = self.base_branch_name();
        let selected_commit = self.detail_hash.clone();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.transform;
        controls(
            ui,
            state,
            &prs,
            base.as_deref(),
            selected_commit.as_deref(),
            acts,
        );
        match common::slot_state(ui, &state.read, "the edits") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let loading = state.read.loading;
        let Some(t) = state.read.ready_mut() else {
            return;
        };
        if known {
            t.provenance.check_tips(|name| {
                tips.iter()
                    .find(|(n, _)| n == name)
                    .and_then(|(_, at)| at.clone())
            });
        }
        common::provenance(ui, &t.provenance);
        let t = &*t;
        summary(ui, t, loading);
        if t.edits.is_empty() {
            common::empty(ui, "No lines changed here (or only binary files).");
            return;
        }
        let tokens = Tokens::get(ui.ctx());
        legend(ui, &tokens);
        let rows = rows(t, state.hide_once);
        let selected = state.selected;
        if rows.is_empty() {
            common::empty(
                ui,
                "Every edit here was made once, and those are hidden: show them to see them.",
            );
            return;
        }
        if table {
            egui::ScrollArea::vertical()
                .id_salt("transform-table")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    patterns_table(ui, t, &rows, selected, acts);
                    if let Some((p, f)) = selected {
                        cell_details(ui, t, p, f, acts);
                    }
                });
            return;
        }
        let max = t.patterns.iter().map(|p| p.edits.len()).max().unwrap_or(1) as f32;
        let layout = Layout {
            label_w: 340.0,
            header_h: 90.0,
            ..Layout::default()
        };
        let grid_h = (layout.header_h + rows.len() as f32 * layout.cell.y + 24.0)
            .min((ui.available_height() * 0.55).max(200.0));
        let picked = ui
            .allocate_ui(egui::vec2(ui.available_width(), grid_h), |ui| {
                heat_grid::show(
                    ui,
                    "transform",
                    layout,
                    (rows.len(), t.files.len()),
                    |r| row_label(t, rows[r]),
                    |c| t.files[c].clone(),
                    |r, c| grid_cell(t, &tokens, rows[r], c, max),
                    selected.and_then(|(p, f)| rows.iter().position(|&r| r == p).map(|r| (r, f))),
                )
            })
            .inner;
        if let Some((r, c)) = picked.cell {
            let p = rows[r];
            acts.push(act(Act::Select(Some((p, c)))));
            let (own, deviant) = t.cell(p, c);
            if let Some(a) = open(t, &[own, deviant].concat()) {
                acts.push(a);
            }
        }
        egui::ScrollArea::vertical()
            .id_salt("transform-cell")
            .auto_shrink([false, true])
            .show(ui, |ui| match selected {
                Some((p, f)) => cell_details(ui, t, p, f, acts),
                None => common::empty(
                    ui,
                    "Pick a cell (or move to one with the arrows and press Enter) to see its edits.",
                ),
            });
    }
}

/// A cell of the matrix: how many of the pattern's edits the file has, marked when some there
/// deviate from it, when it deviates itself, or when it was made once.
fn grid_cell(t: &Transform, tokens: &Tokens, p: usize, f: usize, max: f32) -> Option<Cell> {
    let pattern = &t.patterns[p];
    let own = pattern.in_file(f);
    let deviant = pattern.deviants_in(f);
    if own == 0 && deviant == 0 {
        return None;
    }
    let mut tip = format!("{own} edit{}", if own == 1 { "" } else { "s" });
    if deviant > 0 {
        tip.push_str(&format!(
            ", and {deviant} deviating from it (rows marked !)"
        ));
    }
    let (fill, mark) = if pattern.deviates_from.is_some() {
        tip.push_str("\nDeviates from the pattern it resembles");
        (
            common::tint(tokens.warning, 0.35, tokens),
            Some(common::Mark::Hatched(tokens.warning)),
        )
    } else if deviant > 0 {
        (
            common::heat(tokens, own as f32 / max),
            Some(common::Mark::Glyph("!", tokens.warning)),
        )
    } else if pattern.edits.len() == 1 || matches!(pattern.shape, Shape::Large) {
        tip.push_str("\nMade once: like no other edit");
        (
            common::tint(tokens.text_dim, 0.2, tokens),
            Some(common::Mark::Hollow(tokens.text_dim)),
        )
    } else if pattern.shape == Shape::Layout {
        (
            common::heat(tokens, own as f32 / max),
            Some(common::Mark::Dotted(tokens.text_dim)),
        )
    } else {
        (common::heat(tokens, own as f32 / max), None)
    };
    tip.push_str("\nClick or Enter: open the diff here with its context");
    Some(Cell { fill, mark, tip })
}
