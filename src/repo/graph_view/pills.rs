//! The refs column of a row: its branch and tag pills and the `+N` stack of the others.

mod paint;
use paint::*;

use egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, Ui, pos2,
    vec2,
};

use super::DIMMED;
use super::badge::{Badge, BadgeKind};
use super::drag::PillHit;
use super::menu::pill_menu_id;
use super::row::Row;
use crate::ci::CheckState;
use crate::icons;
use crate::repo::columns::Cols;
use crate::repo::guide::{GuideAct, Hint};
use crate::repo::{Action, RepoTab, ci_status, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// Natural width of the pill, icons, stack count and connector margin on one visible row.
    pub(super) fn required_refs_width(&self, ui: &Ui, i: usize) -> Option<f32> {
        let commit = self.commits.get(i)?;
        let badges = self.graph_badges(commit);
        let first = badges.first()?;
        let marks = self.marks(first, &commit.hash);
        let (lead, trail) = pill_icons(first, marks.pr);
        let font = if first.head {
            theme::semibold(11.0)
        } else {
            theme::medium(11.0)
        };
        let text = ui
            .painter()
            .layout_no_wrap(first.label.clone(), font, Color32::WHITE)
            .size()
            .x;
        Some(
            text + 16.0
                + (lead.len() + trail.len()) as f32 * 15.0
                + if marks.ci.is_some() { 12.0 } else { 0.0 }
                + if badges.len() > 1 { 28.0 } else { 0.0 }
                + 29.0,
        )
    }

    /// Branch and tag pills, left of the graph, joined to the commit's dot. Returns `resp`
    /// with the column's tooltip, and the row's own pill.
    pub(super) fn refs_cell(
        &self,
        ui: &Ui,
        painter: &Painter,
        row: &Row,
        resp: Response,
        acts: &mut Vec<Action>,
    ) -> (Response, Option<PillHit>) {
        let t = Tokens::get(ui.ctx());
        let badges = self.graph_badges(row.commit);
        let (Some(first), Some(node_x)) = (badges.first(), row.node_x) else {
            // No pill was right-clicked: forget one right-clicked on this row before, which
            // may have moved off it since, or its menu would offer that branch here.
            if resp.secondary_clicked() {
                ui.data_mut(|d| d.insert_temp(pill_menu_id(&row.commit.hash), None::<String>));
            }
            return (resp, None);
        };
        let (rect, mid, gx, cols) = (row.rect, row.mid(), row.gx, row.cols);
        let identity = first.full_refs().into_iter().next().unwrap_or_default();
        let offset = if self.ghost.drawing() {
            0.0
        } else {
            self.graph_motion
                .pill_offset(ui.ctx(), &identity, &row.commit.hash, row.i)
        };
        let mid = mid + offset;
        let refs_clip = Rect::from_min_max(
            pos2(rect.left(), ui.clip_rect().top()),
            pos2(gx - 4.0, ui.clip_rect().bottom()),
        );
        let mut rp = if offset.abs() > 0.1 {
            ui.ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Middle,
                    ui.id().with("moving-ref-pills"),
                ))
                .with_clip_rect(refs_clip)
        } else {
            painter.with_clip_rect(refs_clip)
        };
        // In a ghost run: a pill where the operation would put it, or one it would delete.
        let (moved, gone) = self.ghost.pill(&first.full_refs());
        if moved || gone {
            // What is previewed: drawn in full even on a faded ghost row, marked instead.
            rp.set_opacity(1.0);
        }
        let more = badges.len() - 1;
        let more_w = if more > 0 { 22.0 } else { 0.0 };
        let max_w = cols.refs - 10.0 - more_w;
        let pill = ref_pill(
            ui,
            &rp,
            pos2(rect.left() + 6.0, mid),
            first,
            row.color,
            max_w.max(16.0),
            self.marks(first, &row.commit.hash),
        );
        ghost_pill_marks(&rp, &t, pill, moved, gone);
        if offset.abs() <= 0.1 {
            self.pill_clicks(ui, &resp, pill, first, &row.commit.hash, acts);
        }
        let hit = if offset.abs() <= 0.1 {
            Some(PillHit::of(first, pill, &row.commit.hash))
        } else {
            None
        };
        let mut end = pill.right();
        let mut more_rect = None;
        if more > 0 {
            let r = Rect::from_min_size(pos2(end + 3.0, mid - 10.0), vec2(more_w - 3.0, 20.0));
            more_count(&rp, &t, r, more, row.color);
            end = r.right();
            more_rect = Some(r);
        }
        // Connector to the dot, drawn first so the lanes cross over it.
        painter.line_segment(
            [pos2(end, mid), pos2(node_x.min(gx + cols.graph), mid)],
            Stroke::new(1.5, row.color.gamma_multiply(0.8)),
        );
        // Not during a drag: the list would cover the rows below, which then take no drop.
        let dragging = self.dragged(ui.ctx()).is_some();
        let stack_open = !dragging
            && more_rect.is_some_and(|more| {
                let at = pos2(rect.left() + 2.0, mid - 16.0);
                self.ref_stack(ui, &badges, row.i, more, at, cols, row.color, acts)
            });
        let refs_cell = Rect::from_min_max(rect.min, pos2(gx, rect.bottom()));
        // No tooltip in the way of a drag.
        if !stack_open && !dragging && ui.rect_contains_pointer(refs_cell) {
            let mut lines: Vec<String> = badges.iter().map(Badge::describe).collect();
            lines.extend(
                badges
                    .iter()
                    .filter(|b| b.kind == BadgeKind::Branch && b.local)
                    .filter_map(|b| self.stack_line(&b.label)),
            );
            let drag = if first.kind == BadgeKind::PullRequest {
                crate::i18n::text("repo-core-2683")
            } else if first.kind == BadgeKind::Branch {
                crate::i18n::text("repo-core-2684")
            } else {
                crate::i18n::text("repo-core-2685")
            };
            let resp = resp.on_hover_text(crate::i18n::message(
                "repo-core-2686",
                &[
                    ("n", lines.join("\n").to_string().into()),
                    ("author", row.commit.author.to_string().into()),
                    ("date", crate::i18n::date_iso(&row.commit.date).into()),
                    ("drag", drag.to_string().into()),
                ],
            ));
            return (resp, hit);
        }
        (resp, hit)
    }

    /// What a pill of `badge` on commit `hash` carries besides its name: whether the branch has
    /// an open pull request, and the build status of its tip.
    fn marks(&self, badge: &Badge, hash: &str) -> Marks {
        let branch = badge.kind == BadgeKind::Branch;
        Marks {
            pr: branch && self.prs.iter().any(|p| p.branch == badge.label),
            ci: self
                .ci_status(hash)
                .filter(|_| branch)
                .map(|status| status.state),
        }
    }

    /// Clicks on a row's own pill (at `pill`): right-clicking a branch pill puts that branch's
    /// actions on top of the row's menu, double-clicking checks it out.
    fn pill_clicks(
        &self,
        ui: &Ui,
        resp: &Response,
        pill: Rect,
        badge: &Badge,
        hash: &str,
        acts: &mut Vec<Action>,
    ) {
        let on_pill = || {
            ui.ctx()
                .pointer_interact_pos()
                .is_some_and(|p| pill.contains(p))
        };
        if resp.secondary_clicked() {
            let branch = (on_pill() && badge.kind == BadgeKind::Branch).then(|| badge.git_ref());
            ui.data_mut(|d| d.insert_temp(pill_menu_id(hash), branch));
        }
        if resp.double_clicked()
            && on_pill()
            && let Some(act) = self.pill_checkout(badge)
        {
            acts.push(act);
            acts.push(Action::Guide(GuideAct::Withdraw(Hint::DoubleClickPill)));
        } else if resp.clicked()
            && on_pill()
            && badge.kind == BadgeKind::Branch
            && self.pill_checkout(badge).is_some()
        {
            acts.push(Action::Guide(GuideAct::Hint(Hint::DoubleClickPill)));
        }
    }

    /// What double-clicking a pill does: check out that branch (a tracking branch for one only on
    /// a remote, and pulling a local one behind its upstream) or tag; pull when it is the
    /// checked-out branch's remote, further on. Nothing for the checked-out one or while an
    /// operation runs.
    fn pill_checkout(&self, badge: &Badge) -> Option<Action> {
        if badge.head || self.busy_op.is_some() {
            return None;
        }
        // The checked-out branch's remote pill, further on than the branch: pull.
        let current = self.refs.current.as_deref() == Some(badge.label.as_str());
        if badge.kind == BadgeKind::Branch && !badge.local && current {
            // Only for the branch it tracks: `git pull` brings that one in, not another
            // remote's branch of the same name.
            let behind = self
                .refs
                .local
                .iter()
                .find(|b| b.name == badge.label)
                .is_some_and(|b| {
                    b.ahead_behind().1 > 0
                        && badge
                            .remotes
                            .iter()
                            .any(|r| b.upstream == format!("{r}/{}", badge.label))
                });
            return behind.then_some(Action::Pull);
        }
        match badge.kind {
            BadgeKind::Branch if badge.local => {
                let behind = self
                    .refs
                    .local
                    .iter()
                    .find(|b| b.name == badge.label)
                    .is_some_and(|b| matches!(b.ahead_behind(), (0, behind) if behind > 0));
                Some(if behind {
                    Action::CheckoutPull(badge.label.clone())
                } else {
                    Action::Checkout(badge.label.clone())
                })
            }
            BadgeKind::Branch => badge
                .remotes
                .first()
                .map(|remote| Action::CheckoutRemote(format!("{remote}/{}", badge.label))),
            BadgeKind::Tag => Some(Action::Checkout(format!("refs/tags/{}", badge.label))),
            BadgeKind::DetachedHead | BadgeKind::PullRequest => None,
        }
    }

    /// While the pointer is on a row's `+N` (or on the list itself), every pill of the commit
    /// stacked over the row, to pick one to check out. Returns whether it is open.
    #[allow(clippy::too_many_arguments)]
    fn ref_stack(
        &self,
        ui: &Ui,
        badges: &[Badge],
        i: usize,
        more: Rect,
        at: Pos2,
        cols: Cols,
        lane: Color32,
        acts: &mut Vec<Action>,
    ) -> bool {
        let t = Tokens::get(ui.ctx());
        let hash = self.commits.get(i).map_or("", |c| c.hash.as_str());
        // By the commit, as its row: a list (or a pill's menu) left open stays on its commit
        // when rows shift.
        let id = ui.id().with(("ref_stack", hash));
        let last: Option<Rect> = ui.data(|d| d.get_temp(id));
        let slot_id = |n: usize| id.with(("slot", n));
        // A pill's menu keeps the list up while the pointer is on the menu.
        let menu_open =
            (0..badges.len()).any(|n| egui::Popup::is_id_open(ui.ctx(), slot_id(n).with("popup")));
        let open = menu_open
            || ui.ctx().pointer_hover_pos().is_some_and(|p| {
                more.contains(p) || last.is_some_and(|r| r.expand(2.0).contains(p))
            });
        if !open {
            ui.data_mut(|d| d.remove::<Rect>(id));
            return false;
        }
        let slot_w = (cols.refs - 16.0).max(160.0);
        let shown = egui::Area::new(id)
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for (n, b) in badges.iter().enumerate() {
                            let (slot, _) =
                                ui.allocate_exact_size(vec2(slot_w, 20.0), Sense::hover());
                            let resp = ui.interact(slot, slot_id(n), Sense::click());
                            if resp.hovered()
                                || menu_open
                                    && egui::Popup::is_id_open(ui.ctx(), slot_id(n).with("popup"))
                            {
                                ui.painter().rect_filled(
                                    slot.expand(2.0),
                                    CornerRadius::same(t.radius as u8),
                                    t.hover,
                                );
                            }
                            let marks = self.marks(b, hash);
                            ref_pill(ui, ui.painter(), slot.left_center(), b, lane, slot_w, marks);
                            self.stack_slot_clicks(resp, b, i, acts);
                        }
                    });
            });
        ui.data_mut(|d| d.insert_temp(id, shown.response.rect));
        true
    }

    /// Clicks on one pill of a row's stack, which act like those on the row's own pill.
    fn stack_slot_clicks(&self, resp: Response, b: &Badge, i: usize, acts: &mut Vec<Action>) {
        let hint = match self.pill_checkout(b) {
            _ if b.kind == BadgeKind::PullRequest => crate::i18n::text("repo-core-2690"),
            _ if b.head => crate::i18n::text("repo-core-2691"),
            Some(Action::Pull) => crate::i18n::text("repo-core-2692"),
            _ => crate::i18n::text("repo-core-2693"),
        };
        let resp = resp.on_hover_text(format!("{} ({hint})", b.describe()));
        widgets::a11y(&resp, egui::WidgetType::Button, || b.spoken());
        if resp.double_clicked() {
            if let Some(act) = self.pill_checkout(b) {
                acts.push(act);
            }
        } else if resp.clicked() {
            acts.push(Action::Select(i));
        }
        // The same menu as right-clicking the row's own pill.
        let branch = (b.kind == BadgeKind::Branch).then(|| b.git_ref());
        crate::menu_item::context(&resp)
            .style(crate::theme::menu_style)
            .show(|ui| self.commit_menu_items(ui, i, branch, self.busy_op.is_none(), acts));
    }
}

/// The `+N` after a row's pill, counting the commit's other pills.
#[cfg(test)]
#[path = "pills_tests.rs"]
mod tests;
