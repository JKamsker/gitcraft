//! The heatmap's lists: the table beside (or instead of) the picture, and the commits of a cell
//! or a period to choose one from.

use egui::Ui;

use super::{Built, Opened, Pick};
use crate::insights::evolution::Open;
use crate::repo::insights::common;
use crate::repo::{Action, DiffKey, short};

/// The table: each row's totals, then each of its cells; what was clicked.
pub(super) fn table(
    ui: &mut Ui,
    built: &Built,
    selected: Option<(usize, usize)>,
) -> Option<Opened> {
    let m = &built.matrix;
    // A row's line before its cells' lines.
    let mut lines: Vec<(usize, Option<usize>)> =
        Vec::with_capacity(m.rows.len() + built.cells.len());
    let mut cells = built.cells.iter().peekable();
    for r in 0..m.rows.len() {
        lines.push((r, None));
        while let Some(&&(cr, c)) = cells.peek() {
            if cr != r {
                break;
            }
            lines.push((r, Some(c)));
            cells.next();
        }
    }
    let widths = [320.0, 80.0, 70.0, 70.0, 80.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Row · column".to_owned(),
            "Lines".to_owned(),
            "Added".to_owned(),
            "Deleted".to_owned(),
            "Commits".to_owned(),
            "Notes".to_owned(),
        ],
        true,
        false,
    );
    let mut opened = None;
    egui::ScrollArea::vertical()
        .id_salt("insights-evolution-table")
        .auto_shrink([false, false])
        .show_rows(ui, 22.0, lines.len(), |ui, range| {
            for &(r, c) in &lines[range] {
                let Some(row) = m.rows.get(r) else {
                    continue;
                };
                let (cells, is_selected) = match c {
                    None => (
                        vec![
                            if row.folder {
                                format!("{}/", row.shown())
                            } else {
                                row.shown().to_owned()
                            },
                            row.lines.to_string(),
                            format!("+{}", row.added),
                            format!("−{}", row.deleted),
                            row.commits.to_string(),
                            if row.folder {
                                "folder: open to zoom in".to_owned()
                            } else {
                                "file: open its history".to_owned()
                            },
                        ],
                        false,
                    ),
                    Some(c) => {
                        let Some(tally) = m.cells.get(&(r, c)) else {
                            continue;
                        };
                        let column = m.columns.get(c).map(|c| c.label.as_str()).unwrap_or("");
                        let mut notes = Vec::new();
                        if tally.binary_only() {
                            notes.push("binary only");
                        }
                        if tally.renamed > 0 {
                            notes.push("renamed");
                        }
                        (
                            vec![
                                format!("    {column}"),
                                tally.lines.to_string(),
                                format!("+{}", tally.added),
                                format!("−{}", tally.deleted),
                                tally.commits().len().to_string(),
                                notes.join(", "),
                            ],
                            selected == Some((r, c)),
                        )
                    }
                };
                if common::table_row(ui, &widths, &cells, false, is_selected).clicked() {
                    opened = Some(match c {
                        None => Opened::Row(r),
                        Some(c) => Opened::Cell(r, c),
                    });
                }
            }
        });
    opened
}

/// The commits of a cell or a column to choose one from: each selects its commit, each of its
/// paths opens that file's diff. `false` when it was closed (or is gone).
pub(super) fn pick(ui: &mut Ui, built: &Built, pick: &Pick, acts: &mut Vec<Action>) -> bool {
    let m = &built.matrix;
    let (title, list): (String, Vec<(String, Vec<String>)>) = match *pick {
        Pick::Cell(r, c) => {
            let Open::Commits(list) = m.open(&built.history, r, c) else {
                return false;
            };
            let row = m.rows.get(r).map(|r| r.shown()).unwrap_or("");
            let column = m.columns.get(c).map(|c| c.label.as_str()).unwrap_or("");
            (format!("Commits of {row} in {column}"), list)
        }
        Pick::Column(c) => {
            let Some(col) = m.columns.get(c) else {
                return false;
            };
            let list = col
                .commits
                .iter()
                .filter_map(|&i| built.history.commits.get(i))
                .map(|commit| (commit.hash.clone(), Vec::new()))
                .collect();
            (format!("Commits of {}", col.label), list)
        }
    };
    let mut open = true;
    ui.horizontal(|ui| {
        common::heading(ui, &title);
        if ui.small_button("Close").on_hover_text("Esc").clicked() {
            open = false;
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("insights-evolution-pick")
        .max_height(150.0)
        .show(ui, |ui| {
            for (hash, paths) in &list {
                let commit = built.history.commits.iter().find(|c| &c.hash == hash);
                let subject = commit.map(|c| c.subject.as_str()).unwrap_or("");
                let author = commit.map(|c| c.author.as_str()).unwrap_or("");
                let resp = common::table_row(
                    ui,
                    &[90.0, 140.0],
                    &[
                        short(hash).to_owned(),
                        author.to_owned(),
                        subject.to_owned(),
                    ],
                    false,
                    false,
                )
                .on_hover_text("Select the commit");
                if resp.clicked() {
                    acts.push(Action::SelectHash(hash.clone()));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(96.0);
                    for path in paths {
                        if ui
                            .small_button(path)
                            .on_hover_text("Open this file's diff in the commit")
                            .clicked()
                        {
                            acts.push(Action::OpenDiff(DiffKey::Commit {
                                hash: hash.clone(),
                                path: path.clone(),
                            }));
                        }
                    }
                });
            }
        });
    open
}
