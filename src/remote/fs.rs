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
