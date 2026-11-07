use std::path::Path;
pub(crate) fn run(
    repo: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    super::super::command::activity_bytes(repo, args, input, active)
}
