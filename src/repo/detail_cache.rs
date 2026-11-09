//! Commit details read before, by hash: a commit clicked again (or read ahead, as a neighbour of
//! the one selected) shows at once. Whether HEAD contains it is part of a detail, so each is
//! kept with the HEAD it was read at and only given back while HEAD is still that.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::git::{ChangedFile, CommitDetail};

/// How many details are kept, the least recently used going first.
const CAPACITY: usize = 256;
/// A detail with more files than this (a large merge) is read again rather than kept.
const MAX_FILES: usize = 5_000;

#[derive(Default)]
pub(super) struct DetailCache {
    /// Oldest first: the HEAD each was read at, and the detail.
    entries: VecDeque<(String, CommitDetail)>,
}

impl DetailCache {
    pub(super) fn get(&mut self, hash: &str, head: &str) -> Option<CommitDetail> {
        let i = self
            .entries
            .iter()
            .position(|(at, d)| d.hash == hash && at == head)?;
        let entry = self.entries.remove(i)?;
        let detail = entry.1.clone();
        self.entries.push_back(entry);
        Some(detail)
    }

    pub(super) fn contains(&self, hash: &str, head: &str) -> bool {
        self.entries
            .iter()
            .any(|(at, d)| d.hash == hash && at == head)
    }

    pub(super) fn insert(&mut self, head: &str, detail: CommitDetail) {
        if detail.files.len() > MAX_FILES {
            return;
        }
        self.entries.retain(|(_, d)| d.hash != detail.hash);
        self.entries.push_back((head.to_owned(), detail));
        while self.entries.len() > CAPACITY {
            self.entries.pop_front();
        }
    }
}

/// How many commits' files are kept, the oldest read going first.
const FILES_CAPACITY: usize = 4_000;

/// The files of commits (as their detail lists them), read ahead in batches for the rows on
/// screen: a commit clicked shows its files at once, before (or without) its whole detail. A
/// commit's files never change, so nothing is ever stale.
#[derive(Default)]
pub(super) struct FilesCache {
    files: HashMap<String, Arc<Vec<ChangedFile>>>,
    /// In the order read, for letting the oldest go.
    order: VecDeque<String>,
    /// Asked for and not given (a batch failed): not asked for again by the read-ahead.
    unreadable: HashSet<String>,
    /// A batch is being read.
    pub(super) reading: bool,
}

impl FilesCache {
    pub(super) fn get(&self, hash: &str) -> Option<Arc<Vec<ChangedFile>>> {
        self.files.get(hash).cloned()
    }

    /// Whether `hash` is either known or not to be asked for again.
    pub(super) fn settled(&self, hash: &str) -> bool {
        self.files.contains_key(hash) || self.unreadable.contains(hash)
    }

    pub(super) fn insert(&mut self, hash: String, files: Vec<ChangedFile>) {
        if files.len() > MAX_FILES || self.files.contains_key(&hash) {
            return;
        }
        self.order.push_back(hash.clone());
        self.files.insert(hash, Arc::new(files));
        while self.order.len() > FILES_CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.files.remove(&old);
            }
        }
    }

    pub(super) fn unreadable(&mut self, hashes: impl IntoIterator<Item = String>) {
        self.unreadable.extend(hashes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_read_ahead_are_kept_and_the_oldest_go_first() {
        let mut cache = FilesCache::default();
        for i in 0..FILES_CAPACITY + 1 {
            cache.insert(i.to_string(), Vec::new());
        }
        assert!(cache.get("0").is_none());
        assert!(cache.get("1").is_some());
        assert!(!cache.settled("x"));
        cache.unreadable(["x".to_owned()]);
        assert!(cache.settled("x"));
    }

    fn detail(hash: &str) -> CommitDetail {
        CommitDetail {
            hash: hash.to_owned(),
            parents: Vec::new(),
            author: String::new(),
            email: String::new(),
            author_date: String::new(),
            committer: String::new(),
            commit_date: String::new(),
            message: String::new(),
            files: Vec::new(),
            in_head: true,
            merges_after: false,
        }
    }

    #[test]
    fn a_detail_comes_back_only_while_head_is_where_it_was_read() {
        let mut cache = DetailCache::default();
        cache.insert("head1", detail("a"));
        assert!(cache.get("a", "head1").is_some());
        assert!(cache.get("a", "head2").is_none());
        assert!(!cache.contains("b", "head1"));
        cache.insert("head2", detail("a"));
        assert!(cache.get("a", "head1").is_none());
        assert!(cache.contains("a", "head2"));
    }

    #[test]
    fn the_least_recently_used_goes_first() {
        let mut cache = DetailCache::default();
        for i in 0..CAPACITY {
            cache.insert("h", detail(&i.to_string()));
        }
        assert!(cache.get("0", "h").is_some());
        cache.insert("h", detail("new"));
        assert!(cache.contains("0", "h"));
        assert!(!cache.contains("1", "h"));
        assert!(cache.contains("new", "h"));
    }
}
