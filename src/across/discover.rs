//! Finding the repositories on a machine gitcraft hasn't opened yet: here, in a WSL
//! distribution, or on an SSH host of `~/.ssh/config`. The home folder is searched a few levels
//! deep (hidden folders and `node_modules` skipped), in one round trip for another machine.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::remote::{self, Host};

/// How deep under the home folder repositories are looked for.
pub const DEPTH: usize = 4;

/// The most repositories one search gives.
pub const MAX_FOUND: usize = 300;

/// How long a search of another machine may take.
const SEARCH_FOR: Duration = Duration::from_secs(60);

/// A machine whose repositories can be looked for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Machine {
    Here,
    There(Host),
}

impl Machine {
    pub fn label(&self) -> String {
        match self {
            Machine::Here => "This computer".to_owned(),
            Machine::There(host) => host.label(),
        }
    }
}

/// This computer, the WSL distributions installed and the hosts of `~/.ssh/config`. Asks
/// `wsl.exe` on Windows: not on the window's thread.
pub fn machines() -> Vec<Machine> {
    let mut all = vec![Machine::Here];
    all.extend(
        remote::wsl_distros()
            .into_iter()
            .map(|d| Machine::There(Host::Wsl(d))),
    );
    all.extend(
        remote::ssh_config_hosts()
            .into_iter()
            .filter(|h| remote::valid_ssh_host(h))
            .map(|h| Machine::There(Host::Ssh(h))),
    );
    all
}

/// The repositories under the home folder of `machine`, in the form the app gives paths.
pub fn find_repos(machine: &Machine) -> Result<Vec<PathBuf>, String> {
    match machine {
        Machine::Here => {
            let home = home_dir().ok_or("couldn't tell where your home folder is")?;
            Ok(find_under(&home, DEPTH))
        }
        Machine::There(host) => find_there(host),
    }
}

fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|v| std::env::var_os(v).filter(|h| !h.is_empty()))
        .map(PathBuf::from)
}

/// Whether a folder met on the way down is skipped: hidden ones (caches, `.cargo`…) and
/// dependency folders, which hold copies of other people's repositories.
fn skipped(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules"
}

