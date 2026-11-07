//! One palette row: the label with the letters typed in the accent colour, and on the right its
//! kind and the keys that do the same.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Align2, FontId, Sense, Stroke, Ui, pos2, vec2};

use crate::theme::{self, Tokens};

/// A row's height.
pub(super) const ROW_H: f32 = 28.0;

/// What a row shows.
pub(super) struct Row<'a> {
    pub label: &'a str,
    /// The characters of `label` (indices into its `chars()`) that matched, in order.
    pub matched: &'a [usize],
    pub hint: &'a str,
    pub shortcut: Option<&'a str>,
}

/// Draws a row; `scroll` brings the selected one into view. True when clicked.
pub(super) fn palette_row(ui: &mut Ui, t: &Tokens, row: &Row<'_>, selected: bool) -> bool {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    crate::repo::widgets::a11y_selected(&r, || crate::a11y::labels::join([row.label, row.hint]), selected);
    if !ui.is_rect_visible(rect) {
        return r.clicked();
    }
    if selected {
        ui.painter().rect_filled(rect, t.radius_sm, t.row_selected);
    } else if r.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let y = rect.center().y;
    let mut right = rect.right() - 10.0;
    if let Some(keys) = row.shortcut {
        right = keycap(
            ui,
            t,
            &keys.replace("Ctrl", crate::manual::command()),
            right,
            y,
        ) - 8.0;
    }
    if !row.hint.is_empty() {
        let hint = ui.painter().text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            row.hint,
            FontId::proportional(11.0),
            t.text_faint,
        );
        right = hint.left() - 12.0;
    }
    let left = rect.left() + 10.0;
    let job = highlighted(row.label, row.matched, t, (right - left).max(20.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter()
        .galley(pos2(left, y - galley.size().y / 2.0), galley, t.text);
    r.clicked()
}

/// `label` on one line no wider than `width` (cut with an ellipsis), the `matched` characters
/// in the accent colour and heavier.
fn highlighted(label: &str, matched: &[usize], t: &Tokens, width: f32) -> LayoutJob {
    let plain = TextFormat {
        font_id: FontId::proportional(13.0),
        color: t.text,
        ..Default::default()
    };
    let hit = TextFormat {
        font_id: theme::semibold(13.0),
        color: t.accent,
        ..Default::default()
    };
    let mut job = LayoutJob {
        wrap: TextWrapping::truncate_at_width(width),
        ..Default::default()
    };
    for (text, is_hit) in segments(label, matched) {
        let format = if is_hit { hit.clone() } else { plain.clone() };
        job.append(text, 0.0, format);
    }
    job
}

/// `label` cut into runs of matched and unmatched characters, in order.
fn segments<'a>(label: &'a str, matched: &[usize]) -> Vec<(&'a str, bool)> {
    let mut out = Vec::new();
    let mut hits = matched.iter().peekable();
    let mut start = 0;
    let mut run_hit = None;
    for (n, (byte, _)) in label.char_indices().enumerate() {
        while hits.peek().is_some_and(|&&h| h < n) {
            hits.next();
        }
        let hit = hits.peek().is_some_and(|&&h| h == n);
        match run_hit {
            Some(was) if was != hit => {
                out.push((&label[start..byte], was));
                start = byte;
            }
            _ => {}
        }
        run_hit = Some(hit);
    }
    if let Some(was) = run_hit {
        out.push((&label[start..], was));
    }
    out
}

/// Keys drawn like a keycap ending at `right`; returns where it starts.
fn keycap(ui: &Ui, t: &Tokens, keys: &str, right: f32, y: f32) -> f32 {
    let galley = ui
        .painter()
        .layout_no_wrap(keys.to_owned(), theme::mono(11.0), t.text_dim);
    let size = galley.size() + vec2(12.0, 4.0);
    let cap = egui::Rect::from_min_size(pos2(right - size.x, y - size.y / 2.0), size);
    let p = ui.painter();
    p.rect(
        cap,
        t.radius_sm,
        t.field,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
    p.galley(pos2(cap.left() + 6.0, cap.top() + 2.0), galley, t.text_dim);
    cap.left()
}

#[cfg(test)]
mod tests {
    use super::segments;

    #[test]
    fn matched_characters_are_cut_into_runs() {
        assert_eq!(
            segments("Rebase dev", &[0, 1, 2]),
            [("Reb", true), ("ase dev", false)]
        );
        assert_eq!(
            segments("Cherry-pick", &[0, 7]),
            [("C", true), ("herry-", false), ("p", true), ("ick", false)]
        );
        assert_eq!(segments("Fetch", &[]), [("Fetch", false)]);
        assert!(segments("", &[]).is_empty());
    }

    #[test]
    fn runs_cut_between_characters_never_inside_one() {
        assert_eq!(
            segments("Grüße ü", &[2, 3, 6]),
            [("Gr", false), ("üß", true), ("e ", false), ("ü", true)]
        );
        assert_eq!(segments("🚀 go", &[0]), [("🚀", true), (" go", false)]);
    }

    #[test]
    fn positions_past_the_end_are_ignored() {
        assert_eq!(segments("ab", &[1, 5, 9]), [("a", false), ("b", true)]);
    }
}
