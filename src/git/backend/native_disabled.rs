use super::*;
pub(super) fn head(_: &Path) -> Result<String, String> {
    Err("feature disabled".into())
}
pub(super) fn refs(_: &Path) -> Result<super::super::Refs, String> {
    Err("feature disabled".into())
}
pub(super) fn remotes(_: &Path) -> Result<Vec<super::super::Remote>, String> {
    Err("feature disabled".into())
}
pub(super) fn git_dir(_: &Path) -> Result<PathBuf, String> {
    Err("feature disabled".into())
}
