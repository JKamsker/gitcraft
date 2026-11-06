//! A `.lock` file git left in a repository: how old it is, whether git is running, and
//! removing it. On whichever machine the repository is: here through `std::fs`, elsewhere
//! through a shell there. Never on the window's thread.

use std::path::{Path, PathBuf};

use crate::git::run;
use crate::remote::{self, Request};

/// Exit codes of the scripts below, besides 0.
const IS_LINK: i32 = 10;
const NOT_FOUND: i32 = 11;
const NOT_A_FILE: i32 = 12;
const CHANGED: i32 = 13;

/// A lock file as it was seen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockFile {
    /// Its path as git printed it, on the repository's machine.
    pub shown: String,
    /// When it was last written (Unix seconds, that machine's clock).
    pub modified: i64,
    /// How long ago that was, in seconds.
    pub age: u64,
}

/// The lock `lock` (as git printed it) of the repository at `repo`, checked to be one: a
/// plain file named `….lock` in the repository's git folder. `Ok(None)` when it is gone.
pub fn inspect(repo: &Path, lock: &str) -> Result<Option<LockFile>, String> {
    let lock = within_git_dir(repo, lock)?;
    let Some(t) = remote::parse(repo) else {
        return inspect_here(&lock);
    };
    let f = remote::sh_path(&lock);
    let script = format!(
        "{}\nprintf '%s %s\\n' \"$m\" \"$(date +%s)\"",
        stat_script(&f)
    );
    let out = exec(&t.host, script)?;
    match out.code {
        0 => {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut words = text.split_whitespace().map(str::parse::<i64>);
            let (Some(Ok(modified)), Some(Ok(now))) = (words.next(), words.next()) else {
                return Err(format!("could not read the age of {lock}: {text}"));
            };
            Ok(Some(LockFile {
                shown: lock,
                modified,
                age: now.saturating_sub(modified).max(0) as u64,
            }))
        }
        NOT_FOUND => Ok(None),
        code => Err(refused(&lock, code, &out.stderr)),
    }
}

/// Removes the lock seen as `seen`, unless it was written again since (a git is using it) or
/// is no longer a plain file.
pub fn remove(repo: &Path, seen: &LockFile) -> Result<(), String> {
    let lock = within_git_dir(repo, &seen.shown)?;
    let Some(t) = remote::parse(repo) else {
        return remove_here(&lock, seen.modified);
    };
    let f = remote::sh_path(&lock);
    let script = format!(
        "{}\n[ \"$m\" = {} ] || exit {CHANGED}\nexec rm -f -- {f}",
        stat_script(&f),
        seen.modified
    );
    let out = exec(&t.host, script)?;
    match out.code {
        0 => Ok(()),
        // Gone already: what was asked is done.
        NOT_FOUND => Ok(()),
        code => Err(refused(&lock, code, &out.stderr)),
    }
}

/// Sets `m` to the modification time of the file `f` (a shell word), after checking that it
/// is a plain file (GNU or BSD `stat`).
fn stat_script(f: &str) -> String {
    format!(
        "[ -L {f} ] && exit {IS_LINK}\n[ -e {f} ] || exit {NOT_FOUND}\n[ -f {f} ] || exit \
         {NOT_A_FILE}\nm=$(stat -c %Y -- {f} 2>/dev/null || stat -f %m -- {f}) || exit 1"
    )
}

fn exec(host: &remote::Host, script: String) -> Result<remote::Output, String> {
    remote::exec(
        host,
        &Request {
            script,
            ..Request::default()
        },
    )
}

fn refused(lock: &str, code: i32, stderr: &[u8]) -> String {
    match code {
        IS_LINK => format!("{lock} is a symbolic link, not a lock git made: left alone"),
        NOT_A_FILE => format!("{lock} is not a file: left alone"),
        CHANGED => format!("{lock} was written again since: a git is using it, left alone"),
        _ => {
            let said = String::from_utf8_lossy(stderr).trim().to_owned();
            format!("could not reach {lock}: {said} (exit status {code})")
        }
    }
}

fn inspect_here(lock: &str) -> Result<Option<LockFile>, String> {
    let path = PathBuf::from(lock);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("could not read {lock}: {e}")),
    };
    if meta.file_type().is_symlink() {
        return Err(refused(lock, IS_LINK, &[]));
    }
    if !meta.is_file() {
        return Err(refused(lock, NOT_A_FILE, &[]));
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    Ok(Some(LockFile {
        shown: lock.to_owned(),
        modified,
        age: crate::git::now().saturating_sub(modified).max(0) as u64,
    }))
}

fn remove_here(lock: &str, modified: i64) -> Result<(), String> {
    match inspect_here(lock)? {
        None => Ok(()),
        Some(now) if now.modified != modified => Err(refused(lock, CHANGED, &[])),
        Some(_) => std::fs::remove_file(lock).map_err(|e| format!("could not remove {lock}: {e}")),
    }
}

