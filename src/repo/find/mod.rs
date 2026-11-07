//! Find in what the central pane shows: a diff (unified or side by side), a file or its blame.
//! Ctrl+F there opens the find bar instead of the commit search; every match is marked, Enter
//! and F3 step through them (Shift for back), and Esc closes it.

mod bar;
mod keys;
mod marks;
mod matcher;
mod order;
pub(in crate::repo) mod rendered;
mod scan;
mod sources;
#[cfg(test)]
mod tests;

pub(crate) use marks::{LineMarks, paint_line, paint_marks};
pub(crate) use matcher::Query;
pub(crate) use order::{Hit, Layout};
use scan::{BUDGET, Scan};
pub(crate) use scan::{Source, SourceKey};
pub(crate) use sources::search_diff;

/// Where the view showing the lines was scrolled last frame, to start a search from where the
/// reader is and to scroll only when the match isn't in view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Seen {
    /// The scroll offsets.
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// The height of the lines' viewport and the width the text has in it.
    pub(crate) height: f32,
    pub(crate) text_w: f32,
    pub(crate) line_h: f32,
}

impl Seen {
    /// The first row in view.
    fn top_row(&self) -> usize {
        if self.line_h > 0.0 {
            (self.y / self.line_h).ceil() as usize
        } else {
            0
        }
    }
}

/// The find bar of a repository's tab and its search.
#[derive(Default)]
pub(crate) struct Find {
    rendered_document: Option<(String, rendered::Document)>,
    pub(crate) open: bool,
    /// Give the text field the keyboard on the next frame.
    pub(crate) focus: bool,
    pub(crate) query: Query,
    scan: Option<Scan>,
    /// The matches in the order of the rows shown, what that order is of (the scan's number,
    /// side by side) and how many of the scan's lines with matches it holds.
    order: Vec<Hit>,
    order_of: Option<(u64, bool)>,
    ordered_lines: usize,
    /// Numbers the scans, so that the order follows a new one.
    scans: u64,
    /// The match stepped to.
    current: Option<Hit>,
    /// The match that was current before the lines were read again, to find again.
    pending: Option<Hit>,
    /// The search runs again on its own (the lines were read again): it neither moves the
    /// current match nor scrolls.
    quiet: bool,
    /// Scroll to the current match on the next frame.
    reveal: bool,
    /// Lines were searched since the bar was last drawn.
    searched: bool,
    seen: Seen,
}

impl Find {
    /// Opens the bar (or takes the keyboard back to it) with its text selected.
    pub(crate) fn show(&mut self) {
        self.open = true;
        self.focus = true;
    }

    /// Closes the bar; the text stays for the next time.
    pub(crate) fn close(&mut self) {
        self.open = false;
        self.focus = false;
        self.drop_results();
        self.rendered_document = None;
    }

    /// Forgets the search: nothing shown has lines to search (an image, a spinner…).
    fn drop_results(&mut self) {
        self.scan = None;
        self.order.clear();
        self.order_of = None;
        self.current = None;
        self.pending = None;
    }

    /// Called as the bar is drawn: the results of a view that no longer searches (another
    /// file without lines took its place) are dropped, rather than shown and stepped through.
    fn drop_unsearched(&mut self) {
        if !std::mem::take(&mut self.searched) {
            self.drop_results();
        }
    }

