//! The tab's recent views, for back and forward: a bounded list of places with the one shown
//! among them, like a browser's history. It only keeps the places; showing one is the tab's
//! business (see `navigation`).

/// How many places are kept; the oldest go first.
pub(super) const KEEP: usize = 50;

/// The places visited, oldest first, and which one is shown.
#[derive(Clone, Debug)]
pub(super) struct NavHistory<T> {
    entries: Vec<T>,
    /// Index of the place shown in `entries` (meaningless while it is empty).
    at: usize,
    keep: usize,
}

impl<T> Default for NavHistory<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            at: 0,
            keep: KEEP,
        }
    }
}

impl<T: Clone + PartialEq> NavHistory<T> {
    /// A history keeping at most `keep` places (at least one).
    #[cfg(test)]
    pub(super) fn with_capacity(keep: usize) -> Self {
        Self {
            entries: Vec::new(),
            at: 0,
            keep: keep.max(1),
        }
    }

    /// The place shown, if any was visited.
    pub(super) fn current(&self) -> Option<&T> {
        self.entries.get(self.at)
    }

    /// Goes to a new place: the places that were forward of the one shown are dropped, as in a
    /// browser. The place already shown is not added twice.
    pub(super) fn visit(&mut self, entry: T) {
        if self.current() == Some(&entry) {
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.at + 1);
        }
        self.entries.push(entry);
        if self.entries.len() > self.keep {
            let extra = self.entries.len() - self.keep;
            self.entries.drain(..extra);
        }
        self.at = self.entries.len() - 1;
    }

    /// The place shown became `entry` without going anywhere (another commit selected in the
    /// same graph, or what a step back could really show): the places back and forward stay.
    /// Should it become the same as the place before or after it, the two are one: a step back
    /// or forward always goes somewhere else.
    pub(super) fn replace_current(&mut self, entry: T) {
        match self.entries.get_mut(self.at) {
            Some(current) => *current = entry,
            None => return self.visit(entry),
        }
        self.merge_around(self.at);
    }

    /// Drops the place shown, which turned out to be gone, while stepping `step` (`-1` back,
    /// `1` forward) through it: the place the step started from is the one shown again, so
    /// stepping on the same way goes past it. Places it left side by side and equal are one.
    pub(super) fn drop_current(&mut self, step: i32) {
        if self.entries.is_empty() {
            return;
        }
        let gone = self.at;
        self.entries.remove(gone);
        if self.entries.is_empty() {
            self.at = 0;
            return;
        }
        // Back: the start was after it, and moved down into its index; forward: before it.
        self.at = if step < 0 {
            gone.min(self.entries.len() - 1)
        } else {
            gone.saturating_sub(1)
        };
        // The two places either side of the gap.
        if gone > 0 && gone < self.entries.len() && self.entries[gone - 1] == self.entries[gone] {
            self.entries.remove(gone);
            if self.at >= gone {
                self.at -= 1;
            }
        }
    }

    /// Makes the place at `i` one with an equal neighbour, keeping it shown if it was.
    fn merge_around(&mut self, i: usize) {
        if i + 1 < self.entries.len() && self.entries[i + 1] == self.entries[i] {
            self.entries.remove(i + 1);
        }
        if i > 0 && self.entries[i - 1] == self.entries[i] {
            self.entries.remove(i);
            if self.at >= i {
                self.at -= 1;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn can_back(&self) -> bool {
        !self.entries.is_empty() && self.at > 0
    }

    #[cfg(test)]
    pub(super) fn can_forward(&self) -> bool {
        self.at + 1 < self.entries.len()
    }

    /// The place a step back (`-1`) or forward (`1`) would show, without going there.
    pub(super) fn peek(&self, step: i32) -> Option<&T> {
        let to = self.target(step)?;
        self.entries.get(to)
    }

    /// Goes a step back (`-1`) or forward (`1`), and gives the place to show; `None` (and
    /// nothing changes) when there is nothing that way.
    pub(super) fn go(&mut self, step: i32) -> Option<&T> {
        let to = self.target(step)?;
        self.at = to;
        self.entries.get(to)
    }

    /// The index `step` places from the one shown, if there is one.
    fn target(&self, step: i32) -> Option<usize> {
        if self.entries.is_empty() || step == 0 {
            return None;
        }
        let to = self.at.checked_add_signed(step as isize)?;
        (to < self.entries.len()).then_some(to)
    }

    #[cfg(test)]
    pub(super) fn entries(&self) -> &[T] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::NavHistory;

    fn history(places: &[u32]) -> NavHistory<u32> {
        let mut h = NavHistory::with_capacity(10);
        for &p in places {
            h.visit(p);
        }
        h
    }

    #[test]
    fn an_empty_history_goes_nowhere() {
        let mut h: NavHistory<u32> = NavHistory::default();
        assert_eq!(h.current(), None);
        assert!(!h.can_back());
        assert!(!h.can_forward());
        assert_eq!(h.go(-1), None);
        assert_eq!(h.go(1), None);
        assert_eq!(h.peek(-1), None);
        assert_eq!(h.current(), None);
    }

    #[test]
    fn one_place_has_nothing_back_or_forward() {
        let mut h = history(&[1]);
        assert_eq!(h.current(), Some(&1));
        assert!(!h.can_back());
        assert!(!h.can_forward());
        assert_eq!(h.go(-1), None);
        assert_eq!(h.go(0), None);
        assert_eq!(h.current(), Some(&1));
    }

    #[test]
    fn back_and_forward_walk_the_places() {
        let mut h = history(&[1, 2, 3]);
        assert_eq!(h.current(), Some(&3));
        assert_eq!(h.peek(-1), Some(&2));
        assert_eq!(h.go(-1), Some(&2));
        assert_eq!(h.go(-1), Some(&1));
        assert!(!h.can_back());
        assert_eq!(h.go(-1), None);
        assert_eq!(h.current(), Some(&1));
        assert!(h.can_forward());
        assert_eq!(h.peek(1), Some(&2));
        assert_eq!(h.go(1), Some(&2));
        assert_eq!(h.go(1), Some(&3));
        assert!(!h.can_forward());
        assert_eq!(h.go(1), None);
        assert_eq!(h.entries(), &[1, 2, 3]);
    }

    #[test]
    fn visiting_after_going_back_drops_what_was_forward() {
        let mut h = history(&[1, 2, 3, 4]);
        h.go(-1);
        h.go(-1);
        h.visit(9);
        assert_eq!(h.entries(), &[1, 2, 9]);
        assert_eq!(h.current(), Some(&9));
        assert!(!h.can_forward());
        assert_eq!(h.go(-1), Some(&2));
    }

    #[test]
    fn the_place_shown_is_not_added_twice() {
        let mut h = history(&[1, 2, 2, 2]);
        assert_eq!(h.entries(), &[1, 2]);
        // Not even after going back: visiting the place shown keeps what is forward.
        h.go(-1);
        h.visit(1);
        assert_eq!(h.entries(), &[1, 2]);
        assert!(h.can_forward());
        // A place seen earlier, not the one shown, is a new visit.
        h.go(1);
        h.visit(1);
        assert_eq!(h.entries(), &[1, 2, 1]);
    }

    #[test]
    fn only_the_latest_places_are_kept() {
        let mut h = NavHistory::with_capacity(3);
        for p in 1..=5 {
            h.visit(p);
        }
        assert_eq!(h.entries(), &[3, 4, 5]);
        assert_eq!(h.go(-1), Some(&4));
        assert_eq!(h.go(-1), Some(&3));
        assert_eq!(h.go(-1), None);
        // A capacity of 0 still keeps the place shown.
        let mut h = NavHistory::with_capacity(0);
        h.visit(1);
        h.visit(2);
        assert_eq!(h.entries(), &[2]);
        let h: NavHistory<u32> = NavHistory::default();
        assert_eq!(h.keep, super::KEEP);
    }

    #[test]
    fn replacing_the_place_shown_keeps_back_and_forward() {
        let mut h = history(&[1, 2, 3]);
        h.go(-1);
        h.replace_current(7);
        assert_eq!(h.entries(), &[1, 7, 3]);
        assert!(h.can_back());
        assert!(h.can_forward());
        // In an empty history it is the first visit.
        let mut h = NavHistory::with_capacity(4);
        h.replace_current(5);
        assert_eq!(h.entries(), &[5]);
    }

    #[test]
    fn steps_further_than_one_land_or_go_nowhere() {
        let mut h = history(&[1, 2, 3, 4]);
        assert_eq!(h.peek(-3), Some(&1));
        assert_eq!(h.peek(-4), None);
        assert_eq!(h.go(-2), Some(&2));
        assert_eq!(h.go(5), None);
        assert_eq!(h.current(), Some(&2));
    }

    #[test]
    fn a_gone_place_is_dropped_on_the_way_back() {
        let mut h = history(&[1, 2, 3, 4]);
        assert_eq!(h.go(-1), Some(&3));
        h.drop_current(-1);
        // Back where the step started, the next step back goes past it.
        assert_eq!(h.current(), Some(&4));
        assert_eq!(h.entries(), &[1, 2, 4]);
        assert_eq!(h.go(-1), Some(&2));
    }

    #[test]
    fn a_gone_place_is_dropped_on_the_way_forward() {
        let mut h = history(&[1, 2, 3, 4]);
        h.go(-3);
        assert_eq!(h.go(1), Some(&2));
        h.drop_current(1);
        assert_eq!(h.current(), Some(&1));
        assert_eq!(h.go(1), Some(&3));
    }

    #[test]
    fn dropping_merges_the_places_it_leaves_side_by_side() {
        // Back from the last 1 through a gone 2: the two 1s are one, nothing further back.
        let mut h = history(&[1, 2, 1]);
        h.go(-1);
        h.drop_current(-1);
        assert_eq!(h.entries(), &[1]);
        assert_eq!(h.current(), Some(&1));
        assert_eq!(h.go(-1), None);
        // Forward from the first 1.
        let mut h = history(&[1, 2, 1, 3]);
        h.go(-3);
        h.go(1);
        h.drop_current(1);
        assert_eq!(h.entries(), &[1, 3]);
        assert_eq!(h.current(), Some(&1));
        assert_eq!(h.go(1), Some(&3));
        // The only place gone: nothing left.
        let mut h = history(&[5]);
        h.drop_current(-1);
        assert_eq!(h.current(), None);
        h.drop_current(1);
        assert_eq!(h.current(), None);
    }

    #[test]
    fn replacing_with_a_neighbour_makes_them_one() {
        let mut h = history(&[1, 2, 3]);
        h.go(-1);
        h.replace_current(1);
        assert_eq!(h.entries(), &[1, 3]);
        assert_eq!(h.current(), Some(&1));
        assert_eq!(h.peek(1), Some(&3));
        let mut h = history(&[1, 2, 3]);
        h.go(-1);
        h.replace_current(3);
        assert_eq!(h.entries(), &[1, 3]);
        assert_eq!(h.current(), Some(&3));
        assert_eq!(h.peek(-1), Some(&1));
    }
}
