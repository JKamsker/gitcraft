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
fn header(t: &Table) -> Vec<String> {
    let mut names: Vec<String> = t.rows.first().cloned().unwrap_or_default();
    let widest = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    for k in names.len()..widest {
        names.push(crate::i18n::message(
            "repo-core-2245",
            &[("k", format!("{}", k + 1).into())],
        ));
    }
    names
}

fn cell(row: &[String], i: Option<usize>) -> &str {
    i.and_then(|i| row.get(i)).map_or("", String::as_str)
}

/// The values of column `i` of a table, header aside.
fn column_values(t: &Table, i: usize) -> Vec<&str> {
    t.rows.iter().skip(1).map(|r| cell(r, Some(i))).collect()
}

/// A column's values counted, to compare columns by.
struct Counts<'a> {
    counts: HashMap<&'a str, usize>,
    len: usize,
    distinct: usize,
}

impl<'a> Counts<'a> {
    fn of(values: &[&'a str]) -> Self {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for v in values {
            *counts.entry(v).or_default() += 1;
        }
        let distinct = counts.keys().filter(|v| !v.is_empty()).count();
        Counts {
            counts,
            len: values.len(),
            distinct,
        }
    }

    /// The share of values two columns have in common, wherever the rows went.
    fn alike(&self, other: &Counts) -> f32 {
        let n = self.len.max(other.len);
        if n == 0 {
            return 0.0;
        }
        let common: usize = self
            .counts
            .iter()
            .map(|(v, c)| (*c).min(other.counts.get(v).copied().unwrap_or(0)))
            .sum();
        common as f32 / n as f32
    }
}

