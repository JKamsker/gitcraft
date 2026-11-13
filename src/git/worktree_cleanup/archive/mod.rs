//! Archives of a worktree's untracked text files, kept before it is cleaned up: a ZIP of the
//! files and a manifest (the repository, the worktree's path, its branch and commit, each
//! file's path, size and SHA-256), in a folder of their own per repository, outside every
//! worktree. Archives are only ever added or deleted by hand: none is evicted to make room;
//! one that would go over the storage limit is refused instead.

mod restore;
mod write;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub(crate) use restore::RestoreReport;
pub(in crate::git) use write::{Source, create};

use super::files::{footprint, human, sha256_hex};

const MANIFEST_VERSION: u32 = 1;

/// Where archives are kept and how much they may take together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Store {
    pub root: PathBuf,
    /// Bytes all archives may take together.
    pub quota: u64,
}

/// The repository an archive belongs to: its common git folder, and a key made from it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::git) struct Repository {
    pub common_dir: String,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::git) struct Entry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// A restore that was done: when, into which worktree, and what it made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::git) struct Restored {
    pub at: i64,
    pub into: String,
    pub created: Vec<String>,
    /// Files already there with other content: restored beside them, under these names.
    pub renamed: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::git) struct Manifest {
    pub version: u32,
    pub id: String,
    pub created: i64,
    pub repository: Repository,
    pub worktree: String,
    pub branch: String,
    pub head: String,
    pub files: Vec<Entry>,
    #[serde(default)]
    pub restored: Vec<Restored>,
}

/// An archive found in the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(in crate::git) manifest: Manifest,
    pub zip: PathBuf,
    /// The ZIP and its manifest on disk.
    pub bytes: u64,
}

impl Store {
    fn folder(&self, key: &str) -> Result<PathBuf, String> {
        let clean = !key.is_empty() && key.bytes().all(|b| b.is_ascii_hexdigit());
        if !clean {
            return Err(crate::i18n::text("worktree-cleanup-archive-bad-key").to_owned());
        }
        Ok(self.root.join(key))
    }

    fn paths(&self, key: &str, id: &str) -> Result<(PathBuf, PathBuf), String> {
        let clean = !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if !clean {
            return Err(crate::i18n::text("worktree-cleanup-archive-bad-key").to_owned());
        }
        let folder = self.folder(key)?;
        Ok((
            folder.join(format!("{id}.zip")),
            folder.join(format!("{id}.json")),
        ))
    }

    /// The bytes every archive takes together (every file under the root, links not
    /// followed).
    pub fn usage(&self) -> u64 {
        if !self.root.exists() {
            return 0;
        }
        footprint(&self.root).bytes
    }

