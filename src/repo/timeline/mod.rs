//! The timeline: every operation gitcraft ran on the repository and every branch move seen
//! made elsewhere (the terminal, another client), kept across restarts, as a tree. Undoing
//! walks up it and redoing down; doing something new after undoing starts a branch of it, so
//! the future undone is still there to jump back to (Ctrl+Shift+Z).
//!
//! The model is here; keeping it on disk is [`store`], the commits it needs kept from gc are
//! [`snapshot`], noticing moves made elsewhere is [`observe`], the tab's side is [`tab`] and
//! the view is [`view`].

pub(super) mod observe;
pub(super) mod receipt;
#[cfg(test)]
mod receipt_tests;
mod receipt_view;
pub(super) mod remove;
mod remove_tab;
#[cfg(test)]
mod remove_tests;
mod remove_view;
pub(super) mod snapshot;
pub(super) mod store;
mod tab;
#[cfg(test)]
mod tab_tests;
#[cfg(test)]
mod tests;
mod tree;
pub(super) mod view;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::UndoEntry;

/// Entries kept: the oldest go beyond this (and the commits only they kept, see [`snapshot`]).
pub(super) const MAX_ENTRIES: usize = 300;
/// Changed files kept per entry; the rest are only counted.
pub(super) const MAX_FILES: usize = 200;

/// Where HEAD and the local branches are. In an entry, only the branches it changed are there
/// (one that didn't exist on that side with no commit): the rest are as they are now, or as
/// the entries around it have them (see [`Timeline::state_at`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct RefState {
    /// The branch checked out (`main`), empty when HEAD is detached or unborn.
    pub(crate) branch: String,
    /// The commit HEAD is at, empty when unborn.
    pub(crate) head: String,
    /// Each local branch (`main`, `feat/x`) and its commit.
    pub(crate) branches: BTreeMap<String, String>,
}

impl RefState {
    /// The state `refs` (as the tab read them) describe.
    pub(crate) fn of(refs: &crate::git::Refs) -> Self {
        Self {
            branch: refs.current.clone().unwrap_or_default(),
            head: refs.head.clone(),
            branches: refs
                .local
                .iter()
                .map(|b| (b.name.clone(), b.target.clone()))
                .collect(),
        }
    }
}

/// One way a branch (or HEAD) differs between two states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefChange {
    /// The branch, or `HEAD` for where HEAD points.
    pub(crate) name: String,
    /// Its commit before, empty when it didn't exist (for HEAD: `branch` or the commit).
    pub(crate) old: String,
    pub(crate) new: String,
}

/// What differs from `before` to `after`: the branches made, moved or deleted, then HEAD when
/// it is on another branch or detached elsewhere.
pub(crate) fn changes(before: &RefState, after: &RefState) -> Vec<RefChange> {
    let mut names: Vec<&String> = before
        .branches
        .keys()
        .chain(after.branches.keys())
        .collect();
    names.sort();
    names.dedup();
    let mut list: Vec<RefChange> = names
        .into_iter()
        .filter_map(|name| {
            let old = before.branches.get(name).cloned().unwrap_or_default();
            let new = after.branches.get(name).cloned().unwrap_or_default();
            (old != new).then(|| RefChange {
                name: name.clone(),
                old,
                new,
            })
        })
        .collect();
    let detached_moved = before.branch.is_empty() && before.head != after.head;
    if before.branch != after.branch || (after.branch.is_empty() && detached_moved) {
        let at = |s: &RefState| {
            if s.branch.is_empty() {
                s.head.clone()
            } else {
                s.branch.clone()
            }
        };
        list.push(RefChange {
            name: "HEAD".to_owned(),
            old: at(before),
            new: at(after),
        });
    }
    list
}

/// `before` and `after` with only the branches that differ, a branch missing on one side there
/// with no commit: what an entry keeps of them.
pub(crate) fn sparse(before: &RefState, after: &RefState) -> (RefState, RefState) {
    let mut b = RefState {
        branch: before.branch.clone(),
        head: before.head.clone(),
        branches: BTreeMap::new(),
    };
    let mut a = RefState {
        branch: after.branch.clone(),
        head: after.head.clone(),
        branches: BTreeMap::new(),
    };
    for change in changes(before, after)
        .into_iter()
        .filter(|c| c.name != "HEAD")
    {
        b.branches.insert(change.name.clone(), change.old);
        a.branches.insert(change.name, change.new);
    }
    (b, a)
}

