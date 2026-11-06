//! Lock files a stopped git command may leave behind (`index.lock`, `refs/…/x.lock`): git
//! removes its own when asked to stop politely, but not when it is killed outright (Windows
//! has no polite way).

use std::path::{Path, PathBuf};

use super::run;

/// The lock files git keeps at the top of a repository's git folder.
const TOP_LOCKS: [&str; 6] = [
    "index.lock",
    "HEAD.lock",
    "ORIG_HEAD.lock",
    "packed-refs.lock",
    "shallow.lock",
    "config.lock",
];

/// The lock files in the repository at `repo` (one of this machine), its worktree's git folder
/// and the common one, refs included. Another machine's stopped commands are stopped politely
/// and remove their own, so none are looked for there.
pub fn leftover_locks(repo: &Path) -> Vec<PathBuf> {
    if crate::remote::is_remote(repo) {
        return Vec::new();
    }
    let Ok(out) = run(
        Some(repo),
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ],
    ) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = out.lines().map(PathBuf::from).collect();
    dirs.dedup();
    let mut found = Vec::new();
    for dir in &dirs {
        found.extend(
            TOP_LOCKS
                .iter()
                .map(|name| dir.join(name))
                .filter(|p| p.is_file()),
        );
    }
    if let Some(common) = dirs.last() {
        locks_under(&common.join("refs"), &mut found, 0);
    }
    found
}

/// How deep in `refs` locks are looked for (`refs/remotes/origin/feature/x.lock` is 4).
const MAX_DEPTH: usize = 12;

fn locks_under(dir: &Path, found: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth < MAX_DEPTH {
            locks_under(&path, found, depth + 1);
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "lock") {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_locks_a_killed_git_left() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-locks é {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        run(Some(&dir), &["init", "-q"])?;
        assert!(leftover_locks(&dir).is_empty());
        let git = dir.join(".git");
        let deep = git.join("refs/remotes/origin/feature");
        std::fs::create_dir_all(&deep).map_err(|e| e.to_string())?;
        for lock in [git.join("index.lock"), deep.join("x y.lock")] {
            std::fs::write(&lock, "").map_err(|e| e.to_string())?;
        }
        let mut found = leftover_locks(&dir);
        found.sort();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|p| p.ends_with("index.lock")));
        assert!(found.iter().any(|p| p.ends_with("x y.lock")));
        Ok(())
    }
}