    /// The archives of the repository `key` (every repository's when `None`), newest first.
    /// A manifest that can't be read is skipped.
    pub fn list(&self, key: Option<&str>) -> Vec<Listed> {
        let mut out = Vec::new();
        let folders: Vec<PathBuf> = match key {
            Some(key) => self.folder(key).into_iter().collect(),
            None => std::fs::read_dir(&self.root)
                .map(|d| d.flatten().map(|e| e.path()).collect())
                .unwrap_or_default(),
        };
        for folder in folders {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json")
                    && let Some(listed) = read_listed(&path)
                {
                    out.push(listed);
                }
            }
        }
        out.sort_by(|a, b| {
            (b.manifest.created, &b.manifest.id).cmp(&(a.manifest.created, &a.manifest.id))
        });
        out
    }

    /// Deletes the archive `listed`.
    pub fn delete(&self, listed: &Listed) -> Result<(), String> {
        self.delete_manifest(&listed.manifest)
    }

    /// Deletes an archive: its ZIP, then its manifest.
    pub(in crate::git) fn delete_manifest(&self, manifest: &Manifest) -> Result<(), String> {
        let (zip, json) = self.paths(&manifest.repository.key, &manifest.id)?;
        for path in [zip, json] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }

    /// Whether the archives would be kept in `folder` (or a folder in it): never in the worktree
    /// being removed. A folder not made yet is judged by where it would be made.
    pub(in crate::git) fn is_inside(&self, folder: &Path) -> bool {
        let real = |p: &Path| {
            let mut existing = p.to_path_buf();
            let mut rest = Vec::new();
            while !existing.exists() {
                match (
                    existing.file_name().map(|n| n.to_owned()),
                    existing.parent(),
                ) {
                    (Some(name), Some(parent)) => {
                        rest.push(name);
                        existing = parent.to_path_buf();
                    }
                    _ => break,
                }
            }
            let mut out = existing.canonicalize().unwrap_or(existing);
            out.extend(rest.iter().rev());
            out
        };
        real(&self.root).starts_with(real(folder))
    }

    /// The archives of the repository `repo` (any of its worktrees) is part of.
    pub fn list_for(&self, repo: &Path) -> Result<Vec<Listed>, String> {
        Ok(self.list(Some(&repository(repo)?.key)))
    }

    /// Restores `archive` into the worktree `dest` (see [`restore::restore`]).
    pub fn restore(&self, archive: &Listed, dest: &Path) -> Result<RestoreReport, String> {
        restore::restore(self, archive, dest)
    }

    /// "12.0 MB of 268.4 MB used", for `used` bytes.
    pub fn usage_line(&self, used: u64) -> String {
        crate::i18n::message(
            "worktree-cleanup-archive-usage",
            &[
                ("used", human(used).into()),
                ("quota", human(self.quota).into()),
            ],
        )
    }

    /// The archives' default folder: in the app's data folder; in tests, a temporary one of
    /// the test run's own, never the user's.
    pub fn default_root() -> Option<PathBuf> {
        if cfg!(test) {
            return Some(
                std::env::temp_dir().join(format!("gitcraft-archives-test-{}", std::process::id())),
            );
        }
        crate::portable::data_dir().map(|d| d.join("worktree-archives"))
    }

    /// Records a restore in the archive's manifest (written whole, then moved in place).
    pub(in crate::git) fn record(
        &self,
        manifest: &Manifest,
        restored: Restored,
    ) -> Result<Manifest, String> {
        let mut manifest = manifest.clone();
        manifest.restored.push(restored);
        let (_, json) = self.paths(&manifest.repository.key, &manifest.id)?;
        write_json(&json, &manifest)?;
        Ok(manifest)
    }
}

fn read_listed(json: &Path) -> Option<Listed> {
    let text = std::fs::read(json).ok()?;
    let manifest: Manifest = serde_json::from_slice(&text).ok()?;
    let zip = json.with_extension("zip");
    let zip_size = std::fs::symlink_metadata(&zip)
        .ok()
        .filter(|m| m.is_file())?
        .len();
    Some(Listed {
        bytes: zip_size + text.len() as u64,
        manifest,
        zip,
    })
}

/// Writes `value` as JSON to `path` through a temporary file beside it, so that a reader never
/// sees half of it.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, path).map_err(|e| {
        let _ = std::fs::remove_file(&partial);
        e.to_string()
    })
}

impl Listed {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    pub fn branch(&self) -> &str {
        &self.manifest.branch
    }

    /// The worktree's folder it was archived from.
    pub fn worktree(&self) -> &str {
        &self.manifest.worktree
    }

    pub fn file_count(&self) -> usize {
        self.manifest.files.len()
    }

    /// Whether it waits to be restored into a worktree of `branch` at `head`: the same branch
    /// at the commit it was archived from (a branch of the same name made again elsewhere is
    /// another one), never restored before.
    pub fn awaits(&self, branch: &str, head: &str) -> bool {
        let m = &self.manifest;
        m.restored.is_empty() && m.branch == branch && m.head == head
    }

    /// "feature/x · 3 files · 4.1 KB · 2 hours ago", and whether it was restored.
    pub fn describe(&self) -> String {
        let (listed, m) = (self, &self.manifest);
        crate::i18n::message(
            "worktree-cleanup-archive-line",
            &[
                ("branch", m.branch.clone().into()),
                ("count", m.files.len().into()),
                ("size", human(listed.bytes).into()),
                (
                    "when",
                    crate::git::relative_time(m.created, crate::git::now()).into(),
                ),
                ("restored", m.restored.len().into()),
            ],
        )
    }
}

/// The repository `path` (any of its worktrees) is part of.
pub(in crate::git) fn repository(path: &Path) -> Result<Repository, String> {
    let dir = crate::git::run(Some(path), &["rev-parse", "--git-common-dir"])?;
    let dir = Path::new(dir.trim());
    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        path.join(dir)
    };
    let dir = dir.canonicalize().map_err(|e| e.to_string())?;
    let common_dir = dir.to_string_lossy().into_owned();
    let key = sha256_hex(common_dir.as_bytes())[..16].to_owned();
    Ok(Repository { common_dir, key })
}
