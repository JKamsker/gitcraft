//! Validate Difftastic's versioned-by-shape JSON; never show a text fallback as syntax.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Deserialize;

const MAX_ROWS: usize = 20_000;

pub(in crate::repo) struct Structural {
    pub(super) language: String,
    pub(super) rows: Vec<Row>,
    pub(super) unchanged: bool,
    pub(super) longest: usize,
}

pub(super) enum Row {
    Heading(usize),
    Pair(Option<Line>, Option<Line>),
}

pub(super) struct Line {
    pub number: usize,
    pub text: String,
    /// Byte ranges, checked against this UTF-8 line.
    pub changed: Vec<Range<usize>>,
}

#[derive(Deserialize)]
struct Report {
    language: String,
    status: String,
    #[serde(default)]
    chunks: Vec<Vec<Pair>>,
}

#[derive(Deserialize)]
struct Pair {
    lhs: Option<Side>,
    rhs: Option<Side>,
}

#[derive(Deserialize)]
struct Side {
    line_number: usize,
    changes: Vec<Change>,
}

#[derive(Deserialize)]
struct Change {
    start: usize,
    end: usize,
    content: String,
}

const BAD_REPORT: &str =
    "Difftastic returned an incompatible syntax report. Update difft and try again.";

pub(super) fn parse(json: &[u8], old: &str, new: &str) -> Result<Structural, String> {
    let report: Report = serde_json::from_slice(json).map_err(|_| BAD_REPORT.to_owned())?;
    if report.language == "Text" {
        return Err("Difftastic doesn't support this file's language.".to_owned());
    }
    if report.language.starts_with("Text (") {
        return Err(format!("Difftastic used a text diff: {}.", report.language));
    }
    if report.language == "Binary" {
        return Err("Difftastic classified this file as binary.".to_owned());
    }
    if report.language.is_empty() || report.language.len() > 100 {
        return Err(BAD_REPORT.to_owned());
    }
    let (before, after): (Vec<_>, Vec<_>) = (old.split('\n').collect(), new.split('\n').collect());
    let mut rows = Vec::new();
    match report.status.as_str() {
        "unchanged" if report.chunks.is_empty() => {}
        "created" if old.is_empty() && !new.is_empty() => {
            full_side(&mut rows, &after, false)?;
        }
        "deleted" if !old.is_empty() && new.is_empty() => {
            full_side(&mut rows, &before, true)?;
        }
        "changed" if !report.chunks.is_empty() => {
            for (i, chunk) in report.chunks.into_iter().enumerate() {
                if chunk.is_empty() {
                    return Err(BAD_REPORT.to_owned());
                }
                let (mut left, mut right) = (BTreeMap::new(), BTreeMap::new());
                for pair in chunk {
                    if pair.lhs.is_none() && pair.rhs.is_none() {
                        return Err(BAD_REPORT.to_owned());
                    }
                    if let Some(side) = pair.lhs {
                        collect(side, &before, &mut left)?;
                    }
                    if let Some(side) = pair.rhs {
                        collect(side, &after, &mut right)?;
                    }
                }
                rows.push(Row::Heading(i + 1));
                // Each side stays in source order. Wrapping can put the novel tokens on
                // different lines; matching by row here would hide that very change.
                let (mut left, mut right) = (left.into_values(), right.into_values());
                loop {
                    let (a, b) = (left.next(), right.next());
                    if a.is_none() && b.is_none() {
                        break;
                    }
                    rows.push(Row::Pair(a, b));
                    if rows.len() > MAX_ROWS {
                        return Err("This syntax comparison has too many lines to show.".to_owned());
                    }
                }
            }
        }
        _ => return Err(BAD_REPORT.to_owned()),
    }
    let longest = rows
        .iter()
        .filter_map(|r| match r {
            Row::Pair(a, b) => Some([a.as_ref(), b.as_ref()]),
            _ => None,
        })
        .flatten()
        .flatten()
        .map(|l| {
            l.text
                .chars()
                .map(|c| if c == '\t' { 4 } else { c.len_utf8() })
                .sum()
        })
        .max()
        .unwrap_or(0);
    Ok(Structural {
        language: report.language,
        rows,
        unchanged: report.status == "unchanged",
        longest,
    })
}

fn collect(side: Side, source: &[&str], lines: &mut BTreeMap<usize, Line>) -> Result<(), String> {
    let text = source.get(side.line_number).ok_or(BAD_REPORT)?;
    if text.len() > 20_000 {
        return Err("A line is too long for syntax comparison.".to_owned());
    }
    let mut changed = Vec::new();
    for change in side.changes {
        if change.start >= change.end
            || text.get(change.start..change.end) != Some(change.content.as_str())
        {
            return Err(BAD_REPORT.to_owned());
        }
        changed.push(change.start..change.end);
    }
    let line = lines.entry(side.line_number).or_insert_with(|| Line {
        number: side.line_number + 1,
        text: (*text).to_owned(),
        changed: Vec::new(),
    });
    line.changed.extend(changed);
    line.changed.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in line.changed.drain(..) {
        if let Some(last) = merged.last_mut()
            && last.end >= r.start
        {
            last.end = last.end.max(r.end);
        } else {
            merged.push(r);
        }
    }
    line.changed = merged;
    Ok(())
}

fn full_side(rows: &mut Vec<Row>, lines: &[&str], old: bool) -> Result<(), String> {
    if lines.len() > MAX_ROWS {
        return Err("This syntax comparison has too many lines to show.".to_owned());
    }
    for (i, &text) in lines.iter().enumerate() {
        if text.len() > 20_000 {
            return Err("A line is too long for syntax comparison.".to_owned());
        }
        let line = Some(Line {
            number: i + 1,
            text: text.to_owned(),
            changed: if text.is_empty() {
                vec![]
            } else {
                std::iter::once(0..text.len()).collect()
            },
        });
        rows.push(if old {
            Row::Pair(line, None)
        } else {
            Row::Pair(None, line)
        });
    }
    Ok(())
}
