//! The refs column of a row: its branch and tag pills and the `+N` stack of the others.

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
        let refs_clip = Rect::from_min_max(rect.min, pos2(gx - 4.0, rect.bottom()));
        let mut rp = painter.with_clip_rect(refs_clip);
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
        self.pill_clicks(ui, &resp, pill, first, &row.commit.hash, acts);
        let hit = PillHit::of(first, pill, &row.commit.hash);
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
                "Pull request tip; double-click its entry in the left pane to open details"
            } else if first.kind == BadgeKind::Branch {
                "Double-click a pill to check it out, or drag it onto a branch or a commit"
            } else {
                "Double-click a pill to check it out; drag the row to cherry-pick or move the commit"
            };
            let resp = resp.on_hover_text(format!(
                "{}\nTip commit by {} · {}\n{drag}",
                lines.join("\n"),
                row.commit.author,
                row.commit.date
            ));
            return (resp, Some(hit));
        }
        (resp, Some(hit))
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
            _ if b.kind == BadgeKind::PullRequest => "pull request tip",
            _ if b.head => "checked out",
            Some(Action::Pull) => "double-click to pull",
            _ => "double-click to check out",
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
        egui::Popup::context_menu(&resp)
            .style(crate::theme::menu_style)
            .show(|ui| self.commit_menu_items(ui, i, branch, self.busy_op.is_none(), acts));
    }
}

/// The `+N` after a row's pill, counting the commit's other pills.
fn more_count(painter: &Painter, t: &Tokens, r: Rect, more: usize, lane: Color32) {
    painter.rect_filled(
        r,
        CornerRadius::same(t.radius as u8),
        theme::palette::white_text_fill(
            lane.lerp_to_gamma(Color32::BLACK, 0.35)
                .lerp_to_gamma(Color32::BLACK, DIMMED),
        ),
    );
    painter.text(
        r.center(),
        Align2::CENTER_CENTER,
        format!("+{more}"),
        theme::medium(10.5),
        Color32::WHITE,
    );
}

/// What a pill carries besides its name and where the branch lives.
#[derive(Clone, Copy, Default)]
struct Marks {
    /// The branch has an open pull request.
    pr: bool,
    /// The build status of its tip.
    ci: Option<CheckState>,
}

/// A ghost run's marks on a pill: a dashed outline where it would go, a line through one it
/// would delete.
fn ghost_pill_marks(p: &Painter, t: &Tokens, pill: Rect, moved: bool, gone: bool) {
    if moved {
        let r = pill.expand(2.0);
        let corners = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        p.add(egui::Shape::dashed_line(
            &corners,
            Stroke::new(1.2, t.text),
            3.0,
            2.0,
        ));
    }
    if gone {
        let ends = [
            pos2(pill.left() + 2.0, pill.center().y),
            pos2(pill.right() - 2.0, pill.center().y),
        ];
        // A halo of the background under the line keeps it apart from the pill's colour.
        p.line_segment(ends, Stroke::new(4.0, t.dock));
        p.line_segment(ends, Stroke::new(2.0, t.danger));
    }
}

fn pill_fill(t: &Tokens, badge: &Badge, lane: Color32) -> Color32 {
    let fill = match badge.kind {
        BadgeKind::Branch if badge.local => lane,
        // Only on a remote: a darker shade of the lane.
        BadgeKind::Branch | BadgeKind::PullRequest => lane.lerp_to_gamma(Color32::BLACK, 0.35),
        BadgeKind::Tag => Color32::from_rgb(104, 104, 112),
        BadgeKind::DetachedHead => t.warning,
    };
    // Every pill but the checked-out one is a darker shade of its colour.
    let fill = if badge.head {
        fill
    } else {
        fill.lerp_to_gamma(Color32::BLACK, DIMMED)
    };
    theme::palette::white_text_fill(fill)
}

