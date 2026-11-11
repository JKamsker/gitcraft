//! Throwaway worktrees for running tests and rehearsing operations, out of the user's way:
//! detached, on the repository's machine, in its temporary folder (never inside the user's
//! checkout, where tools looking in parent folders would find its files), removed when done
//! however the work ended (dropped, stopped or failed). One left by a gitcraft of this machine
//! that crashed is removed the next time one is made.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::git::run;
use crate::remote;

/// The folder, in the temporary folder, that holds them (a folder per repository in it).
pub const SCRATCH_DIR: &str = "gitcraft-scratch";

/// What the worktrees are made for: the first part of their names.
const LABELS: [&str; 4] = ["test", "rehearse", "interaction", "resolve"];

/// Numbers the worktrees this process makes.
static MADE: AtomicU64 = AtomicU64::new(0);

/// A worktree of `repo`, detached, removed when dropped.
#[derive(Debug)]
pub struct Scratch {
    repo: PathBuf,
    /// Where it is, in the form the command layer takes (another machine's, encoded).
    pub path: PathBuf,
    removed: bool,
    preserved: AtomicBool,
}

impl Scratch {
    /// A new one, at `rev`, for `label` (`test`, `rehearse`, `interaction`), its files not
    /// checked out: each use checks out what it needs.
    pub fn add_empty(repo: &Path, label: &str, rev: &str) -> Result<Scratch, String> {
        let root = scratch_root(repo)?;
        sweep_left_over(repo, &root);
        let name = ScratchName {
            label: label.to_owned(),
            machine: this_machine(),
            pid: std::process::id(),
            n: MADE.fetch_add(1, Ordering::Relaxed),
        };
        let path = remote::join(&root, &name.to_string());
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
        run(Some(repo), &args).map_err(|e| {
            crate::i18n::message(
                "backend-could-not-make-a-scratch-worktree-e-76b327",
                &[("detail", (e).to_string().into())],
            )
        })?;
        Ok(Scratch {
            repo: repo.to_owned(),
            path,
            removed: false,
            preserved: AtomicBool::new(false),
        })
    }

    /// Retain uncertain resolver work across drop and future stale-worktree sweeps.
    pub(crate) fn preserve(&self) -> Result<(), String> {
        self.preserved.store(true, Ordering::Release);
        std::fs::write(
            self.path.with_extension("preserved"),
            b"cleanup unconfirmed\n",
        )
        .map_err(|e| e.to_string())
    }

