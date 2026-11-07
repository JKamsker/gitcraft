//! The few file operations the app does on a repository's own files, on whichever machine
//! the repository is: here through `std::fs`, elsewhere through a shell there.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{Request, Target, exec, parse, sh_path, sh_quote};

/// Exit codes the scripts below answer with, besides 0.
const NOT_FOUND: i32 = 11;
const NOT_A_FILE: i32 = 12;
const IS_LINK: i32 = 10;

fn run(t: &Target, script: String, stdin: Option<Vec<u8>>) -> io::Result<super::Output> {
    exec(
        &t.host,
        &Request {
            script,
            stdin,
            ..Request::default()
        },
    )
    .map_err(io::Error::other)
}

fn failed(out: &super::Output) -> io::Error {
    match out.code {
        NOT_FOUND => io::Error::from(io::ErrorKind::NotFound),
        NOT_A_FILE => io::Error::other("not a file"),
        _ => {
            let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            io::Error::other(if said.is_empty() {
                format!("exit status {}", out.code)
            } else {
                said
            })
        }
    }
}

/// Reads the file at `path` (following links).
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    let Some(t) = parse(path) else {
        return std::fs::read(path);
    };
    let f = sh_path(&t.path);
    let out = run(
        &t,
        format!("[ -e {f} ] || exit {NOT_FOUND}\n[ -f {f} ] || exit {NOT_A_FILE}\nexec cat -- {f}"),
        None,
    )?;
    match out.code {
        0 => Ok(out.stdout),
        _ => Err(failed(&out)),
    }
}

pub fn read_to_string(path: &Path) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Writes `bytes` to the file at `path`, created or replaced.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let Some(t) = parse(path) else {
        return std::fs::write(path, bytes);
    };
    // Never through a symbolic link, which a repository can point anywhere (`~/.bashrc`):
    // checked and written in one go, there.
    let f = sh_path(&t.path);
    let out = run(
        &t,
        format!("if [ -L {f} ]; then echo 'it is a symbolic link' >&2; exit 1; fi\nexec cat > {f}"),
        Some(bytes.to_vec()),
    )?;
    match out.code {
        0 => Ok(()),
        _ => Err(failed(&out)),
    }
}

pub fn create_dir_all(path: &Path) -> io::Result<()> {
    let Some(t) = parse(path) else {
        return std::fs::create_dir_all(path);
    };
    let out = run(&t, format!("exec mkdir -p -- {}", sh_path(&t.path)), None)?;
    match out.code {
        0 => Ok(()),
        _ => Err(failed(&out)),
    }
}

/// Copies the file `from` to `to` (both on the same machine), keeping its modification time:
/// a copy of git's index must stay as old as the index, or git would take files changed in the
/// same instant as it for unchanged.
pub fn copy_keeping_time(from: &Path, to: &Path) -> io::Result<()> {
    match (parse(from), parse(to)) {
        (None, None) => {
            let modified = std::fs::metadata(from)?.modified()?;
            std::fs::copy(from, to)?;
            std::fs::File::options()
                .write(true)
                .open(to)?
                .set_modified(modified)
        }
        (Some(a), Some(b)) if a.host == b.host => {
            let script = format!("exec cp -p -- {} {}", sh_path(&a.path), sh_path(&b.path));
            let out = run(&a, script, None)?;
            match out.code {
                0 => Ok(()),
                _ => Err(failed(&out)),
            }
        }
        _ => Err(io::Error::other("can't copy a file between two machines")),
    }
}

/// Removes the file at `path`; one that isn't there is no error.
pub fn remove_file(path: &Path) -> io::Result<()> {
    let Some(t) = parse(path) else {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    };
    let out = run(&t, format!("exec rm -f -- {}", sh_path(&t.path)), None)?;
    match out.code {
        0 => Ok(()),
        _ => Err(failed(&out)),
    }
}

