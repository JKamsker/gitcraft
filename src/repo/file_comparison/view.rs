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
                state.pair = Some(Err(crate::i18n::text(
                    "repo-workflows-file_comparison-file-comparison-view-725513de",
                )
                .into()));
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
                    &[(
                        "state-selected-len-1",
                        (state.selected.len()).clone().into(),
                    )],
                ))
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
            ui.colored_label(t.danger, crate::repo::activity::display_workflow_error(&e));
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
                    ui.colored_label(t.danger, crate::repo::activity::display_workflow_error(&e));
                }
            }
        }
    }
}

fn migration(
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
            ("data-moves-len-1", (data.moves.len()).clone().into()),
            ("data-other-changes-2", (data.other_changes).clone().into()),
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
                            .clone()
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
                            .unwrap_or_else(|| "unknown".into())
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
fn matrix(
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
        &[("data-paths-len-1", (data.paths.len()).clone().into())],
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
fn lens(
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
            .unwrap_or("unborn")
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
        if let Some(reason) = &part.reason {
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
fn boundary_button(
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
        &[
            ("text", (text).clone().into()),
            ("index-1-1", (index + 1).clone().into()),
        ],
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
fn line_cell(ui: &mut egui::Ui, text: &Option<String>, width: f32, undoes: bool, t: &Tokens) {
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
    if let Some(reason) = &pair.boundary.reason {
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
