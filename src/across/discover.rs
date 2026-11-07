//! Finding the repositories on a machine gitcraft hasn't opened yet: here, in a WSL
//! distribution, or on an SSH host of `~/.ssh/config`. The home folder is searched a few levels
//! deep (hidden folders, `node_modules`, `AppData` and `Library` skipped, and not inside a
//! repository), in one round trip for another machine.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::remote::{self, Host};

/// How deep under the home folder repositories are looked for.
pub const DEPTH: usize = 4;

/// The most repositories one search gives.
pub const MAX_FOUND: usize = 300;

/// How long a search of another machine may take.
const SEARCH_FOR: Duration = Duration::from_secs(60);

/// Hosts of `~/.ssh/config` that are forges, where nobody's home folder holds their work.
const FORGES: [&str; 6] = [
    "github.com",
    "gitlab.com",
    "codeberg.org",
    "bitbucket.org",
    "ssh.github.com",
    "altssh.gitlab.com",
];

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

    /// The machine `repo` (in the form the app gives paths) is on.
    pub fn of(repo: &Path) -> Machine {
        remote::parse(repo).map_or(Machine::Here, |t| Machine::There(t.host))
    }
}

/// What a search found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// The repositories, sorted.
    pub repos: Vec<PathBuf>,
    /// Whether the search stopped at [`MAX_FOUND`], with more left unsearched.
    pub cut: bool,
}

/// This computer, the WSL distributions installed and the hosts of `~/.ssh/config` (less the
/// forges). Asks `wsl.exe` on Windows: not on the window's thread.
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
            .filter(|h| remote::valid_ssh_host(h) && !is_forge(h))
            .map(|h| Machine::There(Host::Ssh(h))),
    );
    all
}

/// Whether `host` (`git@github.com`, `gitlab.com:22`) is one of the [`FORGES`].
fn is_forge(host: &str) -> bool {
    let name = host.rsplit('@').next().unwrap_or(host);
    let name = name.split(':').next().unwrap_or(name);
    FORGES.iter().any(|f| f.eq_ignore_ascii_case(name))
}

