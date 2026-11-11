//! Tab strip geometry, scrolling, placement, and fade painting.

use super::*;

pub(super) fn tabs_needed(titles: &[Title], active: usize) -> f32 {
    let active_w = titles.get(active).map_or(0.0, |t| t.width);
    let another = if titles.len() > 1 {
        GAP + MIN_TAB_W + LIST_W
    } else {
        0.0
    };
    active_w + another + PLUS_W
}

/// The width of a row of tabs of `widths`.
pub(super) fn row_width(widths: &[f32]) -> f32 {
    let gaps = widths.len().saturating_sub(1) as f32 * GAP;
    widths.iter().sum::<f32>() + gaps
}

/// Where tab `i` starts and ends along the row.
pub(super) fn tab_span(widths: &[f32], i: usize) -> Option<(f32, f32)> {
    let width = *widths.get(i)?;
    let left = widths.iter().take(i).map(|w| w + GAP).sum::<f32>();
    Some((left, left + width))
}

/// The scroll that brings `left..right` into a view of `room` from `offset`, moving as little
/// as it can: the start shows when it doesn't all fit.
pub(super) fn scroll_to_show(offset: f32, room: f32, left: f32, right: f32) -> f32 {
    if left < offset || right - left > room {
        left
    } else if right > offset + room {
        right - room
    } else {
        offset
    }
}

/// The tabs not wholly in a view of `room` scrolled to `offset`.
pub(super) fn out_of_view(widths: &[f32], offset: f32, room: f32) -> Vec<usize> {
    (0..widths.len())
        .filter(|&i| {
            tab_span(widths, i)
                .is_some_and(|(left, right)| left < offset - 0.5 || right > offset + room + 0.5)
        })
        .collect()
}

/// A band in `color` fading out across `rect`, toward its right end when `toward_right` is
/// false, so that what is under it fades into `color` at the other end.
pub(super) fn fade(painter: &egui::Painter, rect: Rect, color: Color32, toward_right: bool) {
    let (left, right) = if toward_right {
        (Color32::TRANSPARENT, color)
    } else {
        (color, Color32::TRANSPARENT)
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Each tab's left edge from `left` on, the dragged one (index, left edge) moved to the slot
/// its middle is over, and that slot.
pub(super) fn places(
    left: f32,
    n: usize,
    width: impl Fn(usize) -> f32,
    dragged: Option<(usize, f32)>,
) -> (Vec<f32>, Option<usize>) {
    let mut order: Vec<usize> = (0..n).collect();
    let mut slot = None;
    if let Some((from, x)) = dragged {
        let middle = x + width(from) / 2.0;
        let mut edge = left;
        let mut s = 0;
        for j in (0..n).filter(|&j| j != from) {
            if edge + width(j) / 2.0 < middle {
                s += 1;
            }
            edge += width(j) + GAP;
        }
        order.retain(|&j| j != from);
        order.insert(s, from);
        slot = Some(s);
    }
    let mut place = vec![left; n];
    let mut x = left;
    for &j in &order {
        if let Some(p) = place.get_mut(j) {
            *p = x;
        }
        x += width(j) + GAP;
    }
    (place, slot)
}