/// Paints a pill with white text and icons, at most `max_w` wide; returns its rect.
fn ref_pill(
    ui: &Ui,
    painter: &Painter,
    left_center: Pos2,
    badge: &Badge,
    lane: Color32,
    max_w: f32,
    marks: Marks,
) -> Rect {
    const ICON: f32 = 15.0;
    let t = Tokens::get(ui.ctx());
    let white = Color32::WHITE;
    let fill = pill_fill(&t, badge, lane);
    let (mut lead, mut trail) = pill_icons(badge, marks.pr);
    let font = if badge.head {
        theme::semibold(11.0)
    } else {
        theme::medium(11.0)
    };
    // At the column's minimum, the branch name takes precedence over its decorations. Keep
    // room for at least a short name such as `dev`; the tooltip still describes every mark.
    let name_w = painter
        .layout_no_wrap(badge.label.clone(), font.clone(), white)
        .size()
        .x;
    let short_w = painter
        .layout_no_wrap("dev".to_owned(), font.clone(), white)
        .size()
        .x;
    let icon_room = (max_w - 16.0 - name_w.min(short_w)).max(0.0);
    let mut ci = marks.ci;
    let mut dot_w = if ci.is_some() { 12.0 } else { 0.0 };
    while (lead.len() + trail.len()) as f32 * ICON + dot_w > icon_room {
        if trail.pop().is_some() {
            continue;
        }
        if ci.take().is_some() {
            dot_w = 0.0;
            continue;
        }
        if lead.pop().is_none() {
            break;
        }
    }
    let icons_w = (lead.len() + trail.len()) as f32 * ICON + dot_w;
    let text_room = (max_w - 16.0 - icons_w).max(16.0);
    let keep_end = crate::settings::Settings::branch_names_keep_end(ui.ctx());
    let label = truncate(painter, &badge.label, &font, text_room, keep_end);
    let galley = painter.layout_no_wrap(label, font, white);
    let w = (galley.size().x + 16.0 + icons_w).min(max_w);
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - 10.0), vec2(w, 20.0));
    painter.rect_filled(r, CornerRadius::same(t.radius as u8), fill);
    // Small manual widths can leave too little room for all marks. Text and icons must
    // stay inside the pill's actual hitbox; SVG images use the Ui's clip, not this painter's.
    let painter = &painter.with_clip_rect(painter.clip_rect().intersect(r));
    let mut x = r.left() + 7.0;
    for icon in lead {
        let icon_rect = Rect::from_center_size(pos2(x + 6.0, r.center().y), vec2(12.0, 12.0));
        if painter.clip_rect().contains_rect(icon_rect) {
            icons::paint(ui, icon_rect, icon, 11.0, white);
        }
        x += ICON;
    }
    let gw = galley.size().x;
    painter.galley(pos2(x, r.center().y - galley.size().y / 2.0), galley, white);
    x += gw + 1.0;
    for icon in trail {
        let icon_rect = Rect::from_center_size(pos2(x + 8.0, r.center().y), vec2(12.0, 12.0));
        if painter.clip_rect().contains_rect(icon_rect) {
            icons::paint(ui, icon_rect, icon, 11.0, white);
        }
        x += ICON;
    }
    if let Some(state) = ci {
        // On a ring of the pill's colour, so it reads on any lane.
        let center = pos2((x + 7.0).min(r.right() - 7.0), r.center().y);
        painter.circle_filled(center, ci_status::DOT_R + 1.5, Color32::WHITE);
        ci_status::paint_dot(painter, &t, center, ci_status::DOT_R, state);
    }
    r
}

/// The icons before and after a pill's label; `pr`: the branch has a pull request.
fn pill_icons(badge: &Badge, pr: bool) -> (Vec<&'static str>, Vec<&'static str>) {
    let mut lead = Vec::new();
    let mut trail = Vec::new();
    match badge.kind {
        BadgeKind::Tag => lead.push("tag"),
        BadgeKind::DetachedHead => lead.push("check"),
        BadgeKind::PullRequest => trail.push("git-pull-request"),
        BadgeKind::Branch => {
            if badge.head {
                lead.push("check");
            }
            // Where the branch lives: this computer, a remote, or both.
            if badge.local {
                trail.push("laptop");
            }
            if !badge.remotes.is_empty() {
                trail.push("cloud");
            }
            if pr {
                trail.push("git-pull-request");
            }
        }
    }
    (lead, trail)
}

