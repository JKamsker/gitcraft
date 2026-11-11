use super::*;

impl DiffData {
    /// Opens gap `g` as `how` says; returns how the rows moved, in order.
    pub fn unfold(&mut self, g: usize, how: Unfold) -> Vec<Shift> {
        let Some(gap) = self.context.as_ref().and_then(|c| c.gaps.get(g)).cloned() else {
            return Vec::new();
        };
        let hidden = gap.hidden();
        match how {
            Unfold::Up(n) => self.open_up(g, n.min(hidden)).into_iter().collect(),
            Unfold::Down(n) => self.open_down(g, n.min(hidden)),
            Unfold::All if gap.opens_down(g) => self.open_down(g, hidden),
            Unfold::All => self.open_up(g, hidden).into_iter().collect(),
            Unfold::Function => self.open_function(g),
        }
    }

    /// Shows the `n` lines right above the hunk below gap `g`: they become its first lines.
    pub(super) fn open_up(&mut self, g: usize, n: u32) -> Option<Shift> {
        let ctx = self.context.as_mut()?;
        let gap = ctx.gaps.get(g)?.clone();
        if n == 0 || gap.trailing {
            return None;
        }
        let header = self.lines.get(gap.row)?;
        let (os, on, ns, nn, tail) = header_numbers(&header.raw)?;
        let from = gap.last + 1 - n;
        let new = ctx.lines(from, gap.last, gap.old_offset)?;
        let first_old = old_of(from, gap.old_offset);
        let raw = format!("@@ -{first_old},{} +{from},{} @@{tail}", on + n, nn + n);
        // An empty range starts at the line before it; one with lines at its first line.
        let _ = (os, ns);
        ctx.gaps[g].last -= n;
        self.set_header(gap.row, raw);
        Some(self.insert(gap.row + 1, new))
    }

    /// Shows the `n` lines right below the hunk above gap `g`: they become its last lines (and
    /// past the end of the file, the note that its last line has no newline).
    pub(super) fn open_down(&mut self, g: usize, n: u32) -> Vec<Shift> {
        let Some(ctx) = self.context.as_mut() else {
            return Vec::new();
        };
        let Some(gap) = ctx.gaps.get(g).cloned() else {
            return Vec::new();
        };
        if n == 0 || !gap.opens_down(g) {
            return Vec::new();
        }
        let to = gap.first + n - 1;
        let Some(mut new) = ctx.lines(gap.first, to, gap.old_offset) else {
            return Vec::new();
        };
        if gap.trailing && to as usize == ctx.raw.len() && ctx.no_newline_at_end {
            new.push((
                super::super::parse::meta("\\ No newline at end of file"),
                None,
            ));
        }
        ctx.gaps[g].first += n;
        let emptied = ctx.gaps[g].hidden() == 0;
        // The hunk above gets `n` more lines on both sides.
        if let Some(h) = self.lines[..gap.row]
            .iter()
            .rposition(|l| l.kind == Kind::Hunk)
            && let Some((os, on, ns, nn, tail)) = header_numbers(&self.lines[h].raw)
        {
            // An empty range is numbered by the line before it; one with lines by its first.
            let start = |s: u32, count: u32| if count == 0 { s + 1 } else { s };
            let raw = format!(
                "@@ -{},{} +{},{} @@{tail}",
                start(os, on),
                on + n,
                start(ns, nn),
                nn + n
            );
            self.set_header(h, raw);
        }
        let mut shifts = vec![self.insert(gap.row, new)];
        if gap.trailing && emptied {
            let row = shifts[0].row(gap.row);
            shifts.extend(self.remove_fold_row(row));
        }
        shifts
    }

    /// Opens gap `g` and the one after it as far as the function of the hunk between them.
    pub(super) fn open_function(&mut self, g: usize) -> Vec<Shift> {
        let Some(ctx) = self.context.as_ref() else {
            return Vec::new();
        };
        let Some(gap) = ctx.gaps.get(g) else {
            return Vec::new();
        };
        let Some(header) = self.lines.get(gap.row) else {
            return Vec::new();
        };
        let Some((_, _, ns, nn, _)) = header_numbers(&header.raw) else {
            return Vec::new();
        };
        let Some((start, end)) = gap.function else {
            return Vec::new();
        };
        let up = if start < gap.first {
            gap.hidden()
        } else {
            (gap.last + 1).saturating_sub(start).min(gap.hidden())
        };
        let hunk_end = before(ns, nn) + nn;
        let down = ctx
            .gaps
            .get(g + 1)
            .map_or(0, |next| end.saturating_sub(hunk_end).min(next.hidden()));
        let mut shifts: Vec<Shift> = self.open_up(g, up).into_iter().collect();
        shifts.extend(self.open_down(g + 1, down));
        shifts
    }

