//! Immutable raw file versions shared by migration, staging and branch-matrix views.
mod index_transaction;
mod lens;
mod migration;
#[cfg(test)]
pub(crate) mod tests;
mod transaction;

pub(crate) use lens::{Lens, LensSpec, read_lens};
pub(crate) use migration::{Migration, MigrationSource, read_migration};
pub(crate) use transaction::{Scratch, apply_lens_patch, patch_header};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub(crate) const FILE_LIMIT: usize = 2 * 1024 * 1024;
pub(crate) const MAX_ROWS: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub path: Vec<u8>,
    pub mode: String,
    pub oid: String,
}
impl Entry {
    pub fn name(&self) -> String {
        display_path(&self.path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Version {
    pub mode: Option<String>,
    pub oid: Option<String>,
    pub bytes: Vec<u8>,
    pub warning: Option<String>,
}
impl Version {
    pub fn missing() -> Self {
        Self {
            mode: None,
            oid: None,
            bytes: Vec::new(),
            warning: None,
        }
    }
    pub fn text(&self) -> Option<&str> {
        if self.warning.is_some() || self.bytes.contains(&0) {
            return None;
        }
        std::str::from_utf8(&self.bytes).ok()
    }
    pub fn regular(&self) -> bool {
        self.mode
            .as_deref()
            .is_some_and(|m| matches!(m, "100644" | "100755"))
    }
    pub fn label(&self) -> String {
        match &self.mode {
            None => "Missing".into(),
            Some(m) => format!(
                "{} · {} bytes{}",
                m,
                self.bytes.len(),
                if self.bytes.contains(&0) {
                    " · binary"
                } else {
                    ""
                }
            ),
        }
    }
}

pub(super) fn output(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<crate::remote::Output, String> {
    super::command::comparison_output(repo, args, &[], active)
}
pub(super) fn run(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let out = output(repo, args, active)?;
    if out.code == 0 {
        Ok(out.stdout)
    } else {
        Err(format!(
            "Could not compare files: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
pub(super) fn hash(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(crate) fn resolve(
    repo: &Path,
    revision: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    let bytes = run(
        repo,
        &[
            "--no-lazy-fetch",
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
        active,
    )?;
    let oid = std::str::from_utf8(&bytes)
        .map_err(|_| "Invalid revision")?
        .trim();
    if !hash(oid) {
        return Err("Git returned an invalid commit id".into());
    }
    Ok(oid.into())
}
pub(super) fn head(repo: &Path, active: &dyn Fn() -> bool) -> Result<Option<String>, String> {
    let out = output(
        repo,
        &["--no-lazy-fetch", "rev-parse", "--verify", "-q", "HEAD"],
        active,
    )?;
    if out.code != 0 {
        // Only an unborn branch is missing; inaccessible/corrupt repositories aren't empty.
        let symbolic = output(repo, &["symbolic-ref", "-q", "HEAD"], active)?;
        if symbolic.code == 0 {
            return Ok(None);
        }
        return Err("Could not resolve HEAD".into());
    }
    let oid = String::from_utf8(out.stdout)
        .map_err(|_| crate::i18n::text("backend-invalid-head-e1661b"))?;
    let oid = oid.trim();
    if !hash(oid) {
        return Err(crate::i18n::text("backend-invalid-head-e1661b").into());
    }
    Ok(Some(oid.into()))
}
pub(crate) fn valid_path(repo: &Path, path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\0')
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && !matches!(p, "." | ".."))
        && (crate::remote::is_remote(repo)
            || Path::new(path)
                .components()
                .all(|p| matches!(p, Component::Normal(_))))
}
pub(crate) fn display_path(path: &[u8]) -> String {
    match std::str::from_utf8(path) {
        Ok(p) => p
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\r', "\\r"),
        Err(_) => format!(
            "{} [non-UTF8: {}]",
            String::from_utf8_lossy(path),
            path.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ),
    }
}

type IndexEntries = (Vec<u8>, BTreeMap<Vec<u8>, Entry>);
pub(super) fn index(repo: &Path, active: &dyn Fn() -> bool) -> Result<IndexEntries, String> {
    let bytes = run(repo, &["ls-files", "--stage", "-z"], active)?;
    let mut entries = BTreeMap::new();
    for row in bytes.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let tab = row
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("Invalid index record")?;
        let metadata = std::str::from_utf8(&row[..tab]).map_err(|_| "Invalid index metadata")?;
        let fields: Vec<_> = metadata.split(' ').collect();
        if fields.len() != 3 || !hash(fields[1]) {
            return Err("Invalid index entry".into());
        }
        if fields[2] != "0" {
            return Err("Resolve index conflicts before opening these file comparisons".into());
        }
        let entry = Entry {
            path: row[tab + 1..].to_vec(),
            mode: fields[0].into(),
            oid: fields[1].into(),
        };
        entries.insert(entry.path.clone(), entry);
    }
    Ok((bytes, entries))
}
pub(super) fn tree(
    repo: &Path,
    oid: &str,
    folder: &str,
    active: &dyn Fn() -> bool,
) -> Result<BTreeMap<Vec<u8>, Entry>, String> {
    if !folder.is_empty() && !valid_path(repo, folder) {
        return Err("Choose a repository-relative folder without . or .. components".into());
    }
    let mut args = vec![
        "--no-lazy-fetch",
        "--literal-pathspecs",
        "ls-tree",
        "-r",
        "-z",
        "--full-tree",
        oid,
        "--",
    ];
    if !folder.is_empty() {
        args.push(folder);
    }
    let bytes = run(repo, &args, active)?;
    let mut entries = BTreeMap::new();
    for row in bytes.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let tab = row
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("Invalid tree record")?;
        let metadata = std::str::from_utf8(&row[..tab]).map_err(|_| "Invalid tree metadata")?;
        let fields: Vec<_> = metadata.split(' ').collect();
        if fields.len() != 3 || !hash(fields[2]) {
            return Err(crate::i18n::text("backend-invalid-tree-entry-8d7b6f").into());
        }
        let entry = Entry {
            path: row[tab + 1..].to_vec(),
            mode: fields[0].into(),
            oid: fields[2].into(),
        };
        entries.insert(entry.path.clone(), entry);
        if entries.len() > MAX_ROWS {
            return Err("More than 2,000 files; choose a narrower folder".into());
        }
    }
    Ok(entries)
}
pub(crate) fn blob(
    repo: &Path,
    entry: Option<&Entry>,
    active: &dyn Fn() -> bool,
) -> Result<Version, String> {
    let Some(entry) = entry else {
        return Ok(Version::missing());
    };
    if entry.mode == "160000" {
        return Ok(Version {
            mode: Some(entry.mode.clone()),
            oid: Some(entry.oid.clone()),
            bytes: format!("Submodule commit {}\n", entry.oid).into_bytes(),
            warning: None,
        });
    }
    let size = run(
        repo,
        &["--no-lazy-fetch", "cat-file", "-s", &entry.oid],
        active,
    )?;
    let size = std::str::from_utf8(&size)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .ok_or(crate::i18n::text("backend-invalid-blob-size-924b45"))?;
    if size > FILE_LIMIT {
        return Err("File comparison is limited to 2 MiB per version".into());
    }
    let bytes = run(
        repo,
        &["--no-lazy-fetch", "cat-file", "blob", &entry.oid],
        active,
    )?;
    if bytes.len() != size {
        return Err("The blob could not be read completely".into());
    }
    Ok(Version {
        mode: Some(entry.mode.clone()),
        oid: Some(entry.oid.clone()),
        bytes,
        warning: None,
    })
}

#[derive(Clone, Debug)]
pub(crate) struct Column {
    pub label: String,
    pub oid: String,
    pub files: BTreeMap<Vec<u8>, Entry>,
}
#[derive(Clone, Debug)]
pub(crate) struct Matrix {
    pub columns: Vec<Column>,
    pub paths: Vec<Vec<u8>>,
}
pub(crate) fn matrix(
    repo: &Path,
    branches: &[String],
    folder: &str,
    active: &dyn Fn() -> bool,
) -> Result<Matrix, String> {
    if !(2..=8).contains(&branches.len()) {
        return Err("Choose between two and eight branches".into());
    }
    if folder.starts_with('/') {
        return Err("Choose a repository-relative folder".into());
    }
    let folder = folder.trim_end_matches('/');
    let mut columns = Vec::new();
    let mut paths = BTreeSet::new();
    for branch in branches {
        let oid = resolve(repo, branch, active)?;
        let files = tree(repo, &oid, folder, active)?;
        paths.extend(files.keys().cloned());
        if paths.len() > MAX_ROWS {
            return Err("More than 2,000 distinct paths; choose a narrower folder".into());
        }
        columns.push(Column {
            label: branch.clone(),
            oid,
            files,
        });
    }
    Ok(Matrix {
        columns,
        paths: paths.into_iter().collect(),
    })
}

pub(crate) fn raw_output(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    run(repo, args, active)
}
pub(crate) fn raw_working(
    repo: &Path,
    path: &str,
    active: &dyn Fn() -> bool,
) -> Result<Version, String> {
    lens::working(repo, path, active)
}
