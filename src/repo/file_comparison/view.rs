mod lens;
use lens::*;

mod matrix;
use matrix::*;

mod migration;
use migration::*;

use super::*;
use crate::theme::Tokens;
use egui::{RichText, Sense, Stroke, vec2};

impl RepoTab {
    pub(in crate::repo) fn file_comparison_view(
        &mut self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
    ) {
        let Some(state) = &mut self.file_comparison else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let title = match state.page {
            Page::Migration => {
                crate::i18n::text("repo-workflows-file_comparison-file-comparison-view-edc82098")
            }
            Page::CodeMoves => {
                crate::i18n::text("repo-workflows-file_comparison-file-comparison-view-2ad51b3c")
            }
            Page::Lens => {
                crate::i18n::text("repo-workflows-file_comparison-file-comparison-view-fb60b8e0")
            }
            Page::Matrix => {
                crate::i18n::text("repo-workflows-file_comparison-file-comparison-view-90e2e8fa")
            }
        };
        ui.horizontal_wrapped(|ui| {
            ui.heading(title);
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-3f64b850",
                ))
                .clicked()
            {
                acts.push(Action::RefreshFileComparison);
            }
            if state.loading
                && ui
                    .button(crate::i18n::text(
                        "repo-workflows-file_comparison-file-comparison-view-ca74fb37",
                    ))
                    .clicked()
            {
                state.generation.store(0, Ordering::Relaxed);
                state.request = 0;
                state.loading = false;
                state.pair = Some(Err("Comparison cancelled. Refresh to try again.".into()));
            }
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-7b8bea18",
                ))
                .clicked()
            {
                acts.push(Action::CloseFileComparison);
            }
        });
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-file_comparison-file-comparison-view-22d5cf54",
            ))
            .small()
            .color(t.text_dim),
        );
        match state.page {
            Page::CodeMoves => {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(
                        &mut state.source,
                        0,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-daec459a",
                        ),
                    );
                    ui.selectable_value(
                        &mut state.source,
                        3,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-d1d29085",
                        ),
                    );
                    ui.selectable_value(
                        &mut state.source,
                        1,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-82a9c46f",
                        ),
                    );
                    ui.selectable_value(
                        &mut state.source,
                        2,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-f2473f24",
                        ),
                    );
                    ui.selectable_value(
                        &mut state.source,
                        4,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-51d4ac7c",
                        ),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    if matches!(state.source, 2 | 4) {
                        ui.label(crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-21819769",
                        ));
                        ui.text_edit_singleline(&mut state.from);
                    }
                    if matches!(state.source, 1 | 2) {
                        ui.label(crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-89fa7940",
                        ));
                        ui.text_edit_singleline(&mut state.to);
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-dfc3916d",
                        ))
                        .clicked()
                    {
                        acts.push(Action::RefreshFileComparison);
                    }
                });
                ui.label(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-f08307c5",
                ));
            }
            Page::Migration => {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(
                        &mut state.source,
                        0,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-2d2cd49c",
                        ),
                    );
                    ui.selectable_value(
                        &mut state.source,
                        1,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-efa19808",
                        ),
                    );
                    ui.selectable_value(
                        &mut state.source,
                        2,
                        crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-f2473f24",
                        ),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    if state.source == 2 {
                        ui.label(crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-21819769",
                        ));
                        ui.add(egui::TextEdit::singleline(&mut state.from).desired_width(140.));
                    }
                    if state.source != 0 {
                        ui.label(if state.source == 1 {
                            crate::i18n::text(
                                "repo-workflows-file_comparison-file-comparison-view-82a9c46f",
                            )
                        } else {
                            crate::i18n::text(
                                "repo-workflows-file_comparison-file-comparison-view-f4b06ef6",
                            )
                        });
                        ui.add(egui::TextEdit::singleline(&mut state.to).desired_width(140.));
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-86154135",
                        ))
                        .clicked()
                    {
                        acts.push(Action::RefreshFileComparison);
                    }
                });
                ui.label(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-aa2b8150",
                ));
            }
            Page::Lens => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-file_comparison-file-comparison-view-50009ce1",
                    ));
                    if ui
                        .add(egui::TextEdit::singleline(&mut state.path).desired_width(260.))
                        .changed()
                    {
                        state.previous = None;
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-103acf25",
                        ))
                        .clicked()
                    {
                        acts.push(Action::RefreshFileComparison);
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-file_comparison-file-comparison-view-40379eb3",
                        ))
                        .clicked()
                    {
                        acts.push(Action::ComparisonNormalPatch);
                    }
                });
                ui.label(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-327070c2",
                ));
            }
            Page::Matrix => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::text(
                        "repo-workflows-file_comparison-file-comparison-view-74ccd433",
                    ));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.folder)
                            .hint_text(crate::i18n::text(
                                "repo-workflows-file_comparison-file-comparison-view-f315d745",
                            ))
                            .desired_width(230.),
                    );
                    if ui
                        .add_enabled(
                            (2..=8).contains(&state.selected.len()),
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-file_comparison-file-comparison-view-fdc19d89",
                            )),
                        )
                        .clicked()
                    {
                        acts.push(Action::RefreshFileComparison);
                    }
                });
                egui::CollapsingHeader::new(crate::i18n::message(
                    "repo-workflows-file_comparison-file-comparison-view-1e2e1383",
                    &[("state-selected-len-1", (state.selected.len()).into())],
                ))
                .id_salt("file-comparison-branches")
                .default_open(state.selected.len() < 2)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for branch in &state.branches {
                            let mut checked = state.selected.contains(branch);
                            if ui.checkbox(&mut checked, branch).changed() {
                                if checked {
                                    state.selected.insert(branch.clone());
                                } else {
                                    state.selected.remove(branch);
                                }
                            }
                        }
                    });
                });
                ui.label(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-04166b68",
                ));
            }
        }
        if state.loading {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new().size(16.));
                ui.label(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-5fa4365c",
                ));
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let data = state.data.as_ref().and_then(|r| r.as_ref().ok()).cloned();
        if let Some(Err(e)) = &state.data {
            ui.colored_label(t.danger, crate::repo::activity::display_workflow_error(e));
        }
        if let Some(data) = data {
            ui.separator();
            match data.as_ref() {
                Data::CodeMoves(m) => code_moves::view(ui, state, m, &t, acts),
                Data::Migration(m) => migration(ui, state, m, &t, acts),
                Data::Matrix(m) => matrix(ui, state, m, &t, acts),
                Data::Lens(l) => lens(ui, state, l, &t, self.busy_op.is_none(), acts),
            }
        }
        if let Some(pair) = &state.pair {
            ui.separator();
            match pair {
                Ok(p) => show_pair(ui, p, &t),
                Err(e) => {
                    ui.colored_label(t.danger, crate::repo::activity::display_workflow_error(e));
                }
            }
        }
    }
}

