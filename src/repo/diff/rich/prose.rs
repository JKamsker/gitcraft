//! Prose: the paragraphs of a text file (Markdown, AsciiDoc, reStructuredText, plain text)
//! before and after the change, paired up, with the words that changed in each. A paragraph
//! only wrapped differently is the same paragraph.

use super::Sides;
use super::words::{self, Change};

/// The extensions read as prose.
pub const EXTENSIONS: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "adoc", "asciidoc", "asc", "rst", "txt",
    "text",
];
/// Paragraphs at least this much alike (by their words) are one paragraph edited.
const SAME_PARAGRAPH: f32 = 0.5;
/// How far ahead among the added paragraphs the edited version of a removed one is looked for.
const LOOK_AHEAD: usize = 6;
/// How many pairs of words may be compared in all to pair edited paragraphs; past it, the rest
/// show removed and added (a huge rewrite would otherwise take seconds).
const PAIRING_BUDGET: usize = 20_000_000;
/// Past this many pairs of paragraphs to compare, the middle that differs is removed then added.
const MAX_CELLS: usize = 4_000_000;

/// A paragraph: its lines, and the number of its first line.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Para {
    text: String,
    line: u32,
}

impl Para {
    fn words(&self) -> Vec<&str> {
        words::words(&self.text)
    }

    /// The paragraph compared by its words, however it is wrapped.
    fn key(&self) -> String {
        self.words().join(" ")
    }
}

/// A part of the text after the change, or of what it removed.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// The same words on both sides; `rewrapped` when its lines are cut differently.
    Kept {
        old: u32,
        new: u32,
        text: String,
        rewrapped: bool,
    },
    Removed {
        old: u32,
        text: String,
    },
    Added {
        new: u32,
        text: String,
    },
    /// Edited: its words, each kept, removed or added.
    Changed {
        old: u32,
        new: u32,
        words: Vec<(Change, String)>,
    },
}

/// The change as prose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProseDiff {
    pub parts: Vec<Part>,
}

/// The paragraphs of `text`: runs of lines between blank ones.
fn paragraphs(text: &str) -> Vec<Para> {
    let mut out = Vec::new();
    let mut current: Option<Para> = None;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            out.extend(current.take());
            continue;
        }
        match &mut current {
            Some(p) => {
                p.text.push('\n');
                p.text.push_str(line);
            }
            None => {
                current = Some(Para {
                    text: line.to_owned(),
                    line: n as u32 + 1,
                })
            }
        }
    }
    out.extend(current);
    out
}

/// The text's change paragraph by paragraph.
pub fn diff(sides: &Sides) -> ProseDiff {
    let old = sides.old.as_deref().map(paragraphs).unwrap_or_default();
    let new = sides.new.as_deref().map(paragraphs).unwrap_or_default();
    let old_keys: Vec<String> = old.iter().map(Para::key).collect();
    let new_keys: Vec<String> = new.iter().map(Para::key).collect();
    let mut parts = Vec::new();
    let mut budget = PAIRING_BUDGET;
    let mut removed: Vec<&Para> = Vec::new();
    let mut added: Vec<&Para> = Vec::new();
    for step in align(&old_keys, &new_keys) {
        match step {
            (Some(i), Some(j)) => {
                pair_up(&mut parts, &mut removed, &mut added, &mut budget);
                parts.push(Part::Kept {
                    old: old[i].line,
                    new: new[j].line,
                    text: new[j].text.clone(),
                    rewrapped: old[i].text != new[j].text,
                });
            }
            (Some(i), None) => removed.push(&old[i]),
            (None, Some(j)) => added.push(&new[j]),
            (None, None) => {}
        }
    }
    pair_up(&mut parts, &mut removed, &mut added, &mut budget);
    ProseDiff { parts }
}

/// The paragraphs removed and added between two kept ones: each removed one paired with the
/// added one it became, when there is one alike enough, in the order of the text after.
fn pair_up<'a>(
    parts: &mut Vec<Part>,
    removed: &mut Vec<&'a Para>,
    added: &mut Vec<&'a Para>,
    budget: &mut usize,
) {
    let mut next_added = 0;
    for r in removed.drain(..) {
        let rw = r.words();
        let found = (next_added..added.len().min(next_added + LOOK_AHEAD)).find(|&k| {
            let aw = added[k].words();
            let cost = rw.len().saturating_mul(aw.len());
            if cost > *budget {
                return false;
            }
            *budget -= cost;
            words::similarity(&rw, &aw) >= SAME_PARAGRAPH
        });
        match found {
            Some(k) => {
                for a in &added[next_added..k] {
                    parts.push(Part::Added {
                        new: a.line,
                        text: a.text.clone(),
                    });
                }
                parts.push(Part::Changed {
                    old: r.line,
                    new: added[k].line,
                    words: words::diff_text(&r.text, &added[k].text),
                });
                next_added = k + 1;
            }
            None => parts.push(Part::Removed {
                old: r.line,
                text: r.text.clone(),
            }),
        }
    }
    for a in &added[next_added..] {
        parts.push(Part::Added {
            new: a.line,
            text: a.text.clone(),
        });
    }
    added.clear();
}

/// Whether `n` items against `m` can be aligned item by item (or [`align`] gives up past
/// what both start and end with).
pub(super) fn fits(n: usize, m: usize) -> bool {
    n.saturating_mul(m) <= MAX_CELLS
}

/// `old` against `new` in order: pairs of equal items, and those on one side only.
pub(super) fn align<T: PartialEq>(old: &[T], new: &[T]) -> Vec<(Option<usize>, Option<usize>)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<(Option<usize>, Option<usize>)> =
        (0..prefix).map(|i| (Some(i), Some(i))).collect();
    let (n, m) = (old.len() - prefix - suffix, new.len() - prefix - suffix);
    if n.saturating_mul(m) > MAX_CELLS {
        out.extend((0..n).map(|i| (Some(prefix + i), None)));
        out.extend((0..m).map(|j| (None, Some(prefix + j))));
    } else {
        let same = |i: usize, j: usize| old[prefix + i] == new[prefix + j];
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if same(i, j) {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && same(i, j) {
                out.push((Some(prefix + i), Some(prefix + j)));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                out.push((Some(prefix + i), None));
                i += 1;
            } else {
                out.push((None, Some(prefix + j)));
                j += 1;
            }
        }
    }
    out.extend((0..suffix).map(|k| (Some(old.len() - suffix + k), Some(new.len() - suffix + k))));
    out
}
