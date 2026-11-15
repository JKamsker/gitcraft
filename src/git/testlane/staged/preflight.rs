//! Conservative portable namespace, symlink containment, filter and storage policy.
use crate::git;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use unicode_normalization::UnicodeNormalization;
const STORAGE: u64 = 512 * 1024 * 1024;
const FILES: usize = 10_000;
const PATH_UNITS: usize = 240;

pub(super) struct Entry {
    path: String,
    mode: String,
    oid: String,
    size: u64,
}
fn fail(key: &'static str) -> String {
    crate::i18n::text(key).into()
}

pub(super) fn dense(repo: &Path) -> Result<(), String> {
    let sparse = git::run_env_codes(
        Some(repo),
        &["config", "--bool", "--get", "core.sparseCheckout"],
        &[],
        &[0, 1],
    )?;
    let entries = git::run_bytes(Some(repo), &["ls-files", "-t", "-z"])?;
    if sparse.trim() == "true" || entries.split(|b| *b == 0).any(|e| e.starts_with(b"S ")) {
        return Err(fail("staged-sparse"));
    }
    Ok(())
}

pub(super) fn manifest(repo: &Path, tree: &str, root: &Path) -> Result<Vec<Entry>, String> {
    let raw = git::run_bytes(Some(repo), &["ls-tree", "-r", "-z", "-l", tree])?;
    let mut entries = Vec::new();
    for row in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row).map_err(|_| fail("staged-paths"))?;
        let (header, path) = row.split_once('\t').ok_or_else(|| fail("staged-paths"))?;
        let columns: Vec<_> = header.split_whitespace().collect();
        if columns.len() != 4 {
            return Err(fail("staged-paths"));
        }
        if columns[0] == "160000" {
            return Err(fail("staged-submodules"));
        }
        entries.push(Entry {
            path: path.into(),
            mode: columns[0].into(),
            oid: columns[2].into(),
            size: columns[3].parse().map_err(|_| fail("staged-paths"))?,
        });
        if entries.len() > FILES {
            return Err(fail("staged-budget"));
        }
    }
    portable(&entries, root, STORAGE)?;
    filters(repo, tree, &entries)?;
    links(repo, &entries)?;
    Ok(entries)
}

fn folded(path: &str) -> String {
    path.nfd()
        .flat_map(char::to_uppercase)
        .collect::<String>()
        .nfd()
        .collect()
}
fn portable(entries: &[Entry], root: &Path, budget: u64) -> Result<(), String> {
    let mut namespace = BTreeMap::new();
    let mut footprint = 0u64;
    for entry in entries {
        crate::processes::deadline::check()?;
        if !["100644", "100755", "120000"].contains(&entry.mode.as_str()) {
            return Err(fail("staged-paths"));
        }
        // Count all roots conservatively, including Windows extended-path prefixes.
        if root
            .join(&entry.path)
            .to_string_lossy()
            .encode_utf16()
            .count()
            > PATH_UNITS
        {
            return Err(fail("staged-paths"));
        }
        let mut prefix = String::new();
        for component in entry.path.split('/') {
            let stem = component
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            let reserved = ["CON", "PRN", "AUX", "NUL", "CLOCK$"].contains(&stem.as_str())
                || ["COM", "LPT"].iter().any(|p| {
                    stem.strip_prefix(p).is_some_and(|n| {
                        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].contains(&n)
                    })
                });
            if component.is_empty()
                || component == "."
                || component == ".."
                || component.eq_ignore_ascii_case(".git")
                || component.ends_with(['.', ' '])
                || component.len() > 255
                || reserved
                || component
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
            {
                return Err(fail("staged-paths"));
            }
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            if let Some(previous) = namespace.insert(folded(&prefix), prefix.clone())
                && previous != prefix
            {
                return Err(fail("staged-paths"));
            }
        }
        footprint = footprint
            .checked_add(entry.size.saturating_add(4095) / 4096 * 4096)
            .ok_or_else(|| fail("staged-budget"))?;
    }
    footprint = footprint.saturating_add(namespace.len() as u64 * 4096);
    if footprint > budget {
        return Err(fail("staged-budget"));
    }
    Ok(())
}

fn filters(repo: &Path, tree: &str, entries: &[Entry]) -> Result<(), String> {
    let input: Vec<u8> = entries
        .iter()
        .flat_map(|e| e.path.bytes().chain([0]))
        .collect();
    let attributes = git::run_captured(
        repo,
        &[
            "check-attr",
            &format!("--source={tree}"),
            "--all",
            "-z",
            "--stdin",
        ],
        Some(&input),
    )?;
    if attributes.code != Some(0) {
        return Err(attributes.stderr);
    }
    for tuple in attributes
        .raw
        .split(|b| *b == 0)
        .collect::<Vec<_>>()
        .as_chunks::<3>()
        .0
    {
        // --all cannot distinguish -filter from a driver literally named "unset".
        // Conservatively refuse every effective filter declaration.
        if tuple[1] == b"filter" {
            return Err(fail("staged-filters"));
        }
    }
    Ok(())
}

fn links(repo: &Path, entries: &[Entry]) -> Result<(), String> {
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
    Ok(())
}

/// Validate the resulting namespace/types too: never accept a silent symlink fallback.
pub(super) fn faithful(root: &Path, entries: &[Entry]) -> Result<(), String> {
    for entry in entries {
        crate::processes::deadline::check()?;
        let path = root.join(&entry.path);
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| fail("staged-paths"))?;
        if (entry.mode == "120000" && !metadata.is_symlink())
            || (entry.mode != "120000" && !metadata.is_file())
        {
            return Err(fail("staged-paths"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if entry.mode != "120000"
                && ((metadata.permissions().mode() & 0o111 != 0) != (entry.mode == "100755"))
            {
                return Err(fail("staged-paths"));
            }
        }
    }
    let filemode = if cfg!(unix) {
        "core.filemode=true"
    } else {
        "core.filemode=false"
    };
    git::run(Some(root), &["-c", filemode, "diff-files", "--quiet"])?;
    Ok(())
}

#[cfg(test)]
mod tests;