    /// Searches the lines shown, on from where the last frame stopped. `line`: the text of
    /// line `i` of `total` (`None` for those not searched); `layout`: how they show in rows.
    pub(crate) fn update<'a>(
        &mut self,
        ctx: &egui::Context,
        key: SourceKey,
        total: usize,
        line: impl Fn(usize) -> Option<&'a str>,
        layout: Layout<'_>,
    ) {
        if !self.open {
            return;
        }
        self.searched = true;
        let stale = self
            .scan
            .as_ref()
            .is_none_or(|s| s.key != key || s.query != self.query);
        if stale {
            let old = self.scan.as_ref();
            let same_view = old.is_some_and(|s| s.key.what == key.what);
            // The same lines read again (a file saved, a hunk staged): the match stays.
            self.quiet = same_view && old.is_some_and(|s| s.query == self.query);
            self.pending = if self.quiet { self.current } else { None };
            if old.is_some_and(|s| s.key.what != key.what) {
                // Another view: where the last one was scrolled is no guide.
                self.seen = Seen::default();
            }
            // A new search starts from where the reader is.
            self.current = None;
            self.scans += 1;
            self.scan = Some(Scan::new(key, &self.query, total));
        }
        let Some(scan) = &mut self.scan else { return };
        scan.advance(line, BUDGET);
        if !scan.done() {
            ctx.request_repaint();
        }
        self.order_matches(layout);
        let done = self.scan.as_ref().is_some_and(Scan::done);
        self.settle_current(done);
    }

    /// Puts the matches found so far in the order of the rows: a line per row, only the new
    /// ones are added; side by side, rows mix lines from far apart, so they are ordered once
    /// all are found.
    fn order_matches(&mut self, layout: Layout<'_>) {
        let Some(scan) = &self.scan else { return };
        let of = (self.scans, layout.is_split());
        if self.order_of != Some(of) {
            self.order.clear();
            self.ordered_lines = 0;
            self.order_of = Some(of);
        }
        match layout {
            Layout::Lines => {
                let new = scan.found.get(self.ordered_lines..).unwrap_or_default();
                self.order.extend(order::ordered(new, layout));
                self.ordered_lines = scan.found.len();
            }
            Layout::Split(_) if scan.done() && self.ordered_lines != scan.found.len() => {
                self.order = order::ordered(&scan.found, layout);
                self.ordered_lines = scan.found.len();
            }
            Layout::Split(_) => {}
        }
    }

    /// Keeps the current match one of the matches: the first at or below the top of the view
    /// when there is none yet (or the earlier ones, once all are found and none is below).
    fn settle_current(&mut self, done: bool) {
        // Read again: the match that was current, once it is found again.
        if let Some(hit) = self.pending {
            if let Some(found) = self.position_of(&hit) {
                self.current = self.order.get(found).copied();
                self.pending = None;
                return;
            }
            if !done {
                return;
            }
            self.pending = None;
        }
        if let Some(hit) = self.current {
            if let Some(found) = self.position_of(&hit) {
                // Rows change when the layout does: the same match, where it is now.
                self.current = self.order.get(found).copied();
                return;
            }
            self.current = None;
        }
        let top = self.seen.top_row();
        let below = self.order.iter().find(|h| h.row >= top).copied();
        let pick = below.or_else(|| done.then(|| self.order.first().copied()).flatten());
        if pick.is_some() {
            self.current = pick;
            self.reveal = !self.quiet;
        }
    }

    /// Where `hit` is among the matches in order, looked up by its line and characters (its
    /// row differs between unified and side by side).
    fn position_of(&self, hit: &Hit) -> Option<usize> {
        let same = |h: &Hit| h.line == hit.line && h.start == hit.start && h.end == hit.end;
        // Ordered by row, where the rows are of the layout the hit came from: fast unless the
        // layout changed.
        match self.order.binary_search(hit) {
            Ok(i) => Some(i),
            Err(_) => self.order.iter().position(same),
        }
    }

    /// Steps `delta` matches on (back when negative), round from the last to the first.
    pub(crate) fn step(&mut self, delta: i32) {
        let n = self.order.len();
        if n == 0 {
            return;
        }
        let at = self.current.and_then(|h| self.position_of(&h));
        let next = match at {
            Some(i) => (i as i64 + i64::from(delta)).rem_euclid(n as i64) as usize,
            None if delta < 0 => n - 1,
            None => 0,
        };
        self.current = self.order.get(next).copied();
        self.reveal = true;
    }

    /// The match to scroll to this frame, if the last step asked for it.
    pub(crate) fn take_reveal(&mut self) -> Option<Hit> {
        if !self.open {
            return None;
        }
        std::mem::take(&mut self.reveal)
            .then_some(self.current)
            .flatten()
    }

    /// Notes where the view was scrolled this frame.
    pub(crate) fn saw(&mut self, seen: Seen) {
        self.seen = seen;
    }

    /// The scroll offsets (down, sideways) that bring `hit` in `text` (its line) into view, a
    /// character being `char_w` wide; `None` for a direction it's already in view in.
    pub(crate) fn offsets_for(
        &self,
        hit: &Hit,
        text: &str,
        char_w: f32,
    ) -> (Option<f32>, Option<f32>) {
        let s = self.seen;
        let top = hit.row as f32 * s.line_h;
        let y = (top < s.y || top + s.line_h > s.y + s.height)
            .then(|| (top - (s.height - s.line_h) / 2.0).max(0.0));
        let (start, end) = (columns(text, hit.start), columns(text, hit.end));
        let (left, right) = (start as f32 * char_w, end as f32 * char_w);
        let x = (left < s.x || right > s.x + s.text_w).then(|| (left - s.text_w / 3.0).max(0.0));
        (y, x)
    }

    /// `scroll` scrolled to `hit` in `text` (its line), where it isn't in view.
    pub(crate) fn scroll_to(
        &self,
        scroll: egui::ScrollArea,
        hit: &Hit,
        text: &str,
        char_w: f32,
    ) -> egui::ScrollArea {
        let (y, x) = self.offsets_for(hit, text, char_w);
        let scroll = match y {
            Some(y) => scroll.vertical_scroll_offset(y),
            None => scroll,
        };
        match x {
            Some(x) => scroll.horizontal_scroll_offset(x),
            None => scroll,
        }
    }

    /// The matches to mark on line `i`.
    pub(crate) fn marks(&self, i: usize) -> Option<LineMarks<'_>> {
        let ranges = self.scan.as_ref().filter(|_| self.open)?.on_line(i);
        if ranges.is_empty() {
            return None;
        }
        let current = self
            .current
            .filter(|h| h.line == i)
            .map(|h| h.chars())
            .and_then(|c| ranges.iter().position(|r| *r == c));
        Some(LineMarks { ranges, current })
    }

    /// What the bar says: the error, the search going on, nothing found, or which match of
    /// how many is the current one. `None` when nothing is typed.
    pub(crate) fn summary(&self) -> Option<(String, bool)> {
        let scan = self.scan.as_ref()?;
        if let Some(e) = &scan.error {
            return Some((e.clone(), true));
        }
        if scan.query.text.is_empty() {
            return None;
        }
        let n = self.order.len();
        let more = if scan.capped { "+" } else { "" };
        let text = match (
            n,
            scan.done(),
            self.current.and_then(|h| self.position_of(&h)),
        ) {
            (0, true, _) => "no results".to_owned(),
            (0, false, _) => {
                let (at, of) = scan.progress();
                format!("searching… {}%", at * 100 / of.max(1))
            }
            (_, true, Some(i)) => format!("{} of {n}{more}", i + 1),
            (_, false, Some(i)) => format!("{} of {n}…", i + 1),
            (_, true, None) => format!("{n}{more} {}", results(n)),
            (_, false, None) => format!("{n} {}…", results(n)),
        };
        Some((text, false))
    }

    /// Whether the search stopped at the most matches it keeps.
    pub(crate) fn capped(&self) -> bool {
        self.scan.as_ref().is_some_and(|s| s.capped)
    }

    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<Hit> {
        self.current
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.order.len()
    }
}

/// "result" or "results", for `n` of them.
fn results(n: usize) -> &'static str {
    if n == 1 { "result" } else { "results" }
}

/// The columns the first `chars` characters of `text` take: a tab four (as it is drawn).
fn columns(text: &str, chars: usize) -> usize {
    text.chars()
        .take(chars)
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// The text of line `i` of a list of lines.
pub(crate) fn nth<S: AsRef<str>>(lines: &[S], i: usize) -> Option<&str> {
    lines.get(i).map(AsRef::as_ref)
}