pub(super) fn show_pair(ui: &mut egui::Ui, pair: &model::Pair, t: &Tokens) {
    ui.heading(crate::i18n::text(
        "repo-workflows-file_comparison-show-pair-98154a15",
    ));
    ui.label(format!("{} → {}", pair.labels[0], pair.labels[1]));
    ui.label(crate::i18n::message(
        "repo-workflows-file_comparison-show-pair-e3ccb1fe",
        &[
            (
                "pair-versions-0-label-1",
                (pair.versions[0].label()).clone().into(),
            ),
            (
                "pair-versions-1-label-2",
                (pair.versions[1].label()).clone().into(),
            ),
        ],
    ));
    if pair.versions[0].bytes == pair.versions[1].bytes {
        ui.label(crate::i18n::text(
            "repo-workflows-file_comparison-show-pair-2f04c026",
        ));
        return;
    }
    if let Some(reason) = pair.boundary.display_reason() {
        ui.label(reason);
        ui.label(crate::i18n::text(
            "repo-workflows-file_comparison-show-pair-06ebe02e",
        ));
        for (name, version) in [
            crate::i18n::text("repo-workflows-file_comparison-show-pair-9bb72500"),
            crate::i18n::text("repo-workflows-file_comparison-show-pair-7b68fe55"),
        ]
        .into_iter()
        .zip(&pair.versions)
        {
            ui.monospace(format!(
                "{name}: {}",
                version
                    .bytes
                    .iter()
                    .take(512)
                    .map(|b| format!("{b:02x} "))
                    .collect::<String>()
            ));
        }
        return;
    }
    super::text::show(ui, &pair.boundary, t);
}
fn shorten(text: &str, n: usize) -> String {
    let mut chars = text.chars();
    let mut out = chars.by_ref().take(n).collect::<String>();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}
