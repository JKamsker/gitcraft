//! Independence requires held filesystem ownership, rather than shell path checks.
#[cfg(any(target_os = "linux", windows))]
pub(super) mod local;
#[cfg(all(test, any(target_os = "linux", windows)))]
pub(super) use local::make_with_checkpoints;
#[cfg(any(target_os = "linux", windows))]
pub(super) use local::prepare;
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub repo: std::path::PathBuf,
    kind: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    #[cfg(any(target_os = "linux", windows))]
    Local(local::Snapshot),
    Remote(super::helper::RemotePlan),
}
pub(super) fn prepare(
    repo: &std::path::Path,
    primary: &std::path::Path,
    topology: &super::topology::Topology,
    active: &dyn Fn() -> bool,
) -> Result<Snapshot, String> {
    if crate::remote::is_remote(repo) {
        return super::helper::prepare(repo, active).map(|plan| Snapshot {
            repo: repo.into(),
            kind: Kind::Remote(plan),
        });
    }
    #[cfg(any(target_os = "linux", windows))]
    {
        local::prepare(repo, primary, topology, active).map(|plan| Snapshot {
            repo: repo.into(),
            kind: Kind::Local(plan),
        })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (primary, topology);
        Err("Make independent requires supported held filesystem ownership handles. Read-only inspection remains available on this platform.".into())
    }
}
pub(crate) fn make_independent(
    snapshot: &Snapshot,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    match &snapshot.kind {
        #[cfg(any(target_os = "linux", windows))]
        Kind::Local(plan) => local::make_independent(plan, active),
        Kind::Remote(plan) => super::helper::execute(plan, active),
    }
}
#[cfg(all(test, any(target_os = "linux", windows)))]
pub(super) fn make_with_checkpoints(
    snapshot: &Snapshot,
    active: &dyn Fn() -> bool,
    checkpoint: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<String, String> {
    match &snapshot.kind {
        Kind::Local(plan) => local::make_with_checkpoints(plan, active, checkpoint),
        Kind::Remote(_) => Err("Local ownership fixtures require a local snapshot".into()),
    }
}