/// `lock` as a path of the repository's machine, if it is a lock in the repository's git
/// folder (or the one its worktrees share): never any other file.
fn within_git_dir(repo: &Path, lock: &str) -> Result<String, String> {
    let name = lock.rsplit(['/', '\\']).next().unwrap_or_default();
    if !name.ends_with(".lock") || name == ".lock" {
        return Err(format!("{lock} is not a lock file"));
    }
    if lock.split(['/', '\\']).any(|part| part == "..") {
        return Err(format!("{lock} is not a path git makes locks at"));
    }
    let dirs = run(
        Some(repo),
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ],
    )?;
    let here = !remote::is_remote(repo);
    let lock_key = comparable(lock, here);
    let inside = dirs
        .lines()
        .map(|dir| comparable(dir.trim_end_matches('\r'), here))
        .filter(|dir| !dir.is_empty())
        .any(|dir| lock_key.starts_with(&format!("{}/", dir.trim_end_matches('/'))));
    if inside {
        Ok(lock.to_owned())
    } else {
        Err(format!(
            "{lock} is not in this repository's git folder: left alone"
        ))
    }
}

/// A path to compare with another of the same machine: `/` between names, and on Windows
/// (`here`: git runs on this machine) without regard to case.
fn comparable(path: &str, here: bool) -> String {
    let path = path.replace('\\', "/");
    if cfg!(windows) && here {
        path.to_lowercase()
    } else {
        path
    }
}

/// The git processes running on the machine of the repository at `repo` (their command
/// lines), but for those this app started itself where that can be told.
pub fn git_processes(repo: &Path) -> Result<Vec<String>, String> {
    match remote::parse(repo) {
        Some(t) => {
            let out = exec(&t.host, "exec ps -A -o pid= -o ppid= -o args=".to_owned())?;
            if out.code != 0 {
                return Err(format!(
                    "could not list the processes there: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            Ok(git_lines(&String::from_utf8_lossy(&out.stdout), None))
        }
        None => processes_here(),
    }
}

#[cfg(unix)]
fn processes_here() -> Result<Vec<String>, String> {
    let out = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=", "-o", "ppid=", "-o", "args="])
        .output()
        .map_err(|e| format!("could not list the processes: {e}"))?;
    if !out.status.success() {
        return Err("could not list the processes: ps failed".to_owned());
    }
    Ok(git_lines(
        &String::from_utf8_lossy(&out.stdout),
        Some(std::process::id()),
    ))
}

#[cfg(windows)]
fn processes_here() -> Result<Vec<String>, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH", "/FI", "IMAGENAME eq git.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("could not list the processes: {e}"))?;
    Ok(tasklist_git(&String::from_utf8_lossy(&out.stdout)))
}

/// `"git.exe","1234",…` lines of `tasklist /FO CSV`: `git.exe (process 1234)` each.
#[cfg(any(windows, test))]
pub(super) fn tasklist_git(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| {
            let mut fields = line.split("\",\"");
            let image = fields.next()?.trim_start_matches('"');
            let pid = fields.next()?;
            image
                .eq_ignore_ascii_case("git.exe")
                .then(|| format!("git.exe (process {pid})"))
        })
        .collect()
}

/// The git command lines of `ps -o pid= -o ppid= -o args=` output: those whose program is
/// `git` or one of its helpers (`git-remote-https`…), but for the descendants of `own` (this
/// process), cut to a readable length.
pub(super) fn git_lines(ps: &str, own: Option<u32>) -> Vec<String> {
    let rows: Vec<(u32, u32, &str)> = ps
        .lines()
        .filter_map(|line| {
            let (pid, rest) = first_word(line)?;
            let (ppid, args) = first_word(rest)?;
            Some((pid.parse().ok()?, ppid.parse().ok()?, args))
        })
        .collect();
    let parent = |pid: u32| rows.iter().find(|r| r.0 == pid).map(|r| r.1);
    let ours = |mut pid: u32| {
        let Some(own) = own else { return false };
        // Up the parents, a few levels at most (a cycle can't hold it up).
        for _ in 0..16 {
            match parent(pid) {
                Some(p) if p == own => return true,
                Some(p) if p != pid && p != 0 => pid = p,
                _ => return false,
            }
        }
        false
    };
    rows.iter()
        .filter(|(pid, _, args)| is_git(args) && !ours(*pid))
        .map(|(_, _, args)| {
            let mut shown: String = args.chars().take(100).collect();
            if args.chars().count() > 100 {
                shown.push('…');
            }
            shown
        })
        .collect()
}

/// The first word of `s` and what follows it, its spaces left out.
fn first_word(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    (end > 0).then(|| (&s[..end], s[end..].trim_start()))
}

/// Whether the command line `args` runs git or one of its helpers.
fn is_git(args: &str) -> bool {
    let program = args.split_whitespace().next().unwrap_or_default();
    let name = program.rsplit(['/', '\\']).next().unwrap_or_default();
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name == "git" || name.starts_with("git-")
}
