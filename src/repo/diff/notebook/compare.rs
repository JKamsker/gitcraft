//! Two versions of a notebook compared cell by cell: cells matched by their ids (or, without
//! ids, by their sources in order), each with its source diffed line by line and whether its
//! outputs, execution count or metadata changed.

use super::parse::{Cell, Notebook};
use crate::repo::diff::line_diff::{Edit, diff_lines};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Unchanged,
    Added,
    Removed,
    Modified,
}

/// A cell and what happened to it.
#[derive(Clone, Debug, PartialEq)]
pub struct CellChange {
    pub old: Option<Cell>,
    pub new: Option<Cell>,
    pub status: Status,
    /// Its number in the notebook (the new one, or the old one for a removed cell).
    pub number: usize,
    /// It moved among the other cells.
    pub moved: bool,
    /// Its source, line by line.
    pub source: Vec<(Edit, String)>,
    pub outputs_changed: bool,
    pub count_changed: bool,
    pub metadata_changed: bool,
}

/// What changed between two notebooks.
#[derive(Clone, Debug, PartialEq)]
pub struct NotebookDiff {
    pub cells: Vec<CellChange>,
    /// The notebook's own metadata, line by line, when it changed.
    pub metadata: Option<Vec<(Edit, String)>>,
}

fn lines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

fn owned(d: Vec<(Edit, &str)>) -> Vec<(Edit, String)> {
    d.into_iter().map(|(e, l)| (e, l.to_owned())).collect()
}

/// The two notebooks compared (`None` for a side without the file).
pub fn compare(old: Option<&Notebook>, new: Option<&Notebook>) -> NotebookDiff {
    let empty = Notebook::default();
    let (a, b) = (old.unwrap_or(&empty), new.unwrap_or(&empty));
    let pairs = match_cells(&a.cells, &b.cells);
    let mut last_old = None;
    let cells = pairs
        .into_iter()
        .map(|(o, n)| {
            // Matched by id, before a cell that came before it: moved.
            let moved = o.is_some() && n.is_some() && last_old.zip(o).is_some_and(|(l, i)| i < l);
            if o.is_some() && n.is_some() {
                last_old = last_old.max(o);
            }
            let mut c = change(o.map(|i| &a.cells[i]), n.map(|i| &b.cells[i]));
            c.number = n.or(o).map_or(0, |i| i + 1);
            c.moved = moved;
            if moved && c.status == Status::Unchanged {
                c.status = Status::Modified;
            }
            c
        })
        .collect();
    let metadata = (old.is_some() && new.is_some() && a.metadata != b.metadata)
        .then(|| owned(diff_lines(&lines(&a.metadata), &lines(&b.metadata))));
    NotebookDiff { cells, metadata }
}

/// The cells paired: by id when every cell of both sides has one, else by their sources in
/// order (a removed and an added cell next to each other being the same one edited).
pub(crate) fn match_cells(a: &[Cell], b: &[Cell]) -> Vec<(Option<usize>, Option<usize>)> {
    // Every cell with an id of its own, on both sides.
    let ids = |cells: &[Cell]| {
        let mut seen = std::collections::HashSet::new();
        cells
            .iter()
            .all(|c| c.id.as_deref().is_some_and(|id| seen.insert(id)))
    };
    if !a.is_empty() && !b.is_empty() && ids(a) && ids(b) {
        return match_by_id(a, b);
    }
    let keys = |cells: &[Cell]| -> Vec<String> {
        cells
            .iter()
            .map(|c| format!("{}\u{0}{}", c.kind.label(), c.source))
            .collect()
    };
    let (ka, kb) = (keys(a), keys(b));
    let (ra, rb): (Vec<&str>, Vec<&str>) = (
        ka.iter().map(String::as_str).collect(),
        kb.iter().map(String::as_str).collect(),
    );
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut dels: Vec<usize> = Vec::new();
    let mut adds: Vec<usize> = Vec::new();
    let flush = |out: &mut Vec<(Option<usize>, Option<usize>)>,
                 dels: &mut Vec<usize>,
                 adds: &mut Vec<usize>| {
        // Removed then added in one run: the same cells edited, in order; the rest stay apart.
        let n = dels.len().max(adds.len());
        for k in 0..n {
            out.push((dels.get(k).copied(), adds.get(k).copied()));
        }
        dels.clear();
        adds.clear();
    };
    for (edit, _) in diff_lines(&ra, &rb) {
        match edit {
            Edit::Same => {
                flush(&mut out, &mut dels, &mut adds);
                out.push((Some(i), Some(j)));
                i += 1;
                j += 1;
            }
            Edit::Del => {
                dels.push(i);
                i += 1;
            }
            Edit::Add => {
                adds.push(j);
                j += 1;
            }
        }
    }
    flush(&mut out, &mut dels, &mut adds);
    out
}

