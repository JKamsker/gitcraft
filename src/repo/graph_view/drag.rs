//! Dragging in the graph: a row's branch pill drags that branch, the rest of the row its
//! commit; the pill and the row are both drop targets; and the graph scrolls when a drag
//! nears its top or bottom edge.

use egui::{Rect, Response, Ui, vec2};

use super::ROW_GAP;
use super::badge::{Badge, BadgeKind};
use super::row::Row;
use crate::repo::drag_drop::{DragBranch, DragCommit, DragItem, DropTarget};
use crate::repo::{Action, RepoTab};

/// How close to the graph's top or bottom edge a drag scrolls it.
const EDGE: f32 = 36.0;
/// How fast it scrolls at the very edge, in points a second.
const SPEED: f32 = 900.0;

/// A row's own pill, as a drag source and a drop target.
pub(super) struct PillHit {
    pub(super) rect: Rect,
    /// The branch it is, for a branch pill.
    branch: Option<DragBranch>,
    /// What dropping on it means: a branch, or the detached HEAD (a tag's is the row's).
    target: Option<DropTarget>,
}

impl PillHit {
    pub(super) fn branch_at(&self, pointer: Option<egui::Pos2>) -> bool {
        self.branch.is_some() && pointer.is_some_and(|pos| self.rect.contains(pos))
    }

    pub(super) fn of(badge: &Badge, rect: Rect, hash: &str) -> Self {
        let branch = (badge.kind == BadgeKind::Branch).then(|| DragBranch {
            name: badge.git_ref(),
            remote: !badge.local,
            tip: hash.to_owned(),
        });
        let target = match badge.kind {
            BadgeKind::Branch => branch.clone().map(DropTarget::Branch),
            BadgeKind::DetachedHead => Some(DropTarget::DetachedHead),
            BadgeKind::Tag | BadgeKind::PullRequest => None,
        };
        Self {
            rect,
            branch,
            target,
        }
    }
}

impl RepoTab {
    /// Drag and drop on the commit row `row` (`resp`), with its pill at `pill`.
    pub(super) fn row_drag_drop(
        &self,
        ui: &Ui,
        resp: &Response,
        row: &Row,
        pill: Option<&PillHit>,
        acts: &mut Vec<Action>,
    ) {
        let commit = || DragCommit {
            hash: row.commit.hash.clone(),
            summary: row.commit.summary.clone(),
            merge: row.commit.parents.len() > 1,
        };
        self.drag_source(resp, || {
            let origin = ui.input(|i| i.pointer.press_origin())?;
            let on_pill = pill.filter(|p| p.rect.contains(origin));
            Some(match on_pill.and_then(|p| p.branch.clone()) {
                Some(branch) => DragItem::Branch(branch),
                None => DragItem::Commit(commit()),
            })
        });
        let Some(payload) = self.dragged(ui.ctx()) else {
            return;
        };
        // A merge put aside is no commit to drop anything on.
        if self.parked_at(row.i).is_some() {
            return;
        }
        let pointer = ui.ctx().pointer_interact_pos();
        let over_row = resp.contains_pointer();
        // The pill takes the drop when it is under the pointer and takes what is dragged;
        // otherwise the row does.
        let mut on_pill = false;
        if let Some((rect, target)) = pill.and_then(|p| Some((p.rect, p.target.as_ref()?))) {
            let offers = self.drop_offers(&payload, target);
            on_pill = over_row && !offers.is_empty() && pointer.is_some_and(|p| rect.contains(p));
            let light = rect.expand(1.0);
            self.drop_zone_with(ui, &payload, light, target, offers, on_pill, acts);
        }
        let target = DropTarget::Commit(commit());
        let offers = self.drop_offers(&payload, &target);
        let band = row.rect.shrink2(vec2(0.0, ROW_GAP / 2.0));
        let hovered = over_row && !on_pill;
        self.drop_zone_with(ui, &payload, band, &target, offers, hovered, acts);
    }

    /// While something is dragged with the pointer near the top or bottom of the visible part
    /// of a scrolled list (`ui`: the graph's, the left pane's), scrolls that way, faster nearer
    /// the edge. egui leaves the mouse wheel alone during a drag, so this is how a target out
    /// of sight is reached.
    pub(in crate::repo) fn drag_auto_scroll(&self, ui: &Ui) {
        if self.dragged(ui.ctx()).is_none() {
            return;
        }
        let Some(pointer) = ui.ctx().pointer_hover_pos() else {
            return;
        };
        let view = ui.clip_rect();
        if pointer.x < view.left() || pointer.x > view.right() {
            return;
        }
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let delta = scroll_speed(view.top(), view.bottom(), pointer.y) * dt;
        if delta != 0.0 {
            ui.scroll_with_delta_animation(vec2(0.0, delta), egui::style::ScrollAnimation::none());
            ui.ctx().request_repaint();
        }
    }
}

