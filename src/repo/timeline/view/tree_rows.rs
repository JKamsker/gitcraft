//! Timeline tree lane and row rendering.

use super::*;

impl RepoTab {
    /// The tree, a row per entry, newest first.
    pub(in crate::repo::timeline::view) fn tree_rows(
        &self,
        ui: &mut Ui,
        selected: Option<u64>,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let (rows, lanes) = tree::layout(&self.timeline);
        let done = self.timeline.path_to(self.timeline.current);
        let lines = ui.painter().add(egui::Shape::Noop);
        let now = crate::git::now();
        let mut centers: Vec<Pos2> = Vec::with_capacity(rows.len());
        let shown_lanes = lanes.min(MAX_LANES);
        let graph_w = shown_lanes as f32 * LANE_W + 8.0;
        for row in &rows {
            let entry = row.id.and_then(|id| self.timeline.get(id));
            let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("timeline_row", row.id));
            if selected == row.id {
                ui.painter().rect_filled(rect, 0.0, t.row_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, t.hover);
            }
            let center = pos2(
                rect.left() + 8.0 + row.lane.min(MAX_LANES - 1) as f32 * LANE_W + LANE_W / 2.0,
                rect.center().y,
            );
            centers.push(center);
            let is_current = row.id == self.timeline.current;
            let on_path = row.id.is_none_or(|id| done.contains(&id));
            let color = t.lane(row.lane);
            if is_current {
                ui.painter().circle_filled(center, 5.5, color);
                ui.painter()
                    .circle_stroke(center, 8.0, Stroke::new(1.5, color));
            } else if on_path {
                ui.painter().circle_filled(center, 4.5, color);
            } else {
                ui.painter().circle_filled(center, 4.5, t.card);
                ui.painter()
                    .circle_stroke(center, 4.5, Stroke::new(1.5, color));
            }
            let text_x = rect.left() + 8.0 + graph_w;
            let (label, faint) = match entry {
                Some(e) => (crate::repo::messages::display_operation(&e.label), !on_path),
                None => (
                    crate::i18n::text("repo-workflows-timeline-tree-rows-6fa556f4").to_owned(),
                    false,
                ),
            };
            let label_color = if faint { t.text_faint } else { t.text };
            let when = entry.map(|e| crate::git::relative_time(e.time, now));
            let right = rect.right() - 8.0;
            let mut text_right = right;
            if let Some(when) = &when {
                let galley = ui.painter().layout_no_wrap(
                    when.clone(),
                    egui::FontId::proportional(11.0),
                    t.text_faint,
                );
                text_right -= galley.size().x + 8.0;
                ui.painter().galley(
                    pos2(
                        right - galley.size().x,
                        rect.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    t.text_faint,
                );
            }
            if entry.is_some_and(|e| e.origin == Origin::Outside) {
                let galley = ui.painter().layout_no_wrap(
                    crate::i18n::text("repo-workflows-r1-elsewhere").to_owned(),
                    theme::medium(10.5),
                    t.warning,
                );
                text_right -= galley.size().x + 8.0;
                ui.painter().galley(
                    pos2(text_right + 4.0, rect.center().y - galley.size().y / 2.0),
                    galley,
                    t.warning,
                );
            }
            let galley = ui.painter().layout(
                label,
                egui::FontId::proportional(12.5),
                label_color,
                (text_right - text_x).max(20.0),
            );
            let clipped = ui.painter().with_clip_rect(egui::Rect::from_min_max(
                pos2(text_x, rect.top()),
                pos2(text_right, rect.bottom()),
            ));
            clipped.galley(pos2(text_x, rect.center().y - 8.0), galley, label_color);
            let resp = resp.on_hover_text(match entry {
                Some(e) => format!(
                    "{}\n{}",
                    crate::repo::messages::display_operation(&e.label),
                    if e.origin == Origin::Outside {
                        crate::i18n::text("repo-workflows-timeline-tree-rows-d5686d26")
                    } else {
                        crate::i18n::text("repo-workflows-timeline-tree-rows-601272f1")
                    }
                ),
                None => crate::i18n::text("repo-workflows-timeline-tree-rows-06b96bb5").to_owned(),
            });
            crate::repo::ghost::offer(&resp, || crate::repo::ghost::GhostOp::Timeline {
                state: self.timeline.state_at(&self.ref_state(), row.id),
                label: entry.map_or_else(|| "Before the oldest entry".into(), |e| e.label.clone()),
            });
            if resp.clicked() {
                acts.push(Action::Timeline(TimelineAct::Select(row.id)));
            }
            if resp.double_clicked() {
                acts.push(Action::Timeline(TimelineAct::AskJump(row.id)));
            }
        }
        // The lines, under the dots.
        let mut shapes = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let Some((parent_row, _)) = row.parent else {
                continue;
            };
            let (Some(from), Some(to)) = (centers.get(i), centers.get(parent_row)) else {
                continue;
            };
            let color = t.lane(row.lane).gamma_multiply(0.7);
            let bend = pos2(from.x, to.y - ROW_H / 2.0);
            shapes.push(egui::Shape::line_segment(
                [*from, bend],
                Stroke::new(1.5, color),
            ));
            shapes.push(egui::Shape::line_segment(
                [bend, *to],
                Stroke::new(1.5, color),
            ));
        }
        ui.painter().set(lines, egui::Shape::Vec(shapes));
    }
}
