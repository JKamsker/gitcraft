//! One owned slot per repository, two globally; cancellation never frees a running slot.
use crate::git::worktree_resources;
use crate::repo::RepoTab;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
static RUNNING: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
struct Lease(PathBuf);
impl Lease {
    fn acquire(repo: PathBuf) -> Result<Self, String> {
        let mut running = RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if running.len() >= 2 || !running.insert(repo.clone()) {
            return Err(crate::i18n::text("resources-busy").into());
        }
        Ok(Self(repo))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        RUNNING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.0);
    }
}
impl RepoTab {
    pub(in crate::repo::insights::constellation) fn constellation_resources(
        &mut self,
        ctx: &egui::Context,
        path: String,
        preview: bool,
    ) {
        // Capability rejection performs no Git, task-file, canonicalization or probe I/O.
        if crate::remote::is_remote(&self.path)
            || crate::remote::is_remote(std::path::Path::new(&path))
        {
            if let Some(view) = self.insights.as_mut() {
                view.constellation.stop_resources();
                let state = &mut view.constellation.resources;
                state.path = path;
                let id = state.result.start();
                state
                    .result
                    .accept(id, Err(crate::i18n::text("resources-remote").into()));
            }
            return;
        }
        let repo = self.path.clone();
        let presence = self.worktree_cleanup.resource_presence(); // Callback only; invoked by worker.
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        if view.kind != crate::repo::insights::Kind::Constellation {
            return;
        }
        if preview && view.constellation.resources.result.ready().is_none() {
            return;
        }
        let lease = view
            .constellation
            .read
            .ready()
            .and_then(|s| s.common_dir.clone())
            .ok_or_else(|| crate::i18n::text("resources-unavailable").to_owned())
            .and_then(Lease::acquire);
        let merged = view.constellation.read.ready().and_then(|s| {
            let wt = s.worktrees.iter().find(|w| w.path == path)?;
            let branch = wt.branch.as_ref()?;
            if !s.merged.as_ref()?.contains(branch) {
                return None;
            }
            Some(crate::git::worktree_cleanup::Evidence {
                tip: wt.head.clone(),
                base_ref: s.target.as_ref()?.rev.clone(),
                proof: crate::git::worktree_cleanup::Proof::Ancestor,
                describe: String::new(),
            })
        });
        if !preview {
            if let Some(sizes) = view.constellation.disk.ready_mut() {
                sizes.remove(&path);
            }
            view.constellation.forge_read = None;
        }
        let state = &mut view.constellation.resources;
        state.scope.0.cancel();
        if !preview {
            state.stop();
        }
        state.path = path.clone();
        state.preview = false;
        state.scope = crate::tasks::Scope(crate::tasks::cancellation().child());
        let cancel = state.scope.0.clone();
        let request = if preview {
            state.validation.restart()
        } else {
            state.result.restart()
        };
        self.insight_then(
            ctx,
            move || {
                let _lease = lease?;
                crate::tasks::with_cancellation(cancel, || {
                    let path = std::path::Path::new(&path);
                    let before = presence(&repo, path)?;
                    let found =
                        worktree_resources::collect(&repo, path, request, before.clone(), merged)?;
                    if crate::tasks::cancellation().is_cancelled() {
                        return Err("resources-cancelled".into());
                    }
                    if presence(&repo, path)? != before {
                        return Err(crate::i18n::text("resources-stale").into());
                    }
                    Ok(found)
                })
            },
            move |tab, result, _| {
                if let Some(view) = tab.insights.as_mut() {
                    if preview {
                        if view.constellation.resources.verified(request, result) == Some(false) {
                            view.constellation.disk = Default::default();
                            view.constellation.forge_read = None;
                        }
                    } else {
                        let disk = result.as_ref().ok().map(|f| {
                            (
                                f.root.to_string_lossy().into_owned(),
                                measured_disk(&f.measurements).map(|size| super::DiskObservation {
                                    size,
                                    head: f.head.clone(),
                                    fingerprint: f.fingerprint.clone(),
                                    observed: f.collected,
                                }),
                            )
                        });
                        if view.constellation.resources.result.accept(request, result) {
                            if let Some((path, size)) = disk {
                                let state = &mut view.constellation.disk;
                                let mut sizes = state.ready().cloned().unwrap_or_default();
                                sizes.insert(path, size);
                                let id = state.start();
                                state.accept(id, Ok(sizes));
                            }
                            tab.constellation_build();
                        }
                    }
                }
            },
        );
    }
}

/// Unknown measurements never become a zero-byte disk total in the graph/table.
fn measured_disk(
    measurements: &[worktree_resources::Measurement; 5],
) -> Result<crate::insights::constellation::read::Disk, String> {
    use worktree_resources::Completeness;
    if measurements
        .iter()
        .any(|m| !matches!(m.status, Completeness::Complete | Completeness::Partial(_)))
    {
        return Err(crate::i18n::text("resources-unavailable").into());
    }
    Ok(crate::insights::constellation::read::Disk {
        bytes: measurements.iter().map(|m| m.bytes).sum(),
        files: measurements.iter().map(|m| m.files).sum(),
        partial: measurements
            .iter()
            .any(|m| m.status != Completeness::Complete),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn cancellation_keeps_repository_slot_occupied_until_worker_exits() -> Result<(), String> {
        let repo = PathBuf::from("/synthetic/resource-slot-192");
        let lease = super::Lease::acquire(repo.clone())?;
        let token = crate::tasks::Cancellation::default();
        token.cancel();
        assert!(super::Lease::acquire(repo.clone()).is_err());
        drop(lease);
        assert!(super::Lease::acquire(repo).is_ok());
        Ok(())
    }
    #[test]
    fn unknown_sizes_are_not_zero_byte_totals() {
        use crate::git::worktree_resources::{Completeness, Measurement};
        let mut measurements = std::array::from_fn(|_| Measurement::default());
        assert!(super::measured_disk(&measurements).is_ok_and(|m| m.bytes == 0));
        measurements[0].status = Completeness::Unavailable("missing".into());
        assert!(super::measured_disk(&measurements).is_err());
    }
    use std::path::PathBuf;
}
