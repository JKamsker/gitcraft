//! Whole-tree recovery. Cleanup renames; only an explicit purge deletes content. The
//! worktree's private Git metadata (including its index) is retained beside the tree.
mod lock;
mod purge;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Target;
use super::archive::{self, Store};

#[derive(Serialize, Deserialize)]
struct Record {
    original: PathBuf,
    admin: PathBuf,
    branch: String,
}

pub(super) struct Capture {
    pub directory: PathBuf,
    pub registered: PathBuf,
    record: Record,
    _operation: lock::Operation,
}

fn root(repo: &Path) -> Result<PathBuf, String> {
    let common = archive::repository(repo)?.common_dir;
    let common = PathBuf::from(common);
    let root = common.join("gitcraft/recovery");
    for directory in [common.join("gitcraft"), root.clone()] {
        if let Ok(meta) = std::fs::symlink_metadata(directory)
            && !meta.is_dir()
        {
            return Err("Recovery area must use plain directories".into());
        }
    }
    Ok(root)
}

fn vacant(path: &Path) -> bool {
    path.symlink_metadata()
        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}

fn plain_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

impl Capture {
    pub fn move_tree(repo: &Path, target: &Target) -> Result<Self, String> {
        let root = root(repo)?;
        if !plain_dir(&target.path) {
            return Err("Worktree must be a plain directory".into());
        }
        if root.starts_with(target.path.canonicalize().map_err(|e| e.to_string())?) {
            return Err("Recovery area is inside the worktree".into());
        }
        // Resolve the registered spelling while the original still exists. After rename,
        // canonicalization cannot equate Windows short names/case or symlink ancestry.
        let registered = crate::git::worktree_list(repo, None)?
            .into_iter()
            .find(|wt| crate::remote::same_dir(&target.path, &wt.path))
            .ok_or("Worktree registration disappeared before capture")?
            .path;
        let admin = crate::git::run(Some(&target.path), &["rev-parse", "--absolute-git-dir"])?;
        let admin = PathBuf::from(admin.trim())
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let common = PathBuf::from(archive::repository(repo)?.common_dir);
        if admin.parent() != Some(common.join("worktrees").as_path()) || !plain_dir(&admin) {
            return Err("Not a linked worktree's private Git directory".into());
        }
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let directory = tempfile::Builder::new()
            .prefix("tree-")
            .tempdir_in(root)
            .map_err(|e| e.to_string())?
            .keep();
        // Hold exclusion before publishing a record that another window can discover.
        let operation = lock::acquire(
            directory.parent().ok_or("Missing recovery root")?,
            &directory,
        )?;
        let record = Record {
            original: target.path.clone(),
            admin,
            branch: target.branch.clone(),
        };
        let json = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        std::fs::write(directory.join("record.json"), json).map_err(|e| e.to_string())?;
        // No copy/delete fallback, including across devices. No TempDir destructor may
        // recursively delete this directory once it can contain user content.
        std::fs::rename(&target.path, directory.join("tree")).map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            registered,
            record,
            _operation: operation,
        })
    }

    pub fn tree(&self) -> PathBuf {
        self.directory.join("tree")
    }

    /// Unregister this worktree without Git's destructive remove/prune: move its complete
    /// private metadata instead. Its branch and the common object database are unchanged.
    pub fn unregister(&self) -> Result<(), String> {
        std::fs::rename(&self.record.admin, self.directory.join("git-state"))
            .map_err(|e| e.to_string())
    }

    /// Best-effort rollback. Rename cannot overwrite a nonempty directory. On failure the
    /// retained copy stays discoverable and the error names its location.
    pub fn rollback(&self, error: &str) -> String {
        if vacant(&self.record.original)
            && std::fs::rename(self.tree(), &self.record.original).is_ok()
        {
            return error.to_owned();
        }
        format!("{error}. Full tree retained at {}", self.tree().display())
    }
}