/// `test <flag> path`: whether it holds, or why it couldn't be asked (the machine wasn't
/// reached), which a plain `false` would pass off as an answer.
fn test(path: &Path, flag: &str) -> io::Result<bool> {
    match parse(path) {
        Some(t) => {
            // 0: it holds; 1: it doesn't; anything else (or no answer): not known.
            let out = run(&t, format!("[ {flag} {} ]", sh_path(&t.path)), None)?;
            match out.code {
                0 => Ok(true),
                1 => Ok(false),
                _ => Err(failed(&out)),
            }
        }
        None => Ok(match flag {
            "-d" => path.is_dir(),
            "-L" => std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()),
            _ => path.is_file(),
        }),
    }
}

/// Whether `path` is a file (or a link to one); `false` also when that couldn't be asked.
/// Over SSH, a round trip: not for every frame.
pub fn is_file(path: &Path) -> bool {
    test(path, "-f").unwrap_or(false)
}

/// [`is_file`], or why it couldn't be asked.
pub fn try_is_file(path: &Path) -> io::Result<bool> {
    test(path, "-f")
}

/// Whether `path` is a folder (or a link to one), or why that couldn't be asked. Over SSH, a
/// round trip: not for every frame.
pub fn try_is_dir(path: &Path) -> io::Result<bool> {
    test(path, "-d")
}

/// Whether `path` is a symbolic link itself (not what it points to). When that can't be
/// asked, it counts as one: callers refuse to write through a link.
pub fn is_link(path: &Path) -> bool {
    test(path, "-L").unwrap_or(true)
}

/// Whether `path` is a symbolic link itself, or why that couldn't be asked: for a caller that
/// says which it was.
pub fn try_is_link(path: &Path) -> io::Result<bool> {
    test(path, "-L")
}

/// The file `rel` (a path in the repository at `repo`, `/` between names) as it is on that
/// machine, when it is a plain file of the repository there: not a link, not reached through a
/// folder that is a link, and at most `max` bytes; `Ok(None)` when it isn't. Checked and read
/// in one script, and never more than `max + 1` bytes read: a file that grew is refused, not
/// sent. `None` for a repository here. One round trip, so not on the window's thread.
pub fn read_within(repo: &Path, rel: &str, max: u64) -> Option<io::Result<Option<Vec<u8>>>> {
    let t = parse(repo)?;
    let Some(script) = read_within_script(&t.path, rel, max) else {
        return Some(Ok(None));
    };
    Some(
        run(&t, script, None)
            .map(|out| (out.code == 0 && out.stdout.len() as u64 <= max).then_some(out.stdout)),
    )
}

/// The script [`read_within`] runs in `repo` (a path there): the file on its output, exit code
/// 0, when `rel` is such a file. `None` for an empty path.
pub(crate) fn read_within_script(repo: &str, rel: &str, max: u64) -> Option<String> {
    let names: Vec<&str> = rel.split('/').filter(|n| !n.is_empty()).collect();
    if names.is_empty() || names.iter().any(|n| matches!(*n, "." | "..")) {
        return None;
    }
    let (last, folders) = names.split_last()?;
    // Into each folder in turn, by where it really is (`-P`), and then checked to be where it
    // should be: a folder swapped for a link meanwhile shows in `pwd -P`. The shell's folder,
    // once in it, stays that folder whatever is renamed.
    let mut script = format!(
        concat!(
            "cd -P -- {} || exit 1\n",
            "root=$(pwd -P) || exit 1\n",
            "[ \"$root\" = / ] && root=\n",
        ),
        sh_path(repo)
    );
    let mut at = String::new();
    for name in folders {
        at.push('/');
        at.push_str(name);
        let q = sh_quote(name);
        script.push_str(&format!(
            concat!(
                "[ -L {q} ] && exit 1\n",
                "cd -P -- {q} || exit 1\n",
                "[ \"$(pwd -P)\" = \"$root\"{at} ] || exit 1\n",
            ),
            q = q,
            at = sh_quote(&at),
        ));
    }
    // The file: opened once, then what was opened is checked where the system says (Linux,
    // which WSL and most SSH hosts are), else by its name.
    script.push_str(&format!(
        concat!(
            "[ -L {f} ] && exit 1\n",
            "[ -f {f} ] || exit 1\n",
            "exec 3< {f} || exit 1\n",
            "if [ -e /proc/self/fd/3 ]; then\n",
            "  [ \"$(readlink /proc/self/fd/3)\" = \"$(pwd -P)\"/{f} ] || exit 1\n",
            "else\n",
            "  [ -L {f} ] && exit 1\n",
            "fi\n",
            "exec head -c {more} <&3\n"
        ),
        f = sh_quote(last),
        more = max + 1,
    ));
    Some(script)
}

