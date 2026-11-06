//! What the start page shows of each recent repository: its branch and whether it has
//! uncommitted changes, read in the background now and then and kept by folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use crate::git;

/// How often the start page reads its repositories again while it is shown.
const REFRESH_EVERY: Duration = Duration::from_secs(60);

/// What HEAD is in a recent repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Head {
    /// On a branch with commits.
    Branch(String),
    /// On a branch that has no commits yet.
    Unborn(String),
    /// Not on a branch: at this commit (its short hash).
    Detached(String),
}

/// What was found of a recent repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RecentStatus {
    Repo {
        head: Head,
        dirty: bool,
    },
    /// The folder isn't there any more.
    NotFound,
    /// The folder is there, but no repository.
    NotRepo,
    /// git could not tell (the reason, one line).
    Failed(String),
}

impl RecentStatus {
    /// The words the start page shows for it, after the name: the branch, or what is wrong.
    pub(super) fn label(&self) -> String {
        match self {
            RecentStatus::Repo { head, .. } => match head {
                Head::Branch(b) => b.clone(),
                Head::Unborn(b) => format!("{b} · no commits yet"),
                Head::Detached(hash) => format!("detached at {hash}"),
            },
            RecentStatus::NotFound => "folder not found".to_owned(),
            RecentStatus::NotRepo => "not a repository any more".to_owned(),
            RecentStatus::Failed(_) => "couldn't read it".to_owned(),
        }
    }

    pub(super) fn dirty(&self) -> bool {
        matches!(self, RecentStatus::Repo { dirty: true, .. })
    }

    /// Whether it says something is wrong with it (shown faint, in the warning colour).
    pub(super) fn is_problem(&self) -> bool {
        !matches!(self, RecentStatus::Repo { .. })
    }
}

/// Reads `repo`'s branch and whether it has uncommitted changes. `None` for one on another
/// machine no connection is open to (it is not connected to just for this).
pub(super) fn read(repo: &Path) -> Option<RecentStatus> {
    if !crate::remote::is_remote(repo) && !repo.is_dir() {
        return Some(RecentStatus::NotFound);
    }
    // `--no-optional-locks`: never takes the index lock from a git running there.
    let args = [
        "--no-optional-locks",
        "status",
        "--porcelain=v2",
        "--branch",
        "--untracked-files=normal",
        "--ignore-submodules=dirty",
    ];
    Some(match git::run_if_connected(repo, &args)? {
        Ok(out) => parse(&out),
        Err(e) => from_error(&e),
    })
}

/// What `status --porcelain=v2 --branch` printed, read.
pub(super) fn parse(out: &str) -> RecentStatus {
    let mut oid = None;
    let mut branch = None;
    let mut dirty = false;
    for line in out.lines() {
        if let Some(v) = line.strip_prefix("# branch.oid ") {
            oid = Some(v.trim().to_owned());
        } else if let Some(v) = line.strip_prefix("# branch.head ") {
            branch = Some(v.trim().to_owned());
        } else if !line.starts_with('#') && !line.trim().is_empty() {
            dirty = true;
        }
    }
    let oid = oid.unwrap_or_default();
    let branch = branch.unwrap_or_default();
    let head = if branch == "(detached)" {
        Head::Detached(oid.chars().take(7).collect())
    } else if oid == "(initial)" {
        Head::Unborn(branch)
    } else {
        Head::Branch(branch)
    };
    RecentStatus::Repo { head, dirty }
}

/// What a failed `git status` means for the start page.
fn from_error(err: &str) -> RecentStatus {
    let lower = err.to_lowercase();
    if lower.contains("not a git repository") {
        RecentStatus::NotRepo
    } else if lower.contains("isn't a folder on") || lower.contains("cannot change to") {
        RecentStatus::NotFound
    } else {
        let first = err.lines().next().unwrap_or_default();
        RecentStatus::Failed(first.chars().take(200).collect())
    }
}

/// The statuses read so far, by folder, and the reading under way.
#[derive(Default)]
pub(super) struct RecentStatuses {
    /// `None`: one on another machine that wasn't read, as no connection is open to it.
    found: HashMap<PathBuf, Option<RecentStatus>>,
    reading: Option<Receiver<(PathBuf, Option<RecentStatus>)>>,
    /// When the last reading started.
    started: Option<Instant>,
    /// The frame the start page was last drawn in.
    seen_frame: Option<u64>,
}

impl RecentStatuses {
    pub(super) fn get(&self, path: &Path) -> Option<&RecentStatus> {
        self.found.get(path)?.as_ref()
    }