/// The repositories under `root` here, at most `depth` folders down: a folder with a `.git`
/// (folder, or file for a worktree) is one, and isn't looked into further.
pub fn find_under(root: &Path, depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut todo = vec![(root.to_owned(), 0)];
    while let Some((dir, level)) = todo.pop() {
        if found.len() >= MAX_FOUND {
            break;
        }
        if level > 0 && dir.join(".git").exists() {
            found.push(dir);
            continue;
        }
        if level >= depth {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut subdirs: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| !skipped(&e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .collect();
        subdirs.sort();
        todo.extend(subdirs.into_iter().rev().map(|d| (d, level + 1)));
    }
    found.sort();
    found
}

/// The script that lists, from the home folder, the folders holding a `.git` (see
/// [`find_under`]): `$HOME` on the first line, then each one relative to it.
fn script() -> String {
    format!(
        concat!(
            "cd \"$HOME\" 2>/dev/null || {{ echo 'gitcraft: no home folder' >&2; exit 1; }}\n",
            "printf '%s\\n' \"$HOME\"\n",
            "find . -maxdepth {depth} \\( -name .git -print -prune \\) -o ",
            "\\( -type d \\( -name '.?*' -o -name node_modules \\) -prune \\) 2>/dev/null ",
            "| head -n {max}\n",
            "exit 0\n",
        ),
        depth = DEPTH + 1,
        max = MAX_FOUND,
    )
}

/// The repositories [`script`]'s output names, as paths of `host`.
fn parse_found(out: &str, host: &Host) -> Vec<PathBuf> {
    let mut lines = out.lines();
    let Some(home) = lines.next().map(|h| h.trim_end_matches('/')) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = lines
        .filter_map(|line| line.strip_prefix("./")?.strip_suffix("/.git"))
        .map(|rel| remote::encode(host, &format!("{home}/{rel}")))
        .collect();
    found.sort();
    found.dedup();
    without_nested(found)
}

/// `sorted` less the repositories inside another of them (find doesn't stop at a repository
/// the way [`find_under`] does).
fn without_nested(sorted: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in sorted {
        if !kept.iter().any(|outer| path.starts_with(outer)) {
            kept.push(path);
        }
    }
    kept
}

fn find_there(host: &Host) -> Result<Vec<PathBuf>, String> {
    let out = remote::exec(
        host,
        &remote::Request {
            script: script(),
            timeout: Some(SEARCH_FOR),
            ..remote::Request::default()
        },
    )?;
    if out.code != 0 {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(format!("couldn't search {}: {why}", host.label()));
    }
    Ok(parse_found(&String::from_utf8_lossy(&out.stdout), host))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> Result<PathBuf, String> {
        let root =
            std::env::temp_dir().join(format!("gitcraft-discover-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in [
            "code/app/.git",
            "code/app/nested/.git",
            "code/ünï côde/.git",
            "work/client/deep/er/.git",
            "work/too/deep/for/it/.git",
            ".cache/hidden/.git",
            "web/node_modules/dep/.git",
            "notes",
        ] {
            std::fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
        }
        // A worktree's `.git` is a file.
        std::fs::create_dir_all(root.join("code/wt")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("code/wt/.git"), "gitdir: x").map_err(|e| e.to_string())?;
        Ok(root)
    }

    fn relative(root: &Path, found: &[PathBuf]) -> Vec<String> {
        found
            .iter()
            .filter_map(|p| p.strip_prefix(root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    #[test]
    fn finds_repositories_but_not_inside_them_hidden_or_too_deep() -> Result<(), String> {
        let root = tree("here")?;
        let found = find_under(&root, DEPTH);
        assert_eq!(
            relative(&root, &found),
            [
                "code/app",
                "code/wt",
                "code/ünï côde",
                "work/client/deep/er"
            ]
        );
        assert!(find_under(&root.join("missing"), DEPTH).is_empty());
        Ok(())
    }

    #[test]
    fn reads_what_the_script_printed() {
        let out = "/home/me\n./code/app/.git\n./a b/c/.git\n./weird\n./code/app/.git\n./.git\n./code/app/sub/.git\n";
        let host = Host::Ssh("build".into());
        let found = parse_found(out, &host);
        assert_eq!(
            found,
            vec![
                PathBuf::from("ssh://build/home/me/a b/c"),
                PathBuf::from("ssh://build/home/me/code/app"),
            ]
        );
        assert!(parse_found("", &host).is_empty());
        assert!(parse_found("/root/\n", &host).is_empty());
    }

    /// The script another machine is searched with, run here by `sh` with the scratch tree as
    /// home: it finds the same repositories as the search here.
    #[cfg(unix)]
    #[test]
    fn the_script_finds_the_same() -> Result<(), String> {
        let root = tree("script")?;
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(script())
            .env("HOME", &root)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(out.status.success());
        let host = Host::Ssh("h".into());
        let there = parse_found(&String::from_utf8_lossy(&out.stdout), &host);
        let prefix = format!("ssh://h{}", root.to_string_lossy());
        let there: Vec<String> = there
            .iter()
            .filter_map(|p| {
                p.to_string_lossy()
                    .strip_prefix(&prefix)
                    .map(|r| r.trim_start_matches('/').to_owned())
            })
            .collect();
        let mut here = relative(&root, &find_under(&root, DEPTH));
        here.sort();
        let mut there = there;
        there.sort();
        assert_eq!(here, there);
        Ok(())
    }

    #[test]
    fn this_computer_comes_first() {
        assert_eq!(machines().first(), Some(&Machine::Here));
        assert_eq!(Machine::Here.label(), "This computer");
        assert_eq!(
            Machine::There(Host::Wsl("Ubuntu".into())).label(),
            "WSL · Ubuntu"
        );
    }
}