/// A file of a working tree as git sees it: a symbolic link is the path it holds, not what it
/// points to; anything else but a file is refused (see `git::worktree_file`).
pub fn worktree_file(file: &Path) -> Option<io::Result<Vec<u8>>> {
    let t = parse(file)?;
    let f = sh_path(&t.path);
    let script = format!(
        concat!(
            "if [ -L {f} ]; then readlink -- {f} && exit {link}; exit 1; fi\n",
            "[ -e {f} ] || exit {missing}\n",
            "[ -f {f} ] || exit {other}\n",
            "exec cat -- {f}"
        ),
        f = f,
        link = IS_LINK,
        missing = NOT_FOUND,
        other = NOT_A_FILE,
    );
    Some(run(&t, script, None).and_then(|out| match out.code {
        0 => Ok(out.stdout),
        IS_LINK => {
            let mut target = out.stdout;
            if target.last() == Some(&b'\n') {
                target.pop();
            }
            Ok(target)
        }
        _ => Err(failed(&out)),
    }))
}

/// Whether `a` and `b` are the same folder: by what they resolve to here, as they are written
/// elsewhere (resolving them there would be a round trip each).
pub fn same_dir(a: &Path, b: &Path) -> bool {
    match (parse(a), parse(b)) {
        (None, None) => match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => a == b,
        },
        (a, b) => a == b,
    }
}

/// Tells which of many folders is `here`, resolving `here` once: a folder here that can't be
/// resolved (it's gone) is none of them.
pub fn is_same_dir_as(here: &Path) -> impl Fn(&Path) -> bool + use<> {
    let remote = parse(here);
    let resolved = remote
        .is_none()
        .then(|| std::fs::canonicalize(here).ok())
        .flatten();
    move |other: &Path| match (&remote, parse(other)) {
        (None, None) => resolved
            .as_ref()
            .is_some_and(|r| std::fs::canonicalize(other).ok().as_ref() == Some(r)),
        (remote, other) => remote.as_ref() == other.as_ref(),
    }
}

/// Writes `bytes` to a new file in this user's private folder of `t`'s machine, for git there
/// to read (a patch for `git apply`, a rebase's todo list), and returns its path there.
/// `suffix` ends its name. Files of earlier runs older than a day are cleared on the way; this
/// run's stay, however old: undoing a discard of lines applies its patch again.
pub fn host_temp_file(t: &Target, suffix: &str, bytes: &[u8]) -> Result<String, String> {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let ours = format!("{}-", std::process::id());
    let name = format!(
        "{ours}{}-{}-{}",
        crate::git::now(),
        SEQ.fetch_add(1, Ordering::Relaxed),
        suffix
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            .collect::<String>()
    );
    // A folder only this user can enter, never one someone else made (or a link to one): what
    // is in it is run (`exec` lines of a todo list) or applied.
    let script = format!(
        concat!(
            "umask 077\n",
            "d=\"${{TMPDIR:-/tmp}}/gitcraft-$(id -u)\"\n",
            "mkdir -p -m 700 \"$d\" || exit 1\n",
            "if [ -L \"$d\" ] || [ ! -O \"$d\" ]; then echo \"$d isn't gitcraft's\" >&2; exit 1; fi\n",
            // One made before with other rights (by hand, another umask) is closed again.
            "chmod 700 \"$d\" || exit 1\n",
            "find \"$d\" -mindepth 1 -maxdepth 1 -mtime +0 ! -name {ours} -exec rm -rf {{}} + 2>/dev/null\n",
            // The folders of shells that ended without clearing theirs (a dropped connection);
            // a shell still open makes its own again if it needs it.
            "find \"${{TMPDIR:-/tmp}}\" -maxdepth 1 -name 'gitcraft-sh.*' -user \"$(id -u)\" -mtime +7 -exec rm -rf {{}} + 2>/dev/null\n",
            "f=\"$d\"/{name}\n",
            "set -C\n",
            "cat > \"$f\" || exit 1\n",
            "printf '%s' \"$f\"",
        ),
        ours = sh_quote(&format!("{ours}*")),
        name = sh_quote(&name),
    );
    let out = run(t, script, Some(bytes.to_vec())).map_err(|e| e.to_string())?;
    if out.code == 0 {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "could not write a file on {}: {}",
            t.host.label(),
            failed(&out)
        ))
    }
}

