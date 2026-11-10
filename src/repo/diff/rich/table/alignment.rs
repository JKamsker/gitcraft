use super::*;

pub(super) fn header(t: &Table) -> Vec<String> {
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

pub(super) fn cell(row: &[String], i: Option<usize>) -> &str {
    i.and_then(|i| row.get(i)).map_or("", String::as_str)
}

/// The values of column `i` of a table, header aside.
pub(super) fn column_values(t: &Table, i: usize) -> Vec<&str> {
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
pub(super) fn columns(old: &Table, new: &Table) -> Vec<Column> {
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
pub(super) fn project(row: &[String], cols: &[Column], old: bool) -> Vec<String> {
    cols.iter()
        .map(|c| {
            let at = if old { &c.old } else { &c.new };
            cell(row, at.as_ref().map(|(i, _)| *i)).to_owned()
        })
        .collect()
}

/// A row's cells in the columns both sides have: what it is compared by.
pub(super) fn shared(row: &[String], cols: &[Column]) -> Vec<String> {
    row.iter()
        .zip(cols)
        .filter(|(_, c)| c.shared())
        .map(|(v, _)| v.clone())
        .collect()
}

/// The rows of both sides (headers aside), matched and compared.
pub(super) fn rows(old: &Table, new: &Table, cols: &[Column]) -> (Vec<Row>, bool) {
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
pub(super) fn keyed(
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

pub(super) fn fits_rows(a: &[Vec<String>], b: &[Vec<String>]) -> bool {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    fits(a.len() - prefix - suffix, b.len() - prefix - suffix)
}

pub(super) fn row_alike(a: &[String], b: &[String]) -> f32 {
    let n = a.len().max(b.len());
    if n == 0 {
        return 1.0;
    }
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f32 / n as f32
}

/// The most characters a cell of each column shows.
pub(super) fn widths(cols: &[Column], rows: &[Row]) -> Vec<usize> {
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
