//! gitcraft's own files of a repository, in its common git folder: `.git/gitcraft/<name>`,
//! one for all its worktrees, on whichever machine the repository is.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Where gitcraft keeps `name` for the repository of `repo` (any of its worktrees). Runs git:
/// never on the UI thread for a repository elsewhere.
pub(crate) fn repo_file(repo: &Path, name: &str) -> Result<PathBuf, String> {
    let out = crate::git::run(Some(repo), &["rev-parse", "--git-common-dir"])
        .map_err(|e| format!("could not find the git folder of {}: {e}", repo.display()))?;
    let common = out.trim_end_matches(['\n', '\r']);
    if common.is_empty() {
        return Err(format!("{} has no git folder", repo.display()));
    }
    let dir = crate::remote::resolve(repo, common);
    Ok(crate::remote::join(&dir, &format!("gitcraft/{name}")))
}

/// What the file `name` of the repository holds, or the default when there is none yet.
pub(crate) fn read_json<T: DeserializeOwned + Default>(
    repo: &Path,
    name: &str,
) -> Result<T, String> {
    let path = repo_file(repo, name)?;
    match crate::remote::read_to_string(&path) {
        Ok(text) if text.trim().is_empty() => Ok(T::default()),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} can't be read: {e}", crate::remote::display(&path))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!(
            "could not read {}: {e}",
            crate::remote::display(&path)
        )),
    }
}

/// Saves `value` as the file `name` of the repository, making its folder when needed.
pub(crate) fn write_json<T: Serialize>(repo: &Path, name: &str, value: &T) -> Result<(), String> {
    let path = repo_file(repo, name)?;
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        crate::remote::create_dir_all(dir)
            .map_err(|e| format!("could not make {}: {e}", crate::remote::display(dir)))?;
    }
    crate::remote::write(&path, text.as_bytes())
        .map_err(|e| format!("could not save {}: {e}", crate::remote::display(&path)))
}
