//! Throwaway worktrees for running tests and rehearsing operations, out of the user's way:
//! detached, inside the repository's git folder (`<common dir>/gitcraft-scratch/`), on the
//! repository's machine, removed when done, however the work ended (dropped, stopped or
//! failed). One left by a gitcraft that crashed is removed the next time one is made.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::git::run;
use crate::remote;

/// The folder, in the git folder, that holds them.
pub const SCRATCH_DIR: &str = "gitcraft-scratch";

/// Numbers the worktrees this process makes.
static MADE: AtomicU64 = AtomicU64::new(0);

/// A worktree of `repo`, detached, removed when dropped.
#[derive(Debug)]
pub struct Scratch {
    repo: PathBuf,
    /// Where it is, in the form the command layer takes (another machine's, encoded).
    pub path: PathBuf,
    removed: bool,
}

impl Scratch {
    /// A new one, at `rev`, its name starting with `label` (`test`), its files not checked
    /// out: each use checks out what it needs.
    pub fn add_empty(repo: &Path, label: &str, rev: &str) -> Result<Scratch, String> {
        sweep_left_over(repo);
        let dir = remote::join(&common_dir(repo)?, SCRATCH_DIR);
        let n = MADE.fetch_add(1, Ordering::Relaxed);
        let path = remote::join(&dir, &format!("{label}-{}-{n}", std::process::id()));
        let there = remote::git_path(&path);
        let args = [
            "worktree",
            "add",
            "--detach",
            "--force",
            "--no-checkout",
            "-q",
            &there,
            rev,
        ];
        run(Some(repo), &args).map_err(|e| format!("could not make a scratch worktree: {e}"))?;
        Ok(Scratch {
            repo: repo.to_owned(),
            path,
            removed: false,
        })
    }

    /// Removes it now, saying why if it couldn't be.
    pub fn remove(mut self) -> Result<(), String> {
        self.removed = true;
        remove_worktree(&self.repo, &self.path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.removed {
            self.removed = true;
            let _ = remove_worktree(&self.repo, &self.path);
        }
    }
}

/// Removes the worktree at `path` of `repo`, and git's record of it even when its folder is
/// gone already.
fn remove_worktree(repo: &Path, path: &Path) -> Result<(), String> {
    let there = remote::git_path(path);
    let removed = run(
        Some(repo),
        &["worktree", "remove", "--force", "--force", &there],
    );
    let pruned = run(Some(repo), &["worktree", "prune"]);
    match (removed, pruned) {
        (Ok(_), _) => Ok(()),
        (Err(_), Ok(_)) if !is_listed(repo, path) => Ok(()),
        (Err(e), _) => Err(format!(
            "could not remove the scratch worktree {there}: {e}"
        )),
    }
}

/// Whether git still lists a worktree at `path`.
fn is_listed(repo: &Path, path: &Path) -> bool {
    let there = remote::git_path(path);
    run(Some(repo), &["worktree", "list", "--porcelain"]).is_ok_and(|out| {
        out.lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .any(|p| p == there)
    })
}

/// The repository's common git folder (the main worktree's `.git`), in the form the command
/// layer takes.
pub fn common_dir(repo: &Path) -> Result<PathBuf, String> {
    let out = run(Some(repo), &["rev-parse", "--git-common-dir"])?;
    let printed = out.trim_end_matches(['\n', '\r']);
    if printed.is_empty() {
        return Err("git didn't say where the repository's git folder is".to_owned());
    }
    Ok(remote::resolve(repo, printed))
}

/// Whether `path` (as git prints it) is one of the scratch worktrees.
pub fn is_scratch_path(path: &str) -> bool {
    let path = path.replace('\\', "/");
    path.contains(&format!("/{SCRATCH_DIR}/"))
}

/// The gitcraft process that made the scratch worktree named `name`, from its name.
fn maker_of(name: &str) -> Option<u32> {
    let mut parts = name.rsplitn(3, '-');
    let _n = parts.next()?;
    parts.next()?.parse().ok()
}

/// Removes the scratch worktrees left by a gitcraft that is no longer running (it crashed, or
/// was killed in the middle of a run). Those of this one, or of one still running, stay.
fn sweep_left_over(repo: &Path) {
    let Ok(out) = run(Some(repo), &["worktree", "list", "--porcelain"]) else {
        return;
    };
    let me = std::process::id();
    for printed in out.lines().filter_map(|l| l.strip_prefix("worktree ")) {
        if !is_scratch_path(printed) {
            continue;
        }
        let name = printed.rsplit(['/', '\\']).next().unwrap_or_default();
        match maker_of(name) {
            Some(pid) if pid == me || process_alive(pid) => {}
            _ => {
                let _ = remove_worktree(repo, &remote::resolve(repo, printed));
            }
        }
    }
}

/// Whether the process `pid` of this machine is running.
fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_paths_are_told_apart_on_every_system() {
        assert!(is_scratch_path("/r/.git/gitcraft-scratch/test-1-0"));
        assert!(is_scratch_path(r"C:\r\.git\gitcraft-scratch\test-1-0"));
        assert!(!is_scratch_path("/r/gitcraft-scratch-not"));
        assert!(!is_scratch_path("/home/me/repo"));
    }

    #[test]
    fn the_maker_is_read_from_the_name() {
        assert_eq!(maker_of("test-1234-7"), Some(1234));
        assert_eq!(maker_of("rehearse-99-0"), Some(99));
        assert_eq!(maker_of("odd"), None);
        assert_eq!(maker_of("a-b-c"), None);
    }

    #[test]
    fn this_process_is_alive() {
        assert!(process_alive(std::process::id()));
    }
}
