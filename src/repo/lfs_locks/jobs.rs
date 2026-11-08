//! The background work behind the locks: reading them, which changed files are lockable, and
//! locking, unlocking and making a pattern lockable.

use std::collections::HashSet;
use std::time::Instant;

use super::{LockAct, Panel, unlock_offer};
use crate::git::{self, Asked, LfsLock};
use crate::repo::RepoTab;

/// `work`'s answer, or what it said if it panicked: the flag waiting for the answer is let go
/// of all the same.
pub(super) fn caught<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|p| {
        let said = p
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(crate::i18n::message(
            "repo-workflows-lfs_locks-caught-ff2cd16f",
            &[("said", (said).clone().into())],
        ))
    })
}

/// Whether git-lfs's error says the server has no locks API at all.
fn unsupported(e: &str) -> bool {
    let e = e.to_lowercase();
    ["not supported", "locking api", "404", "not found"]
        .iter()
        .any(|s| e.contains(s))
}

impl RepoTab {
    /// Asks the server for the locks, in the background; one read at a time, another one
    /// asked for meanwhile runs after it.
    pub(super) fn read_locks(&mut self, asked: Asked, ctx: &egui::Context) {
        let state = &mut self.lfs_locks;
        if state.reading.is_some() {
            if state.read_again != Some(Asked::ByUser) {
                state.read_again = Some(asked);
            }
            return;
        }
        state.reads += 1;
        state.reading = Some(Instant::now());
        let read = state.reads;
        let path = self.path.clone();
        let creds = self.credentials.clone();
        self.then(
            ctx,
            move || {
                caught(|| {
                    let locks = git::lfs_locks(&path, creds.as_ref(), asked);
                    Ok((locks, git::lockable_patterns(&path)))
                })
            },
            move |tab, answer, ctx| {
                let (locks, patterns) = answer.unwrap_or_else(|e| (Err(e), Vec::new()));
                tab.locks_read(read, locks, patterns);
                if let Some(again) = tab.lfs_locks.read_again.take() {
                    tab.read_locks(again, ctx);
                }
            },
        );
    }

    fn locks_read(
        &mut self,
        read: u64,
        locks: Result<Vec<LfsLock>, String>,
        patterns: Vec<String>,
    ) {
        let changed: HashSet<String> = self.changed_files().map(|f| f.path.clone()).collect();
        let state = &mut self.lfs_locks;
        if read != state.reads {
            return;
        }
        state.reading = None;
        state.read_at = Some(Instant::now());
        if patterns != state.patterns_seen {
            // `.gitattributes` changed: what is lockable is asked again.
            state.lockable.clear();
            state.lockable_for = None;
            state.patterns_seen = patterns.clone();
        }
        state.patterns = patterns;
        match locks {
            Ok(locks) => {
                state.set_locks(locks);
                state.error = None;
                state.read_ok = true;
                state.unsupported = false;
                if let Some(pushed) = state.pushed.take() {
                    state.offer = unlock_offer(&state.locks, &pushed, &changed);
                }
                // An offer outlives a lock given back elsewhere, or a file edited again.
                let ids: HashSet<&str> = state.locks.iter().map(|l| l.id.as_str()).collect();
                state
                    .offer
                    .retain(|l| ids.contains(l.id.as_str()) && !changed.contains(&l.path));
            }
            Err(e) => {
                state.unsupported = unsupported(&e);
                state.read_ok = false;
                state.error = Some(crate::i18n::message(
                    "repo-workflows-lfs_locks-locks-read-05347473",
                    &[("e", (e).clone().into())],
                ));
            }
        }
        if let Some(panel) = &mut state.panel {
            let ids: HashSet<&str> = state.locks.iter().map(|l| l.id.as_str()).collect();
            panel.chosen.retain(|id| ids.contains(id.as_str()));
        }
    }

