//! Commit details read before, by hash: a commit clicked again (or read ahead, as a neighbour of
//! the one selected) shows at once. Whether HEAD contains it is part of a detail, so each is
//! kept with the HEAD it was read at and only given back while HEAD is still that.

use std::collections::VecDeque;

use crate::git::CommitDetail;

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

#[cfg(test)]
mod tests {
    use super::*;

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
