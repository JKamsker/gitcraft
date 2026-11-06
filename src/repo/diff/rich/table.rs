//! Tables: CSV and TSV files read as rows of cells, compared as a grid. Columns are matched by
//! name (one renamed is told by keeping its place and most of its values); rows by the key in
//! their first column when it is one, else by content. Changed rows mark the cells that
//! changed.

use std::collections::HashMap;

use super::Sides;
use super::prose::{align, fits};

/// More cells than this on a side and the file shows as its plain diff.
const MAX_CELLS: usize = 2_000_000;
/// Rows alike in at least this share of their cells are one row changed.
const SAME_ROW: f32 = 0.5;
/// A column renamed keeps at least this share of its values.
const SAME_COLUMN: f32 = 0.8;

/// A table: its rows of cells and the line each row starts on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub rows: Vec<Vec<String>>,
    pub lines: Vec<u32>,
}

/// Reads delimited text: quoted cells may hold the separator, quotes (doubled) and line
/// breaks. An error says where.
pub fn parse(text: &str, sep: char) -> Result<Table, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut table = Table::default();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut line = 1u32;
    let mut row_line = 1u32;
    let mut quoted = false;
    let mut quote_line = 0;
    let mut chars = text.chars().peekable();
    let mut cells = 0usize;
    let mut at_start = true;
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
            '"' if cell.is_empty() => {
                quoted = true;
                quote_line = line;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' if row.is_empty() && cell.is_empty() => {
                // A blank line is no row.
                line += 1;
                row_line = line;
                at_start = true;
                continue;
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                cells += row.len();
                table.rows.push(std::mem::take(&mut row));
                table.lines.push(row_line);
                line += 1;
                row_line = line;
                at_start = true;
                if cells > MAX_CELLS {
                    return Err(format!("over {MAX_CELLS} cells"));
                }
                continue;
            }
            c if c == sep => row.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
        at_start = false;
    }
    if quoted {
        return Err(format!("line {quote_line}: a quoted cell that never ends"));
    }
    if !at_start || !cell.is_empty() || !row.is_empty() {
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
    let (sep, format) = separator(path).ok_or("not a table")?;
    let read = |text: &Option<String>, side: &str| -> Result<Table, String> {
        text.as_deref()
            .map(|t| {
                parse(t, sep)
                    .map_err(|e| format!("the {format} {side} the change isn't valid ({e})"))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let (old, new) = (read(&sides.old, "before")?, read(&sides.new, "after")?);
    let columns = columns(&old, &new);
    let (rows, keyed) = rows(&old, &new, &columns);
    Ok(TableDiff {
        format,
        columns,
        rows,
        keyed,
    })
}

/// The header row of a table.
fn header(t: &Table) -> &[String] {
    t.rows.first().map_or(&[][..], |r| r.as_slice())
}

fn cell(row: &[String], i: Option<usize>) -> &str {
    i.and_then(|i| row.get(i)).map_or("", String::as_str)
}

/// The columns of both sides: matched by name; one only before and one only after, in the
/// same place or with most of the same values, renamed.
fn columns(old: &Table, new: &Table) -> Vec<Column> {
    let (oh, nh) = (header(old), header(new));
    let mut out: Vec<Column> = Vec::new();
    let mut used_old = vec![false; oh.len()];
    for (j, name) in nh.iter().enumerate() {
        match oh
            .iter()
            .enumerate()
            .position(|(i, n)| n == name && !used_old[i])
        {
            Some(i) => {
                used_old[i] = true;
                out.push(Column {
                    change: Change::Kept,
                    old: Some((i, oh[i].clone())),
                    new: Some((j, name.clone())),
                });
            }
            None => out.push(Column {
                change: Change::Added,
                old: None,
                new: Some((j, name.clone())),
            }),
        }
    }
    // Renames: an added column and a removed one with the same values (or in the same place
    // when there are no values to tell by).
    let olds: Vec<Vec<&str>> = (0..oh.len()).map(|i| column_values(old, i)).collect();
    for col in out.iter_mut().filter(|c| c.change == Change::Added) {
        let Some((j, _)) = col.new.clone() else {
            continue;
        };
        let theirs = column_values(new, j);
        let best = (0..oh.len())
            .filter(|&i| !used_old[i])
            .map(|i| (i, alike(&olds[i], &theirs)))
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
        if let Some((i, score)) = best {
            let mine = &olds[i];
            let same_place = i == j && mine.is_empty() && theirs.is_empty();
            if same_place || score >= SAME_COLUMN {
                used_old[i] = true;
                col.change = Change::Changed;
                col.old = Some((i, oh[i].clone()));
            }
        }
    }
    for (i, name) in oh.iter().enumerate().filter(|(i, _)| !used_old[*i]) {
        // Where it was: after the columns that were before it.
        let at = out
            .iter()
            .position(|c| c.old.as_ref().is_some_and(|(k, _)| *k > i))
            .unwrap_or(out.len());
        out.insert(
            at,
            Column {
                change: Change::Removed,
                old: Some((i, name.clone())),
                new: None,
            },
        );
    }
    out
}

/// The values of column `i` of a table, header aside.
fn column_values(t: &Table, i: usize) -> Vec<&str> {
    t.rows.iter().skip(1).map(|r| cell(r, Some(i))).collect()
}

/// The share of two columns' values they have in common, wherever the rows went.
fn alike(a: &[&str], b: &[&str]) -> f32 {
    let n = a.len().max(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for v in a {
        *counts.entry(v).or_default() += 1;
    }
    let common = b
        .iter()
        .filter(|v| match counts.get_mut(*v) {
            Some(c) if *c > 0 => {
                *c -= 1;
                true
            }
            _ => false,
        })
        .count();
    common as f32 / n as f32
}

/// A row's cells in the grid's columns, from one side.
fn project(row: &[String], cols: &[Column], old: bool) -> Vec<String> {
    cols.iter()
        .map(|c| {
            let at = if old { &c.old } else { &c.new };
            cell(row, at.as_ref().map(|(i, _)| *i)).to_owned()
        })
        .collect()
}

/// The rows of both sides (headers aside), matched and compared.
fn rows(old: &Table, new: &Table, cols: &[Column]) -> (Vec<Row>, bool) {
    let po: Vec<Vec<String>> = old
        .rows
        .iter()
        .skip(1)
        .map(|r| project(r, cols, true))
        .collect();
    let pn: Vec<Vec<String>> = new
        .rows
        .iter()
        .skip(1)
        .map(|r| project(r, cols, false))
        .collect();
    let line = |t: &Table, i: usize| t.lines.get(i + 1).copied();
    let make = |i: Option<usize>, j: Option<usize>| -> Row {
        let o = i.map(|i| po[i].clone());
        let n = j.map(|j| pn[j].clone());
        let change = match (&o, &n) {
            (Some(a), Some(b)) if a == b => Change::Kept,
            (Some(_), Some(_)) => Change::Changed,
            (Some(_), None) => Change::Removed,
            _ => Change::Added,
        };
        Row {
            change,
            old: o,
            new: n,
            old_line: i.and_then(|i| line(old, i)),
            new_line: j.and_then(|j| line(new, j)),
        }
    };
    // A first column whose values are all there and different on both sides is a key.
    let key_col = cols.iter().position(|c| c.old.is_some() && c.new.is_some());
    let unique = |rows: &[Vec<String>], k: usize| {
        let mut seen = std::collections::HashSet::new();
        rows.iter()
            .all(|r| !r[k].is_empty() && seen.insert(r[k].as_str()))
    };
    if let Some(k) = key_col
        .filter(|&k| k == 0 && unique(&po, k) && unique(&pn, k) && !po.is_empty() && !pn.is_empty())
    {
        let by_key: HashMap<&str, usize> = po
            .iter()
            .enumerate()
            .map(|(i, r)| (r[k].as_str(), i))
            .collect();
        let mut matched = vec![false; po.len()];
        let mut out = Vec::new();
        for (j, r) in pn.iter().enumerate() {
            let i = by_key.get(r[k].as_str()).copied();
            if let Some(i) = i {
                matched[i] = true;
            }
            out.push(make(i, Some(j)));
        }
        for i in (0..po.len()).filter(|&i| !matched[i]) {
            // Where it was: before the first row after it that is still there.
            let at = out
                .iter()
                .position(|r: &Row| r.old_line.is_some_and(|l| Some(l) > line(old, i)))
                .unwrap_or(out.len());
            out.insert(at, make(Some(i), None));
        }
        return (out, true);
    }
    let mut out = Vec::new();
    let steps = if fits_rows(&po, &pn) {
        align(&po, &pn)
    } else {
        // Too many to align: row by row in place.
        (0..po.len().max(pn.len()))
            .map(|k| ((k < po.len()).then_some(k), (k < pn.len()).then_some(k)))
            .collect()
    };
    let (mut gone, mut came) = (Vec::new(), Vec::new());
    let flush = |gone: &mut Vec<usize>, came: &mut Vec<usize>, out: &mut Vec<Row>| {
        // Rows replaced in place: paired when alike, else removed and added.
        let mut k = 0;
        for &i in gone.iter() {
            match came.get(k) {
                Some(&j) if row_alike(&po[i], &pn[j]) >= SAME_ROW => {
                    out.push(make(Some(i), Some(j)));
                    k += 1;
                }
                _ => out.push(make(Some(i), None)),
            }
        }
        out.extend(came[k..].iter().map(|&j| make(None, Some(j))));
        gone.clear();
        came.clear();
    };
    for step in steps {
        match step {
            (Some(i), None) => gone.push(i),
            (None, Some(j)) => came.push(j),
            (Some(i), Some(j)) => {
                flush(&mut gone, &mut came, &mut out);
                out.push(make(Some(i), Some(j)));
            }
            (None, None) => {}
        }
    }
    flush(&mut gone, &mut came, &mut out);
    (out, false)
}

fn fits_rows(a: &[Vec<String>], b: &[Vec<String>]) -> bool {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    fits(a.len() - prefix - suffix, b.len() - prefix - suffix)
}

fn row_alike(a: &[String], b: &[String]) -> f32 {
    let n = a.len().max(b.len());
    if n == 0 {
        return 1.0;
    }
    a.iter()
        .zip(b)
        .filter(|(x, y)| x == y && !x.is_empty())
        .count() as f32
        / n as f32
}
