//! The few file operations a teleport does outside git, on whichever machine they are for:
//! here through `std::fs`, elsewhere through a shell kept open there (as `remote::fs` does for
//! a repository's own files).

use std::io::Write;
use std::path::{Path, PathBuf};

use super::Machine;
use crate::remote::{self, Host, Request, Target, sh_path};

/// Exit code of the scripts below for "not there".
const ABSENT: i32 = 11;

/// Runs `script` on `host`, `stdin` on its input.
fn sh(host: &Host, script: String, stdin: Option<Vec<u8>>) -> Result<remote::Output, String> {
    remote::exec(
        host,
        &Request {
            script,
            stdin,
            ..Request::default()
        },
    )
}

/// Why a script failed, in its own words when it said any.
fn failure(out: &remote::Output) -> String {
    let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    if said.is_empty() {
        format!("exit status {}", out.code)
    } else {
        said
    }
}

/// Copies the file `from` to `to` (both on one machine): `false` when `from` isn't there.
pub(super) fn copy_if_there(from: &Path, to: &Path) -> Result<bool, String> {
    let fail = |e: &dyn std::fmt::Display| format!("could not copy {}: {e}", from.display());
    match (remote::parse(from), remote::parse(to)) {
        (None, None) => match std::fs::copy(from, to) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !from.exists() => Ok(false),
            Err(e) => Err(fail(&e)),
        },
        (Some(f), Some(t)) if f.host == t.host => {
            let (src, dst) = (sh_path(&f.path), sh_path(&t.path));
            let out = sh(
                &f.host,
                format!("[ -e {src} ] || exit {ABSENT}\nexec cp -- {src} {dst}"),
                None,
            )?;
            match out.code {
                0 => Ok(true),
                ABSENT => Ok(false),
                _ => Err(fail(&failure(&out))),
            }
        }
        _ => Err(fail(&"the copy is on another machine")),
    }
}

/// Removes the file at `path`; one that isn't there is no error.
pub(super) fn remove_file(path: &Path) -> Result<(), String> {
    let Some(t) = remote::parse(path) else {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("could not remove {}: {e}", path.display()))
            }
            _ => Ok(()),
        };
    };
    let out = sh(&t.host, format!("exec rm -f -- {}", sh_path(&t.path)), None)?;
    match out.code {
        0 => Ok(()),
        _ => Err(format!(
            "could not remove {} on {}: {}",
            t.path,
            t.host.label(),
            failure(&out)
        )),
    }
}

/// Removes the folder `path` and everything in it: only ever one this teleport made.
pub(super) fn remove_dir_all(path: &Path) -> Result<(), String> {
    let Some(t) = remote::parse(path) else {
        return match std::fs::remove_dir_all(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("could not remove {}: {e}", path.display()))
            }
            _ => Ok(()),
        };
    };
    let out = sh(
        &t.host,
        format!("exec rm -rf -- {}", sh_path(&t.path)),
        None,
    )?;
    match out.code {
        0 => Ok(()),
        _ => Err(format!(
            "could not remove {} on {}: {}",
            t.path,
            t.host.label(),
            failure(&out)
        )),
    }
}

/// Whether anything (a file, a folder, a link) is at `path`.
pub(super) fn exists(path: &Path) -> Result<bool, String> {
    let Some(t) = remote::parse(path) else {
        return Ok(path.symlink_metadata().is_ok());
    };
    let p = sh_path(&t.path);
    let out = sh(
        &t.host,
        format!("[ -e {p} ] || [ -L {p} ] || exit {ABSENT}"),
        None,
    )?;
    match out.code {
        0 => Ok(true),
        ABSENT => Ok(false),
        _ => Err(format!(
            "could not look at {} on {}: {}",
            t.path,
            t.host.label(),
            failure(&out)
        )),
    }
}

/// A file written in pieces on a machine (the bundle): removed when dropped.
pub(super) struct Upload {
    /// Where it is, as this app names it.
    path: PathBuf,
    /// Where it is as git there names it.
    there: String,
    /// Removed already: nothing left to do when dropped.
    removed: bool,
}