/// The script that makes (or, with `remove`, removes) a scratch folder in this user's private
/// folder of the machine (see [`host_temp_file`]). Removing refuses anything but a folder of
/// that private folder: a path it is given is never trusted to be one.
fn scratch_script(remove: Option<&str>) -> String {
    let private = concat!(
        "umask 077\n",
        "d=\"${TMPDIR:-/tmp}/gitcraft-$(id -u)\"\n",
        "mkdir -p -m 700 \"$d\" || exit 1\n",
        "if [ -L \"$d\" ] || [ ! -O \"$d\" ]; then echo \"$d isn't gitcraft's\" >&2; exit 1; fi\n",
    );
    match remove {
        // Those a run that ended mid-way left (a dropped connection) go after a day.
        None => format!(
            concat!(
                "{private}",
                "find \"$d\" -mindepth 1 -maxdepth 1 -name 'scratch.*' -mtime +0 -exec rm -rf {{}} + 2>/dev/null\n",
                "mktemp -d \"$d/scratch.XXXXXXXX\"",
            ),
            private = private,
        ),
        Some(path) => format!(
            concat!(
                "{private}p={path}\n",
                "case \"$p\" in \"$d\"/scratch.*) ;; *) echo \"$p isn't a scratch folder\" >&2; exit 1 ;; esac\n",
                "case \"$p\" in */../*|*/..) exit 1 ;; esac\n",
                "rm -rf -- \"$p\"",
            ),
            private = private,
            path = sh_quote(path),
        ),
    }
}

/// Makes a new empty folder in this user's private folder of `t`'s machine, for work done there
/// apart from the repository (a scratch clone), and returns its path there. Remove it with
/// [`remove_host_scratch_dir`].
pub fn host_scratch_dir(t: &Target) -> Result<String, String> {
    let out = run(t, scratch_script(None), None).map_err(|e| e.to_string())?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.code == 0 && !path.is_empty() {
        Ok(path)
    } else {
        Err(format!(
            "could not make a scratch folder on {}: {}",
            t.host.label(),
            failed(&out)
        ))
    }
}

