//! The machine the tests run on: this computer, a WSL distribution or an SSH host. It may be
//! another than the repository's: the commits are then sent there (see `workspace`).

use std::path::{Path, PathBuf};

use crate::remote::{self, Host};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Machine {
    Here,
    There(Host),
}

impl Machine {
    /// The machine `path` (a repository) is on.
    pub fn of(path: &Path) -> Machine {
        remote::parse(path).map_or(Machine::Here, |t| Machine::There(t.host))
    }

    /// "This computer", "WSL · Ubuntu", "SSH · build".
    pub fn label(&self) -> String {
        match self {
            Machine::Here => "this computer".to_owned(),
            Machine::There(host) => host.label(),
        }
    }

    /// The path the app gives `there`, an absolute path on this machine.
    pub fn path(&self, there: &str) -> PathBuf {
        match self {
            Machine::Here => PathBuf::from(there),
            Machine::There(host) => remote::encode(host, there),
        }
    }

    /// How many cores it has (it may take a round trip: never on the window's thread).
    pub fn cores(&self) -> Result<usize, String> {
        let Machine::There(host) = self else {
            return Ok(std::thread::available_parallelism().map_or(1, |n| n.get()));
        };
        let req = remote::Request {
            script: "nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || \
                     sysctl -n hw.ncpu"
                .to_owned(),
            ..remote::Request::default()
        };
        let out = remote::exec(host, &req)?;
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|&n| n > 0 && out.code == 0)
            .ok_or_else(|| format!("could not count the cores of {}", host.label()))
    }

    /// Its home folder, as a path there.
    pub fn home(&self) -> Result<String, String> {
        match self {
            Machine::Here => std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map_err(|_| "could not find the home folder of this computer".to_owned()),
            Machine::There(host) => {
                let req = remote::Request {
                    script: "printf '%s' \"$HOME\"".to_owned(),
                    ..remote::Request::default()
                };
                let out = remote::exec(host, &req)?;
                let home = String::from_utf8_lossy(&out.stdout).into_owned();
                if out.code == 0 && home.starts_with('/') {
                    Ok(home)
                } else {
                    Err(format!(
                        "could not find the home folder on {}",
                        host.label()
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repository_names_its_machine() {
        assert_eq!(Machine::of(Path::new("/home/me/repo")), Machine::Here);
        let ssh = Machine::of(Path::new("ssh://build/home/me/repo"));
        assert_eq!(ssh, Machine::There(Host::Ssh("build".to_owned())));
        assert_eq!(ssh.label(), "SSH · build");
        assert_eq!(Machine::Here.label(), "this computer");
        assert_eq!(ssh.path("/srv/x"), PathBuf::from("ssh://build/srv/x"));
        assert!(Machine::Here.cores().is_ok_and(|n| n >= 1));
    }
}
