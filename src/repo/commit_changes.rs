//! First-file reveal after a backlink or history selection, bound to its detail read.
use super::{DiffKey, RepoTab};
use crate::git::ChangedFile;

#[derive(Default)]
pub(super) struct State {
    pending: Option<String>,
    history: Option<String>,
}

impl State {
    pub(super) fn cancel_pending(&mut self) {
        self.pending = None;
    }

    pub(super) fn follows_history(&self) -> bool {
        self.history.is_some()
    }

    pub(super) fn graph_with_diff(&self, key: &DiffKey) -> bool {
        matches!(key, DiffKey::Commit { hash, .. } if self.history.as_ref() == Some(hash))
    }

    pub(super) fn stop(&mut self) {
        *self = Self::default();
    }
}
enum Destination {
    Diff(DiffKey),
    NoChanges,
    Failed,
}
fn completed(
    pending: &mut Option<String>,
    hash: &str,
    files: Option<&[ChangedFile]>,
) -> Option<Destination> {
    if pending.as_deref() != Some(hash) {
        return None;
    }
    pending.take();
    Some(match files {
        Some(files) => match files.first() {
            Some(file) => Destination::Diff(DiffKey::Commit {
                hash: hash.to_owned(),
                path: file.path.clone(),
            }),
            None => Destination::NoChanges,
        },
        None => Destination::Failed,
    })
}
impl RepoTab {
    /// User history navigation reveals a diff; passive selection restoration during
    /// reload keeps the ordinary select path and must not reopen closed views.
    pub(super) fn select_history(&mut self, row: usize, ctx: &egui::Context) {
        let Some(commit) = self.commits.get(row) else {
            return;
        };
        let hash = commit.hash.clone();
        let reveal = self.stash_at(row).is_none()
            && !hash.starts_with('\u{0}')
            && (self.commit_changes.follows_history()
                || self.diff.is_none()
                || !crate::settings::Settings::get(ctx).show_detail);
        if !reveal {
            self.commit_changes.stop();
            self.select(row, ctx);
            return;
        }
        let already_open = self.selected == Some(row)
            && self.diff.as_ref().is_some_and(
                |view| matches!(&view.key, DiffKey::Commit { hash: shown, .. } if shown == &hash),
            );
        if !already_open {
            self.diff = None;
        }
        self.select(row, ctx);
        if already_open {
            let mut settings = crate::settings::Settings::get(ctx);
            settings.show_detail = true;
            settings.set(ctx);
        } else {
            self.reveal_selected_changes(hash, ctx);
        }
    }