/// Removes a folder [`host_scratch_dir`] made on `t`'s machine, and nothing else.
pub fn remove_host_scratch_dir(t: &Target, path: &str) -> Result<(), String> {
    let out = run(t, scratch_script(Some(path)), None).map_err(|e| e.to_string())?;
    if out.code == 0 {
        Ok(())
    } else {
        Err(failed(&out).to_string())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The scratch folder script, run here by `sh`: it makes a private folder, removes it, and
    /// refuses to remove anything outside gitcraft's own folder.
    #[test]
    fn a_scratch_folder_is_made_and_only_it_is_removed() -> Result<(), String> {
        let sh = |script: String| {
            std::process::Command::new("sh")
                .args(["-c", &script])
                .output()
                .map_err(|e| e.to_string())
        };
        let made = sh(scratch_script(None))?;
        let path = String::from_utf8_lossy(&made.stdout).trim().to_owned();
        assert!(made.status.success() && Path::new(&path).is_dir(), "{path}");
        let outside = std::env::temp_dir().join(format!("gitcraft-keep-{}", std::process::id()));
        std::fs::create_dir_all(&outside).map_err(|e| e.to_string())?;
        let refused = sh(scratch_script(Some(&outside.display().to_string())))?;
        let escaped = sh(scratch_script(Some(&format!("{path}/../.."))))?;
        let removed = sh(scratch_script(Some(&path)))?;
        let kept = outside.is_dir();
        let _ = std::fs::remove_dir_all(&outside);
        assert!(!refused.status.success() && !escaped.status.success() && kept);
        assert!(removed.status.success() && !Path::new(&path).exists());
        Ok(())
    }

    /// The script run on another machine, run here by `sh` the same way: a linked folder, a
    /// missing file, a file too large and a link are refused; a plain file is read.
    #[test]
    fn a_file_there_is_read_only_when_plain_inside_and_small() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-within-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("docs/s.png"), "picture").map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink("/", dir.join("docs/l")).map_err(|e| e.to_string())?;
        let repo = dir.display().to_string();
        std::os::unix::fs::symlink("s.png", dir.join("docs/link.png"))
            .map_err(|e| e.to_string())?;
        let read = |rel: &str, max: u64| {
            let script = read_within_script(&repo, rel, max).unwrap_or_default();
            std::process::Command::new("sh")
                .args(["-c", &script])
                .output()
                .ok()
                .filter(|out| out.status.success() && out.stdout.len() as u64 <= max)
                .map(|out| out.stdout)
        };
        let found = (
            read("docs/s.png", 100),
            read("docs/l/etc/hostname", 1 << 20),
            read("docs/none.png", 100),
            read("docs/s.png", 3),
            read("docs/link.png", 100),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found, (Some(b"picture".to_vec()), None, None, None, None));
        assert!(read_within_script(&repo, "", 1).is_none());
        assert!(read_within_script(&repo, "docs/../x", 1).is_none());
        Ok(())
    }

    /// A folder swapped for a link while the script reads: what is outside is never read,
    /// whichever moment the swap falls on.
    #[test]
    fn a_folder_swapped_for_a_link_meanwhile_is_never_read_there() -> Result<(), String> {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let base =
            std::env::temp_dir().join(format!("gitcraft-within-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (dir, outside) = (base.join("repo"), base.join("outside"));
        for d in [dir.join("docs"), outside.join("docs")] {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(dir.join("docs/s.png"), "picture").map_err(|e| e.to_string())?;
        std::fs::write(outside.join("docs/s.png"), "secret").map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let swaps = Arc::new(AtomicUsize::new(0));
        let swapper = {
            let (dir, outside) = (dir.clone(), outside.clone());
            let (stop, swaps) = (stop.clone(), swaps.clone());
            std::thread::spawn(move || {
                let (real, moved) = (dir.join("docs"), dir.join("docs-real"));
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::fs::rename(&real, &moved);
                    let _ = std::os::unix::fs::symlink(outside.join("docs"), &real);
                    let _ = std::fs::remove_file(&real);
                    let _ = std::fs::rename(&moved, &real);
                    swaps.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        let script =
            read_within_script(&dir.display().to_string(), "docs/s.png", 100).unwrap_or_default();
        let (mut leaked, mut reads) = (0, 0);
        while reads < 300 || swaps.load(Ordering::Relaxed) < 300 {
            let out = std::process::Command::new("sh")
                .args(["-c", &script])
                .output();
            if out.is_ok_and(|o| o.status.success() && o.stdout == b"secret") {
                leaked += 1;
            }
            reads += 1;
        }
        stop.store(true, Ordering::Relaxed);
        let _ = swapper.join();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(leaked, 0);
        Ok(())
    }
}
