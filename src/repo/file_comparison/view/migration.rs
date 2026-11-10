//! Migration ribbon presentation.

use super::*;

pub(super) fn migration(
    ui: &mut egui::Ui,
    state: &State,
    data: &files::Migration,
    t: &Tokens,
    acts: &mut Vec<Action>,
) {
    ui.label(&data.label);
    ui.label(crate::i18n::message(
        "repo-workflows-file_comparison-migration-41aec147",
        &[
            ("data-moves-len-1", (data.moves.len()).into()),
            ("data-other-changes-2", (data.other_changes).into()),
        ],
    ));
    if let Some(warning) = &data.warning {
        ui.colored_label(
            t.warning,
            crate::i18n::message(
                "repo-workflows-file_comparison-migration-35255dde",
                &[("warning", (warning).clone().into())],
            ),
        );
    }
    if data.moves.is_empty() {
        ui.label(crate::i18n::text(
            "repo-workflows-file_comparison-migration-a419e1f7",
        ));
        return;
    }
    egui::ScrollArea::both()
        .id_salt("migration_ribbons")
        .max_height(330.)
        .show_rows(ui, 58., data.moves.len(), |ui, range| {
            ui.set_min_width(600.);
            for i in range {
                let m = &data.moves[i];
                let label = crate::i18n::message(
                    "repo-workflows-file_comparison-migration-208e67e7",
                    &[
                        ("m-before-name-1", (m.before.name()).clone().into()),
                        ("m-after-name-2", (m.after.name()).clone().into()),
                        (
                            "if-m-copy-copy-else-rename-3",
                            (if m.copy {
                                crate::i18n::text(
                                    "repo-workflows-file_comparison-migration-e21f935f",
                                )
                            } else {
                                crate::i18n::text(
                                    "repo-workflows-file_comparison-migration-3064d79a",
                                )
                            })
                            .into(),
                        ),
                        (
                            "m-similarity-map-s-format-s-unwr-4",
                            (m.similarity.map(|s| format!("{s}%")).unwrap_or_else(|| {
                                crate::i18n::text(
                                    "repo-workflows-file_comparison-migration-06869a99",
                                )
                                .into()
                            }))
                            .clone()
                            .into(),
                        ),
                    ],
                );
                let (rect, response) = ui
                    .allocate_exact_size(vec2(ui.available_width().max(600.), 54.), Sense::click());
                crate::a11y::a11y(&response, egui::WidgetType::Button, &label);
                let p = ui.painter();
                p.rect_filled(
                    rect,
                    4.,
                    if response.hovered() {
                        t.pressed
                    } else {
                        t.card
                    },
                );
                let left = egui::pos2(rect.left() + 170., rect.center().y);
                let right = egui::pos2(rect.right() - 170., rect.center().y);
                p.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    [left, left + vec2(90., -18.), right - vec2(90., -18.), right],
                    false,
                    egui::Color32::TRANSPARENT,
                    Stroke::new(if m.copy { 3. } else { 5. }, t.accent.gamma_multiply(0.65)),
                ));
                let folder = |path: &[u8]| {
                    let at = path.iter().rposition(|b| *b == b'/');
                    at.map(|n| files::display_path(&path[..n]))
                        .unwrap_or_else(|| "(root)".into())
                };
                p.text(
                    rect.left_top() + vec2(8., 5.),
                    egui::Align2::LEFT_TOP,
                    shorten(&folder(&m.before.path), 25),
                    egui::FontId::proportional(12.),
                    t.text,
                );
                p.text(
                    rect.right_top() + vec2(-8., 5.),
                    egui::Align2::RIGHT_TOP,
                    shorten(&folder(&m.after.path), 25),
                    egui::FontId::proportional(12.),
                    t.text,
                );
                p.text(
                    rect.center_top() + vec2(0., 5.),
                    egui::Align2::CENTER_TOP,
                    format!(
                        "{} · {}",
                        if m.copy {
                            crate::i18n::text("repo-workflows-file_comparison-migration-e21f935f")
                        } else {
                            crate::i18n::text("repo-workflows-file_comparison-migration-3064d79a")
                        },
                        m.similarity
                            .map(|s| format!("{s}%"))
                            .unwrap_or_else(
                                || crate::i18n::text("repo-workflows-r1-unknown").into()
                            )
                    ),
                    egui::FontId::proportional(12.),
                    t.text,
                );
                p.text(
                    rect.center_bottom() - vec2(0., 3.),
                    egui::Align2::CENTER_BOTTOM,
                    shorten(&format!("{} → {}", m.before.name(), m.after.name()), 90),
                    egui::FontId::monospace(11.),
                    t.text_dim,
                );
                let clicked = response.clicked();
                response.on_hover_text(&label);
                if clicked {
                    acts.push(Action::ComparisonMove {
                        request: state.data_request,
                        index: i,
                    });
                }
            }
        });
}
