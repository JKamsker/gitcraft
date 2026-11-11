//! Three-way staging lens and boundary actions.

use super::*;

pub(super) fn lens(
    ui: &mut egui::Ui,
    state: &State,
    lens: &model::LensData,
    t: &Tokens,
    idle: bool,
    acts: &mut Vec<Action>,
) {
    let snapshot = &lens.snapshot;
    ui.label(format!(
        "{} · HEAD {}",
        snapshot.spec.path,
        snapshot
            .head
            .as_deref()
            .map(|h| &h[..8])
            .unwrap_or_else(|| crate::i18n::text("repo-workflows-paint-unborn"))
    ));
    if let Some(previous) = &snapshot.spec.previous {
        ui.label(crate::i18n::message(
            "repo-workflows-file_comparison-lens-bfae8a90",
            &[("previous", (previous).clone().into())],
        ));
    }
    if lens.undoing_staged {
        ui.colored_label(
            t.warning,
            crate::i18n::text("repo-workflows-file_comparison-lens-4902e29c"),
        );
    }
    if let Some(w) = &snapshot.staging_warning {
        ui.colored_label(t.warning, w);
    }
    for (name, version) in [
        "HEAD",
        crate::i18n::text("repo-workflows-file_comparison-lens-daec459a"),
        crate::i18n::text("repo-workflows-file_comparison-lens-d1d29085"),
    ]
    .into_iter()
    .zip(&snapshot.versions)
    {
        ui.label(format!("{name}: {}", version.label()));
        if let Some(w) = &version.warning {
            ui.colored_label(t.warning, w);
        }
    }
    let allowed = idle
        && !state.loading
        && snapshot.staging_warning.is_none()
        && snapshot
            .versions
            .iter()
            .all(|v| v.warning.is_none() && v.mode.as_deref() != Some("160000"));
    ui.horizontal_wrapped(|ui| {
        for (i, label) in [
            (
                0,
                crate::i18n::text("repo-workflows-file_comparison-lens-6e60c4a1"),
            ),
            (
                1,
                crate::i18n::text("repo-workflows-file_comparison-lens-8f600488"),
            ),
        ] {
            let part = &lens.boundaries[i];
            let changed = snapshot.versions[i].bytes != snapshot.versions[i + 1].bytes
                || snapshot.versions[i].mode != snapshot.versions[i + 1].mode;
            let enabled = allowed && part.writable() && changed;
            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                acts.push(Action::ComparisonPick {
                    request: state.data_request,
                    boundary: i,
                    lines: Default::default(),
                    whole: true,
                });
            }
        }
    });
    for part in &lens.boundaries {
        if let Some(reason) = part.display_reason() {
            ui.colored_label(t.text_dim, reason);
        }
    }
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-file_comparison-lens-3a1781b9",
        ))
        .small()
        .color(t.text_dim),
    );
    let width = (ui.available_width() / 3. - 55.).max(180.);
    ui.horizontal(|ui| {
        ui.add_sized([width, 24.], egui::Label::new("HEAD"));
        ui.add_sized(
            [64., 24.],
            egui::Label::new(crate::i18n::text(
                "repo-workflows-file_comparison-lens-75673075",
            )),
        );
        ui.add_sized(
            [width, 24.],
            egui::Label::new(crate::i18n::text(
                "repo-workflows-file_comparison-lens-daec459a",
            )),
        );
        ui.add_sized(
            [64., 24.],
            egui::Label::new(crate::i18n::text(
                "repo-workflows-file_comparison-lens-92bc93d0",
            )),
        );
        ui.add_sized(
            [width, 24.],
            egui::Label::new(crate::i18n::text(
                "repo-workflows-file_comparison-lens-d1d29085",
            )),
        );
    });
    egui::ScrollArea::both()
        .id_salt("staging_lens_rows")
        .show_rows(ui, 24., lens.rows.len(), |ui, range| {
            ui.set_min_width(width * 3. + 150.);
            for i in range {
                let row = &lens.rows[i];
                ui.horizontal(|ui| {
                    line_cell(ui, &row.text[0], width, row.undoes, t);
                    boundary_button(ui, state, row, (i, 0), allowed, &lens.boundaries[0], acts);
                    line_cell(ui, &row.text[1], width, row.undoes, t);
                    boundary_button(ui, state, row, (i, 1), allowed, &lens.boundaries[1], acts);
                    line_cell(ui, &row.text[2], width, row.undoes, t);
                    if row.undoes {
                        ui.colored_label(
                            t.warning,
                            crate::i18n::text("repo-workflows-file_comparison-lens-b72fb6c1"),
                        );
                    }
                });
            }
        });
}
pub(super) fn boundary_button(
    ui: &mut egui::Ui,
    state: &State,
    row: &model::Row,
    position: (usize, usize),
    allowed: bool,
    part: &model::Boundary,
    acts: &mut Vec<Action>,
) {
    let (index, side) = position;
    let text = if side == 0 {
        crate::i18n::text("repo-workflows-file_comparison-boundary-button-033f86f9")
    } else {
        crate::i18n::text("repo-workflows-file_comparison-boundary-button-de838855")
    };
    let label = crate::i18n::message(
        "repo-workflows-file_comparison-boundary-button-0cadc525",
        &[("text", (text).into()), ("index-1-1", (index + 1).into())],
    );
    let enabled = allowed
        && part.reason.is_none()
        && !row.changes[side].is_empty()
        && (part.old_mode == part.new_mode || part.old_mode.is_none() || part.new_mode.is_none());
    let response = ui.add_enabled(enabled, egui::Button::new(text).min_size(vec2(64., 20.)));
    crate::a11y::a11y(&response, egui::WidgetType::Button, &label);
    if response.clicked() {
        acts.push(Action::ComparisonPick {
            request: state.data_request,
            boundary: side,
            lines: row.changes[side].clone(),
            whole: false,
        });
    }
}
pub(super) fn line_cell(
    ui: &mut egui::Ui,
    text: &Option<String>,
    width: f32,
    undoes: bool,
    t: &Tokens,
) {
    let value = text
        .as_deref()
        .map(|s| shorten(s.strip_suffix('\n').unwrap_or(s), 2000))
        .unwrap_or_else(|| "—".into());
    ui.add_sized(
        [width, 22.],
        egui::Label::new(RichText::new(value).monospace().color(if undoes {
            t.warning
        } else {
            t.text
        }))
        .truncate(),
    );
}
