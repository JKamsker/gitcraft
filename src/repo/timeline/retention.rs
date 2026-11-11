//! Timeline retention and adoption of persisted history.

use super::*;

impl Timeline {
    /// Forgets the oldest entries past [`MAX_ENTRIES`]; never the current one.
    pub(in crate::repo::timeline) fn cap(&mut self) {
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
