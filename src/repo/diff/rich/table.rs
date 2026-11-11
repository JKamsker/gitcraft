//! Tables: CSV and TSV files read as rows of cells, compared as a grid. Columns are matched by
//! name (one renamed is told by keeping most of its values); rows by the key in their first
//! column when it is one, else by content. Rows compare by the columns both sides have, so a
//! column added or removed doesn't make every row changed.

use std::collections::{HashMap, HashSet};

use super::Sides;
use super::prose::{align, fits};

/// More cells than this on a side and the file shows as its plain diff.
const MAX_CELLS: usize = 2_000_000;
/// Rows alike in at least this share of their shared cells are one row changed.
const SAME_ROW: f32 = 0.5;
/// A column renamed keeps at least this share of its values.
const SAME_COLUMN: f32 = 0.8;
/// A column told renamed by its values needs this many different ones (a yes/no column or an
/// empty one says nothing).
const MIN_DISTINCT: usize = 3;
/// Rows matched by a first column that are mostly unlike each other: it is no key (ids
/// numbered anew), and rows are matched by content instead.
const KEY_AGREEMENT: f32 = 0.5;
/// Added rows looked past for the row a removed one became.
const LOOK_AHEAD: usize = 8;

/// A table: its rows of cells and the line each row starts on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub rows: Vec<Vec<String>>,
    pub lines: Vec<u32>,
}

/// Reads delimited text. In CSV, quoted cells may hold the separator, quotes (doubled) and line
/// breaks; TSV has no quoting. Lines end in `\n`, `\r\n` or `\r`. An error says where.
pub fn parse(text: &str, sep: char) -> Result<Table, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let quoting = sep != '\t';
    let mut table = Table::default();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    // The row has something, even an empty quoted cell (`""` is a value, not a blank line).
    let mut started = false;
    let mut line = 1u32;
    let mut row_line = 1u32;
    let mut quoted = false;
    let mut quote_line = 0;
    let mut chars = text.chars().peekable();
    let mut cells = 0usize;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    cell.push('\n');
                }
                _ => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if quoting && cell.is_empty() => {
                quoted = true;
                started = true;
                quote_line = line;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                line += 1;
                if started || !row.is_empty() || !cell.is_empty() {
                    row.push(std::mem::take(&mut cell));
                    cells += row.len();
                    table.rows.push(std::mem::take(&mut row));
                    table.lines.push(row_line);
                    if cells > MAX_CELLS {
                        return Err(crate::i18n::message(
                            "repo-core-2237",
                            &[("MAX-CELLS", format!("{}", MAX_CELLS).into())],
                        ));
                    }
                }
                // A blank line is no row.
                row_line = line;
                started = false;
            }
            c if c == sep => {
                row.push(std::mem::take(&mut cell));
                started = true;
            }
            _ => {
                cell.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(crate::i18n::message(
            "repo-core-2238",
            &[("quote-line", format!("{}", quote_line).into())],
        ));
    }
    if started || !row.is_empty() || !cell.is_empty() {
        row.push(cell);
        table.rows.push(row);
        table.lines.push(row_line);
    }
    Ok(table)
}

/// What happened to a column or a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Kept,
    Added,
    Removed,
    /// A column renamed, or a row with cells changed.
    Changed,
}

/// A column of the grid: its name before and after, where it is on each side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub change: Change,
    pub old: Option<(usize, String)>,
    pub new: Option<(usize, String)>,
}

impl Column {
    /// On both sides: its cells can be compared.
    pub fn shared(&self) -> bool {
        self.old.is_some() && self.new.is_some()
    }

    /// Its name as shown: `old → new` when renamed.
    pub fn name(&self) -> String {
        match (&self.old, &self.new) {
            (Some((_, o)), Some((_, n))) if o != n => format!("{o} → {n}"),
            (_, Some((_, n))) => n.clone(),
            (Some((_, o)), None) => o.clone(),
            _ => String::new(),
        }
    }
}

/// A row of the grid: its cells before and after (by column of the grid), and its lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub change: Change,
    pub old: Option<Vec<String>>,
    pub new: Option<Vec<String>>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableDiff {
    /// `CSV` or `TSV`.
    pub format: &'static str,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    /// Rows matched by the key in their first column (else by content).
    pub keyed: bool,
    /// The most characters a cell of each column shows (`old → new` for a changed one),
    /// counted once for the view.
    pub widths: Vec<usize>,
}

/// The separator of `path`, if it is a table.
pub fn separator(path: &str) -> Option<(char, &'static str)> {
    match super::extension(path).as_str() {
        "csv" => Some((',', "CSV")),
        "tsv" | "tab" => Some(('\t', "TSV")),
        _ => None,
    }
}

/// Compares both sides of the table `path`.
pub fn diff(path: &str, sides: &Sides) -> Result<TableDiff, String> {
    let (sep, format) = separator(path).ok_or(crate::i18n::text("repo-core-2244"))?;
    let read = |text: &Option<String>, side: &str| -> Result<Table, String> {
        text.as_deref()
            .map(|t| {
                parse(t, sep).map_err(|e| {
                    crate::i18n::message(
                        "repo-core-2048",
                        &[
                            ("format", format.to_string().into()),
                            ("side", side.to_string().into()),
                            ("e", e.to_string().into()),
                        ],
                    )
                })
            })
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let (old, new) = (
        read(&sides.old, crate::i18n::text("repo-core-2049"))?,
        read(&sides.new, crate::i18n::text("repo-core-2050"))?,
    );
    let columns = columns(&old, &new);
    let (rows, keyed) = rows(&old, &new, &columns);
    let widths = widths(&columns, &rows);
    Ok(TableDiff {
        format,
        columns,
        rows,
        keyed,
        widths,
    })
}

/// The header row of a table, with names for cells past it (`column 5`).
mod alignment;
use alignment::{columns, rows, widths};