    /// Asks git which changed files are lockable: those not asked about yet, for the working
    /// tree as last read. One run at a time.
    pub(super) fn read_lockable(&mut self, ctx: &egui::Context) {
        let read = self.work_read;
        let state = &self.lfs_locks;
        let mut paths: Vec<String> = self
            .changed_files()
            .map(|f| f.path.clone())
            .filter(|p| !state.lockable.contains_key(p))
            .collect();
        paths.sort();
        paths.dedup();
        self.lfs_locks.lockable_for = Some(read);
        if paths.is_empty() {
            return;
        }
        self.lfs_locks.lockable_running = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || caught(|| git::lockable_paths(&path, &paths).map(|found| (paths, found))),
            move |tab, answer, _| {
                let state = &mut tab.lfs_locks;
                state.lockable_running = false;
                // A failure (an old git) leaves the files unknown: only the warnings go.
                if let Ok((asked, found)) = answer {
                    let found: HashSet<String> = found.into_iter().collect();
                    for p in asked {
                        let lockable = found.contains(&p);
                        state.lockable.insert(p, lockable);
                    }
                    state.lockable_gen += 1;
                }
            },
        );
    }

    pub(in crate::repo) fn apply_lock_act(&mut self, act: LockAct, ctx: &egui::Context) {
        match act {
            LockAct::Refresh => self.read_locks(Asked::ByUser, ctx),
            LockAct::OpenPanel => {
                self.lfs_locks.panel = Some(Panel::default());
                self.lfs_locks.job_error = None;
                self.read_locks(Asked::ByUser, ctx);
            }
            LockAct::ClosePanel => self.lfs_locks.panel = None,
            LockAct::DismissOffer => self.lfs_locks.offer.clear(),
            LockAct::Lock(paths) => {
                let label = match paths.as_slice() {
                    [one] => format!("Lock {one}"),
                    many => format!("Lock {} files", many.len()),
                };
                self.lock_job(label, ctx, move |repo, creds| {
                    let failed: Vec<String> = paths
                        .iter()
                        .filter_map(|p| {
                            git::lfs_lock(repo, p, creds)
                                .err()
                                .map(|e| format!("{p}: {e}"))
                        })
                        .collect();
                    match failed.as_slice() {
                        [] => Ok(()),
                        [one] if paths.len() == 1 => Err(one
                            .split_once(": ")
                            .map_or(one.clone(), |(_, e)| e.to_owned())),
                        _ => Err(failed.join("; ")),
                    }
                });
            }
            LockAct::Unlock { locks, force } => {
                if self.lfs_locks.busy.is_some() {
                    return;
                }
                let label = match (locks.as_slice(), force) {
                    ([(_, path)], false) => format!("Unlock {path}"),
                    ([(_, path)], true) => format!("Force unlock {path}"),
                    (_, false) => format!("Unlock {} files", locks.len()),
                    (_, true) => format!("Force unlock {} files", locks.len()),
                };
                let ids: HashSet<String> = locks.iter().map(|(id, _)| id.clone()).collect();
                self.lfs_locks.offer.retain(|l| !ids.contains(&l.id));
                self.lock_job(label, ctx, move |repo, creds| {
                    git::lfs_unlock(repo, &locks, force, creds)
                });
            }
            LockAct::TrackLockable(pattern) => {
                let label = format!("Make {} lockable", pattern.trim());
                self.lock_job(label, ctx, move |repo, _| {
                    git::track_lockable(repo, &pattern).map(|_| ())
                });
            }
        }
    }

    /// Runs `work` in the background, then says how it went and reads the locks again.
    fn lock_job(
        &mut self,
        label: String,
        ctx: &egui::Context,
        work: impl FnOnce(&std::path::Path, Option<&git::Credentials>) -> Result<(), String>
        + Send
        + 'static,
    ) {
        if self.lfs_locks.busy.is_some() {
            return;
        }
        self.lfs_locks.busy = Some(label.clone());
        self.lfs_locks.job_error = None;
        let path = self.path.clone();
        let creds = self.credentials.clone();
        let made_lockable = label.starts_with("Make ");
        self.then(
            ctx,
            move || caught(|| work(&path, creds.as_ref())),
            move |tab, result, ctx| {
                tab.lfs_locks.busy = None;
                tab.status = Some(match &result {
                    Ok(()) if made_lockable => (
                        crate::i18n::message(
                            "repo-workflows-lfs_locks-lock-job-35b33376",
                            &[("label", (label).clone().into())],
                        ),
                        false,
                    ),
                    Ok(()) => (
                        crate::i18n::message(
                            "repo-workflows-lfs_locks-lock-job-36f5baee",
                            &[("label", (label).clone().into())],
                        ),
                        false,
                    ),
                    Err(e) => (
                        crate::i18n::message(
                            "repo-workflows-lfs_locks-lock-job-8336fbfe",
                            &[("label", (label).clone().into()), ("e", (e).clone().into())],
                        ),
                        true,
                    ),
                });
                tab.lfs_locks.job_error = result.as_ref().err().map(|e| {
                    crate::i18n::message(
                        "repo-workflows-lfs_locks-lock-job-8336fbfe",
                        &[("label", (label).clone().into()), ("e", (e).clone().into())],
                    )
                });
                if let Some(panel) = &mut tab.lfs_locks.panel {
                    panel.confirm_force = None;
                    if result.is_ok() {
                        panel.chosen.clear();
                        panel.new_pattern.clear();
                    }
                }
                if made_lockable {
                    // `.gitattributes` changed, which status shows.
                    tab.reload_status(ctx);
                }
                tab.read_locks(Asked::ByUser, ctx);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{caught, unsupported};

    #[test]
    fn a_job_that_panics_still_answers() {
        let r: Result<(), String> = caught(|| std::panic::resume_unwind(Box::new("boom")));
        assert_eq!(r, Err("Internal error: boom".to_owned()));
        assert_eq!(caught(|| Ok(3)), Ok(3));
    }

    #[test]
    fn servers_without_locks() {
        assert!(unsupported(
            "Remote \"origin\" does not support the Git LFS locking API"
        ));
        assert!(unsupported("Not Found"));
        assert!(!unsupported("Lock exists"));
    }
}
