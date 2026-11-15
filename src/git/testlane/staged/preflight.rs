//! Conservative portable namespace, symlink containment, filter and storage policy.
use crate::git;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use unicode_normalization::UnicodeNormalization;
mod links;
mod materialize;
mod names;
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
    let raw = git::command::run_inert_objects(
        repo,
        &["ls-tree", "-r", "-z", "-l", tree],
        None,
        8 * 1024 * 1024,
        &[0],
    )?;
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
    links::validate(repo, &entries, root)?;
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
                .trim_end_matches(' ')
                .to_ascii_uppercase();
            let reserved = ["CON", "PRN", "AUX", "NUL", "CLOCK$", "CONIN$", "CONOUT$"]
                .contains(&stem.as_str())
                || ["COM", "LPT"].iter().any(|p| {
                    stem.strip_prefix(p).is_some_and(|n| {
                        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].contains(&n)
                    })
                });
            if component.is_empty()
                || component == "."
                || component == ".."
                || names::git_alias(component)
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

/// Read directory entries as well as stat paths: case-insensitive/8.3 aliases must
/// not let two expected names silently describe the same materialized entry.
fn namespace(root: &Path, entries: &[Entry]) -> Result<(), String> {
    let mut expected = BTreeSet::new();
    for entry in entries {
        let mut prefix = String::new();
        for part in entry.path.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            expected.insert(prefix.clone());
        }
    }
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|_| fail("staged-paths"))? {
            crate::processes::deadline::check()?;
            let entry = entry.map_err(|_| fail("staged-paths"))?;
            if directory == root && entry.file_name() == ".git" {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| fail("staged-paths"))?
                .to_str()
                .ok_or_else(|| fail("staged-paths"))?
                .replace(std::path::MAIN_SEPARATOR, "/");
            if !expected.remove(&relative) {
                return Err(fail("staged-paths"));
            }
            if entry
                .file_type()
                .map_err(|_| fail("staged-paths"))?
                .is_dir()
            {
                directories.push(path);
            }
        }
    }
    if expected.is_empty() {
        Ok(())
    } else {
        Err(fail("staged-paths"))
    }
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
    namespace(root, entries)?;
    Ok(())
}

#[cfg(test)]
mod tests;

pub(super) fn materialize(repo: &Path, root: &Path, entries: &[Entry]) -> Result<(), String> {
    materialize::write(repo, root, entries)
}

#[cfg(test)]
mod materialize_tests;
