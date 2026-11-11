//! Comparison of pull strategies.

use egui::{Align2, FontId, Rect, Stroke, Ui, pos2, vec2};

use super::{area, draw};
use crate::settings::Settings;
use crate::theme::Tokens;

use crate::repo::pull::PullMode;
/// A dot of the pull diagrams, its line to the one below, and its label.
struct Dot {
    lane: usize,
    label: &'static str,
    /// Drawn hollow: a commit made anew (a rebase's copy).
    new: bool,
    merge: bool,
}

/// Each way of pulling, side by side: your branch and its remote had both moved on; what the
/// branch looks like after.
pub(super) fn pull(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 10.0;
    let each = (width - 2.0 * gap) / 3.0;
    let h = 140.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let dot = |lane, label, new, merge| Dot {
        lane,
        label,
        new,
        merge,
    };
    for (i, mode) in PullMode::ALL.into_iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + i as f32 * (each + gap), rect.top()),
            vec2(each, h),
        );
        p.rect_filled(r, t.radius, t.dock);
        let (name, rows): (&str, Vec<Dot>) = match mode {
            PullMode::Merge => (
                crate::i18n::text("interaction-preview-merge"),
                vec![
                    dot(
                        0,
                        crate::i18n::text("interaction-preview-merge-dot"),
                        false,
                        true,
                    ),
                    dot(
                        1,
                        crate::i18n::text("interaction-preview-theirs"),
                        false,
                        false,
                    ),
                    dot(
                        0,
                        crate::i18n::text("interaction-preview-yours"),
                        false,
                        false,
                    ),
                    dot(0, "", false, false),
                ],
            ),
            PullMode::FastForwardOnly => (
                crate::i18n::text("interaction-preview-ff"),
                vec![
                    dot(
                        1,
                        crate::i18n::text("interaction-preview-theirs"),
                        false,
                        false,
                    ),
                    dot(
                        0,
                        crate::i18n::text("interaction-preview-yours"),
                        false,
                        false,
                    ),
                    dot(0, "", false, false),
                ],
            ),
            PullMode::Rebase => (
                crate::i18n::text("interaction-preview-rebase"),
                vec![
                    dot(
                        0,
                        crate::i18n::text("interaction-preview-yours-new"),
                        true,
                        false,
                    ),
                    dot(
                        0,
                        crate::i18n::text("interaction-preview-theirs"),
                        false,
                        false,
                    ),
                    dot(0, "", false, false),
                ],
            ),
        };
        let pitch = 28.0;
        let x0 = r.left() + 18.0;
        let lane_x = |lane: usize| x0 + lane as f32 * 16.0;
        let y = |row: usize| r.top() + 22.0 + row as f32 * pitch;
        let last = rows.len() - 1;
        // Lines first: each dot to the one below in its lane, and the forks.
        for (row, d) in rows.iter().enumerate().take(last) {
            let below = rows[row + 1..]
                .iter()
                .position(|b| b.lane == d.lane)
                .map(|k| row + 1 + k)
                .unwrap_or(last);
            let color = t.lane(d.lane);
            if rows[below].lane == d.lane {
                p.line_segment(
                    [pos2(lane_x(d.lane), y(row)), pos2(lane_x(d.lane), y(below))],
                    Stroke::new(2.0, color),
                );
            }
            if d.lane == 1 {
                // A remote's commit forks off the shared one at the bottom.
                crate::repo::graph_view::edge(
                    &p,
                    pos2(lane_x(1), y(row)),
                    pos2(lane_x(0), y(last)),
                    color,
                    2.0,
                );
            }
            if d.merge {
                crate::repo::graph_view::edge(
                    &p,
                    pos2(lane_x(0), y(row)),
                    pos2(lane_x(1), y(row + 1)),
                    t.lane(1),
                    2.0,
                );
            }
        }
        for (row, d) in rows.iter().enumerate() {
            let c = pos2(lane_x(d.lane), y(row));
            let color = t.lane(d.lane);
            if d.new {
                p.circle_filled(c, 5.5, t.dock);
                p.circle_stroke(c, 5.0, Stroke::new(2.0, color));
            } else {
                p.circle_filled(c, if d.merge { 3.5 } else { 5.5 }, color);
            }
            draw::text(
                &p,
                pos2(lane_x(2) + 4.0, c.y),
                Align2::LEFT_CENTER,
                d.label,
                FontId::proportional(11.0),
                t.text_dim,
                r.right() - lane_x(2) - 8.0,
            );
        }
        // Where the branch ends up, or why it doesn't move.
        let note_y = r.bottom() - 14.0;
        let (note, color) = match mode {
            PullMode::Merge => (
                crate::i18n::text("interaction-preview-merge-note"),
                t.text_faint,
            ),
            PullMode::FastForwardOnly => {
                (crate::i18n::text("interaction-preview-ff-note"), t.warning)
            }
            PullMode::Rebase => (
                crate::i18n::text("interaction-preview-rebase-note"),
                t.text_faint,
            ),
        };
        draw::text(
            &p,
            pos2(r.left() + 8.0, note_y),
            Align2::LEFT_CENTER,
            note,
            FontId::proportional(10.5),
            color,
            r.width() - 14.0,
        );
        draw::choice(&p, t, r, name, s.pull_mode == mode);
    }
    ui.add_space(4.0);
    super::super::layout::note(ui, crate::i18n::text("interaction-preview-ff-all"));
}