fn record(repo: &Path, directory: &Path) -> Result<Record, String> {
    let root = root(repo)?;
    if directory.parent() != Some(root.as_path())
        || !plain_dir(directory)
        || !directory
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("tree-"))
    {
        return Err("Invalid recovery item".into());
    }
    let bytes = crate::safe_read::read_inside(directory, "record.json", 64 << 10)
        .ok_or("Unreadable recovery record")?;
    let record: Record = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let common = PathBuf::from(archive::repository(repo)?.common_dir);
    if record.admin.parent() != Some(common.join("worktrees").as_path()) {
        return Err("Recovery metadata belongs to another repository".into());
    }
    Ok(record)
}

impl Store {
    /// Retained full trees, independent of the configurable ZIP location and quota.
    pub fn recoveries(repo: &Path) -> Result<Vec<(PathBuf, String)>, String> {
        let root = root(repo)?;
        let mut out = Vec::new();
        if !root.exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
            let directory = entry.map_err(|e| e.to_string())?.path();
            if let Ok(record) = record(repo, &directory)
                && (plain_dir(&directory.join("tree")) || plain_dir(&directory.join("git-state")))
            {
                out.push((
                    directory,
                    format!("{} · {}", record.branch, record.original.display()),
                ));
            }
        }
        out.sort();
        Ok(out)
    }

    /// Restore the full tree and its index, refusing an occupied original path. The record
    /// stays behind on partial failure; no registration is restored without a retained tree.
    pub fn restore_recovery(repo: &Path, directory: &Path) -> Result<(), String> {
        let _operation = lock::acquire(&root(repo)?, directory)?;
        let record = record(repo, directory)?;
        let tree = directory.join("tree");
        if !plain_dir(&tree) || !vacant(&record.original) {
            return Err(
                "Original path is occupied or the retained tree is unavailable; recovery kept"
                    .into(),
            );
        }
        std::fs::rename(&tree, &record.original).map_err(|e| e.to_string())?;
        let state = directory.join("git-state");
        if plain_dir(&state) {
            if record.admin.symlink_metadata().is_ok() {
                // Put the tree back in recovery if registration cannot be restored.
                let _ = std::fs::rename(&record.original, &tree);
                return Err("Worktree registration is occupied; recovery kept".into());
            }
            if let Err(e) = std::fs::rename(state, &record.admin) {
                let _ = std::fs::rename(&record.original, &tree);
                return Err(e.to_string());
            }
        }
        if !plain_dir(&record.admin) {
            let _ = std::fs::rename(&record.original, &tree);
            return Err("Worktree metadata is unavailable; full tree retained".into());
        }
        Ok(())
    }

    /// Only called after a separate explicit confirmation. Traversal is anchored to held
    /// directory handles; mount boundaries/reparse points are refused.
    pub fn purge_recovery(
        repo: &Path,
        directory: &Path,
        check_use: impl FnOnce(&Path) -> Result<Option<String>, String>,
    ) -> Result<(), String> {
        purge_after_validation(repo, directory, || {}, check_use)
    }
}

fn purge_after_validation(
    repo: &Path,
    directory: &Path,
    after: impl FnOnce(),
    check_use: impl FnOnce(&Path) -> Result<Option<String>, String>,
) -> Result<(), String> {
    let _operation = lock::acquire(&root(repo)?, directory)?;
    let held = purge::Purge::open(&root(repo)?, directory)?;
    record(repo, directory)?;
    eprintln!(
        "gitcraft confirmed recovery purge requested: {}",
        directory.display()
    );
    after();
    // The mandatory observer check runs after filesystem preflight, immediately before
    // destructive traversal, while restore and other purges are still excluded.
    let result = held.remove(|| {
        if let Some(reason) = check_use(directory)? {
            return Err(reason);
        }
        Ok(())
    });
    eprintln!(
        "gitcraft recovery purge result for {}: {result:?}",
        directory.display()
    );
    result
}

#[cfg(test)]
#[path = "tests/purge_tests.rs"]
mod tests;
