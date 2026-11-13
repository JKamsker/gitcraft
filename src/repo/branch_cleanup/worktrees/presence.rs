//! Which worktrees the app has open: every repository view (a tab, a worktree view parked
//! behind one, a tab in another window) is listed here with its folder and how many terminals
//! it has open, from when it is made until it goes. The cleanup offers none of them for
//! removal: something may be working there.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// A view: its folder as the tab has it, and its terminals.
struct Open {
    id: u64,
    path: PathBuf,
    terminals: usize,
}

static OPEN: Mutex<Vec<Open>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

fn open() -> MutexGuard<'static, Vec<Open>> {
    OPEN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A view's place on the list, taken off when the view goes.
pub(in crate::repo) struct Presence {
    id: u64,
    terminals: usize,
}

impl Presence {
    pub fn new(path: &Path) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        open().push(Open {
            id,
            path: path.to_owned(),
            terminals: 0,
        });
        Self { id, terminals: 0 }
    }

    /// How many terminals the view has open now; the list is only touched when that changed.
    pub fn terminals(&mut self, count: usize) {
        if count == self.terminals {
            return;
        }
        self.terminals = count;
        if let Some(entry) = open().iter_mut().find(|e| e.id == self.id) {
            entry.terminals = count;
        }
    }

    /// What uses the worktree at `path`, other than this view: "open in another tab", "2
    /// terminals open in it".
    pub fn others(&self, path: &Path) -> Vec<String> {
        let same = crate::remote::is_same_dir_as(path);
        let (mut views, mut terminals) = (0, 0);
        for entry in open().iter().filter(|e| e.id != self.id && same(&e.path)) {
            views += 1;
            terminals += entry.terminals;
        }
        let mut why = Vec::new();
        if views > 0 {
            why.push(crate::i18n::text("worktree-cleanup-in-use-tab").to_owned());
        }
        if terminals > 0 {
            why.push(crate::i18n::message(
                "worktree-cleanup-in-use-terminals",
                &[("count", terminals.into())],
            ));
        }
        why
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        open().retain(|e| e.id != self.id);
    }
}