    pub(super) fn open_commit_changes(&mut self, hash: String, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() || self.reword.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-0392").into(),
                true,
            )));
            return;
        }
        self.leave_pages();
        self.doctor = None;
        self.nav_jump = true;
        self.pending_goto = None;
        self.diff = None;
        self.file_view = None;
        self.compare = None;
        // Works outside a filtered/bounded graph, without loading the entire history.
        self.activity_commit(hash.clone(), ctx);
        // A cached history selection may already have opened it. A backlink uses
        // the saved layout rather than opting into transient history following.
        self.commit_changes.history = None;
        if self.diff.as_ref().is_some_and(
            |view| matches!(&view.key, DiffKey::Commit { hash: shown, .. } if shown == &hash),
        ) {
            return;
        }
        self.commit_changes.pending = Some(hash.clone());
        if self.detail.as_ref().is_some_and(|d| d.hash == hash) || self.detail_error.is_some() {
            self.finish_commit_changes(&hash, ctx);
        }
    }
    pub(super) fn finish_commit_changes(&mut self, hash: &str, ctx: &egui::Context) {
        let destination = completed(
            &mut self.commit_changes.pending,
            hash,
            self.detail
                .as_ref()
                .filter(|d| d.hash == hash)
                .map(|d| d.files.as_slice()),
        );
        match destination {
            Some(Destination::Diff(key)) => {
                self.diff = None;
                self.open_diff(key, ctx);
                if let Some(view) = &mut self.diff {
                    view.unfolded = true;
                }
            }
            Some(Destination::NoChanges) => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::text("repo-core-0393").into(),
                    false,
                )))
            }
            Some(Destination::Failed) | None => (), // The existing detail pane displays the error.
        }
    }

    /// Selection opens a transient graph/diff composition, leaving saved layout choices
    /// untouched. At short heights graph_diff_view uses the existing central diff route.
    pub(super) fn reveal_selected_changes(&mut self, hash: String, ctx: &egui::Context) {
        self.commit_changes.history = Some(hash.clone());
        self.commit_changes.pending = Some(hash.clone());
        let mut settings = crate::settings::Settings::get(ctx);
        if !settings.show_detail {
            settings.show_detail = true;
            settings.set(ctx);
        }
        if self.detail.as_ref().is_some_and(|d| d.hash == hash) || self.detail_error.is_some() {
            self.finish_commit_changes(&hash, ctx);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn file(path: &str, status: char) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            status,
            old_path: None,
            added: None,
            removed: None,
        }
    }
    #[test]
    fn completion_opens_target_diff_once_and_handles_deleted_files_and_empty_commits() {
        let mut pending = Some("target".into());
        let files = [file("removed.txt", 'D'), file("other.txt", 'M')];
        assert!(completed(&mut pending, "stale", Some(&files)).is_none());
        assert_eq!(pending.as_deref(), Some("target"));
        assert!(
            matches!(completed(&mut pending,"target",Some(&files)), Some(Destination::Diff(DiffKey::Commit{hash,path})) if hash=="target" && path=="removed.txt")
        );
        assert!(completed(&mut pending, "target", Some(&files)).is_none());
        pending = Some("empty".into());
        assert!(matches!(
            completed(&mut pending, "empty", Some(&[])),
            Some(Destination::NoChanges)
        ));
        pending = Some("error".into());
        assert!(matches!(
            completed(&mut pending, "error", None),
            Some(Destination::Failed)
        ));
        assert!(pending.is_none());
    }
    #[test]
    fn superseded_navigation_never_opens_a_late_diff() {
        let mut pending = Some("new".into());
        assert!(completed(&mut pending, "old", Some(&[file("old.txt", 'M')])).is_none());
        pending = None; // The action dispatcher clears it on any later action.
        assert!(completed(&mut pending, "new", Some(&[file("new.txt", 'M')])).is_none());
    }
}

#[cfg(test)]
#[path = "commit_changes/reveal_tests.rs"]
mod reveal_tests;

/// Owned actual detail/diff loading coverage. Compile only until containment is cleared.
#[cfg(test)]
mod runtime_held_tests {
    use super::*;
    use crate::git;
    #[test]
    fn backlink_target_outside_the_loaded_graph_opens_its_actual_diff() -> Result<(), String> {
        struct Owned(std::path::PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "gitcraft-backlink-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        let owned = Owned(path);
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            git::run(Some(&owned.0), &args)?;
        }
        for text in ["old", "new"] {
            std::fs::write(owned.0.join("file.txt"), text).map_err(|e| e.to_string())?;
            git::run(Some(&owned.0), &["add", "."])?;
            git::run(Some(&owned.0), &["commit", "-qm", "fixture"])?;
        }
        let hash = git::run(Some(&owned.0), &["rev-parse", "HEAD"])?
            .trim()
            .to_owned();
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(owned.0.clone(), &ctx);
        assert!(!tab.index.contains_key(&hash));
        tab.open_commit_changes(hash.clone(), &ctx);
        let start = std::time::Instant::now();
        loop {
            tab.poll(&ctx);
            if let Some(view) = &tab.diff
                && let Some(data) = &view.data
            {
                assert_eq!(
                    view.key,
                    DiffKey::Commit {
                        hash: hash.clone(),
                        path: "file.txt".into()
                    }
                );
                let data = data.as_ref().map_err(Clone::clone)?;
                assert!(data.lines.iter().any(|line| line.text.contains("new")));
                break;
            }
            if start.elapsed() > std::time::Duration::from_secs(10) {
                return Err("Target diff did not arrive".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(tab.commit_changes.pending.is_none());
        Ok(())
    }
}
