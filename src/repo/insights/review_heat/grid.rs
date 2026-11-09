//! The heatmap itself: a row per file (its hunks below it once opened), a column per signal,
//! each signal drawn its own way (a letter for what was seen, a heat for the lines changed
//! since, a hatch for open threads, a `?` for what isn't known); and the same as a table.

use std::collections::HashSet;

use super::{Act, seen_mark};
use crate::insights::review_heat::{FileHeat, Heat, Hunk, Seen, SeenMark};
use crate::repo::insights::common::{self, Mark};
use crate::repo::insights::heat_grid::{self, Cell, Layout};
use crate::repo::insights::{InsightsAct, Kind};
use crate::repo::{Action, DiffKey};
use crate::theme::Tokens;

/// The columns, in order.
const COLUMNS: [&str; 5] = [
    "Seen in gitcraft",
    "Changed since seen",
    "Open threads",
    "Viewed on forge",
    "Coverage",
];

/// A row: a file, or one of its hunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    File(usize),
    Hunk(usize, usize),
}

/// The rows: every file, and the hunks of those opened below them.
pub(super) fn rows(data: &Heat, expanded: &HashSet<usize>) -> Vec<Row> {
    let mut rows = Vec::new();
    for (f, file) in data.files.iter().enumerate() {
        rows.push(Row::File(f));
        if expanded.contains(&f) {
            rows.extend((0..file.hunks.len()).map(|h| Row::Hunk(f, h)));
        }
    }
    rows
}

/// What a row says on its left.
fn row_label(data: &Heat, row: Row, open: bool) -> String {
    match row {
        Row::File(f) => {
            let file = &data.files[f];
            let arrow = if file.hunks.is_empty() {
                " "
            } else if open {
                "▾"
            } else {
                "▸"
            };
            let more = if file.more_hunks { "+" } else { "" };
            format!(
                "{arrow} {} {} ({}{more} hunks)",
                file.file.status,
                file.file.path,
                file.hunks.len()
            )
        }
        Row::Hunk(f, h) => format!("      {}", data.files[f].hunks[h].hunk.label()),
    }
}

/// The signals of a row: what was seen, lines changed since, its threads.
fn signals(data: &Heat, row: Row) -> (SeenMark, Option<u32>, &[usize]) {
    match row {
        Row::File(f) => {
            let file = &data.files[f];
            (file.seen, file.since, &file.threads)
        }
        Row::Hunk(f, h) => {
            let hunk = &data.files[f].hunks[h];
            (hunk.seen, hunk.since, &hunk.threads)
        }
    }
}

/// The most lines changed since in any file, and in any hunk: what the heat is scaled to.
fn most(data: &Heat) -> (u32, u32) {
    let files = data.files.iter().filter_map(|f| f.since).max().unwrap_or(0);
    let hunks = data
        .files
        .iter()
        .flat_map(|f| &f.hunks)
        .filter_map(|h| h.since)
        .max()
        .unwrap_or(0);
    (files.max(1), hunks.max(1))
}

fn cell(data: &Heat, row: Row, column: usize, t: &Tokens) -> Cell {
    let (seen, since, threads) = signals(data, row);
    let unknown = |tip: &str| Cell {
        fill: t.field,
        mark: Some(Mark::Glyph("?", t.text_faint)),
        tip: tip.to_owned(),
    };
    match column {
        0 => {
            let (fill, mark) = seen_mark(seen, t);
            Cell {
                fill,
                mark: Some(mark),
                tip: format!("{} ({})", seen.label(), seen.evidence().label()),
            }
        }
        1 => match since {
            None => unknown(match seen {
                SeenMark::NotSeen => "unknown: no version seen to count changes from",
                SeenMark::Stale | SeenMark::SameAfterRewrite => {
                    "unknown: the version seen was rewritten, its lines can't be matched"
                }
                _ => "unknown",
            }),
            Some(0) => Cell {
                fill: t.field,
                mark: None,
                tip: "no line changed since the version seen (observed)".to_owned(),
            },
            Some(n) => {
                let (files, hunks) = most(data);
                let scale = if matches!(row, Row::File(_)) {
                    files
                } else {
                    hunks
                };
                Cell {
                    fill: common::heat(t, n as f32 / scale as f32),
                    mark: Some(Mark::Fill(t.accent_fg)),
                    tip: format!("{n} lines changed since the version seen (observed)"),
                }
            }
        },
        2 => {
            if data.threads.is_none() {
                return unknown("unknown: the review threads couldn't be read");
            }
            let open = data.open(threads);
            let resolved = threads.len() - open;
            let guess = if data.threads_elsewhere {
                "; read at another head, so where they are is a guess"
            } else {
                ""
            };
            if open > 0 {
                Cell {
                    fill: common::tint(t.warning, (0.15 + 0.1 * open as f32).min(0.5), t),
                    mark: Some(Mark::Hatched(t.warning)),
                    tip: format!("{open} open, {resolved} resolved thread(s) (forge){guess}"),
                }
            } else if resolved > 0 {
                Cell {
                    fill: t.field,
                    mark: Some(Mark::Hollow(t.text_faint)),
                    tip: format!("{resolved} resolved thread(s), none open (forge){guess}"),
                }
            } else {
                Cell {
                    fill: t.field,
                    mark: None,
                    tip: "no review thread on these lines (forge)".to_owned(),
                }
            }
        }
        3 => unknown(
            "unknown: gitcraft doesn't read the forge's \"viewed\" marks; not viewed is not \
             what this says",
        ),
        _ => unknown("unknown: coverage isn't read here; see Test coverage (click)"),
    }
}

