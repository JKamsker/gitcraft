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
    ) {
        let repo = self.path.clone();
        let presence = self
            .worktree_cleanup
            .resource_presence(&repo, std::path::Path::new(&path));
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let lease = view
            .constellation
            .read
            .ready()
            .and_then(|s| s.common_dir.clone())
            .ok_or_else(|| crate::i18n::text("resources-remote").to_owned())
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
        let state = &mut view.constellation.resources;
        state.stop();
        state.path = path.clone();
        state.scope = crate::tasks::Scope(crate::tasks::cancellation().child());
        let cancel = state.scope.0.clone();
        let request = state.result.restart();
        self.insight_then(
            ctx,
            move || {
                let _lease = lease?;
                crate::tasks::with_cancellation(cancel, || {
                    worktree_resources::collect(
                        &repo,
                        std::path::Path::new(&path),
                        request,
                        presence?,
                        merged,
                    )
                })
            },
            move |tab, result, _| {
                if let Some(view) = tab.insights.as_mut() {
                    let disk = result.as_ref().ok().map(|f| {
                        (
                            f.root.to_string_lossy().into_owned(),
                            measured_disk(&f.measurements),
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