/// The repositories under the home folder of `machine`, in the form the app gives paths.
pub fn find_repos(machine: &Machine) -> Result<Found, String> {
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

/// Whether a folder met on the way down is skipped: hidden ones (caches, `.cargo`…),
/// dependency folders, which hold copies of other people's repositories, and the system's own
/// folders of a home (Windows' `AppData`, macOS' `Library`).
fn skipped(name: &str) -> bool {
    name.starts_with('.') || ["node_modules", "AppData", "Library"].contains(&name)
}

/// The repositories under `root` here, at most `depth` folders down: a folder with a `.git`
/// (folder, or file for a worktree) is one, and isn't looked into further.
pub fn find_under(root: &Path, depth: usize) -> Found {
    let mut found = Found::default();
    let mut todo = vec![(root.to_owned(), 0)];
    while let Some((dir, level)) = todo.pop() {
        if found.repos.len() >= MAX_FOUND {
            found.cut = true;
            break;
        }
        if level > 0 && dir.join(".git").exists() {
            found.repos.push(dir);
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
    found.repos.sort();
    found
}

/// The script that lists, from the home folder, the folders holding a `.git` (see
/// [`find_under`]): `$HOME` on the first line, then each one relative to it, one more than
/// [`MAX_FOUND`] at most (to tell a cut list). It doesn't look into a repository: its nested
/// ones (submodules, vendored copies) aren't the user's.
fn script() -> String {
    format!(
        concat!(
            "[ -n \"$HOME\" ] && cd \"$HOME\" 2>/dev/null || ",
            "{{ echo 'gitcraft: no home folder' >&2; exit 1; }}\n",
            "printf '%s\\n' \"$HOME\"\n",
            "find . -mindepth 1 -maxdepth {depth} -type d ",
            "\\( -name '.?*' -o -name node_modules -o -name AppData -o -name Library \\) -prune ",
            "-o -type d -exec test -e '{{}}/.git' \\; -print -prune 2>/dev/null ",
            "| head -n {max}\n",
            "exit 0\n",
        ),
        depth = DEPTH,
        max = MAX_FOUND + 1,
    )
}

/// The repositories [`script`]'s output names, as paths of `host`.
fn parse_found(out: &str, host: &Host) -> Found {
    let mut lines = out.lines();
    let Some(home) = lines.next().map(|h| h.trim_end_matches('/')) else {
        return Found::default();
    };
    let mut rels: Vec<&str> = lines
        .filter_map(|line| line.strip_prefix("./"))
        .filter(|rel| !rel.is_empty())
        .collect();
    let cut = rels.len() > MAX_FOUND;
    rels.truncate(MAX_FOUND);
    let mut repos: Vec<PathBuf> = rels
        .into_iter()
        .map(|rel| remote::encode(host, &format!("{home}/{rel}")))
        .collect();
    repos.sort();
    repos.dedup();
    Found { repos, cut }
}

fn find_there(host: &Host) -> Result<Found, String> {
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
            "AppData/Local/x/.git",
            "Library/y/.git",
            "notes",
        ] {
            std::fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
        }
        // A worktree's `.git` is a file.
        std::fs::create_dir_all(root.join("code/wt")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("code/wt/.git"), "gitdir: x").map_err(|e| e.to_string())?;
        // The home folder being a repository itself (dotfiles) doesn't hide the rest.
        std::fs::create_dir_all(root.join(".git")).map_err(|e| e.to_string())?;
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
            relative(&root, &found.repos),
            [
                "code/app",
                "code/wt",
                "code/ünï côde",
                "work/client/deep/er"
            ]
        );
        assert!(!found.cut);
        assert!(find_under(&root.join("missing"), DEPTH).repos.is_empty());
        Ok(())
    }

    #[test]
    fn says_when_it_stopped_at_the_most() -> Result<(), String> {
        let root = tree("many")?;
        for i in 0..=MAX_FOUND {
            std::fs::create_dir_all(root.join(format!("many/r{i:03}/.git")))
                .map_err(|e| e.to_string())?;
        }
        let found = find_under(&root, DEPTH);
        assert_eq!(found.repos.len(), MAX_FOUND);
        assert!(found.cut);
        Ok(())
    }

    #[test]
    fn reads_what_the_script_printed() {
        let out = "/home/me\n./code/app\n./a b/c\n./code/app\n./\n";
        let host = Host::Ssh("build".into());
        let found = parse_found(out, &host);
        assert_eq!(
            found.repos,
            vec![
                PathBuf::from("ssh://build/home/me/a b/c"),
                PathBuf::from("ssh://build/home/me/code/app"),
            ]
        );
        assert!(!found.cut);
        assert!(parse_found("", &host).repos.is_empty());
        assert!(parse_found("/root/\n", &host).repos.is_empty());
        let many: String = (0..=MAX_FOUND).map(|i| format!("./r{i}\n")).collect();
        let found = parse_found(&format!("/h\n{many}"), &host);
        assert_eq!(found.repos.len(), MAX_FOUND);
        assert!(found.cut);
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
        let mut there: Vec<String> = there
            .repos
            .iter()
            .filter_map(|p| {
                p.to_string_lossy()
                    .strip_prefix(&prefix)
                    .map(|r| r.trim_start_matches('/').to_owned())
            })
            .collect();
        let mut here = relative(&root, &find_under(&root, DEPTH).repos);
        here.sort();
        there.sort();
        assert_eq!(here, there);
        Ok(())
    }

    /// Without a home folder the script says so instead of searching where it happens to be.
    #[cfg(unix)]
    #[test]
    fn the_script_needs_a_home_folder() -> Result<(), String> {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(script())
            .env_remove("HOME")
            .output()
            .map_err(|e| e.to_string())?;
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("no home folder"));
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
        assert_eq!(Machine::of(Path::new("/home/me/x")), Machine::Here);
        assert_eq!(
            Machine::of(Path::new("ssh://build/x")),
            Machine::There(Host::Ssh("build".into()))
        );
    }

    #[test]
    fn forges_arent_machines() {
        assert!(is_forge("github.com"));
        assert!(is_forge("git@GitLab.com"));
        assert!(is_forge("ssh.github.com:443"));
        assert!(!is_forge("build"));
        assert!(!is_forge("github.example.org"));
    }
}