/// Where a row's hunk starts, as the diff numbers it: the new side, or the old one for a file
/// deleted.
fn hunk_line(file: &FileHeat, hunk: &Hunk) -> (bool, u32) {
    if hunk.new_len == 0 && file.file.status == 'D' {
        (true, hunk.old_start.max(1))
    } else {
        (false, hunk.new_start.max(1))
    }
}

/// What opening a row's cell in `column` does.
fn open(data: &Heat, row: Row, column: usize, cell: Option<(usize, usize)>) -> Action {
    if column == 4 {
        return Action::Insights(InsightsAct::Open(Kind::Coverage));
    }
    let (f, hunk) = match row {
        Row::File(f) => (f, None),
        Row::Hunk(f, h) => (f, Some(&data.files[f].hunks[h])),
    };
    let file = &data.files[f];
    let (seen, _, threads) = signals(data, row);
    // What changed since the version seen opens from it; the rest from where the pull
    // request's commits start.
    let since_tip = match &data.seen {
        Some(Seen::Fresh { tip, .. }) if seen == SeenMark::Changed && column <= 1 => {
            Some(tip.clone())
        }
        _ => None,
    };
    let from = since_tip.unwrap_or_else(|| data.from.clone());
    let mut at = hunk.map(|h| hunk_line(file, &h.hunk));
    if column == 2 {
        let all = data.threads.as_deref().unwrap_or(&[]);
        let first = threads
            .iter()
            .filter_map(|&i| all.get(i))
            .find(|t| !t.resolved)
            .or_else(|| threads.first().and_then(|&i| all.get(i)));
        if let Some(line) = first.and_then(|t| Some((t.left, t.line?))) {
            at = Some((line.0, line.1.min(u32::MAX as u64) as u32));
        }
    }
    if at.is_none() {
        at = file.hunks.first().map(|h| hunk_line(file, &h.hunk));
    }
    Action::Insights(InsightsAct::ReviewHeat(Act::Open {
        key: DiffKey::Range {
            from,
            to: Some(data.head.clone()),
            path: file.file.path.clone(),
        },
        at,
        cell,
    }))
}

/// The grid of `rows` by signal, at most `h` tall.
pub(super) fn show(
    ui: &mut egui::Ui,
    data: &Heat,
    rows: &[Row],
    selected: Option<(usize, usize)>,
    h: f32,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let layout = Layout {
        cell: egui::vec2(26.0, 20.0),
        label_w: 340.0,
        header_h: 96.0,
    };
    let height = (layout.header_h + rows.len() as f32 * layout.cell.y + 12.0).min(h);
    let open_files: HashSet<usize> = rows
        .iter()
        .filter_map(|r| match r {
            Row::Hunk(f, _) => Some(*f),
            Row::File(_) => None,
        })
        .collect();
    let picked = ui
        .allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
            heat_grid::show(
                ui,
                ("review-heat", data.number, &data.head),
                layout,
                (rows.len(), COLUMNS.len()),
                |r| {
                    let row = rows[r];
                    let open = matches!(row, Row::File(f) if open_files.contains(&f));
                    row_label(data, row, open)
                },
                |c| COLUMNS[c].to_owned(),
                |r, c| Some(cell(data, rows[r], c, &t)),
                selected.filter(|(r, c)| *r < rows.len() && *c < COLUMNS.len()),
            )
        })
        .inner;
    if let Some((r, c)) = picked.cell {
        acts.push(open(data, rows[r], c, Some((r, c))));
    }
    if let Some(r) = picked.row {
        match rows[r] {
            Row::File(f) => acts.push(Action::Insights(InsightsAct::ReviewHeat(Act::Toggle(f)))),
            row @ Row::Hunk(..) => acts.push(open(data, row, 0, Some((r, 0)))),
        }
    }
    if let Some(4) = picked.column {
        acts.push(Action::Insights(InsightsAct::Open(Kind::Coverage)));
    }
}

/// The same rows as a table: each signal in words. A row opens its diff at its hunk.
pub(super) fn table(ui: &mut egui::Ui, data: &Heat, rows: &[Row], h: f32, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let widths = [300.0, 200.0, 130.0, 150.0, 110.0];
    let head = [
        "File / hunk",
        COLUMNS[0],
        COLUMNS[1],
        COLUMNS[2],
        COLUMNS[3],
        COLUMNS[4],
    ]
    .map(str::to_owned);
    common::table_row(ui, &widths, &head, true, false);
    egui::ScrollArea::vertical()
        .id_salt("review-heat-table")
        .max_height(h)
        .auto_shrink([false, true])
        .show_rows(ui, 22.0, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for r in range {
                let row = rows[r];
                let cells: Vec<String> = std::iter::once(row_label(data, row, false))
                    .chain((0..COLUMNS.len()).map(|c| cell(data, row, c, &t).tip))
                    .collect();
                let file = match row {
                    Row::File(f) | Row::Hunk(f, _) => f,
                };
                let resp = common::table_row(ui, &widths, &cells, false, false);
                let resp = if matches!(row, Row::File(_)) && !data.files[file].hunks.is_empty() {
                    resp.on_hover_text("Click: its diff. Expand all (above) lists its hunks")
                } else {
                    resp.on_hover_text("Click: its diff at the pull request's revisions")
                };
                if resp.clicked() {
                    acts.push(open(data, row, 0, None));
                }
            }
        });
}
