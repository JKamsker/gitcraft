use super::Repository;
use std::path::Path;
pub(crate) fn snapshot(
    _repo: &Path,
    _profiles: Vec<u64>,
    _hidden: Vec<String>,
    _active: &dyn Fn() -> bool,
) -> Result<Repository, String> {
    Err("Repository snapshot pending".into())
}
