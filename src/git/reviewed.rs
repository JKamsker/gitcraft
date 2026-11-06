//! Which version of each pull request was last looked at, kept in the repository itself:
//! `refs/gitcraft/reviewed/<forge host>/<owner/repo>/<number>/{head,base}`. The refs are what is
//! remembered (they outlast a restart) and what keeps those commits here, as the forge may not
//! have them any more once the branch was force-pushed.

use std::path::Path;

use super::range_diff::Series;
use super::{FS, run};

const ROOT: &str = "refs/gitcraft/reviewed/";

/// The ref folder of pull request `number` of the repository `repo_key` (`host/owner/repo`).
/// Whatever git doesn't take in a ref name is replaced by `_`, part by part.
pub fn reviewed_prefix(repo_key: &str, number: u64) -> String {
    let parts: Vec<String> = repo_key
        .split('/')
        .filter(|p| !p.is_empty())
        .map(ref_part)
        .collect();
    format!("{ROOT}{}/{number}/", parts.join("/"))
}

/// One part of a ref name made of `text`: no character git refuses, no leading `.`, no
/// `.lock` at the end, no `..` or `@{`.
fn ref_part(text: &str) -> String {
    let bad = |c: char| c.is_ascii_control() || " ~^:?*[\\".contains(c);
    let mut part: String = text.chars().map(|c| if bad(c) { '_' } else { c }).collect();
    while part.contains("..") {
        part = part.replace("..", "_");
    }
    part = part.replace("@{", "_");
    if part.starts_with('.') {
        part.replace_range(..1, "_");
    }
    if part.ends_with(".lock") || part.ends_with('.') {
        part.push('_');
    }
    if part.is_empty() || part == "@" {
        part = "_".to_owned();
    }
    part
}

/// The version of the pull request last looked at: its commits, from where they fork off its
/// base branch to its head.
pub fn reviewed(repo: &Path, repo_key: &str, number: u64) -> Result<Option<Series>, String> {
    let prefix = reviewed_prefix(repo_key, number);
    let out = run(
        Some(repo),
        &[
            "for-each-ref",
            &format!("--format=%(refname){FS}%(objectname)"),
            &prefix,
        ],
    )?;
    let end = |role: &str| {
        out.lines().find_map(|line| {
            let (name, oid) = line.split_once(FS)?;
            (name.strip_prefix(&prefix)? == role).then(|| oid.to_owned())
        })
    };
    Ok(end("head").map(|head| Series {
        base: end("base").unwrap_or_else(|| head.clone()),
        tip: head,
    }))
}

/// Every pull request of `repo_key` with a version looked at: (number, head).
pub fn reviewed_heads(repo: &Path, repo_key: &str) -> Result<Vec<(u64, String)>, String> {
    let root = reviewed_prefix(repo_key, 0);
    let root = root.strip_suffix("0/").unwrap_or(&root).to_owned();
    let out = run(
        Some(repo),
        &[
            "for-each-ref",
            &format!("--format=%(refname){FS}%(objectname)"),
            &root,
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let (name, oid) = line.split_once(FS)?;
            let number = name.strip_prefix(&root)?.strip_suffix("/head")?;
            Some((number.parse().ok()?, oid.to_owned()))
        })
        .collect())
}

/// Remembers `version` as the one of pull request `number` last looked at. The one before is
/// kept as `previous-head` / `previous-base`, so that a comparison of the two opened just now
/// still has its commits whatever `git gc` does.
pub fn mark_reviewed(
    repo: &Path,
    repo_key: &str,
    number: u64,
    version: &Series,
) -> Result<(), String> {
    let prefix = reviewed_prefix(repo_key, number);
    let before = reviewed(repo, repo_key, number)?.filter(|seen| seen != version);
    let mut ends = vec![("base", &version.base), ("head", &version.tip)];
    if let Some(before) = &before {
        ends.extend([
            ("previous-base", &before.base),
            ("previous-head", &before.tip),
        ]);
    }
    for (role, target) in ends {
        super::not_an_option(target)?;
        run(
            Some(repo),
            &[
                "update-ref",
                "-m",
                "gitcraft: pull request looked at",
                &format!("{prefix}{role}"),
                target,
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_request_has_a_ref_folder_git_takes() {
        assert_eq!(
            reviewed_prefix("github.com/acme/app", 12),
            "refs/gitcraft/reviewed/github.com/acme/app/12/"
        );
        // A port, dots and spaces where git refuses them.
        assert_eq!(
            reviewed_prefix("git.example.org:3000/.team/my repo.lock", 3),
            "refs/gitcraft/reviewed/git.example.org_3000/_team/my_repo.lock_/3/"
        );
        assert_eq!(
            reviewed_prefix("h/a..b/@{x}//", 1),
            "refs/gitcraft/reviewed/h/a_b/_x}/1/"
        );
        for key in ["host/a~b^c:d?e*f[g\\h", "x/..", "x/@", "x/.y."] {
            let name = format!("{}head", reviewed_prefix(key, 1));
            let ok = run(None, &["check-ref-format", &name]);
            assert!(ok.is_ok(), "{name}: {ok:?}");
        }
    }
}