/// How fast to scroll with the pointer at `y` in a view from `top` to `bottom`, in points a
/// second: positive (towards the top) near the top edge, negative near the bottom one, zero
/// in between and away from the view.
fn scroll_speed(top: f32, bottom: f32, y: f32) -> f32 {
    let edge = EDGE.min((bottom - top) / 3.0);
    if y < top - edge || y > bottom + edge || edge <= 0.0 {
        0.0
    } else if y < top + edge {
        SPEED * ((top + edge - y) / edge).min(1.0)
    } else if y > bottom - edge {
        -SPEED * ((y - (bottom - edge)) / edge).min(1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;
    use crate::repo::graph_view::readability_tests::painted_text_center;
    use egui::{Event, Modifiers, PointerButton, Pos2};

    /// A tab on `main` showing the commits `c` (HEAD), `b` and `a`, each on the next.
    fn tab(ctx: &egui::Context) -> RepoTab {
        let mut tab = RepoTab::new_deferred("/nonexistent/graph-drag-test".into(), ctx);
        let hashes = ["c", "b", "a"].map(|h| h.repeat(40));
        tab.commits = hashes
            .iter()
            .enumerate()
            .map(|(i, h)| {
                std::sync::Arc::new(git::Commit {
                    hash: h.clone(),
                    parents: hashes.get(i + 1).cloned().into_iter().collect(),
                    summary: format!("commit {}", &h[..1]),
                    ..Default::default()
                })
            })
            .collect();
        tab.graph = crate::graph::layout(&tab.commits);
        tab.index = hashes
            .iter()
            .enumerate()
            .map(|(i, h)| (h.clone(), i))
            .collect();
        tab.history = tab.commits.clone();
        tab.history_index = tab.index.clone();
        tab.refs.head = hashes[0].clone();
        tab.refs.current = Some("main".to_owned());
        tab
    }

    /// The painted summary is inside its row and away from the branch pill.
    fn row(ctx: &egui::Context, tab: &mut RepoTab, i: usize) -> Pos2 {
        let (_, shapes) = paint_frame(ctx, tab, Vec::new());
        painted_text_center(&shapes, &tab.commits[i].summary)
    }

    /// Runs a frame of the graph with `events`; returns what it asked for.
    fn frame(ctx: &egui::Context, tab: &mut RepoTab, events: Vec<Event>) -> Vec<Action> {
        paint_frame(ctx, tab, events).0
    }

    fn paint_frame(
        ctx: &egui::Context,
        tab: &mut RepoTab,
        events: Vec<Event>,
    ) -> (Vec<Action>, Vec<egui::epaint::ClippedShape>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut acts = Vec::new();
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| tab.graph_view(ui, &mut acts));
        });
        output.textures_delta.clear();
        (acts, std::mem::take(&mut output.shapes))
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    /// Drags from row `from` to row `to` in steps, with `between` sent halfway; returns every
    /// action asked for on the way.
    fn drag(
        ctx: &egui::Context,
        tab: &mut RepoTab,
        from: usize,
        to: usize,
        between: Vec<Event>,
    ) -> Vec<Action> {
        // The first frame establishes table geometry; locate both actual painted rows.
        frame(ctx, tab, Vec::new());
        let (start, end) = (row(ctx, tab, from), row(ctx, tab, to));
        let mut acts = frame(ctx, tab, vec![Event::PointerMoved(start)]);
        acts.extend(frame(ctx, tab, vec![button(start, true)]));
        for n in 1..=8 {
            let at = start + (end - start) * (n as f32 / 8.0);
            let events = if n == 4 { between.clone() } else { Vec::new() };
            acts.extend(frame(
                ctx,
                tab,
                [vec![Event::PointerMoved(at)], events].concat(),
            ));
        }
        acts.extend(frame(ctx, tab, vec![button(end, false)]));
        acts.extend(frame(ctx, tab, Vec::new()));
        acts
    }

    fn drop_menu(acts: &[Action]) -> Option<&crate::repo::drag_drop::DropMenu> {
        acts.iter().find_map(|a| match a {
            Action::DropMenu(menu) => Some(menu),
            _ => None,
        })
    }

    /// Dragging the oldest commit onto HEAD's row opens the menu of moves and squashes there,
    /// and selects nothing; a click still selects.
    #[test]
    fn dragging_a_commit_onto_another_opens_the_drop_menu() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = tab(&ctx);
        // `a` is the root: move `b` instead, onto `c`.
        let acts = drag(&ctx, &mut tab, 1, 0, Vec::new());
        let menu = drop_menu(&acts);
        assert!(menu.is_some(), "no drop menu");
        let labels: Vec<String> = menu
            .map(|m| m.offers.iter().map(|o| o.label()).collect())
            .unwrap_or_default();
        assert_eq!(labels.len(), 3, "{labels:?}");
        assert!(labels[0].starts_with("Move bbbbbbbb above cccccccc"));
        assert!(!acts.iter().any(|a| matches!(a, Action::Select(_))));

        let point = row(&ctx, &mut tab, 2);
        let mut clicks = frame(&ctx, &mut tab, vec![Event::PointerMoved(point)]);
        clicks.extend(frame(&ctx, &mut tab, vec![button(point, true)]));
        clicks.extend(frame(&ctx, &mut tab, vec![button(point, false)]));
        assert!(clicks.iter().any(|a| matches!(a, Action::Select(2))));
        assert!(drop_menu(&clicks).is_none());
    }

    /// Esc during the drag cancels it: the drop then does nothing.
    #[test]
    fn escape_cancels_a_drag() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = tab(&ctx);
        let esc = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let acts = drag(&ctx, &mut tab, 1, 0, vec![esc]);
        assert!(drop_menu(&acts).is_none());
    }

    #[test]
    fn a_drag_scrolls_near_the_edges_only() {
        assert_eq!(scroll_speed(0.0, 600.0, 300.0), 0.0);
        assert!(scroll_speed(0.0, 600.0, 10.0) > 0.0);
        assert!(scroll_speed(0.0, 600.0, 590.0) < 0.0);
        // Faster nearer the edge, no faster past it.
        assert!(scroll_speed(0.0, 600.0, 2.0) > scroll_speed(0.0, 600.0, 20.0));
        assert_eq!(scroll_speed(0.0, 600.0, -5.0), SPEED);
        // Far from the view: nothing.
        assert_eq!(scroll_speed(0.0, 600.0, 900.0), 0.0);
    }
}
