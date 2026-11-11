use super::*;

/// The half of the side-by-side view a side is, for comments (`None`: unified).
pub(super) fn column_of(side: Side) -> Option<DiffSide> {
    match side {
        Side::Old => Some(DiffSide::Left),
        Side::New => Some(DiffSide::Right),
        Side::Unified | Side::Both => None,
    }
}

pub(super) fn gutter_width(side: Side) -> f32 {
    match side {
        Side::Unified => NUM_W * 2.0,
        Side::Old | Side::New => NUM_W,
        Side::Both => 0.0,
    }
}

/// The painted line's identity, change kind, and content are available without hovering.
pub(super) fn line_label(line: &super::super::DiffLine) -> String {
    let kind = match line.kind {
        Kind::Add => crate::i18n::text("repo-core-1702"),
        Kind::Del => crate::i18n::text("repo-core-2319"),
        Kind::Hunk => crate::i18n::text("repo-core-2320"),
        Kind::Context => crate::i18n::text("repo-core-2321"),
        Kind::Fold => "folded",
        Kind::Meta => "metadata",
    };
    crate::i18n::message(
        "repo-core-2322",
        &[
            ("kind", kind.to_string().into()),
            (
                "line",
                line.old
                    .map_or_else(
                        || crate::i18n::text("repo-core-2323").to_owned(),
                        |n| n.to_string(),
                    )
                    .to_string()
                    .into(),
            ),
            (
                "line2",
                line.new
                    .map_or_else(
                        || crate::i18n::text("repo-core-2323").to_owned(),
                        |n| n.to_string(),
                    )
                    .to_string()
                    .into(),
            ),
            ("text", line.text.to_string().into()),
        ],
    )
}

pub(super) fn map_keyboard(ui: &Ui, offset: f32, page: f32, total: f32) -> Option<f32> {
    use egui::Key;
    for (key, value) in [
        (Key::Home, 0.0),
        (Key::End, total),
        (Key::ArrowUp, offset - LINE_H),
        (Key::ArrowDown, offset + LINE_H),
        (Key::PageUp, offset - page),
        (Key::PageDown, offset + page),
    ] {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
            return Some(value.clamp(0.0, (total - page).max(0.0)));
        }
    }
    None
}