/// `state` with what `side` (one side of an entry) says: its branches (deleted where it has no
/// commit for one) and HEAD.
fn apply(state: &mut RefState, side: &RefState) {
    for (name, commit) in &side.branches {
        if commit.is_empty() {
            state.branches.remove(name);
        } else {
            state.branches.insert(name.clone(), commit.clone());
        }
    }
    state.branch = side.branch.clone();
    state.head = side.head.clone();
}

/// Who made a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Origin {
    Gitcraft,
    /// Seen when the repository was read again: made in the terminal, by another client, by
    /// gitcraft while closed for this folder.
    Outside,
}

/// One node of the timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    /// Unique in this timeline, growing with time.
    pub(crate) id: u64,
    /// The entry whose state this started from; `None` for a first one (or once that was
    /// forgotten).
    pub(crate) parent: Option<u64>,
    /// Seconds since the epoch.
    pub(crate) time: i64,
    pub(crate) label: String,
    pub(crate) origin: Origin,
    /// HEAD, and the branches it changed, before and after (see [`sparse`]).
    pub(crate) before: RefState,
    pub(crate) after: RefState,
    /// Files whose content differs between the commits it moved from and to (at most
    /// [`MAX_FILES`]).
    #[serde(default)]
    pub(crate) files: Vec<String>,
    /// How many there are in all.
    #[serde(default)]
    pub(crate) files_total: usize,
    /// The commands that undo and redo it, for an operation of gitcraft's; `None` means
    /// undoing puts the branches back as `before` has them (and redoing as `after`).
    #[serde(default)]
    pub(crate) undo: Option<UndoEntry>,
    /// The child redo goes to: the one last undone from (or made).
    #[serde(default)]
    pub(crate) redo_child: Option<u64>,
    /// What it did to a branch it rewrote (a rebase, a squash, a reorder, an amend).
    #[serde(default)]
    pub(crate) receipt: Option<receipt::Receipt>,
}

impl Entry {
    pub(crate) fn changes(&self) -> Vec<RefChange> {
        changes(&self.before, &self.after)
    }
}

/// What the tab is waiting for to know what an operation did: the read of the repository after
/// it ended (numbered past `after_read`) says where the branches are now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum Awaiting {
    /// Nothing: a read that finds the branches moved found a change made elsewhere.
    #[default]
    Nothing,
    /// An operation runs: what is read meanwhile is half of it.
    Running,
    /// It ended; the first read after it gives `after`.
    Ended { after_read: u64, then: Ended },
}

/// What an ended operation becomes once its result is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ended {
    /// A new entry, under the current one (when it changed anything, or can be undone).
    New {
        label: String,
        undo: Option<UndoEntry>,
    },
    /// The repository went to an entry's state (undo, redo, a jump): it is the current one now,
    /// `None` for the state before every entry; `undo` is the updated way to undo or redo it.
    Moved {
        to: Option<u64>,
        undo: Option<UndoEntry>,
    },
}

/// The timeline of one folder (a worktree has its own: HEAD is its own).
#[derive(Clone, Debug, Default)]
pub(crate) struct Timeline {
    /// Oldest first.
    pub(crate) entries: Vec<Entry>,
    /// The entry whose `after` the repository is in, as far as the timeline knows; `None`
    /// before the first one.
    pub(crate) current: Option<u64>,
    /// The state last read, which the next read is compared with.
    pub(crate) seen: Option<RefState>,
    pub(crate) next_id: u64,
    /// What it waits for (not kept on disk).
    pub(super) awaiting: Awaiting,
    /// Operations that ended before the read after them came in, another one starting
    /// meanwhile: they are taken in, in order, with the next read.
    pub(super) backlog: Vec<Ended>,
    /// What was kept on disk is in (see [`Timeline::adopt`]).
    pub(super) loaded: bool,
    /// A read of the repository that came before that: (its number, the state it found).
    pub(super) early_read: Option<(u64, RefState)>,
    /// Ids of entries whose kept commits can go (see [`snapshot`]), not yet let go of.
    pub(super) dropped: Vec<u64>,
    /// The jump running (its label), and the entry it goes to (`None`: before them all).
    pub(super) jump: Option<(String, Option<u64>)>,
    /// The file on disk couldn't be read: it isn't written over.
    pub(super) read_only: bool,
    /// The first-level entry redo goes to from before them all (the one last undone from, or
    /// made).
    pub(crate) root_redo: Option<u64>,
}

