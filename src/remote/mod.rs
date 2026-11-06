//! Repositories on another machine: in a WSL distribution or behind SSH. The window stays
//! here; git, and every read or write of the repository's files, runs there, through shells
//! kept open to that machine (see [`session`]).
//!
//! Such a repository is still a path, so the rest of the app needs no other notion of it:
//! - WSL: the folder as Windows sees it, `\\wsl.localhost\<distro>\home\me\repo` (`\\wsl$\…`
//!   too), which is also what the folder picker gives for one.
//! - SSH: `ssh://<host>/home/me/repo`, `<host>` as `ssh` takes it: an alias of `~/.ssh/config`,
//!   `user@host`, `user@host:port`. `ssh://<host>/~/repo` is under the remote home.

mod discover;
mod fs;
mod session;

use std::path::{Path, PathBuf};

pub use discover::{ssh_config_hosts, wsl_distros};
pub use fs::{
    create_dir_all, host_temp_file, is_dir, is_file, is_link, read_to_string, same_dir,
    worktree_file, write,
};
pub use session::{Output, Request, exec, spawn};

/// The machine a repository is on, when it isn't this one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Host {
    /// A WSL distribution, by its name (`Ubuntu`).
    Wsl(String),
    /// What `ssh` connects to: an alias, `user@host`, `user@host:port`.
    Ssh(String),
}

impl Host {
    /// "WSL · Ubuntu", "SSH · build-server": where a repository is, for a label.
    pub fn label(&self) -> String {
        match self {
            Host::Wsl(distro) => format!("WSL · {distro}"),
            Host::Ssh(host) => format!("SSH · {host}"),
        }
    }
}

/// A folder (or file) on another machine: its host, and its path there (`/home/me/repo`, or
/// `~/repo` under the home folder), with `/` between names and no `/` at the end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub host: Host,
    pub path: String,
}

