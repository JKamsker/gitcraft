//! Effective configuration origins, plus absent default files that may be created later.
use std::path::{Path, PathBuf};
pub(super) fn paths(repo: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(raw) =
        crate::git::run_bytes(Some(repo), &["config", "--null", "--list", "--show-origin"])
    {
        for pair in raw
            .split(|b| *b == 0)
            .collect::<Vec<_>>()
            .as_chunks::<2>()
            .0
        {
            if let Some(path) = String::from_utf8_lossy(pair[0]).strip_prefix("file:") {
                paths.push(repo.join(path));
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        paths.push(home.join(".gitconfig"));
        paths.push(home.join(".config/git/config"));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        paths.push(PathBuf::from(xdg).join("git/config"));
    }
    for variable in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"] {
        if let Some(file) = std::env::var_os(variable) {
            paths.push(PathBuf::from(file));
        }
    }
    if let Some(profile) = crate::connections::current().git_config {
        paths.push(profile);
    }
    paths
        .into_iter()
        .filter(|p| p.is_absolute())
        .map(|p| (key(&p), p))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .collect()
}
fn key(path: &Path) -> String {
    let absolute = path
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .and_then(|p| path.file_name().map(|name| p.join(name)))
        .unwrap_or_else(|| path.into());
    let text = absolute.to_string_lossy();
    if cfg!(windows) {
        windows_key(&text)
    } else {
        text.into()
    }
}
fn windows_key(text: &str) -> String {
    let text = text.replace('\\', "/").to_lowercase();
    if let Some(network) = text.strip_prefix("//?/unc/") {
        format!("//{network}")
    } else {
        text.strip_prefix("//?/").unwrap_or(&text).into()
    }
}
pub(super) fn contains(files: &[PathBuf], path: &Path) -> bool {
    let wanted = key(path);
    files.iter().any(|file| key(file) == wanted)
}

pub(super) fn covers(path: &Path, root: &Path) -> bool {
    let path = key(path);
    let root = key(root);
    path == root
        || path.strip_prefix(&root).is_some_and(|rest| {
            rest.starts_with(std::path::MAIN_SEPARATOR) || (cfg!(windows) && rest.starts_with('/'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_verbatim_drive_and_unc_forms_have_the_same_identity() {
        assert_eq!(
            windows_key(r"\\?\C:\Repo\config"),
            windows_key(r"c:\repo\config")
        );
        assert_eq!(
            windows_key(r"\\?\UNC\Server\Share\config"),
            windows_key(r"\\server\share\config")
        );
    }
    #[test]
    fn absent_config_replacement_matches_its_parent_without_matching_other_files()
    -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let config = dir.path().join("config");
        assert!(contains(std::slice::from_ref(&config), &config));
        assert!(covers(dir.path(), dir.path()));
        assert!(covers(&config, dir.path()));
        assert!(!covers(&dir.path().with_extension("sibling"), dir.path()));
        assert!(!contains(&[config], &dir.path().join("other")));
        Ok(())
    }
}
