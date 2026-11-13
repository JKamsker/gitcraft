//! Arrangements kept in the repository, one per pull request, under a ref of gitcraft's own:
//! local, never pushed, gone with `git update-ref -d`.

use std::path::Path;

use super::arrange::Saved;
use crate::git;

/// The ref of the pull request at `url` (a digest, so that no name of the forge reaches it).
pub(in crate::repo::insights) fn reference(url: &str) -> String {
    format!("refs/gitcraft/review-purpose/{:x}", md5::compute(url))
}

fn current(repo: &Path, name: &str) -> Result<String, String> {
    git::run_ok_codes(
        Some(repo),
        &["rev-parse", "--verify", "--quiet", name],
        &[0, 1],
    )
    .map(|s| s.trim().to_owned())
}

pub(in crate::repo::insights) fn load(repo: &Path, url: &str) -> Result<Option<Saved>, String> {
    let oid = current(repo, &reference(url))?;
    if oid.is_empty() {
        return Ok(None);
    }
    let text = git::run(Some(repo), &["cat-file", "blob", &oid])?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("the saved grouping can't be read: {e}"))
}

/// Saves `saved` (`None`: forgets the arrangement).
pub(in crate::repo::insights) fn save(
    repo: &Path,
    url: &str,
    saved: Option<&Saved>,
) -> Result<(), String> {
    let name = reference(url);
    let Some(saved) = saved else {
        if current(repo, &name)?.is_empty() {
            return Ok(());
        }
        return git::run(Some(repo), &["update-ref", "-d", &name]).map(|_| ());
    };
    let text = serde_json::to_string(saved).map_err(|e| e.to_string())?;
    let oid = git::run_input(Some(repo), &["hash-object", "-w", "--stdin"], &text)?;
    git::run(Some(repo), &["update-ref", &name, oid.trim()]).map(|_| ())
}
