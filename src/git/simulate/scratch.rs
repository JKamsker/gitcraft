//! The simulation for a git without `merge-tree --write-tree` (before 2.38) or its
//! `--merge-base` (before 2.40): the commits cherry-picked, or the merge made, in a scratch
//! clone in the temporary folder of the repository's machine. The clone borrows the
//! repository's objects (`--shared`) and writes its own only into itself; nothing is written
//! into the repository. It is removed when done.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::super::run;
use super::{CANCELLED, Engine, Merge, Replay, Replayed, Stop, WRITE_OPTS};

/// Options for the clone's commands on top of [`WRITE_OPTS`]: Git LFS (from the user's global
/// config) leaves the files as their pointers, as a commit in memory has them, rather than
/// download them.
const SCRATCH_OPTS: [&str; 6] = [
    "-c",
    "filter.lfs.process=",
    "-c",
    "filter.lfs.smudge=",
    "-c",
    "filter.lfs.required=false",
];

/// A clone of a repository in the temporary folder, removed when dropped.
struct Scratch {
    /// The clone as [`run`] takes it (a path of the repository's machine).
    dir: PathBuf,
    /// What to remove: the folder here, or the scratch folder on the other machine.
    remove: Removal,
}

enum Removal {
    Here(PathBuf),
    There(crate::remote::Target, String),
}

impl Scratch {
    fn of(repo: &Path) -> Result<Self, String> {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let (dir, remove, dest) = match crate::remote::parse(repo) {
            Some(target) => {
                let folder = crate::remote::host_scratch_dir(&target)?;
                let dest = format!("{folder}/clone");
                let dir = crate::remote::encode(&target.host, &dest);
                (dir, Removal::There(target, folder), dest)
            }
            None => {
                let folder = std::env::temp_dir().join(format!(
                    "gitcraft-ghost-run-{}-{}-{}",
                    std::process::id(),
                    SEQ.fetch_add(1, Ordering::Relaxed),
                    super::super::now()
                ));
                let dest = folder.to_string_lossy().into_owned();
                (folder.clone(), Removal::Here(folder), dest)
            }
        };
        let scratch = Self { dir, remove };
        // From the repository's own folder: `.` is the repository wherever it is, a linked
        // worktree included.
        run(
            Some(repo),
            &[
                "clone",
                "--shared",
                "--no-checkout",
                "--quiet",
                "--",
                ".",
                &dest,
            ],
        )
        .map_err(|e| format!("could not make a scratch clone: {e}"))?;
        Ok(scratch)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        let all: Vec<&str> = WRITE_OPTS
            .iter()
            .chain(SCRATCH_OPTS.iter())
            .copied()
            .chain(args.iter().copied())
            .collect();
        run(Some(&self.dir), &all)
    }

    fn rev(&self, rev: &str) -> Result<String, String> {
        self.git(&["rev-parse", "--verify", "-q", rev])
            .map(|s| s.trim().to_owned())
    }

    /// The files in conflict after a cherry-pick or merge stopped.
    fn conflicts(&self) -> Vec<String> {
        self.git(&["diff", "--name-only", "--diff-filter=U", "-z"])
            .map(|out| {
                out.split('\0')
                    .filter(|f| !f.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        match &self.remove {
            Removal::Here(folder) => remove_all(folder),
            Removal::There(target, folder) => {
                let _ = crate::remote::remove_host_scratch_dir(target, folder);
            }
        }
    }
}

/// Removes `folder` and all in it. git makes its object files read-only, which Windows won't
/// delete: they are made writable and the removal tried again.
fn remove_all(folder: &Path) {
    if std::fs::remove_dir_all(folder).is_ok() || !folder.exists() {
        return;
    }
    make_writable(folder);
    let _ = std::fs::remove_dir_all(folder);
}

fn make_writable(path: &Path) {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.is_dir() {
            for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
                make_writable(&entry.path());
            }
        } else if !meta.file_type().is_symlink() {
            let mut perms = meta.permissions();
            // Only files of the scratch clone, which is this user's own: writable by them.
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

/// [`super::replay`] in a scratch clone: each commit cherry-picked onto `onto`.
pub(super) fn replay(
    repo: &Path,
    onto: &str,
    picks: &[String],
    cancel: &dyn Fn() -> bool,
) -> Result<Replay, String> {
    let s = Scratch::of(repo)?;
    s.git(&["checkout", "-q", "--detach", onto])?;
    let mut tree = s.rev(&format!("{onto}^{{tree}}"))?;
    let mut steps = Vec::with_capacity(picks.len());
    for (step, commit) in picks.iter().enumerate() {
        if cancel() {
            return Err(CANCELLED.to_owned());
        }
        let picked = s.git(&[
            "cherry-pick",
            "--allow-empty",
            "--keep-redundant-commits",
            commit,
        ]);
        if let Err(e) = picked {
            let files = s.conflicts();
            if files.is_empty() {
                return Err(e);
            }
            return Ok(Replay {
                onto: onto.to_owned(),
                steps,
                stop: Some(Stop {
                    step,
                    commit: commit.clone(),
                    files,
                }),
                engine: Engine::Scratch,
            });
        }
        let now = s.rev("HEAD^{tree}")?;
        let started_empty =
            s.rev(&format!("{commit}^{{tree}}"))? == s.rev(&format!("{commit}^^{{tree}}"))?;
        let copy = if now == tree && !started_empty {
            // Dropped, as a rebase drops a commit that became empty.
            s.git(&["reset", "-q", "--soft", "HEAD~1"])?;
            None
        } else {
            tree = now;
            Some(s.rev("HEAD")?)
        };
        steps.push(Replayed {
            original: commit.clone(),
            copy,
            tree: tree.clone(),
        });
    }
    Ok(Replay {
        onto: onto.to_owned(),
        steps,
        stop: None,
        engine: Engine::Scratch,
    })
}

/// [`super::merge`] in a scratch clone.
pub(super) fn merge(repo: &Path, ours: &str, theirs: &str) -> Result<Merge, String> {
    let s = Scratch::of(repo)?;
    s.git(&["checkout", "-q", "--detach", ours])?;
    let merged = s.git(&["merge", "-q", "--no-ff", "--no-commit", "--no-edit", theirs]);
    let conflicts = match merged {
        Ok(_) => Vec::new(),
        Err(e) => {
            let files = s.conflicts();
            if files.is_empty() {
                return Err(e);
            }
            files
        }
    };
    let tree = if conflicts.is_empty() {
        s.git(&["write-tree"])?.trim().to_owned()
    } else {
        String::new()
    };
    Ok(Merge {
        tree,
        conflicts,
        engine: Engine::Scratch,
    })
}

#[cfg(test)]
mod tests {
    use super::Scratch;
    use crate::git::simulate::tests::TestRepo;

    /// The clone is in the temporary folder, has the repository's commits, and is gone once
    /// dropped.
    #[test]
    fn a_scratch_clone_goes_when_done() -> Result<(), String> {
        let repo = TestRepo::new("scratch-drop")?;
        let head = repo.commit(&[("a", "1\n")], "base")?;
        let scratch = Scratch::of(&repo.dir)?;
        let dir = scratch.dir.clone();
        let found = scratch.rev(&format!("{head}^{{commit}}"))?;
        assert!(dir.starts_with(std::env::temp_dir()) && dir.is_dir());
        drop(scratch);
        assert_eq!(found, head);
        assert!(!dir.exists());
        Ok(())
    }
}