/// Cells paired by id, in the new order, with the removed ones where they were.
fn match_by_id(a: &[Cell], b: &[Cell]) -> Vec<(Option<usize>, Option<usize>)> {
    use std::collections::HashMap;
    let in_a: HashMap<&str, usize> = a
        .iter()
        .enumerate()
        .filter_map(|(i, c)| Some((c.id.as_deref()?, i)))
        .collect();
    let in_b: std::collections::HashSet<&str> = b.iter().filter_map(|c| c.id.as_deref()).collect();
    let mut out = Vec::new();
    let mut next_a = 0;
    for (j, c) in b.iter().enumerate() {
        let i = c.id.as_deref().and_then(|id| in_a.get(id)).copied();
        if let Some(i) = i {
            // The removed cells before this one show here.
            while next_a < i {
                if !a[next_a].id.as_deref().is_some_and(|id| in_b.contains(id)) {
                    out.push((Some(next_a), None));
                }
                next_a += 1;
            }
            next_a = next_a.max(i + 1);
        }
        if i.is_none() {
            // A new cell where removed ones are next: those first.
            while next_a < a.len() && !a[next_a].id.as_deref().is_some_and(|id| in_b.contains(id)) {
                out.push((Some(next_a), None));
                next_a += 1;
            }
        }
        out.push((i, Some(j)));
    }
    for (k, c) in a.iter().enumerate().skip(next_a) {
        if !c.id.as_deref().is_some_and(|id| in_b.contains(id)) {
            out.push((Some(k), None));
        }
    }
    out
}

