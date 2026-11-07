//! Independence requires held filesystem ownership, rather than shell path checks.
#[cfg(target_os = "linux")]
mod local;
#[cfg(all(test, target_os = "linux"))]
pub(super) use local::make_with_checkpoints;
#[cfg(target_os = "linux")]
pub(super) use local::prepare;
#[cfg(target_os = "linux")]
pub(crate) use local::{Snapshot, make_independent};

#[cfg(not(target_os = "linux"))]
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub repo: std::path::PathBuf,
}
#[cfg(not(target_os = "linux"))]
pub(super) fn prepare(
    _repo: &std::path::Path,
    _primary: &std::path::Path,
    _topology: &super::topology::Topology,
    _active: &dyn Fn() -> bool,
) -> Result<Snapshot, String> {
    Err("Make independent requires Linux-local held directory/file handles. Read-only inspection remains available on this platform.".into())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn make_independent(
    _snapshot: &Snapshot,
    _active: &dyn Fn() -> bool,
) -> Result<String, String> {
    Err("Make independent requires Linux-local held directory/file handles.".into())
}