impl Timeline {
    pub(crate) fn get(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// The entry the repository is in.
    pub(crate) fn current_entry(&self) -> Option<&Entry> {
        self.current.and_then(|id| self.get(id))
    }

    /// The entries one level below `parent` (`None`: the first ones), oldest first.
    pub(crate) fn children(&self, parent: Option<u64>) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.parent == parent)
    }

    /// The entry redo goes to: the child last undone from (or made), else the newest.
    pub(crate) fn redo_target(&self) -> Option<&Entry> {
        let preferred = match self.current {
            Some(id) => self.get(id).and_then(|e| e.redo_child),
            None => self.root_redo.or_else(|| self.root_redo()),
        };
        preferred
            .and_then(|id| self.get(id))
            .filter(|e| e.parent == self.current)
            .or_else(|| self.children(self.current).last())
    }

    /// Below the first entries, which have no parent to remember it: the newest first entry.
    fn root_redo(&self) -> Option<u64> {
        self.children(None).last().map(|e| e.id)
    }

    /// The entries from the first one down to `id`: the path undoing all of it walks back.
    pub(crate) fn path_to(&self, id: Option<u64>) -> Vec<u64> {
        let mut path = Vec::new();
        let mut at = id;
        while let Some(id) = at {
            // A loop (a file edited by hand) ends it.
            if path.contains(&id) {
                break;
            }
            path.push(id);
            at = self.get(id).and_then(|e| e.parent);
        }
        path.reverse();
        path
    }

    /// The branches and HEAD as they are at entry `to` (`None`: before every entry), from
    /// `now`, the repository as it is (at the current entry): up the tree from the current entry
    /// to where the two paths meet, undoing each entry's changes, then down to `to`, doing them.
    /// Branches no entry on the way touched stay as they are now.
    pub(crate) fn state_at(&self, now: &RefState, to: Option<u64>) -> RefState {
        let from_path = self.path_to(self.current);
        let to_path = self.path_to(to);
        let shared = from_path
            .iter()
            .zip(&to_path)
            .take_while(|(a, b)| a == b)
            .count();
        let mut state = now.clone();
        for id in from_path[shared..].iter().rev() {
            if let Some(e) = self.get(*id) {
                apply(&mut state, &e.before);
            }
        }
        for id in &to_path[shared..] {
            if let Some(e) = self.get(*id) {
                apply(&mut state, &e.after);
            }
        }
        state
    }

    /// The first entry on the way from `from` to `to` that only commands undo and redo (a
    /// stash, a tag: it moved no branch), which a jump, putting back branches, would skip.
    pub(crate) fn command_only_between(
        &self,
        from: Option<u64>,
        to: Option<u64>,
    ) -> Option<String> {
        let (a, b) = (self.path_to(from), self.path_to(to));
        let shared = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        a[shared..]
            .iter()
            .chain(&b[shared..])
            .filter_map(|id| self.get(*id))
            .find(|e| e.undo.is_some() && e.changes().is_empty())
            .map(|e| e.label.clone())
    }

    /// Whether the repository is in `id`'s state or one after it on the same path: it was not
    /// undone (or was redone).
    pub(crate) fn is_done(&self, id: u64) -> bool {
        self.path_to(self.current).contains(&id)
    }

    /// Adds an entry under the current one and makes it current; returns its id.
    pub(crate) fn push(&mut self, mut entry: Entry) -> u64 {
        // Past every id used, and past the time in milliseconds: another gitcraft on the same
        // folder (whose refs share the namespace) doesn't hand out the same.
        self.next_id = self
            .next_id
            .max(self.entries.iter().map(|e| e.id).max().unwrap_or(0))
            .max(now_millis())
            + 1;
        let id = self.next_id;
        entry.id = id;
        entry.parent = self.current.filter(|p| self.get(*p).is_some());
        if let Some(undo) = &mut entry.undo {
            undo.id = id;
        }
        match entry.parent.and_then(|p| self.get_mut(p)) {
            Some(parent) => parent.redo_child = Some(id),
            None => self.root_redo = Some(id),
        }
        self.entries.push(entry);
        self.current = Some(id);
        self.cap();
        id
    }

    /// The repository is in `to`'s state now (an undo, a redo or a jump). Undoing from an
    /// entry has redo come back to it.
    pub(crate) fn move_to(&mut self, to: Option<u64>, undo: Option<UndoEntry>) {
        let from = self.current;
        if let Some(mut undo) = undo
            && let Some(entry) = self.get_mut(undo.id)
        {
            undo.id = entry.id;
            entry.undo = Some(undo);
        }
        // Undone one step: redo comes back here.
        if let Some(from) = from
            && self.get(from).is_some_and(|e| e.parent == to)
        {
            match to.and_then(|p| self.get_mut(p)) {
                Some(parent) => parent.redo_child = Some(from),
                None => self.root_redo = Some(from),
            }
        }
        self.current = to.filter(|id| self.get(*id).is_some());
    }

    /// Forgets the oldest entries past [`MAX_ENTRIES`]; never the current one.
    fn cap(&mut self) {
        while self.entries.len() > MAX_ENTRIES {
            let Some(oldest) = self
                .entries
                .iter()
                .map(|e| e.id)
                .find(|id| Some(*id) != self.current)
            else {
                return;
            };
            self.forget(&[oldest]);
        }
    }

    /// Forgets the entries `ids`: what was below them hangs from what they hung from.
    pub(crate) fn forget(&mut self, ids: &[u64]) {
        for &id in ids {
            let Some(at) = self.entries.iter().position(|e| e.id == id) else {
                continue;
            };
            let gone = self.entries.remove(at);
            for e in &mut self.entries {
                if e.parent == Some(id) {
                    e.parent = gone.parent;
                }
                if e.redo_child == Some(id) {
                    e.redo_child = None;
                }
            }
            if self.root_redo == Some(id) {
                self.root_redo = None;
            }
            if self.current == Some(id) {
                self.current = gone.parent;
            }
            self.dropped.push(id);
        }
    }

    /// Forgets every entry but the newest `keep` and the current one with what leads to it.
    pub(crate) fn forget_older(&mut self, keep: usize) {
        let path = self.path_to(self.current);
        let cut = self.entries.len().saturating_sub(keep);
        let old: Vec<u64> = self.entries[..cut]
            .iter()
            .map(|e| e.id)
            .filter(|id| !path.contains(id))
            .collect();
        self.forget(&old);
    }

    /// Takes in what was kept on disk, `saved`, under what this tab did before it came in (the
    /// in-memory journal of an older gitcraft, or operations run while it was read): those go
    /// after the saved entries, under its current one, with new ids.
    pub(crate) fn adopt(&mut self, saved: store::Saved) {
        let mine = std::mem::take(&mut self.entries);
        let my_current = self.current;
        let my_seen = self.seen.take();
        self.entries = saved.entries;
        self.current = saved.current.filter(|id| self.get(*id).is_some());
        self.seen = saved.seen;
        self.root_redo = saved.root_redo.filter(|id| self.get(*id).is_some());
        self.next_id = saved
            .next_id
            .max(self.entries.iter().map(|e| e.id).max().unwrap_or(0));
        let mut ids = std::collections::HashMap::new();
        let base = self.current;
        let mut wanted_redo = Vec::new();
        for mut entry in mine {
            let old = entry.id;
            self.next_id += 1;
            entry.id = self.next_id;
            // Parents come before their children: theirs is known by now.
            entry.parent = match entry.parent {
                Some(p) => ids.get(&p).copied(),
                None => base,
            };
            wanted_redo.push((entry.id, entry.redo_child.take()));
            if let Some(undo) = &mut entry.undo {
                undo.id = entry.id;
            }
            ids.insert(old, entry.id);
            self.entries.push(entry);
        }
        for (id, child) in wanted_redo {
            let child = child.and_then(|c| ids.get(&c).copied());
            if let Some(e) = self.get_mut(id) {
                e.redo_child = child;
            }
        }
        // The first of them is what redo goes to from the saved current entry.
        let first = ids.values().min().copied();
        if let Some(parent) = base.and_then(|p| self.get_mut(p)) {
            parent.redo_child = first.or(parent.redo_child);
        }
        let moved_current = my_current.and_then(|c| ids.get(&c).copied());
        if !ids.is_empty() {
            self.current = moved_current.or(self.current);
            if my_seen.is_some() {
                self.seen = my_seen;
            }
        }
        self.loaded = true;
        self.cap();
    }
}

/// Milliseconds since the epoch.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX / 2))
}
