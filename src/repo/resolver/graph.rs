//! A merge put aside for an agent, in the graph: a row like a stash's, right above the commit
//! it was put aside on, with a robot in a dashed box (a turning arc around it while the agent
//! works, a dot once it is done) and how far along the agent is. Clicking it shows the agent's
//! card; its menu brings the merge back.

use std::sync::Arc;

use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, Ui, pos2, vec2};

use super::{Phase, ResolverAct};
use crate::git::Commit;
use crate::repo::{Action, RepoTab};
use crate::resolver::park::Parked;
use crate::theme::{self, Tokens};

/// How a parked row's hash starts: no commit has it (see `WIP_HASH`).
const PREFIX: &str = "\u{0}parked:";

/// How often the arc turns a step.
const TURN: std::time::Duration = std::time::Duration::from_millis(60);

/// Where the agent of a parked merge is, as its row shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Standing {
    /// It works: how many steps are over, of how many.
    Working(usize, usize),
    /// Done: what it did is there to review.
    Ready,
    /// No agent works on it (stopped, or gitcraft was closed meanwhile).
    Waiting,
}

impl RepoTab {
    /// The parked merge a graph row stands for, if it's one.
    pub(in crate::repo) fn parked_at(&self, row: usize) -> Option<&Parked> {
        let id = self.commits.get(row)?.hash.strip_prefix(PREFIX)?;
        self.resolver.parked.iter().find(|p| p.id == id)
    }

    pub(in crate::repo) fn parked_standing(&self, parked: &Parked) -> Standing {
        match self.resolver.run.as_ref().filter(|r| r.id == parked.id) {
            Some(run) => match run.phase {
                Phase::Preparing | Phase::Working | Phase::Checking => {
                    Standing::Working(run.progress.counts().0, run.progress.counts().1)
                }
                Phase::Ready if !run.results.is_empty() => Standing::Ready,
                _ => Standing::Waiting,
            },
            None => Standing::Waiting,
        }
    }

    /// The row's dot: a dashed box with the robot, the arc turning while the agent works.
    pub(in crate::repo) fn parked_marker(
        &self,
        ui: &Ui,
        gp: &Painter,
        parked: &Parked,
        node: Pos2,
    ) {
        let t = Tokens::get(ui.ctx());
        let standing = self.parked_standing(parked);
        let color = match standing {
            Standing::Ready => t.success,
            Standing::Working(..) => t.accent,
            Standing::Waiting => t.text_dim,
        };
        let r = Rect::from_center_size(node, vec2(15.0, 15.0));
        gp.rect_filled(r, 3.0, t.dock);
        let (l, tp, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
        gp.add(egui::Shape::dashed_line(
            &[
                pos2(l, tp),
                pos2(rt, tp),
                pos2(rt, b),
                pos2(l, b),
                pos2(l, tp),
            ],
            Stroke::new(1.5, color),
            2.5,
            2.0,
        ));
        crate::icons::paint(ui, r, "bot", 10.0, color);
        match standing {
            Standing::Working(..) => {
                // A quarter of a ring around the box, a turn a second: drawn again at the
                // spinner's pace, not every frame.
                let time = ui.ctx().input(|i| i.time) as f32;
                let start = time * std::f32::consts::TAU;
                let points: Vec<Pos2> = (0..=12)
                    .map(|k| {
                        let a = start + k as f32 / 12.0 * std::f32::consts::FRAC_PI_2 * 1.5;
                        node + vec2(a.cos(), a.sin()) * 11.0
                    })
                    .collect();
                gp.add(egui::Shape::line(points, Stroke::new(2.0, t.accent)));
                ui.ctx().request_repaint_after(TURN);
            }
            Standing::Ready => {
                gp.circle_filled(r.right_top(), 3.5, t.success);
            }
            Standing::Waiting => {}
        }
    }

    /// The row's text: what was put aside, and where its agent is.
    pub(in crate::repo) fn parked_description(
        &self,
        painter: &Painter,
        t: &Tokens,
        parked: &Parked,
        (x, mid): (f32, f32),
        background: Color32,
    ) {
        let what = parked.what();
        let mut chars = what.chars();
        let first: String = chars
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default();
        let title = format!("{first}{} put aside", chars.as_str());
        let r = painter.text(
            pos2(x, mid),
            Align2::LEFT_CENTER,
            title,
            theme::medium(12.5),
            t.text,
        );
        let (text, color) = match self.parked_standing(parked) {
            Standing::Working(over, total) if total > 0 => {
                (format!("agent resolving · {over}/{total}"), t.accent)
            }
            Standing::Working(..) => ("agent resolving…".to_owned(), t.accent),
            Standing::Ready => ("resolved · bring it back".to_owned(), t.success),
            Standing::Waiting => ("waiting for you · bring it back".to_owned(), t.warning),
        };
        crate::repo::graph_view::pill(
            painter,
            t,
            pos2(r.right() + 10.0, mid),
            &text,
            color,
            background,
        );
    }

    /// What a screen reader says of a parked merge's row.
    pub(in crate::repo) fn parked_label(&self, parked: &Parked) -> String {
        let standing = match self.parked_standing(parked) {
            Standing::Working(over, total) => {
                format!("an agent is resolving it, {over} of {total} steps done")
            }
            Standing::Ready => "the agent resolved it: bring it back to review".to_owned(),
            Standing::Waiting => "waiting to be brought back".to_owned(),
        };
        format!("{} put aside, {standing}", parked.what())
    }

    /// The menu of a parked merge's row.
    pub(in crate::repo) fn parked_menu(
        &self,
        ui: &mut Ui,
        parked: &Parked,
        acts: &mut Vec<Action>,
    ) {
        if ui
            .button("Bring it back")
            .on_hover_text(
                "Checks out its branch and puts the merge back as it was, conflicts and \
                 staged files included; the agent's work is then there to review",
            )
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::BringBack(parked.id.clone())));
            ui.close();
        }
        if self.resolver_owns(&parked.id) && ui.button("Show the agent").clicked() {
            acts.push(Action::Resolver(ResolverAct::Fold(false)));
            ui.close();
        }
        ui.separator();
        if ui
            .button("Drop it (keep its files on a branch)")
            .on_hover_text(
                "Leaves the merge put aside, without bringing it back: its files (conflict \
                 markers and all) go on a branch of their own named parked/…, so nothing is \
                 lost; the agent's work goes",
            )
            .clicked()
        {
            acts.push(Action::Resolver(ResolverAct::DropParked(parked.id.clone())));
            ui.close();
        }
    }
}

/// The graph rows of the merges `parked`, newest first (at the top of the graph).
pub(in crate::repo) fn parked_rows(parked: &[Parked]) -> Vec<Arc<Commit>> {
    parked.iter().rev().map(parked_row).collect()
}

/// The row of the parked merge `id` among `rows`.
pub(in crate::repo) fn parked_row_of(rows: &[Arc<Commit>], id: &str) -> Option<usize> {
    rows.iter()
        .position(|c| c.hash.strip_prefix(PREFIX) == Some(id))
}

fn parked_row(p: &Parked) -> Arc<Commit> {
    Arc::new(Commit {
        hash: format!("{PREFIX}{}", p.id),
        parents: vec![p.head.clone()],
        time: p.at,
        summary: p.what(),
        ..Commit::default()
    })
}