impl Upload {
    /// A new file on `machine` holding `first`, named after `id`, that only this user can read.
    pub(super) fn start(machine: &Machine, id: &str, first: &[u8]) -> Result<Self, String> {
        match machine {
            Machine::There(host) => {
                let anywhere = Target {
                    host: host.clone(),
                    path: "~".to_owned(),
                };
                let there = remote::host_temp_file(&anywhere, &format!("{id}.bundle"), first)?;
                Ok(Self {
                    path: remote::encode(host, &there),
                    there,
                    removed: false,
                })
            }
            Machine::Here => {
                let path = std::env::temp_dir().join(format!("gitcraft-teleport-{id}.bundle"));
                let mut file = private_new_file(&path)
                    .map_err(|e| format!("could not make {}: {e}", path.display()))?;
                let upload = Self {
                    there: path.to_string_lossy().into_owned(),
                    path,
                    removed: false,
                };
                file.write_all(first)
                    .and_then(|()| file.flush())
                    .map_err(|e| format!("could not write {}: {e}", upload.there))?;
                Ok(upload)
            }
        }
    }

    /// Adds `chunk` at the end.
    pub(super) fn append(&mut self, chunk: &[u8]) -> Result<(), String> {
        let Some(t) = remote::parse(&self.path) else {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&self.path)
                .map_err(|e| format!("could not open {}: {e}", self.there))?;
            return file
                .write_all(chunk)
                .and_then(|()| file.flush())
                .map_err(|e| format!("could not write {}: {e}", self.there));
        };
        // As long as a network operation may take: a piece over a slow link takes a while.
        let out = remote::exec(
            &t.host,
            &Request {
                script: format!("exec cat >> {}", sh_path(&t.path)),
                stdin: Some(chunk.to_vec()),
                long: true,
                ..Request::default()
            },
        )?;
        match out.code {
            0 => Ok(()),
            _ => Err(format!(
                "could not write {} on {}: {}",
                t.path,
                t.host.label(),
                failure(&out)
            )),
        }
    }

    /// Its size as it is on its machine.
    pub(super) fn size(&self) -> Result<u64, String> {
        let Some(t) = remote::parse(&self.path) else {
            return std::fs::metadata(&self.path)
                .map(|m| m.len())
                .map_err(|e| format!("could not read {}: {e}", self.there));
        };
        let out = sh(&t.host, format!("exec wc -c < {}", sh_path(&t.path)), None)?;
        let text = String::from_utf8_lossy(&out.stdout);
        match (out.code, text.trim().parse()) {
            (0, Ok(n)) => Ok(n),
            _ => Err(format!(
                "could not read {} on {}: {}",
                t.path,
                t.host.label(),
                failure(&out)
            )),
        }
    }

    /// Its path as git on its machine is given it.
    pub(super) fn there(&self) -> &str {
        &self.there
    }

    /// Removes it now, saying why when that fails.
    pub(super) fn remove(mut self) -> Result<(), String> {
        self.removed = true;
        remove_file(&self.path)
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        if !self.removed {
            let _ = remove_file(&self.path);
        }
    }
}

/// A new file at `path` (never one that is there, nor through a link) only this user can read.
fn private_new_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// The home folder of `host`, as a path there.
pub(super) fn home(host: &Host) -> Result<String, String> {
    let out = sh(host, "printf '%s' \"$HOME\"".to_owned(), None)?;
    let home = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.code != 0 || !home.starts_with('/') {
        return Err(format!(
            "could not find the home folder on {}: {}",
            host.label(),
            failure(&out)
        ));
    }
    Ok(home.trim_end_matches('/').to_owned())
}

/// `path` with a `~` at its start (a path typed for another machine) made that machine's home
/// folder: git there takes `~` for a name.
pub(super) fn expand_home(path: &Path) -> Result<PathBuf, String> {
    let Some(t) = remote::parse(path) else {
        return Ok(path.to_path_buf());
    };
    let Some(rest) = t.path.strip_prefix('~') else {
        return Ok(path.to_path_buf());
    };
    Ok(remote::encode(
        &t.host,
        &format!("{}{rest}", home(&t.host)?),
    ))
}
