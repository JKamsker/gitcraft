//! Balanced weighted treemap layout.

use super::*;

/// Balanced recursive slice treemap: every positive weight receives exactly one rectangle.
pub(super) fn partition(rect: Rect, weights: &[f64], start: usize, out: &mut Vec<(usize, Rect)>) {
    if weights.is_empty() {
        return;
    }
    if weights.len() == 1 {
        out.push((start, rect));
        return;
    }
    let total: f64 = weights.iter().sum();
    let mut left = weights[0];
    let mut split = 1;
    while split < weights.len() - 1 && left < total / 2.0 {
        left += weights[split];
        split += 1;
    }
    let fraction = (left / total) as f32;
    let (a, b) = if rect.width() >= rect.height() {
        let x = rect.left() + rect.width() * fraction;
        (
            Rect::from_min_max(rect.min, pos2(x, rect.bottom())),
            Rect::from_min_max(pos2(x, rect.top()), rect.max),
        )
    } else {
        let y = rect.top() + rect.height() * fraction;
        (
            Rect::from_min_max(rect.min, pos2(rect.right(), y)),
            Rect::from_min_max(pos2(rect.left(), y), rect.max),
        )
    };
    partition(a, &weights[..split], start, out);
    partition(b, &weights[split..], start + split, out);
}
