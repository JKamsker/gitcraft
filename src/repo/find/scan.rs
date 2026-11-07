//! Searching the lines of what is shown a little each frame, so that a file of a million lines
//! is searched without the window stopping: the matches found so far show at once, and more
//! come in on the next frames.

use std::ops::Range;
use std::time::{Duration, Instant};

use super::matcher::{Matcher, Query};
use crate::repo::highlight::MAX_LINE_CHARS;

/// The time a frame spends searching at most.
pub(super) const BUDGET: Duration = Duration::from_millis(4);
/// Lines searched between two looks at the clock.
const CHUNK: usize = 64;
/// Matches kept at most: beyond this, the search stops and says there are more.
pub(crate) const MAX_HITS: usize = 100_000;

/// What the lines searched are, to search again when they change: what is shown, where its
/// lines are in memory and how many there are (a diff read again has new lines elsewhere).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceKey {
    pub(crate) what: Source,
    pub(crate) at: usize,
    pub(crate) len: usize,
}

/// The view the lines are of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// A diff's lines (by its file).
    Diff(String),
    /// The file of a diff switched to File.
    DiffFile(String),
    Blame {
        path: String,
        rev: Option<String>,
    },
    File {
        path: String,
        rev: Option<String>,
    },
}

/// One search of one view's lines, done a little at a time.
pub(super) struct Scan {
    pub(super) key: SourceKey,
    pub(super) query: Query,
    matcher: Option<Matcher>,
    /// Why the query can't be searched (a broken regular expression).
    pub(super) error: Option<String>,
    /// The next line to search, and how many there are.
    next: usize,
    total: usize,
    /// The lines with matches, in order, and the characters matched in each.
    pub(super) found: Vec<(usize, Vec<Range<usize>>)>,
    pub(super) count: usize,
    /// Stopped at [`MAX_HITS`].
    pub(super) capped: bool,
}

impl Scan {
    pub(super) fn new(key: SourceKey, query: &Query, total: usize) -> Self {
        let (matcher, error) = match Matcher::new(query) {
            Ok(m) => (m, None),
            Err(e) => (None, Some(e)),
        };
        Self {
            key,
            query: query.clone(),
            matcher,
            error,
            next: 0,
            total,
            found: Vec::new(),
            count: 0,
            capped: false,
        }
    }

    /// Every line has been searched (or there is nothing to search for).
    pub(super) fn done(&self) -> bool {
        self.matcher.is_none() || self.capped || self.next >= self.total
    }

    /// Searches on from where the last frame stopped, for about `budget`. `line`: the text of
    /// line `i`, `None` for those not searched (a diff's hunk headers).
    pub(super) fn advance<'a>(
        &mut self,
        line: impl Fn(usize) -> Option<&'a str>,
        budget: Duration,
    ) {
        let Some(matcher) = &self.matcher else { return };
        let started = Instant::now();
        while !self.done() {
            let end = (self.next + CHUNK).min(self.total);
            for i in self.next..end {
                let Some(text) = line(i) else { continue };
                // A line cut to be shown has a note on its end, which isn't searched.
                let text = text
                    .char_indices()
                    .nth(MAX_LINE_CHARS)
                    .map_or(text, |(at, _)| &text[..at]);
                let mut hits = matcher.find(text);
                if hits.is_empty() {
                    continue;
                }
                let room = MAX_HITS - self.count;
                if hits.len() >= room {
                    hits.truncate(room);
                    self.capped = true;
                }
                self.count += hits.len();
                self.found.push((i, hits));
                if self.capped {
                    break;
                }
            }
            self.next = end;
            if started.elapsed() >= budget {
                break;
            }
        }
    }

    /// The characters matched on line `i`.
    pub(super) fn on_line(&self, i: usize) -> &[Range<usize>] {
        self.found
            .binary_search_by_key(&i, |(line, _)| *line)
            .map_or(&[], |k| self.found[k].1.as_slice())
    }

    /// How far the search is: lines searched of all of them.
    pub(super) fn progress(&self) -> (usize, usize) {
        (self.next.min(self.total), self.total)
    }
}

#[cfg(test)]
// Expected matches are lists of ranges, some of one.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn key(len: usize) -> SourceKey {
        SourceKey {
            what: Source::Diff("a".into()),
            at: 0,
            len,
        }
    }

    fn query(text: &str) -> Query {
        Query {
            text: text.to_owned(),
            ..Query::default()
        }
    }

    #[test]
    fn a_scan_goes_on_where_it_stopped_and_skips_lines_not_searched() {
        let lines: Vec<String> = (0..1000).map(|i| format!("line {i}")).collect();
        let mut scan = Scan::new(key(lines.len()), &query("7"), lines.len());
        // No time at all: one chunk per call.
        let mut calls = 0;
        while !scan.done() {
            scan.advance(|i| lines.get(i).map(String::as_str), Duration::ZERO);
            calls += 1;
        }
        assert_eq!(calls, 1000_usize.div_ceil(CHUNK));
        assert_eq!(scan.progress(), (1000, 1000));
        // 7, 17 … 97, 70 … 79, then 107 … in the hundreds: 271 lines have a 7, 300 sevens.
        assert_eq!(scan.count, 300);
        assert_eq!(scan.found.len(), 271);
        assert_eq!(scan.on_line(77), [5..6, 6..7]);
        assert!(scan.on_line(80).is_empty());

        let mut scan = Scan::new(key(lines.len()), &query("line"), lines.len());
        scan.advance(
            |i| (i % 2 == 0).then(|| lines[i].as_str()),
            Duration::from_secs(5),
        );
        assert!(scan.done());
        assert_eq!(scan.count, 500);
    }

    #[test]
    fn a_scan_stops_at_the_most_matches_kept() {
        let line = "x".repeat(1000);
        let total = MAX_HITS / 1000 + 10;
        let mut scan = Scan::new(key(total), &query("x"), total);
        scan.advance(|_| Some(line.as_str()), Duration::from_secs(30));
        assert!(scan.done() && scan.capped);
        assert_eq!(scan.count, MAX_HITS);
    }

    #[test]
    fn nothing_to_search_for_or_a_broken_pattern_is_done_at_once() {
        let scan = Scan::new(key(5), &query(""), 5);
        assert!(scan.done() && scan.error.is_none());
        let mut q = query("(");
        q.regex = true;
        let scan = Scan::new(key(5), &q, 5);
        assert!(scan.done() && scan.error.is_some());
        let scan = Scan::new(key(0), &query("a"), 0);
        assert!(scan.done() && scan.found.is_empty());
    }
}
