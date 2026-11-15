//! Contained link chains and a capability probe on the actual snapshot filesystem.
use super::{Entry, STORAGE, fail, folded, portable};
use crate::git;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) fn validate(repo: &Path, entries: &[Entry], root: &Path) -> Result<(), String> {
    let mut links = BTreeMap::new();
    for entry in entries.iter().filter(|e| e.mode == "120000") {
        crate::processes::deadline::check()?;
        if entry.size > 4096 {
            return Err(fail("staged-links"));
        }
        let target = git::run_bytes(Some(repo), &["cat-file", "blob", &entry.oid])?;
        let target = String::from_utf8(target).map_err(|_| fail("staged-links"))?;
        if target.is_empty() || target.starts_with('/') || target.contains(['\\', ':', '\0']) {
            return Err(fail("staged-links"));
        }
        links.insert(folded(&entry.path), target);
    }
    for path in links.keys() {
        let mut pending: std::collections::VecDeque<_> =
            path.split('/').map(str::to_owned).collect();
        let mut resolved = Vec::new();
        let mut expanded = BTreeSet::new();
        while let Some(part) = pending.pop_front() {
            match part.as_str() {
                "" | "." => continue,
                ".." => {
                    if resolved.pop().is_none() {
                        return Err(fail("staged-links"));
                    }
                }
                _ => resolved.push(part),
            }
            if let Some(target) = links.get(&folded(&resolved.join("/"))) {
                if !expanded.insert(resolved.join("/")) {
                    return Err(fail("staged-links"));
                }
                resolved.pop();
                for part in target.split('/').rev() {
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
        if resolved
            .first()
            .is_some_and(|p| p.eq_ignore_ascii_case(".git"))
        {
            return Err(fail("staged-links"));
        }
    }
    support(repo, entries, root)
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
