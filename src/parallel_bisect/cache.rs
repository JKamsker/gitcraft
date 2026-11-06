//! Test results by tree: a commit whose tree was tested before (a rebase that kept it, a
//! revert back to it, the same test started again after a cancel) is not tested again. Kept
//! for as long as the tab is open, per test command and machine: another command, or another
//! machine with its own tools, may well answer differently.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use super::verdict::Verdict;

/// Shared between the tab (which keeps it) and the bisect running (which fills it).
#[derive(Clone, Default)]
pub struct TreeCache {
    inner: Arc<Mutex<HashMap<Key, Verdict>>>,
}

/// (machine, test command, tree).
type Key = (String, String, String);

impl TreeCache {
    /// What `command` said on `machine` for `tree`, if it ran on it before.
    pub fn get(&self, machine: &str, command: &str, tree: &str) -> Option<Verdict> {
        let map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        map.get(&key(machine, command, tree)).copied()
    }

    /// Keeps what `command` said on `machine` for `tree`. A timeout isn't kept: the next run
    /// may have more time, or a machine less busy.
    pub fn put(&self, machine: &str, command: &str, tree: &str, verdict: Verdict) {
        if verdict == Verdict::TimedOut {
            return;
        }
        let mut map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        map.insert(key(machine, command, tree), verdict);
    }
}

fn key(machine: &str, command: &str, tree: &str) -> Key {
    (
        machine.to_owned(),
        command.trim().to_owned(),
        tree.to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_are_kept_per_machine_command_and_tree() {
        let cache = TreeCache::default();
        cache.put("here", "make test", "t1", Verdict::Bad);
        cache.put("here", "make test", "t2", Verdict::Skip);
        cache.put("here", "make test", "t3", Verdict::TimedOut);
        assert_eq!(cache.get("here", "make test", "t1"), Some(Verdict::Bad));
        // Spaces around the command don't make it another.
        assert_eq!(cache.get("here", " make test\n", "t1"), Some(Verdict::Bad));
        assert_eq!(cache.get("here", "make test", "t2"), Some(Verdict::Skip));
        assert_eq!(cache.get("here", "make test", "t3"), None);
        assert_eq!(cache.get("SSH · build", "make test", "t1"), None);
        assert_eq!(cache.get("here", "make check", "t1"), None);
        // A copy is the same cache.
        let shared = cache.clone();
        shared.put("here", "make test", "t4", Verdict::Good);
        assert_eq!(cache.get("here", "make test", "t4"), Some(Verdict::Good));
    }
}
