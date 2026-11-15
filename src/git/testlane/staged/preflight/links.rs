//! Contained link chains and a capability probe on the actual snapshot filesystem.
use super::{Entry, STORAGE, fail, folded, portable};
use crate::git;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) fn validate(repo: &Path, entries: &[Entry], root: &Path) -> Result<(), String> {
    resolve(&targets(repo, entries)?)?;
    support(repo, entries, root)
}

pub(super) fn targets(repo: &Path, entries: &[Entry]) -> Result<BTreeMap<String, String>, String> {
    let mut links = BTreeMap::new();
    for entry in entries.iter().filter(|e| e.mode == "120000") {
        crate::processes::deadline::check()?;
        if entry.size > 4096 {
            return Err(fail("staged-links"));
        }
        let target = git::command::run_inert_objects(
            repo,
            &["cat-file", "blob", &entry.oid],
            None,
            4097,
            &[0],
        )?;
        if target.len() as u64 != entry.size {
            return Err(fail("staged-links"));
        }
        let target = String::from_utf8(target).map_err(|_| fail("staged-links"))?;
        if target.is_empty() || target.starts_with('/') || target.contains(['\\', ':', '\0']) {
            return Err(fail("staged-links"));
        }
        links.insert(folded(&entry.path), target);
    }
    Ok(links)
}

pub(super) fn resolve(
    links: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    resolve_bounded(links, 1_000_000)
}
fn step(remaining: &mut usize) -> Result<(), String> {
    crate::processes::deadline::check()?;
    *remaining = remaining
        .checked_sub(1)
        .ok_or_else(|| fail("staged-link-budget"))?;
    Ok(())
}
fn resolve_bounded(
    links: &BTreeMap<String, String>,
    mut remaining: usize,
) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for path in links.keys() {
        let mut pending: std::collections::VecDeque<_> =
            path.split('/').map(str::to_owned).collect();
        let mut resolved = Vec::new();
        let mut expanded = BTreeSet::new();
        while let Some(part) = pending.pop_front() {
            step(&mut remaining)?;
            match part.as_str() {
                "" | "." => continue,
                ".." => {
                    if resolved.pop().is_none() {
                        return Err(fail("staged-links"));
                    }
                }
                _ => {
                    if super::names::git_alias(&part) {
                        return Err(fail("staged-links"));
                    }
                    resolved.push(part);
                }
            }
            if let Some(target) = links.get(&folded(&resolved.join("/"))) {
                if !expanded.insert(resolved.join("/")) {
                    return Err(fail("staged-links"));
                }
                resolved.pop();
                for part in target.split('/').rev() {
                    step(&mut remaining)?;
                    pending.push_front(part.into());
                }
            }
        }
        portable(
            &[Entry {
                path: resolved.join("/"),
                mode: "100644".into(),
                oid: String::new(),
                size: 0,
            }],
            Path::new(""),
            STORAGE,
        )
        .map_err(|_| fail("staged-links"))?;
        result.insert(path.clone(), folded(&resolved.join("/")));
    }
    Ok(result)
}

fn support(repo: &Path, entries: &[Entry], root: &Path) -> Result<(), String> {
    if !entries.iter().any(|e| e.mode == "120000") {
        return Ok(());
    }
    let setting = git::run_env_codes(
        Some(repo),
        &["config", "--bool", "--get", "core.symlinks"],
        &[],
        &[0, 1],
    )?;
    if setting.trim() == "false" {
        return Err(fail("staged-paths"));
    }
    let probe = tempfile::tempdir_in(root).map_err(|_| fail("staged-paths"))?;
    let destination = probe.path().join("link");
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink("target", &destination);
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file("target", &destination);
    #[cfg(not(any(unix, windows)))]
    let result: std::io::Result<()> = Err(std::io::Error::other("symlinks unsupported"));
    result.map_err(|_| fail("staged-paths"))?;
    probe.close().map_err(|_| fail("staged-paths"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expansion_budget_bounds_queued_components_and_observes_cancellation() {
        let links = BTreeMap::from([
            ("A".into(), format!("b/{}", "./".repeat(500))),
            ("B".into(), "file".into()),
        ]);
        assert_eq!(
            resolve_bounded(&links, 32).err(),
            Some(fail("staged-link-budget"))
        );
        let cancel = crate::tasks::Cancellation::default();
        cancel.cancel();
        crate::tasks::with_cancellation(cancel, || assert!(resolve(&links).is_err()));
        crate::processes::deadline::within(std::time::Instant::now(), || {
            assert!(resolve(&links).is_err());
        });
    }
    #[test]
    fn links_cannot_resolve_through_git_directory_aliases_on_any_host() {
        for target in [
            "GIT~1/config",
            ".g\u{200c}it/config",
            ".git::$INDEX_ALLOCATION",
            "dir/GIT~1/../helper",
            ".git\u{feff}/HEAD",
        ] {
            let links = BTreeMap::from([("LINK".into(), target.into())]);
            assert!(resolve(&links).is_err(), "{target:?}");
        }
        let links = BTreeMap::from([("LINK".into(), "dir/../helper".into())]);
        assert!(resolve(&links).is_ok());
    }
}