fn change(old: Option<&Cell>, new: Option<&Cell>) -> CellChange {
    let src = |c: Option<&Cell>| c.map(|c| c.source.clone()).unwrap_or_default();
    let (sa, sb) = (src(old), src(new));
    let source = owned(diff_lines(&lines(&sa), &lines(&sb)));
    let (outputs_changed, count_changed, metadata_changed) = match (old, new) {
        (Some(a), Some(b)) => (
            a.outputs != b.outputs,
            a.execution_count != b.execution_count,
            a.metadata != b.metadata,
        ),
        _ => (false, false, false),
    };
    let status = match (old, new) {
        (None, _) => Status::Added,
        (_, None) => Status::Removed,
        (Some(a), Some(b))
            if a.source != b.source || a.kind != b.kind || outputs_changed || metadata_changed =>
        {
            Status::Modified
        }
        _ => Status::Unchanged,
    };
    CellChange {
        old: old.cloned(),
        new: new.cloned(),
        status,
        number: 0,
        moved: false,
        source,
        outputs_changed,
        count_changed,
        metadata_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::diff::notebook::parse::{Cell, CellKind};

    fn cell(id: Option<&str>, source: &str) -> Cell {
        Cell {
            id: id.map(str::to_owned),
            kind: CellKind::Code,
            source: source.to_owned(),
            execution_count: None,
            outputs: Vec::new(),
            metadata: "{}".to_owned(),
        }
    }

    #[test]
    fn matched_by_id_through_moves() {
        let a = [
            cell(Some("1"), "a"),
            cell(Some("2"), "b"),
            cell(Some("3"), "c"),
        ];
        let b = [
            cell(Some("3"), "c"),
            cell(Some("1"), "a2"),
            cell(Some("4"), "d"),
        ];
        assert_eq!(
            match_cells(&a, &b),
            // The removed cell shows where it was, before the cells after it.
            [
                (Some(1), None),
                (Some(2), Some(0)),
                (Some(0), Some(1)),
                (None, Some(2))
            ]
        );
    }

    #[test]
    fn duplicate_ids_match_by_source() {
        let a = [cell(Some("1"), "a"), cell(Some("1"), "b")];
        let b = [
            cell(Some("1"), "a"),
            cell(Some("1"), "b"),
            cell(Some("2"), "c"),
        ];
        assert_eq!(
            match_cells(&a, &b),
            [(Some(0), Some(0)), (Some(1), Some(1)), (None, Some(2))]
        );
    }

    #[test]
    fn moves_and_numbers() {
        let nb = |cells: Vec<Cell>| crate::repo::diff::notebook::parse::Notebook {
            cells,
            metadata: "{}".into(),
        };
        let a = nb(vec![
            cell(Some("1"), "a"),
            cell(Some("2"), "b"),
            cell(Some("3"), "c"),
        ]);
        let b = nb(vec![
            cell(Some("2"), "b"),
            cell(Some("1"), "a"),
            cell(Some("3"), "c"),
        ]);
        let d = compare(Some(&a), Some(&b));
        let shown: Vec<(usize, bool, Status)> = d
            .cells
            .iter()
            .map(|c| (c.number, c.moved, c.status))
            .collect();
        assert_eq!(
            shown,
            [
                (1, false, Status::Unchanged),
                (2, true, Status::Modified),
                (3, false, Status::Unchanged)
            ]
        );
        // Removed: its old number.
        let c = nb(vec![cell(Some("1"), "a"), cell(Some("3"), "c")]);
        let d = compare(Some(&a), Some(&c));
        assert_eq!(
            d.cells.iter().map(|c| c.number).collect::<Vec<_>>(),
            [1, 2, 2]
        );
    }

    #[test]
    fn matched_by_source_without_ids() {
        let a = [
            cell(None, "import x"),
            cell(None, "old()"),
            cell(None, "end"),
        ];
        let b = [
            cell(None, "import x"),
            cell(None, "new()"),
            cell(None, "extra"),
            cell(None, "end"),
        ];
        assert_eq!(
            match_cells(&a, &b),
            [
                (Some(0), Some(0)),
                (Some(1), Some(1)),
                (None, Some(2)),
                (Some(2), Some(3))
            ]
        );
        assert!(match_cells(&[], &[]).is_empty());
    }

    #[test]
    fn what_changed_in_each_cell() {
        let mut a = cell(Some("1"), "x = 1\nprint(x)");
        let mut b = cell(Some("1"), "x = 2\nprint(x)");
        a.execution_count = Some(1);
        b.execution_count = Some(5);
        let c = change(Some(&a), Some(&b));
        assert_eq!(c.status, Status::Modified);
        assert!(c.count_changed && !c.outputs_changed && !c.metadata_changed);
        assert_eq!(c.source.iter().filter(|(e, _)| *e != Edit::Same).count(), 2);
        // Run again only: unchanged, the count folded away.
        b.source = a.source.clone();
        assert_eq!(change(Some(&a), Some(&b)).status, Status::Unchanged);
        // Its metadata (tags) changed only: changed.
        b.metadata = "{\"tags\": [\"x\"]}".into();
        assert_eq!(change(Some(&a), Some(&b)).status, Status::Modified);
        assert_eq!(change(None, Some(&b)).status, Status::Added);
        assert_eq!(change(Some(&a), None).status, Status::Removed);
    }
}