/// The columns of both sides: matched by name; one only before and one only after with most
/// of the same (and enough different) values, or in the same place and alike, renamed.
fn columns(old: &Table, new: &Table) -> Vec<Column> {
    let (oh, nh) = (header(old), header(new));
    let mut out: Vec<Column> = Vec::new();
    let mut used_old = vec![false; oh.len()];
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, n) in oh.iter().enumerate().rev() {
        by_name.entry(n.as_str()).or_default().push(i);
    }
    for (j, name) in nh.iter().enumerate() {
        match by_name.get_mut(name.as_str()).and_then(Vec::pop) {
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
    let old_values: Vec<Vec<&str>> = (0..oh.len()).map(|i| column_values(old, i)).collect();
    let olds: Vec<Counts> = old_values.iter().map(|v| Counts::of(v)).collect();
    let unmatched_old = used_old.iter().filter(|u| !**u).count();
    let unmatched_new = out.iter().filter(|c| c.change == Change::Added).count();
    for col in out.iter_mut().filter(|c| c.change == Change::Added) {
        let Some((j, _)) = col.new.clone() else {
            continue;
        };
        let theirs = Counts::of(&column_values(new, j));
        let best = (0..oh.len())
            .filter(|&i| !used_old[i])
            .map(|i| (i, olds[i].alike(&theirs)))
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
        let Some((i, score)) = best else { continue };
        let by_values =
            score >= SAME_COLUMN && olds[i].distinct.min(theirs.distinct) >= MIN_DISTINCT;
        // The only column gone and the only one new, in the same place, mostly alike.
        let by_place = i == j && unmatched_old == 1 && unmatched_new == 1 && score >= SAME_ROW;
        if by_values || by_place {
            used_old[i] = true;
            col.change = Change::Changed;
            col.old = Some((i, oh[i].clone()));
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

/// A row's cells in the grid's columns, from one side.
fn project(row: &[String], cols: &[Column], old: bool) -> Vec<String> {
    cols.iter()
        .map(|c| {
            let at = if old { &c.old } else { &c.new };
            cell(row, at.as_ref().map(|(i, _)| *i)).to_owned()
        })
        .collect()
}

/// A row's cells in the columns both sides have: what it is compared by.
fn shared(row: &[String], cols: &[Column]) -> Vec<String> {
    row.iter()
        .zip(cols)
        .filter(|(_, c)| c.shared())
        .map(|(v, _)| v.clone())
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
    let so: Vec<Vec<String>> = po.iter().map(|r| shared(r, cols)).collect();
    let sn: Vec<Vec<String>> = pn.iter().map(|r| shared(r, cols)).collect();
    let line = |t: &Table, i: usize| t.lines.get(i + 1).copied();
    let make = |i: Option<usize>, j: Option<usize>| -> Row {
        let change = match (i, j) {
            (Some(i), Some(j)) if so[i] == sn[j] => Change::Kept,
            (Some(_), Some(_)) => Change::Changed,
            (Some(_), None) => Change::Removed,
            _ => Change::Added,
        };
        Row {
            change,
            old: i.map(|i| po[i].clone()),
            new: j.map(|j| pn[j].clone()),
            old_line: i.and_then(|i| line(old, i)),
            new_line: j.and_then(|j| line(new, j)),
        }
    };
    if let Some(pairs) = keyed(cols, &po, &pn, &so, &sn) {
        let mut matched = vec![false; po.len()];
        let mut out = Vec::new();
        for (j, i) in pairs.into_iter().enumerate() {
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
    let steps = if fits_rows(&so, &sn) {
        align(&so, &sn)
    } else {
        // Too many to align: row by row in place.
        (0..so.len().max(sn.len()))
            .map(|k| ((k < so.len()).then_some(k), (k < sn.len()).then_some(k)))
            .collect()
    };
    let (mut gone, mut came) = (Vec::new(), Vec::new());
    let flush = |gone: &mut Vec<usize>, came: &mut Vec<usize>, out: &mut Vec<Row>| {
        // Rows replaced in place: paired when alike, else removed and added.
        // A row is looked for a few rows ahead too, so one added row before the rest (ids
        // numbered anew) doesn't leave every row unpaired.
        let mut k = 0;
        for &i in gone.iter() {
            let ahead = came[k..]
                .iter()
                .take(LOOK_AHEAD)
                .position(|&j| row_alike(&so[i], &sn[j]) >= SAME_ROW);
            match ahead {
                Some(p) => {
                    out.extend(came[k..k + p].iter().map(|&j| make(None, Some(j))));
                    out.push(make(Some(i), Some(came[k + p])));
                    k += p + 1;
                }
                None => out.push(make(Some(i), None)),
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

/// The rows of the new side paired by a key in the first column: its values all there and
/// different on both sides, and the rows it pairs mostly alike in their other shared cells
/// (ids numbered anew pair unrelated rows, and are no key). `None` when it is no key.
fn keyed(
    cols: &[Column],
    po: &[Vec<String>],
    pn: &[Vec<String>],
    so: &[Vec<String>],
    sn: &[Vec<String>],
) -> Option<Vec<Option<usize>>> {
    if po.is_empty() || pn.is_empty() || !cols.first().is_some_and(Column::shared) {
        return None;
    }
    let unique = |rows: &[Vec<String>]| {
        let mut seen = HashSet::new();
        rows.iter()
            .all(|r| !r[0].is_empty() && seen.insert(r[0].as_str()))
    };
    if !unique(po) || !unique(pn) {
        return None;
    }
    let by_key: HashMap<&str, usize> = po
        .iter()
        .enumerate()
        .map(|(i, r)| (r[0].as_str(), i))
        .collect();
    let pairs: Vec<Option<usize>> = pn
        .iter()
        .map(|r| by_key.get(r[0].as_str()).copied())
        .collect();
    let paired: Vec<(usize, usize)> = pairs
        .iter()
        .enumerate()
        .filter_map(|(j, i)| Some(((*i)?, j)))
        .collect();
    // With nothing but the key to compare by, it is a key.
    let others = cols.iter().filter(|c| c.shared()).count() > 1;
    if others && !paired.is_empty() {
        let agree = paired
            .iter()
            .filter(|&&(i, j)| row_alike(&so[i][1..], &sn[j][1..]) >= SAME_ROW)
            .count();
        if (agree as f32) < KEY_AGREEMENT * paired.len() as f32 {
            return None;
        }
    }
    Some(pairs)
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
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f32 / n as f32
}

/// The most characters a cell of each column shows.
fn widths(cols: &[Column], rows: &[Row]) -> Vec<usize> {
    let mut widths: Vec<usize> = cols.iter().map(|c| c.name().chars().count()).collect();
    for r in rows {
        for (c, w) in widths.iter_mut().enumerate() {
            let len = |side: &Option<Vec<String>>| {
                side.as_ref()
                    .and_then(|v| v.get(c))
                    .map_or(0, |v| v.chars().count())
            };
            let (o, n) = (len(&r.old), len(&r.new));
            let changed = r.change == Change::Changed
                && cols[c].shared()
                && r.old.as_ref().and_then(|v| v.get(c)) != r.new.as_ref().and_then(|v| v.get(c));
            *w = (*w).max(if changed { o + n + 3 } else { o.max(n) });
        }
    }
    widths
}
