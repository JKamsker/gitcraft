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
            Page::Migration => "File migration map",
            Page::Lens => "Three-state staging lens",
            Page::Matrix => "File versions across branches",
        };
        ui.horizontal_wrapped(|ui| {
            ui.heading(title);
            if ui.button("Refresh file view").clicked() {
                acts.push(Action::RefreshFileComparison);
            }
            if state.loading && ui.button("Cancel comparison").clicked() {
                state.generation.store(0, Ordering::Relaxed);
                state.request = 0;
                state.loading = false;
                state.pair = Some(Err("Comparison cancelled. Refresh to try again.".into()));
            }
            if ui.button("Close file view").clicked() {
                acts.push(Action::CloseFileComparison);
            }
        });
        ui.label(RichText::new("Raw snapshots · refresh after repository changes · no decrypted data or automatic downloads · 2 MiB per version").small().color(t.text_dim));
        match state.page {
            Page::Migration => {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut state.source, 0, "Staged moves");
                    ui.selectable_value(&mut state.source, 1, "Commit moves");
                    ui.selectable_value(&mut state.source, 2, "Between commits");
                });
                ui.horizontal_wrapped(|ui| {
                    if state.source == 2 {
                        ui.label("From");
                        ui.add(egui::TextEdit::singleline(&mut state.from).desired_width(140.));
                    }
                    if state.source != 0 {
                        ui.label(if state.source == 1 { "Commit" } else { "To" });
                        ui.add(egui::TextEdit::singleline(&mut state.to).desired_width(140.));
                    }
                    if ui.button("Find file migrations").clicked() {
                        acts.push(Action::RefreshFileComparison);
                    }
                });
                ui.label("Git detects renames/copies at 50% similarity, within 2,000 candidates. Stage reorganized files first to include newly named working files. Content edits open separately from the move.");
            }
            Page::Lens => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("File");
                    if ui
                        .add(egui::TextEdit::singleline(&mut state.path).desired_width(260.))
                        .changed()
                    {
                        state.previous = None;
                    }
                    if ui.button("Read three versions").clicked() {
                        acts.push(Action::RefreshFileComparison);
                    }
                    if ui.button("Open normal patch").clicked() {
                        acts.push(Action::ComparisonNormalPatch);
                    }
                });
                ui.label("HEAD → staged → working copy. Stage or unstage lines at either boundary; refresh if the file changes. Binary files and files requiring Git conversions stay read-only here.");
            }
            Page::Matrix => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Folder");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.folder)
                            .hint_text("Repository root")
                            .desired_width(230.),
                    );
                    if ui
                        .add_enabled(
                            (2..=8).contains(&state.selected.len()),
                            egui::Button::new("Compare branch versions"),
                        )
                        .clicked()
                    {
                        acts.push(Action::RefreshFileComparison);
                    }
                });
                egui::CollapsingHeader::new(format!(
                    "Choose branches ({} selected, maximum 8)",
                    state.selected.len()
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
                ui.label("Files are rows; pinned branch commits are columns. V-markers identify equal bytes, including across rows. Missing cells and file modes are separate. Click two cells to open their content diff.");
            }
        }
        if state.loading {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new().size(16.));
                ui.label("Reading immutable versions…");
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        let data = state.data.as_ref().and_then(|r| r.as_ref().ok()).cloned();
        if let Some(Err(e)) = &state.data {
            ui.colored_label(t.danger, e);
        }
        if let Some(data) = data {
            ui.separator();
            match data.as_ref() {
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
                    ui.colored_label(t.danger, e);
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
    ui.label(format!(
        "{} renames/copies · {} other changes",
        data.moves.len(),
        data.other_changes
    ));
    if let Some(warning) = &data.warning {
        ui.colored_label(
            t.warning,
            format!("Git detection warning (some moves may be unknown): {warning}"),
        );
    }
    if data.moves.is_empty() {
        ui.label(
            "No Git-detected renames or copies. Added/deleted paths are not assumed to be moves.",
        );
        return;
    }
    egui::ScrollArea::both()
        .id_salt("migration_ribbons")
        .max_height(330.)
        .show_rows(ui, 58., data.moves.len(), |ui, range| {
            ui.set_min_width(600.);
            for i in range {
                let m = &data.moves[i];
                let label = format!(
                    "{} → {} · {} {}",
                    m.before.name(),
                    m.after.name(),
                    if m.copy { "Copy" } else { "Rename" },
                    m.similarity
                        .map(|s| format!("{s}%"))
                        .unwrap_or_else(|| "similarity unknown".into())
                );
                let (rect, response) = ui
                    .allocate_exact_size(vec2(ui.available_width().max(600.), 54.), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label)
                });
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
                        if m.copy { "Copy" } else { "Rename" },
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
    ui.label(format!(
        "{} files · select another cell after the first",
        data.paths.len()
    ));
    egui::ScrollArea::both()
        .id_salt("branch_matrix")
        .max_height(350.)
        .show(ui, |ui| {
            ui.set_min_width(220. + 150. * data.columns.len() as f32);
            ui.horizontal(|ui| {
                ui.add_sized([220., 30.], egui::Label::new("File"));
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
                                    None => "Missing".into(),
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
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        true,
                                        &label,
                                    )
                                });
                                let clicked = response.clicked();
                                response.on_hover_text(format!("{label}\nPinned {}", c.oid));
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
        ui.label(format!("HEAD uses {previous}; content-only unstaging keeps the staged move. Change the move itself in the normal patch."));
    }
    if lens.undoing_staged {
        ui.colored_label(t.warning,"Working copy matches HEAD, but the next commit still changes this file. Working edits undo the staged edit.");
    }
    if let Some(w) = &snapshot.staging_warning {
        ui.colored_label(t.warning, w);
    }
    for (name, version) in ["HEAD", "Staged", "Working copy"]
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
        for (i, label) in [(0, "Unstage entire boundary"), (1, "Stage entire boundary")] {
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
    ui.label(RichText::new("Long lines are abbreviated at 2,000 characters; boundary actions use the complete raw line. Use the normal patch to inspect it.").small().color(t.text_dim));
    let width = (ui.available_width() / 3. - 55.).max(180.);
    ui.horizontal(|ui| {
        ui.add_sized([width, 24.], egui::Label::new("HEAD"));
        ui.add_sized([64., 24.], egui::Label::new("← unstage"));
        ui.add_sized([width, 24.], egui::Label::new("Staged"));
        ui.add_sized([64., 24.], egui::Label::new("← stage"));
        ui.add_sized([width, 24.], egui::Label::new("Working copy"));
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
                        ui.colored_label(t.warning, "undoes staged");
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
    let text = if side == 0 { "Unstage" } else { "Stage" };
    let label = format!("{text} lens row {}", index + 1);
    let enabled = allowed
        && part.reason.is_none()
        && !row.changes[side].is_empty()
        && (part.old_mode == part.new_mode || part.old_mode.is_none() || part.new_mode.is_none());
    let response = ui.add_enabled(enabled, egui::Button::new(text).min_size(vec2(64., 20.)));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
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
    ui.heading("Content edits only");
    ui.label(format!("{} → {}", pair.labels[0], pair.labels[1]));
    ui.label(format!(
        "Before: {} · After: {}",
        pair.versions[0].label(),
        pair.versions[1].label()
    ));
    if pair.versions[0].bytes == pair.versions[1].bytes {
        ui.label(
            "No content edits: identical bytes. Names, presence and file modes remain separate.",
        );
        return;
    }
    if let Some(reason) = &pair.boundary.reason {
        ui.label(reason);
        ui.label("Byte preview (first512 bytes per side; full sizes shown above)");
        for (name, version) in ["Before", "After"].into_iter().zip(&pair.versions) {
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
    egui::ScrollArea::both()
        .id_salt("snapshot_content_diff")
        .show_rows(ui, 20., pair.boundary.lines.len(), |ui, range| {
            for i in range {
                let line = &pair.boundary.lines[i];
                let color = match line.kind {
                    super::super::diff::Kind::Add => t.accent,
                    super::super::diff::Kind::Del => t.warning,
                    _ => t.text,
                };
                ui.add(
                    egui::Label::new(
                        RichText::new(shorten(&line.raw, 2000))
                            .monospace()
                            .color(color),
                    )
                    .wrap_mode(egui::TextWrapMode::Extend),
                );
            }
        });
}
fn shorten(text: &str, n: usize) -> String {
    let mut chars = text.chars();
    let mut out = chars.by_ref().take(n).collect::<String>();
    if chars.next().is_some() {
        out.push('…');
    }
    out
}
