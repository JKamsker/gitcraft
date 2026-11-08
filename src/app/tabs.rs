//! Opening, closing, moving and reopening tabs. The folders of the last repository tabs closed
//! are kept for Ctrl+Shift+T.

use std::path::{Path, PathBuf};

use super::panels::Toast;
use super::{App, Tab};

/// How many closed tabs can be reopened.
const MAX_CLOSED: usize = 10;

impl App {
    pub(super) fn close_tab(&mut self, i: usize, ctx: &egui::Context) {
        if i >= self.tabs.len() {
            return;
        }
        // Work still going on in a parked worktree isn't closed with a click on what is showing.
        if let Some(Tab::Repo(r)) = self.tabs.get_mut(i)
            && let Some(next) = r.successor_with_work()
        {
            *r = next;
            r.resume(ctx);
            let name = r.label(true);
            let name = name.branch.unwrap_or(name.repo);
            self.active = i;
            self.toasts.push(Toast::new(
                crate::i18n::message("interaction-tabs-successor", &[("name", name.into())]),
                false,
            ));
            return;
        }
        self.save_closing_tab(i);
        if let Tab::Repo(r) = self.tabs.remove(i) {
            remember_closed(&mut self.closed, &r.path);
        }
        // A drag going on holds a tab by its index, which now names another one.
        self.tab_drag = None;
        if self.tabs.is_empty() {
            self.tabs.push(Tab::Home);
        }
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
    }

    /// Opens the repository of the tab closed last whose folder is still there.
    pub(super) fn reopen_closed_tab(&mut self, ctx: &egui::Context) {
        while let Some(path) = self.closed.pop() {
            if reopenable(&path) {
                self.open_repo(&path, ctx);
                return;
            }
        }
    }

    /// Moves tab `from` to index `to`; the same tab stays active.
    pub(super) fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = if self.active == from {
            to
        } else if from < self.active && to >= self.active {
            self.active - 1
        } else if from > self.active && to <= self.active {
            self.active + 1
        } else {
            self.active
        };
    }

    /// Goes `step` tabs to the right (left when negative), round from the last to the first.
    pub(super) fn step_tab(&mut self, step: isize) {
        self.active = stepped(self.active, self.tabs.len(), step);
    }

    /// Goes to the tab at `place` (1 for the first), when there is one.
    pub(super) fn go_to_tab(&mut self, place: usize) {
        if let Some(i) = at_place(place, self.tabs.len()) {
            self.active = i;
        }
    }

    pub(super) fn go_to_last_tab(&mut self) {
        self.active = self.tabs.len().saturating_sub(1);
    }

    pub(super) fn new_tab(&mut self) {
        self.tabs.push(Tab::Home);
        self.active = self.tabs.len() - 1;
    }
}

/// The tab `step` places from `active` among `count`, round from one end to the other.
pub(super) fn stepped(active: usize, count: usize, step: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let count = count as isize;
    (active as isize + step).rem_euclid(count) as usize
}

/// The index of the tab at `place` (1 for the first) among `count`, if there is one.
pub(super) fn at_place(place: usize, count: usize) -> Option<usize> {
    place.checked_sub(1).filter(|&i| i < count)
}

/// Puts `path` on top of the closed tabs, once, dropping the oldest past `MAX_CLOSED`.
fn remember_closed(closed: &mut Vec<PathBuf>, path: &Path) {
    closed.retain(|p| p != path);
    closed.push(path.to_owned());
    if closed.len() > MAX_CLOSED {
        closed.remove(0);
    }
}

/// Whether a tab saved for `path` is opened again (on start, on a profile switch, on reopening
/// a closed one): a folder here that is still there. One on another machine is kept unasked:
/// asking would hold up the window for a connection. Its tab says so if it's gone.
pub(super) fn reopenable(path: &std::path::Path) -> bool {
    crate::remote::is_remote(path) || path.is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tab on another machine is kept unasked (on a profile switch too); a folder here only
    /// while it is there.
    #[test]
    fn tabs_elsewhere_are_reopened_unasked() {
        assert!(reopenable(Path::new("ssh://build/home/me/app")));
        assert!(reopenable(&std::env::temp_dir()));
        assert!(!reopenable(Path::new("/nonexistent/gitcraft/repo")));
    }

    /// Ctrl+Tab goes round: past the last tab to the first, before the first to the last.
    #[test]
    fn stepping_through_the_tabs_wraps() {
        assert_eq!(stepped(0, 3, 1), 1);
        assert_eq!(stepped(2, 3, 1), 0);
        assert_eq!(stepped(0, 3, -1), 2);
        assert_eq!(stepped(1, 3, -1), 0);
        assert_eq!(stepped(0, 1, 1), 0);
        assert_eq!(stepped(0, 1, -1), 0);
        assert_eq!(stepped(0, 0, 1), 0);
        assert_eq!(stepped(4, 5, 7), 1);
    }

    /// Ctrl+1 … Ctrl+8 go to that tab when there is one, and do nothing past the last.
    #[test]
    fn a_tab_is_picked_by_its_place() {
        assert_eq!(at_place(1, 3), Some(0));
        assert_eq!(at_place(3, 3), Some(2));
        assert_eq!(at_place(4, 3), None);
        assert_eq!(at_place(0, 3), None);
        assert_eq!(at_place(1, 0), None);
    }

    #[test]
    fn the_last_closed_is_on_top_once() {
        let mut closed = Vec::new();
        for p in ["a", "b", "a"] {
            remember_closed(&mut closed, Path::new(p));
        }
        assert_eq!(closed, [PathBuf::from("b"), PathBuf::from("a")]);
    }

    #[test]
    fn only_the_last_few_are_kept() {
        let mut closed = Vec::new();
        for i in 0..MAX_CLOSED + 3 {
            remember_closed(&mut closed, Path::new(&i.to_string()));
        }
        assert_eq!(closed.len(), MAX_CLOSED);
        assert_eq!(closed[0], PathBuf::from("3"));
    }
}
