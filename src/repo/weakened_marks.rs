//! The _weakens checks_ badges of a tab's file lists: the scans of the commits, comparisons and
//! working trees shown, each read once in the background and kept for when it is shown again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::Ui;

use super::DiffKey;
use super::widgets::{FileRowResponse, RowExtra};
use crate::weakened::{BADGE, FileFlags, Scope};

/// A scan's files that weaken the checks, by path, or why it couldn't be read.
pub(crate) type Found = Arc<Result<HashMap<String, FileFlags>, String>>;

/// Most scans kept; the oldest go first past it.
const KEPT: usize = 48;

#[derive(Default)]
struct Store {
    /// `None` while being read.
    scans: HashMap<Scope, Option<Found>>,
    /// Which scans were asked for, oldest first.
    order: Vec<Scope>,
}

/// Shared with the background reads, which fill it in.
#[derive(Clone, Default)]
pub(crate) struct WeakenedMarks {
    store: Arc<Mutex<Store>>,
}

impl WeakenedMarks {
    /// The scan of `scope` in `repo`: `None` until it is read (then the window is repainted),
    /// the reading started on the first ask.
    pub(crate) fn get(&self, ctx: &egui::Context, repo: &Path, scope: Scope) -> Option<Found> {
        let Ok(mut store) = self.store.lock() else {
            return None;
        };
        if let Some(entry) = store.scans.get(&scope) {
            return entry.clone();
        }
        store.scans.insert(scope.clone(), None);
        store.order.push(scope.clone());
        while store.order.len() > KEPT {
            let old = store.order.remove(0);
            store.scans.remove(&old);
        }
        drop(store);
        self.start(ctx.clone(), repo.to_owned(), scope);
        None
    }

    /// Reads `scope` on a thread of its own and keeps what it found.
    fn start(&self, ctx: egui::Context, repo: PathBuf, scope: Scope) {
        let store = self.store.clone();
        let asked = scope.clone();
        let spawned = std::thread::Builder::new()
            .name("weakened-checks".into())
            .spawn(move || {
                let found = crate::weakened::scan(&repo, &scope).map(|files| {
                    files
                        .into_iter()
                        .map(|f| (f.path.clone(), f))
                        .collect::<HashMap<_, _>>()
                });
                keep(&store, &scope, found);
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            let failed = Err(format!("Could not start looking for weakened checks: {e}"));
            keep(&self.store, &asked, failed);
        }
    }
}

/// Keeps what the scan of `scope` found, unless it was dropped meanwhile.
fn keep(store: &Mutex<Store>, scope: &Scope, found: Result<HashMap<String, FileFlags>, String>) {
    if let Ok(mut store) = store.lock()
        && let Some(entry) = store.scans.get_mut(scope)
    {
        *entry = Some(Arc::new(found));
    }
}

/// The scan a file list opening `key` belongs to.
pub(crate) fn scope_of(key: &DiffKey, work_read: u64) -> Scope {
    match key {
        DiffKey::Commit { hash, .. } => Scope::Commit(hash.clone()),
        DiffKey::Range { from, to, .. } => Scope::Range {
            from: from.clone(),
            to: to.clone(),
        },
        DiffKey::Work(_) => Scope::Work(work_read),
    }
}

/// `extra` with the badge when `path` weakens the checks in `found`.
pub(crate) fn extra<'a>(
    found: Option<&'a Found>,
    path: &str,
    extra: &'a RowExtra<'a>,
) -> (RowExtra<'a>, Option<&'a FileFlags>) {
    let flags = found
        .and_then(|f| f.as_ref().as_ref().ok())
        .and_then(|files| files.get(path));
    match flags {
        Some(flags) => (RowExtra::Badged(extra, BADGE), Some(flags)),
        None => (*extra, None),
    }
}

/// The badge's hover text: what was found, line by line.
pub(crate) fn hover(ui: &Ui, row: &FileRowResponse, flags: Option<&FileFlags>) {
    if let (Some(rect), Some(flags)) = (row.badge, flags) {
        ui.interact(rect, row.row.id.with("weakened"), egui::Sense::hover())
            .on_hover_text(flags.summary());
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{WeakenedMarks, scope_of};
    use crate::repo::DiffKey;
    use crate::weakened::repo_tests::{commit, repo, write};

    /// The badges of a commit's file list and of its diff's hunks, read the way the tab
    /// reads them: in the background, kept once read.
    #[test]
    fn file_rows_and_hunks_get_their_badge() -> Result<(), String> {
        let dir = repo("marks")?;
        write(&dir, "src/a b.rs", "fn a() {}\n\n#[test]\nfn t() {}\n")?;
        commit(&dir, "one")?;
        write(
            &dir,
            "src/a b.rs",
            "fn a() {}\n\n#[test]\n#[ignore]\nfn t() {}\n",
        )?;
        let hash = commit(&dir, "two")?;
        let key = DiffKey::Commit {
            hash: hash.clone(),
            path: String::new(),
        };
        let marks = WeakenedMarks::default();
        let ctx = egui::Context::default();
        let scope = scope_of(&key, 0);
        let started = Instant::now();
        let found = loop {
            if let Some(found) = marks.get(&ctx, &dir, scope.clone()) {
                break found;
            }
            if started.elapsed() > Duration::from_secs(20) {
                return Err("the scan never came back".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let files = found.as_ref().as_ref().map_err(Clone::clone)?;
        assert!(files["src/a b.rs"].summary().contains("adds `#[ignore`"));

        let diff_key = DiffKey::Commit {
            hash,
            path: "src/a b.rs".into(),
        };
        let data = crate::repo::diff::load(&dir, &diff_key, Default::default(), true)?;
        assert_eq!(data.weakened.len(), 1);
        let (line, tip) = data.weakened.iter().next().ok_or("a hunk badge")?;
        assert_eq!(data.lines[*line].kind, crate::repo::diff::Kind::Hunk);
        assert!(tip.contains("Skips or disables tests"), "{tip}");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