    /// Checks out `rev` in it, whatever was changed there.
    pub fn checkout(&self, rev: &str) -> Result<(), String> {
        run(
            Some(&self.path),
            &["checkout", "--detach", "--force", "-q", rev],
        )
        .map(|_| ())
        .map_err(|e| {
            let at = rev.get(..8).unwrap_or(rev);
            crate::i18n::message(
                "backend-could-not-check-out-at-in-a-scratch-worktree-e-e85eed",
                &[
                    ("at", (at).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })
    }

    /// Removes it now, saying why if it couldn't be.
    pub fn remove(mut self) -> Result<(), String> {
        self.removed = true;
        remove_worktree(&self.repo, &self.path)
    }

    /// Removes it now, trying `attempts` times `pause` apart while it can't be (a program that
    /// worked in it may hold its folder a moment longer).
    pub fn remove_retrying(
        mut self,
        attempts: u32,
        pause: std::time::Duration,
    ) -> Result<(), String> {
        self.removed = true;
        let mut tried = 1;
        loop {
            match remove_worktree(&self.repo, &self.path) {
                Err(_) if tried < attempts => {
                    tried += 1;
                    std::thread::sleep(pause);
                }
                done => return done,
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.removed && !self.preserved.load(Ordering::Acquire) {
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
        (Err(e), _) => Err(crate::i18n::message(
            "backend-could-not-remove-the-scratch-worktree-there-e-a9a777",
            &[
                ("there", (there).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
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

/// The repository's common git folder (the main worktree's `.git`), as git there names it.
fn common_dir_there(repo: &Path) -> Result<String, String> {
    let out = run(Some(repo), &["rev-parse", "--git-common-dir"])?;
    let printed = out.trim_end_matches(['\n', '\r']);
    if printed.is_empty() {
        return Err(crate::i18n::text(
            "backend-git-didn-t-say-where-the-repository-s-git-folder-is-ecfd5a",
        )
        .to_owned());
    }
    Ok(remote::git_path(&remote::resolve(repo, printed)))
}

/// The folder of `repo`'s scratch worktrees: `<temporary folder>/gitcraft-scratch/<key>` on
/// the repository's machine, `key` telling repositories apart.
pub fn scratch_root(repo: &Path) -> Result<PathBuf, String> {
    let common = common_dir_there(repo)?;
    let key = format!("{:x}", md5::compute(common.as_bytes()));
    let key = key.get(..12).unwrap_or(&key);
    Ok(match remote::parse(repo) {
        Some(t) => remote::encode(&t.host, &format!("/tmp/{SCRATCH_DIR}/{key}")),
        None => std::env::temp_dir().join(SCRATCH_DIR).join(key),
    })
}

/// A scratch worktree's folder name, `<label>-<machine>-<pid>-<n>`: the machine and process of
/// the gitcraft that made it, so that one left by a crash can be told from one in use.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ScratchName {
    label: String,
    machine: String,
    pid: u32,
    n: u64,
}

impl std::fmt::Display for ScratchName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}-{}", self.label, self.machine, self.pid, self.n)
    }
}

impl ScratchName {
    /// The name `name` is, when it is one.
    fn parse(name: &str) -> Option<ScratchName> {
        let mut parts = name.split('-');
        let (label, machine) = (parts.next()?, parts.next()?);
        let (pid, n) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
        if parts.next().is_some() || !LABELS.contains(&label) || machine.is_empty() {
            return None;
        }
        Some(ScratchName {
            label: label.to_owned(),
            machine: machine.to_owned(),
            pid,
            n,
        })
    }
}

/// This machine's name as it goes in a scratch name: letters and digits only.
fn this_machine() -> String {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_default();
    machine_word(&name)
}

fn machine_word(name: &str) -> String {
    let word: String = name
        .trim()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(24)
        .collect();
    if word.is_empty() {
        "here".to_owned()
    } else {
        word
    }
}

/// Whether `path` (as git prints it) is one of the scratch worktrees: a scratch name, in a
/// repository's folder of [`SCRATCH_DIR`].
pub fn is_scratch_path(path: &str) -> bool {
    let path = path.replace('\\', "/");
    let mut parts = path.trim_end_matches('/').rsplit('/');
    let (Some(name), Some(_key), Some(dir)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    dir == SCRATCH_DIR && ScratchName::parse(name).is_some()
}

/// Removes the scratch worktrees of `repo` (in `root`) left by a gitcraft of this machine that
/// is no longer running (it crashed, or was killed in the middle of a run). Those of this one,
/// of one still running, of another machine, and anything not named as gitcraft names them,
/// stay.
fn sweep_left_over(repo: &Path, root: &Path) {
    let Ok(out) = run(Some(repo), &["worktree", "list", "--porcelain"]) else {
        return;
    };
    // By the folders' names, not the whole path: the same folder can be printed otherwise than
    // it was given (Windows' short `RUNNER~1` against the long name, the case of a drive).
    let key = root
        .file_name()
        .map(|k| k.to_string_lossy().into_owned())
        .unwrap_or_default();
    let me = this_machine();
    for printed in out.lines().filter_map(|l| l.strip_prefix("worktree ")) {
        let Some(name) = scratch_name_in(printed, &key) else {
            continue;
        };
        let path = remote::resolve(repo, printed);
        if path.with_extension("preserved").exists() {
            continue;
        }
        if name.machine == me && name.pid != std::process::id() && !process_alive(name.pid) {
            let _ = remove_worktree(repo, &remote::resolve(repo, printed));
        }
    }
}

/// The scratch name of the worktree git prints at `printed`, when it is in the scratch folder
/// `key` (`…/gitcraft-scratch/<key>/<name>`).
fn scratch_name_in(printed: &str, key: &str) -> Option<ScratchName> {
    let mut parts = printed.trim_end_matches(['/', '\\']).rsplit(['/', '\\']);
    let (name, folder, dir) = (parts.next()?, parts.next()?, parts.next()?);
    (folder == key && dir == SCRATCH_DIR)
        .then(|| ScratchName::parse(name))
        .flatten()
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

/// This machine's name as scratch names have it, for tests elsewhere.
#[cfg(test)]
pub(crate) fn tests_machine() -> String {
    this_machine()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_paths_are_told_apart_on_every_system() {
        assert!(is_scratch_path(
            "/tmp/gitcraft-scratch/0123abcd/test-box-1-0"
        ));
        assert!(is_scratch_path(
            r"C:\Temp\gitcraft-scratch\0123abcd\rehearse-PC1-7-2"
        ));
        // Folders of the user's that happen to be called so.
        assert!(!is_scratch_path("/home/me/gitcraft-scratch/wip"));
        assert!(!is_scratch_path("/home/me/gitcraft-scratch/x/wip"));
        assert!(!is_scratch_path("/home/me/other/k/test-box-1-0"));
        assert!(!is_scratch_path("test-box-1-0"));
    }

    #[test]
    fn names_say_who_made_them() {
        let name = ScratchName {
            label: "test".into(),
            machine: "box".into(),
            pid: 1234,
            n: 7,
        };
        assert_eq!(name.to_string(), "test-box-1234-7");
        assert_eq!(ScratchName::parse("test-box-1234-7"), Some(name));
        assert_eq!(ScratchName::parse("wip"), None);
        assert_eq!(ScratchName::parse("test-1234-7"), None);
        assert_eq!(ScratchName::parse("other-box-1-2"), None);
        assert_eq!(ScratchName::parse("test-box-1-2-3"), None);
        assert_eq!(machine_word(" my-pc.local\n"), "mypclocal");
        assert_eq!(machine_word(""), "here");
    }

    /// Found by the folders' names, however the path before them is written.
    #[test]
    fn a_scratch_worktree_is_found_by_its_folders() {
        let long = "C:/Users/runneradmin/AppData/Local/Temp/gitcraft-scratch/abc/test-box-1-0";
        let found = scratch_name_in(long, "abc").map(|n| n.pid);
        assert_eq!(found, Some(1));
        let back = r"C:\Users\RUNNER~1\Temp\gitcraft-scratch\abc\test-box-2-0";
        assert_eq!(scratch_name_in(back, "abc").map(|n| n.pid), Some(2));
        assert!(scratch_name_in(long, "other").is_none());
        assert!(scratch_name_in("/home/me/abc/test-box-1-0", "abc").is_none());
        assert!(scratch_name_in("/tmp/gitcraft-scratch/abc/wip", "abc").is_none());
    }

    #[test]
    fn this_process_is_alive() {
        assert!(process_alive(std::process::id()));
    }
}
