//! Branch comparison matrix presentation.

use super::*;

pub(super) fn matrix(
    ui: &mut egui::Ui,
    state: &State,
    data: &files::Matrix,
    t: &Tokens,
    acts: &mut Vec<Action>,
) {
    use std::collections::{BTreeMap, BTreeSet};
    let ids: BTreeSet<_> = data
        .columns
        .iter()
        .flat_map(|c| c.files.values().map(|f| f.oid.clone()))
        .collect();
    let markers: BTreeMap<_, _> = ids
        .into_iter()
        .enumerate()
        .map(|(i, id)| (id, format!("V{}", i + 1)))
        .collect();
    ui.label(crate::i18n::message(
        "repo-workflows-file_comparison-matrix-ebfcc1d5",
        &[("data-paths-len-1", (data.paths.len()).into())],
    ));
    egui::ScrollArea::both()
        .id_salt("branch_matrix")
        .max_height(350.)
        .show(ui, |ui| {
            ui.set_min_width(220. + 150. * data.columns.len() as f32);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [220., 30.],
                    egui::Label::new(crate::i18n::text(
                        "repo-workflows-file_comparison-matrix-50009ce1",
                    )),
                );
                for c in &data.columns {
                    ui.add_sized(
                        [146., 30.],
                        egui::Label::new(format!("{}\n{}", c.label, &c.oid[..8])),
                    );
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("matrix_rows")
                .max_height(290.)
                .show_rows(ui, 24., data.paths.len(), |ui, range| {
                    for row in range {
                        let path = &data.paths[row];
                        let modes: BTreeSet<_> = data
                            .columns
                            .iter()
                            .filter_map(|c| c.files.get(path).map(|e| e.mode.as_str()))
                            .collect();
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [220., 22.],
                                egui::Label::new(files::display_path(path)).truncate(),
                            );
                            for (column, c) in data.columns.iter().enumerate() {
                                let text = match c.files.get(path) {
                                    Some(e) => format!(
                                        "{} {}",
                                        markers.get(&e.oid).map(String::as_str).unwrap_or("?"),
                                        e.mode
                                    ),
                                    None => crate::i18n::text(
                                        "repo-workflows-file_comparison-matrix-6be36ca4",
                                    )
                                    .into(),
                                };
                                let label =
                                    format!("{} | {} | {text}", files::display_path(path), c.label);
                                let selected = state.cells.contains(&(path.clone(), column));
                                let response =
                                    ui.add_sized(
                                        [146., 22.],
                                        egui::Button::new(RichText::new(&text).color(
                                            if modes.len() > 1 { t.warning } else { t.text },
                                        ))
                                        .fill(
                                            if selected {
                                                t.accent.gamma_multiply(0.3)
                                            } else {
                                                t.card
                                            },
                                        ),
                                    );
                                crate::a11y::a11y(&response, egui::WidgetType::Button, &label);
                                let clicked = response.clicked();
                                response.on_hover_text(crate::i18n::message(
                                    "repo-workflows-file_comparison-matrix-c81d97c3",
                                    &[
                                        ("label", (label).clone().into()),
                                        ("c-oid-1", (c.oid).clone().into()),
                                    ],
                                ));
                                if clicked {
                                    acts.push(Action::ComparisonCell {
                                        request: state.data_request,
                                        path: path.clone(),
                                        column,
                                    });
                                }
                            }
                        });
                    }
                });
        });
}