    /// Called each frame the start page is drawn: takes in what was read, and reads all of
    /// `paths` again when the page was just shown, after a while, or when one was never read.
    pub(super) fn update(&mut self, paths: &[PathBuf], ctx: &egui::Context) {
        self.take_in();
        let frame = ctx.cumulative_frame_nr();
        let just_shown = self.seen_frame.is_none_or(|f| f + 1 < frame);
        self.seen_frame = Some(frame);
        if self.reading.is_some() {
            return;
        }
        let stale = self.started.is_none_or(|t| t.elapsed() >= REFRESH_EVERY);
        let unread = paths.iter().any(|p| !self.found.contains_key(p));
        if just_shown || stale || unread {
            self.start(paths.to_vec(), ctx);
        }
        ctx.request_repaint_after(REFRESH_EVERY);
    }

    fn start(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for path in paths {
                let status = read(&path);
                if tx.send((path, status)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        self.reading = Some(rx);
        self.started = Some(Instant::now());
    }

    fn take_in(&mut self) {
        let Some(rx) = &self.reading else { return };
        loop {
            match rx.try_recv() {
                // `None`, not reachable without connecting: nothing is shown for it.
                Ok((path, status)) => {
                    self.found.insert(path, status);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.reading = None;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_v2_is_read() {
        let clean = "# branch.oid 0123456789abcdef\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        assert_eq!(
            parse(clean),
            RecentStatus::Repo {
                head: Head::Branch("main".into()),
                dirty: false
            }
        );
        let dirty = "# branch.oid 0123456789abcdef\n# branch.head feat/ü\n1 .M N... 100644 100644 100644 a b f.txt\n? new.txt\n";
        assert_eq!(
            parse(dirty),
            RecentStatus::Repo {
                head: Head::Branch("feat/ü".into()),
                dirty: true
            }
        );
        let detached = "# branch.oid 0123456789abcdef\n# branch.head (detached)\n";
        assert_eq!(parse(detached).label(), "detached at 0123456");
        let unborn = "# branch.oid (initial)\n# branch.head trunk\n";
        assert_eq!(parse(unborn).label(), "trunk · no commits yet");
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert_eq!(
            from_error("fatal: not a git repository (or any of the parent directories): .git"),
            RecentStatus::NotRepo
        );
        assert_eq!(
            from_error("/home/x isn't a folder on SSH · box"),
            RecentStatus::NotFound
        );
        assert!(matches!(from_error("boom\nmore"), RecentStatus::Failed(m) if m == "boom"));
        assert_eq!(RecentStatus::NotFound.label(), "folder not found");
    }

    fn git_ok(dir: &Path, args: &[&str]) {
        assert!(git::run(Some(dir), args).is_ok(), "git {args:?}");
    }

    /// A fresh folder named after `name`, its path with spaces and letters outside ASCII.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-recent {name} Größe 日本-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn repo(name: &str) -> PathBuf {
        let dir = folder(name);
        git_ok(&dir, &["init", "-q", "-b", "trunk"]);
        git_ok(&dir, &["config", "user.name", "t"]);
        git_ok(&dir, &["config", "user.email", "t@t"]);
        dir
    }

    #[test]
    fn real_repositories_are_read() {
        let dir = repo("states");
        // No commits yet.
        assert_eq!(
            read(&dir),
            Some(RecentStatus::Repo {
                head: Head::Unborn("trunk".into()),
                dirty: false
            })
        );
        git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "one"]);
        git_ok(&dir, &["checkout", "-q", "-b", "feature/ü"]);
        assert_eq!(
            read(&dir),
            Some(RecentStatus::Repo {
                head: Head::Branch("feature/ü".into()),
                dirty: false
            })
        );
        // A new file is an uncommitted change.
        let _ = std::fs::write(dir.join("new file.txt"), "x\n");
        assert!(read(&dir).is_some_and(|s| s.dirty()));
        git_ok(&dir, &["add", "-A"]);
        git_ok(&dir, &["commit", "-q", "-m", "two"]);
        assert!(read(&dir).is_some_and(|s| !s.dirty()));
        // Detached.
        let head = git::run(Some(&dir), &["rev-parse", "HEAD"]).unwrap_or_default();
        git_ok(&dir, &["checkout", "-q", "--detach"]);
        let label = read(&dir).map(|s| s.label());
        assert_eq!(label, Some(format!("detached at {}", &head.trim()[..7])));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gone_folders_and_plain_folders_say_so() {
        let dir = folder("plain");
        // A folder in no repository (the temporary folder is in none either).
        let status = read(&dir);
        let gone = dir.join("no such folder");
        assert_eq!(read(&gone), Some(RecentStatus::NotFound));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            matches!(status, Some(RecentStatus::NotRepo)),
            "{status:?} (is the temporary folder inside a repository?)"
        );
        assert_eq!(read(&dir), Some(RecentStatus::NotFound));
    }

    #[test]
    fn one_on_another_machine_is_not_connected_to() {
        let path = Path::new("ssh://gitcraft-never-connected.invalid/home/me/repo");
        assert_eq!(read(path), None);
    }
}
