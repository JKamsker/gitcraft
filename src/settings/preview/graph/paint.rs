//! Paint the sample history using the supplied preview column layout.

use super::super::draw;
use super::history::{HISTORY, RefKind, history_graph};
use super::{HEADER_H, Layout, Trailing, initials, mock_date, name_hash};
use crate::repo::graph_view::{ROW_GAP, ROW_PITCH, edge};
use crate::theme::{self, Tokens};
use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, pos2, vec2};

/// The header strip and `rows` rows of the history in `rect`, laid out by `l`.
pub(super) fn paint_graph(p: &Painter, t: &Tokens, rect: Rect, l: &Layout, rows: usize) {
    let g = history_graph();
    p.rect_filled(rect, 0.0, t.dock);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
    p.rect_filled(header, 0.0, t.chrome);
    p.line_segment(
        [header.left_bottom(), header.right_bottom()],
        Stroke::new(1.0, t.separator),
    );
    let font = theme::semibold(10.0);
    let right = rect.right();
    let desc_x = l.gx() + l.graph + 6.0;
    let mut labels = Vec::new();
    if l.refs > 0.0 {
        labels.push((
            l.left + 10.0,
            crate::i18n::text("interaction-preview-refs-header"),
        ));
    }
    labels.push((l.gx() + 8.0, crate::i18n::text("interaction-preview-graph")));
    labels.push((desc_x, crate::i18n::text("interaction-preview-description")));
    let mut x = right - l.trailing_w();
    for (name, w, _) in &l.trailing {
        labels.push((x, name));
        x += w;
    }
    for (i, (x, label)) in labels.iter().enumerate() {
        let limit = labels.get(i + 1).map_or(right, |(next, _)| next - 6.0);
        draw::text(
            p,
            pos2(*x, header.center().y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            t.text_faint,
            limit - x,
        );
    }

    for (i, (m, row)) in HISTORY.iter().zip(g.rows.iter()).take(rows).enumerate() {
        let top = header.bottom() + i as f32 * ROW_PITCH;
        let r = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), ROW_PITCH));
        if i == 0 {
            p.rect_filled(r.shrink2(vec2(0.0, ROW_GAP / 2.0)), 0.0, t.row_selected);
        }
        let mid = r.center().y;
        let color = t.lane(row.col);
        let node = pos2(l.lane_x(row.col), mid);

        // Pills, joined to the dot.
        if let Some((first, kind)) = m.refs.first()
            && l.refs > 0.0
        {
            let more = m.refs.len() - 1;
            let more_w = if more > 0 { 22.0 } else { 0.0 };
            let max_w = (l.refs_true - 10.0 - more_w).max(16.0);
            let clip = p.with_clip_rect(Rect::from_min_max(
                pos2(l.left, r.top()),
                pos2(l.gx() - 4.0, r.bottom()),
            ));
            let fill = match kind {
                RefKind::Head | RefKind::Local => color,
                RefKind::Remote => color.lerp_to_gamma(Color32::BLACK, 0.35),
                RefKind::Tag => Color32::from_rgb(104, 104, 112),
            };
            let pill = draw::pill(
                &clip,
                t,
                pos2(l.left + 6.0, mid),
                draw::PillLabel {
                    text: first,
                    keep_end: l.keep_branch_end,
                },
                fill,
                *kind == RefKind::Head,
                max_w,
            );
            let mut end = pill.right();
            if more > 0 {
                let m = Rect::from_min_size(pos2(end + 3.0, mid - 10.0), vec2(more_w - 3.0, 20.0));
                clip.rect_filled(m, t.radius, color.gamma_multiply(0.25));
                clip.text(
                    m.center(),
                    Align2::CENTER_CENTER,
                    format!("+{more}"),
                    theme::medium(10.5),
                    t.text,
                );
                end = m.right();
            }
            let end = end.min(l.gx() - 4.0);
            p.line_segment(
                [pos2(end, mid), pos2(node.x, mid)],
                Stroke::new(1.5, color.gamma_multiply(0.8)),
            );
        }

        // Lanes and the dot.
        let gp = p.with_clip_rect(Rect::from_min_max(
            pos2(l.gx(), r.top()),
            pos2(l.gx() + l.graph, r.bottom()),
        ));
        for &lane in row.through.iter() {
            gp.line_segment(
                [
                    pos2(l.lane_x(lane), r.top()),
                    pos2(l.lane_x(lane), r.bottom()),
                ],
                Stroke::new(2.0, t.lane(lane)),
            );
        }
        for &lane in row.incoming.iter() {
            edge(&gp, node, pos2(l.lane_x(lane), r.top()), t.lane(lane), 2.0);
        }
        // The last row drawn: its lines run on, as into older history.
        for &lane in row.outgoing.iter() {
            edge(
                &gp,
                node,
                pos2(l.lane_x(lane), r.bottom()),
                t.lane(lane),
                2.0,
            );
        }
        if m.parents.len() > 1 {
            gp.circle_filled(node, 3.5, color);
        } else {
            gp.circle_filled(node, 6.0, color);
            if i == 0 {
                gp.circle_filled(node, 2.5, t.dock);
            }
        }

        // The description, then the trailing columns.
        let trailing_left = right - l.trailing_w();
        draw::text(
            p,
            pos2(desc_x + 2.0, mid),
            Align2::LEFT_CENTER,
            crate::i18n::text(m.summary),
            FontId::proportional(12.5),
            t.text,
            trailing_left - desc_x - 12.0,
        );
        let mut x = trailing_left;
        let small = FontId::proportional(11.5);
        for (_, w, what) in &l.trailing {
            let room = w - 8.0;
            match what {
                Trailing::Hash => {
                    draw::text(
                        p,
                        pos2(x, mid),
                        Align2::LEFT_CENTER,
                        m.hash7,
                        theme::mono(11.0),
                        t.text_faint,
                        room,
                    );
                }
                Trailing::Author => {
                    let c = pos2(x + 10.0, mid);
                    p.circle_filled(c, 10.0, t.lane(name_hash(m.author)));
                    p.text(
                        c,
                        Align2::CENTER_CENTER,
                        initials(m.author),
                        theme::semibold(8.0),
                        Color32::WHITE,
                    );
                    draw::text(
                        p,
                        pos2(x + 26.0, mid),
                        Align2::LEFT_CENTER,
                        m.author,
                        small.clone(),
                        t.text_dim,
                        room - 26.0,
                    );
                }
                Trailing::When => {
                    draw::text(
                        p,
                        pos2(x, mid),
                        Align2::LEFT_CENTER,
                        crate::i18n::text(m.when),
                        small.clone(),
                        t.text_dim,
                        room,
                    );
                }
                Trailing::Date => {
                    draw::text(
                        p,
                        pos2(x, mid),
                        Align2::LEFT_CENTER,
                        &mock_date(m.date),
                        theme::mono(11.0),
                        t.text_faint,
                        room,
                    );
                }
            }
            x += w;
        }
    }
}