    /// Puts `new` lines in at row `at`, moving everything after them down.
    pub(super) fn insert(&mut self, at: usize, new: Vec<(DiffLine, Option<Spans>)>) -> Shift {
        let shift = Shift {
            at,
            count: new.len() as isize,
        };
        let mut lines = Vec::with_capacity(new.len());
        let mut spans = Vec::with_capacity(new.len());
        for (mut line, mut span) in new {
            if let Some(whole) = highlight::cap_line(&mut line.text, span.as_mut()) {
                self.uncut.insert(usize::MAX - lines.len(), whole);
            }
            self.longest = self.longest.max(line.text.chars().count());
            lines.push(line);
            spans.push(span);
        }
        let count = lines.len();
        self.shift_rows(shift);
        // The cut lines were noted at stand-in rows; now that the rows moved, at their own.
        for k in 0..count {
            if let Some(whole) = self.uncut.remove(&(usize::MAX - k)) {
                self.uncut.insert(at + k, whole);
            }
        }
        let pos = self
            .split
            .iter()
            .position(|&(l, r)| l.max(r).is_some_and(|i| i >= at + count))
            .unwrap_or(self.split.len());
        self.split
            .splice(pos..pos, (at..at + count).map(|i| (Some(i), Some(i))));
        self.lines.splice(at..at, lines);
        self.spans.splice(at..at, spans);
        self.changed
            .splice(at..at, std::iter::repeat_n(None, count));
        // Without moves (a combined diff) there is no table of them per line.
        if at <= self.moves.of_line.len() {
            self.moves
                .of_line
                .splice(at..at, std::iter::repeat_n(None, count));
        }
        shift
    }

    /// Takes out the trailing fold row at `row`, once it hides nothing.
    pub(super) fn remove_fold_row(&mut self, row: usize) -> Option<Shift> {
        if self.lines.get(row)?.kind != Kind::Fold {
            return None;
        }
        self.lines.remove(row);
        self.spans.remove(row);
        self.changed.remove(row);
        if row < self.moves.of_line.len() {
            self.moves.of_line.remove(row);
        }
        self.split
            .retain(|&(l, r)| l != Some(row) && r != Some(row));
        let shift = Shift {
            at: row + 1,
            count: -1,
        };
        self.shift_rows(shift);
        Some(shift)
    }

    /// Renumbers what points at rows from `shift.at` on.
    pub(super) fn shift_rows(&mut self, shift: Shift) {
        let row = |i: usize| shift.row(i);
        for (l, r) in &mut self.split {
            *l = l.map(row);
            *r = r.map(row);
        }
        let uncut = std::mem::take(&mut self.uncut);
        self.uncut = uncut
            .into_iter()
            .map(|(i, s)| {
                if i > usize::MAX / 2 {
                    (i, s)
                } else {
                    (row(i), s)
                }
            })
            .collect();
        // A block's end is past its last line: it moves with that line, never over rows put
        // in right after it.
        let range = |r: &std::ops::Range<usize>| row(r.start)..row(r.end - 1) + 1;
        for block in &mut self.moves.blocks {
            block.del = range(&block.del);
            block.add = range(&block.add);
        }
        if let Some(ctx) = &mut self.context {
            for gap in &mut ctx.gaps {
                gap.row = row(gap.row);
            }
        }
        super::super::presentation::invalidate_rows(self);
    }

    /// Writes a hunk header anew, as shown and as patches read it.
    pub(super) fn set_header(&mut self, row: usize, raw: String) {
        if let Some(line) = self.lines.get_mut(row) {
            line.text = raw.trim_end_matches('\r').to_owned();
            line.raw = raw;
        }
    }
}
