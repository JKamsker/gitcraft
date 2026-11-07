//! Independence requires held filesystem ownership, rather than shell path checks.
#[cfg(any(target_os = "linux", windows))]
mod local;
#[cfg(all(test, any(target_os = "linux", windows)))]
pub(super) use local::make_with_checkpoints;
#[cfg(any(target_os = "linux", windows))]
pub(super) use local::prepare;
#[cfg(any(target_os = "linux", windows))]
pub(crate) use local::{Snapshot, make_independent};

#[cfg(not(any(target_os = "linux", windows)))]
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub repo: std::path::PathBuf,
}
#[cfg(not(any(target_os = "linux", windows)))]
pub(super) fn prepare(
    _repo: &std::path::Path,
    _primary: &std::path::Path,
    _topology: &super::topology::Topology,
    _active: &dyn Fn() -> bool,
) -> Result<Snapshot, String> {
    Err("Make independent requires Linux-local held directory/file handles. Read-only inspection remains available on this platform.".into())
}
#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) fn make_independent(
    _snapshot: &Snapshot,
    _active: &dyn Fn() -> bool,
) -> Result<String, String> {
    Err("Make independent requires Linux-local held directory/file handles.".into())
}