/// `text`, cut with an ellipsis to fit `width`.
fn truncate(painter: &Painter, text: &str, font: &FontId, width: f32, keep_end: bool) -> String {
    let fits = |s: &str| {
        painter
            .layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
            <= width
    };
    crate::settings::fit_branch_name(text, keep_end, fits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{Branch, Refs};

    fn tab(refs: Refs) -> RepoTab {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pills-test".into(), &ctx);
        tab.refs = refs;
        tab.rebuild_graph_labels();
        tab
    }

    fn remote_pill(label: &str, remote: &str) -> Badge {
        Badge {
            label: label.to_owned(),
            kind: BadgeKind::Branch,
            local: false,
            remotes: vec![remote.to_owned()],
            head: false,
        }
    }

    #[test]
    fn white_pill_labels_meet_aa_for_every_builtin_and_lane_setting() {
        use crate::theme::{LaneColors, Tweaks, catalog::BUILTIN, palette::contrast};
        for builtin in BUILTIN {
            for lanes in LaneColors::ALL {
                let mut t = builtin.palette().tokens();
                Tweaks {
                    lanes,
                    ..Tweaks::default()
                }
                .apply(&mut t);
                for lane in t.lanes {
                    for head in [false, true] {
                        for (kind, local) in [
                            (BadgeKind::Branch, true),
                            (BadgeKind::Branch, false),
                            (BadgeKind::Tag, false),
                            (BadgeKind::DetachedHead, false),
                        ] {
                            let badge = Badge {
                                label: "test".to_owned(),
                                kind,
                                local,
                                remotes: vec![],
                                head,
                            };
                            assert!(
                                contrast(Color32::WHITE, pill_fill(&t, &badge, lane)) >= 4.5,
                                "{}: {:?}, head={head}, lanes={lanes:?}",
                                builtin.name,
                                badge.kind
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn minimum_column_keeps_dev_and_the_extra_refs_count_visible() {
        let mut tab = tab(Refs {
            current: Some("dev".to_owned()),
            ..Refs::default()
        });
        tab.prs.push(crate::forge::PullRequest {
            number: 1,
            title: "Dev changes".to_owned(),
            branch: "dev".to_owned(),
            base: "main".to_owned(),
            from_fork: false,
            sha: "a".to_owned(),
            author: String::new(),
            url: String::new(),
            draft: false,
        });
        tab.ci.by_sha.insert(
            "a".to_owned(),
            crate::ci::Status {
                state: CheckState::Passed,
                checks: Vec::new(),
            },
        );
        let commit = crate::git::Commit {
            hash: "a".to_owned(),
            refs: ["HEAD -> dev", "refs/remotes/origin/dev", "tag: v1"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            ..crate::git::Commit::default()
        };
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let row = Row {
                i: 0,
                commit: &commit,
                rect: Rect::from_min_size(Pos2::ZERO, vec2(600.0, 32.0)),
                cols: Cols {
                    refs: crate::repo::columns::MIN_REFS_W,
                    graph: 60.0,
                    hash: 0.0,
                    author: 0.0,
                    when: 0.0,
                    date: 0.0,
                    push: 0.0,
                },
                gx: crate::repo::columns::MIN_REFS_W,
                lane_w: 20.0,
                node_x: Some(90.0),
                color: Color32::BLUE,
            };
            let resp = ui.interact(row.rect, ui.id().with("narrow-refs"), Sense::click());
            let (_, hit) = tab.refs_cell(ui, ui.painter(), &row, resp, &mut Vec::new());
            assert!(hit.is_some_and(|hit| hit.rect.right() <= row.gx - 4.0));
        });
        let mut names = Vec::new();
        for shape in &output.shapes {
            if let egui::epaint::Shape::Text(text) = &shape.shape {
                let label = text.galley.text();
                if label == "dev" || label == "+1" {
                    assert!(text.pos.x >= shape.clip_rect.left());
                    assert!(text.pos.x + text.galley.size().x <= shape.clip_rect.right());
                    names.push(label);
                }
            }
        }
        assert!(
            names.contains(&"dev"),
            "the short branch name must stay whole"
        );
        assert!(
            names.contains(&"+1"),
            "the extra refs count must stay reachable"
        );
        output.textures_delta.clear();
    }

    #[test]
    fn only_the_tracked_remote_pill_of_the_checked_out_branch_pulls() {
        let main = Branch {
            name: "main".to_owned(),
            target: "a".to_owned(),
            is_head: true,
            upstream: "origin/main".to_owned(),
            track: "[behind 2]".to_owned(),
        };
        let tab = tab(Refs {
            local: vec![main],
            current: Some("main".to_owned()),
            head: "a".to_owned(),
            ..Refs::default()
        });
        assert!(matches!(
            tab.pill_checkout(&remote_pill("main", "origin")),
            Some(Action::Pull)
        ));
        // `git pull` would bring in origin/main, not fork/main.
        assert!(tab.pill_checkout(&remote_pill("main", "fork")).is_none());
    }
}
