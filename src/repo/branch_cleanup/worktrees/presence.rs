//! Which worktrees the app has open: every repository view (a tab, a worktree view parked
//! behind one, a tab in another window) is listed here with its folder and how many terminals
//! it has open, from when it is made until it goes. The cleanup offers none of them for
//! removal: something may be working there.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

type Probe = Arc<dyn Fn() -> Option<Vec<PathBuf>> + Send + Sync>;

/// A view: its folder as the tab has it, and its terminals.
struct Open {
    id: u64,
    path: PathBuf,
    terminals: Vec<Probe>,
}

static OPEN: Mutex<Vec<Open>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

fn open() -> MutexGuard<'static, Vec<Open>> {
    OPEN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A view's place on the list, taken off when the view goes.
pub(in crate::repo) struct Presence {
    id: u64,
    terminals: Vec<Probe>,
}

impl Presence {
    pub fn new(path: &Path) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        open().push(Open {
            id,
            path: path.to_owned(),
            terminals: Vec::new(),
        });
        Self {
            id,
            terminals: Vec::new(),
        }
    }

    /// How many terminals the view has open now; the list is only touched when that changed.
    pub fn terminals(&mut self, paths: Vec<Probe>) {
        if paths.len() == self.terminals.len()
            && paths
                .iter()
                .zip(&self.terminals)
                .all(|(a, b)| Arc::ptr_eq(a, b))
        {
            return;
        }
        self.terminals = paths.clone();
        if let Some(entry) = open().iter_mut().find(|e| e.id == self.id) {
            entry.terminals = paths;
        }
    }

    /// What uses the worktree at `path`, other than this view: "open in another tab", "2
    /// terminals open in it".
    pub fn others(&self, path: &Path) -> Vec<String> {
        Self::using(path, Some(self.id))
    }

    pub(super) fn using(path: &Path, except: Option<u64>) -> Vec<String> {
        reasons(path, except, &open())
    }
}

fn reasons(path: &Path, except: Option<u64>, entries: &[Open]) -> Vec<String> {
    let same = |other: &Path| crate::remote::same_dir(path, other);
    let root = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let (mut views, mut terminals, mut unknown) = (0, 0, false);
    for entry in entries {
        views += usize::from(
            Some(entry.id) != except
                && (same(&entry.path)
                    || entry
                        .path
                        .canonicalize()
                        .unwrap_or_else(|_| entry.path.clone())
                        .starts_with(&root)),
        );
        for probe in &entry.terminals {
            match probe() {
                Some(paths) if !paths.is_empty() => {
                    terminals += usize::from(
                        paths
                            .into_iter()
                            .any(|cwd| cwd.canonicalize().unwrap_or(cwd).starts_with(&root)),
                    );
                }
                _ => unknown = true,
            }
        }
    }
    let mut why = Vec::new();
    if views > 0 {
        why.push(crate::i18n::text("worktree-cleanup-in-use-tab").to_owned());
    }
    if unknown {
        why.push(crate::i18n::text("worktree-cleanup-terminal-unknown").to_owned());
    }
    if terminals > 0 {
        why.push(crate::i18n::message(
            "worktree-cleanup-in-use-terminals",
            &[("count", terminals.into())],
        ));
    }
    why
}

impl Drop for Presence {
    fn drop(&mut self) {
        open().retain(|e| e.id != self.id);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unavailable_terminal_cwd_blocks_instead_of_using_launch_path() {
        let root = std::env::temp_dir().join("224-unknown-cwd");
        let entry = super::Open {
            id: 0,
            path: root.join("tab"),
            terminals: vec![std::sync::Arc::new(|| None)],
        };
        assert!(!super::reasons(&root.join("worktree"), None, &[entry]).is_empty());
    }

    #[test]
    fn purge_refuses_an_unavailable_terminal_observation() -> Result<(), String> {
        use crate::git::worktree_cleanup::archive::Store;
        let (repo, entry) = super::super::tests_in_use::retained("224 purge unknown terminal")?;
        // Inject an isolated observation; do not poison the process-wide test registry.
        let observation = super::Open {
            id: 0,
            path: repo.path().to_owned(),
            terminals: vec![std::sync::Arc::new(|| None)],
        };
        assert!(
            Store::purge_recovery(repo.path(), &entry, |path| {
                let reasons = super::reasons(path, None, &[observation]);
                Ok((!reasons.is_empty()).then(|| reasons.join("; ")))
            })
            .is_err()
        );
        assert!(entry.join("tree/base.txt").is_file());
        Ok(())
    }
}
