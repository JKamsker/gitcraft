//! Telling whether two repositories are clones of one project: by their remotes' URLs, which
//! name one place in many ways (`git@host:o/r.git`, `https://host/o/r`, `ssh://git@host/o/r`).

use std::path::{Path, PathBuf};

use crate::git;

/// `url` reduced to what names the project: the host (lowercase, without user or port) and the
/// path (without `.git` or slashes at its ends), or a path on disk as it is.
pub(crate) fn project_key(url: &str) -> String {
    let url = url.trim();
    let (host, path) = match url.split_once("://") {
        Some((scheme, rest)) if !scheme.eq_ignore_ascii_case("file") => {
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            (Some(bare_host(authority)), path)
        }
        Some((_, rest)) => (None, rest),
        // `user@host:path`, as scp writes it; not `C:\x` (a drive) nor a path with `/` first.
        None => match url.split_once(':') {
            Some((authority, path))
                if !authority.contains('/') && !authority.contains('\\') && authority.len() > 1 =>
            {
                (Some(bare_host(authority)), path)
            }
            _ => (None, url),
        },
    };
    let path = path.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    match host {
        Some(host) => format!("{host}/{}", path.to_lowercase()),
        None => path.to_owned(),
    }
}

/// `user@host:port` → `host`, lowercase.
fn bare_host(authority: &str) -> String {
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = match host.strip_prefix('[') {
        Some(v6) => v6.split_once(']').map_or(v6, |(a, _)| a),
        None => host.split_once(':').map_or(host, |(h, _)| h),
    };
    host.to_lowercase()
}

/// Whether some URL of `theirs` names a project of `ours`.
pub(crate) fn same_project(ours: &[String], theirs: &[String]) -> bool {
    let ours: Vec<String> = ours.iter().map(|u| project_key(u)).collect();
    theirs
        .iter()
        .map(|u| project_key(u))
        .any(|key| !key.is_empty() && ours.contains(&key))
}

/// The URLs of `repo`'s remotes (none for one without), or why they couldn't be read.
pub(crate) fn remote_urls(repo: &Path) -> Result<Vec<String>, String> {
    Ok(git::remotes(repo)?.into_iter().map(|r| r.url).collect())
}

/// Of `candidates` (folders on one machine), the tops of the repositories that are clones of
/// the project `urls` name, each once, in their order. Those that can't be read are skipped.
pub(crate) fn find_clones(candidates: &[PathBuf], urls: &[String]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        let Ok(top) = git::toplevel(candidate) else {
            continue;
        };
        if found.contains(&top) {
            continue;
        }
        if remote_urls(&top).is_ok_and(|theirs| same_project(urls, &theirs)) {
            found.push(top);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_project_named_many_ways() {
        let key = "github.com/jkamsker/gitcraft";
        for url in [
            "git@github.com:JKamsker/gitcraft.git",
            "https://github.com/JKamsker/gitcraft",
            "https://github.com/JKamsker/gitcraft.git/",
            "ssh://git@github.com/JKamsker/gitcraft.git",
            "ssh://git@GitHub.com:22/JKamsker/gitcraft",
            "https://token@github.com/JKamsker/gitcraft.git",
        ] {
            assert_eq!(project_key(url), key, "{url}");
        }
        assert_eq!(
            project_key("ssh://git@[::1]:2222/o/r.git"),
            "::1/o/r",
            "an IPv6 host"
        );
    }

    #[test]
    fn paths_on_disk_stay_paths() {
        assert_eq!(project_key("/srv/git/app.git"), "srv/git/app");
        assert_eq!(project_key("file:///srv/git/app.git"), "srv/git/app");
        assert_eq!(project_key(r"C:\repos\app"), r"C:\repos\app");
    }

    #[test]
    fn projects_match_by_any_remote() {
        let ours = vec![
            "git@github.com:me/fork.git".to_owned(),
            "https://github.com/up/app".to_owned(),
        ];
        assert!(same_project(
            &ours,
            &["https://github.com/up/app.git".to_owned()]
        ));
        assert!(!same_project(
            &ours,
            &["https://gitlab.com/up/app".to_owned()]
        ));
        assert!(!same_project(&ours, &[]));
        assert!(!same_project(&[], &["".to_owned()]));
    }
}