/// Where `path` is when it names a folder of another machine (see the module's doc), else
/// `None`: a path of this machine.
pub fn parse(path: &Path) -> Option<Target> {
    let s = path.to_string_lossy().replace('\\', "/");
    if let Some(rest) = strip_prefix_ignore_case(&s, "ssh:") {
        // `ssh://host/x`, or `ssh:/host/x` when something folded the double slash.
        let rest = rest
            .strip_prefix("//")
            .or_else(|| rest.strip_prefix('/'))
            .unwrap_or(rest);
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        if !valid_ssh_host(host) {
            return None;
        }
        let path = if path == "~" || path.starts_with("~/") {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        return Some(Target {
            host: Host::Ssh(host.to_owned()),
            path: normalize(&path),
        });
    }
    wsl_target(&s)
}

/// Whether `path` is a folder of another machine.
pub fn is_remote(path: &Path) -> bool {
    parse(path).is_some()
}

/// `path` as the user reads it: the machine and the path there for one elsewhere
/// ("SSH · build  /home/me/repo"), else as it is.
pub fn display(path: &Path) -> String {
    match parse(path) {
        Some(t) => format!("{}  {}", t.host.label(), t.path),
        None => path.display().to_string(),
    }
}

/// Whether programs of this machine (an editor, the file manager) can open `path`: a file in
/// WSL, through its `\\wsl.localhost` path, can; one over SSH can't, and handing `ssh://…` to
/// the system would start whatever claims that kind of link.
pub fn openable_here(path: &Path) -> Result<(), String> {
    // Anything that starts so, also with a host that isn't valid (and so isn't parsed as one).
    let s = path.to_string_lossy();
    if strip_prefix_ignore_case(&s, "ssh:").is_none() {
        return Ok(());
    }
    let place = parse(path).map_or_else(|| "another machine".to_owned(), |t| t.host.label());
    Err(format!(
        "{} is on {place}: programs of this machine can't open it",
        path.display()
    ))
}

/// The host as `ssh` is given it: `user@host:port` only reads as one in the URL form.
pub fn ssh_destination(host: &str) -> String {
    if host.contains(':') {
        format!("ssh://{host}")
    } else {
        host.to_owned()
    }
}

/// What a terminal of this machine runs to give a shell on `t`'s machine, in `t`'s folder:
/// (program, arguments, label). Over SSH, on a terminal of its own (`-t`), where it can ask
/// for a passphrase or a password if it has to.
pub fn terminal_command(t: &Target) -> (String, Vec<String>, String) {
    match &t.host {
        Host::Wsl(distro) => (
            "wsl.exe".to_owned(),
            vec![
                "-d".to_owned(),
                distro.clone(),
                "--cd".to_owned(),
                t.path.clone(),
            ],
            distro.clone(),
        ),
        Host::Ssh(dest) => {
            let dest = ssh_destination(dest);
            let remote = format!("cd -- {} && exec \"${{SHELL:-sh}}\" -l", sh_path(&t.path));
            (
                "ssh".to_owned(),
                vec!["-t".to_owned(), "--".to_owned(), dest, remote],
                t.host.label(),
            )
        }
    }
}

/// The path this app gives `path` (absolute there, or under `~`) on `host`.
pub fn encode(host: &Host, path: &str) -> PathBuf {
    let path = normalize(path);
    match host {
        Host::Ssh(host) if path.starts_with('~') => PathBuf::from(format!("ssh://{host}/{path}")),
        Host::Ssh(host) => PathBuf::from(format!("ssh://{host}{path}")),
        Host::Wsl(distro) => {
            let inside = path.trim_start_matches('/').replace('/', "\\");
            PathBuf::from(format!(r"\\wsl.localhost\{distro}\{inside}"))
        }
    }
}

/// A path git printed for the repository at `repo`: relative to it, or absolute on its machine
/// (`--git-path` in a linked worktree, `--show-toplevel`, `worktree list`).
pub fn resolve(repo: &Path, printed: &str) -> PathBuf {
    match parse(repo) {
        Some(t) if printed.starts_with('/') => encode(&t.host, printed),
        _ => repo.join(printed),
    }
}

/// `s` quoted for `sh`: one word, whatever is in it.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A path for `sh`, `~` (which quoting would keep from being expanded) as the home folder.
pub fn sh_path(path: &str) -> String {
    match path.strip_prefix('~') {
        Some("") => "\"$HOME\"".to_owned(),
        Some(rest) if rest.starts_with('/') => format!("\"$HOME\"{}", sh_quote(rest)),
        _ => sh_quote(path),
    }
}

/// `/a//b/` → `/a/b`; `~/x/` → `~/x`. `..` is left alone: on the other machine it may go
/// through a link.
fn normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// What `ssh` can be given as the host: `[user@]host[:port]`, the names of letters, digits,
/// `.`, `-` and `_` and starting with a letter or digit, or an IPv6 address in brackets. Never
/// something it would read as an option (`-oProxyCommand=…` in a saved tab), nor anything a
/// shell would make more of: the host goes as it is into the user's `ProxyCommand` (`%h`).
pub fn valid_ssh_host(host: &str) -> bool {
    let (user, rest) = match host.rsplit_once('@') {
        Some((user, rest)) => (Some(user), rest),
        None => (None, host),
    };
    let (address, port) = match rest.strip_prefix('[') {
        Some(inside) => {
            let Some((v6, after)) = inside.split_once(']') else {
                return false;
            };
            let port = match after {
                "" => None,
                _ => match after.strip_prefix(':') {
                    Some(port) => Some(port),
                    None => return false,
                },
            };
            if v6.is_empty() || !v6.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
                return false;
            }
            (None, port)
        }
        None => match rest.split_once(':') {
            Some((address, port)) => (Some(address), Some(port)),
            None => (Some(rest), None),
        },
    };
    user.is_none_or(ssh_name)
        && address.is_none_or(ssh_name)
        && port.is_none_or(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// A user or host name for [`valid_ssh_host`].
fn ssh_name(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// A WSL distribution's name as `wsl.exe -d` takes it.
pub fn valid_distro(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// `//wsl.localhost/Ubuntu/home/me` (slashes already turned) → Ubuntu, `/home/me`. Only on
/// Windows: elsewhere there is no WSL, and such a path is a share like any other.
fn wsl_target(s: &str) -> Option<Target> {
    if !cfg!(windows) {
        return None;
    }
    let rest = [
        "//?/unc/wsl.localhost/",
        "//?/unc/wsl$/",
        "//wsl.localhost/",
        "//wsl$/",
    ]
    .iter()
    .find_map(|prefix| strip_prefix_ignore_case(s, prefix))?;
    let (distro, path) = rest.split_once('/').unwrap_or((rest, ""));
    valid_distro(distro).then(|| Target {
        host: Host::Wsl(distro.to_owned()),
        path: normalize(&format!("/{path}")),
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_tests;
