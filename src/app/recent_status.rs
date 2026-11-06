//! What the start page shows of each recent repository: its branch and whether it has
//! uncommitted changes, read in the background now and then and kept by folder.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use crate::git;

/// How often the start page reads its repositories again while it is shown (and the window
/// has the focus).
const REFRESH_EVERY: Duration = Duration::from_secs(60);

/// How long a reading may take: a repository that hasn't answered by then (a folder on a
/// network share that hangs) is left out of the next readings.
const READ_FOR: Duration = Duration::from_secs(90);

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
    /// It didn't answer in time.
    TimedOut,
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
            RecentStatus::TimedOut => "didn't answer in time".to_owned(),
            RecentStatus::Failed(_) => "couldn't read it".to_owned(),
        }
    }

    /// Why it couldn't be read, for the label's tooltip.
    pub(super) fn reason(&self) -> Option<&str> {
        match self {
            RecentStatus::Failed(why) => Some(why),
            _ => None,
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

/// The `git status` the start page reads a recent repository with.
/// - `core.fsmonitor=false`: a repository's own config could name a program for git to run;
///   a repository found by a scan was never opened, and nobody asked for this read.
/// - `--no-optional-locks`: never takes the index lock from a git running there.
const STATUS_ARGS: [&str; 8] = [
    "-c",
    "core.fsmonitor=false",
    "--no-optional-locks",
    "status",
    "--porcelain=v2",
    "--branch",
    "--untracked-files=normal",
    "--ignore-submodules=dirty",
];

/// Reads `repo`'s branch and whether it has uncommitted changes. `None` for one on another
/// machine no connection is open to (it is not connected to just for this).
pub(super) fn read(repo: &Path) -> Option<RecentStatus> {
    if !crate::remote::is_remote(repo) {
        if !repo.is_dir() {
            return Some(RecentStatus::NotFound);
        }
        // Its own repository, not one of a folder above it (whose `.git` it lost).
        if !repo.join(".git").exists() {
            return Some(RecentStatus::NotRepo);
        }
    }
    Some(match git::run_if_connected(repo, &STATUS_ARGS)? {
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
    } else if lower.contains("didn't answer in time") {
        RecentStatus::TimedOut
    } else {
        let first = err.lines().next().unwrap_or_default();
        RecentStatus::Failed(first.chars().take(200).collect())
    }
}

/// A reading under way: one thread per repository, so that one that hangs holds up no other.
struct Reading {
    rx: Receiver<(PathBuf, Option<RecentStatus>)>,
    /// Not answered yet.
    waiting: HashSet<PathBuf>,
    started: Instant,
}

/// The statuses read so far, by folder, and the reading under way.
#[derive(Default)]
pub(super) struct RecentStatuses {
    /// `None`: one on another machine that wasn't read, as no connection is open to it.
    found: HashMap<PathBuf, Option<RecentStatus>>,
    reading: Option<Reading>,
    /// When the last reading started.
    started: Option<Instant>,
    /// The frame the start page was last drawn in.
    seen_frame: Option<u64>,
    /// Repositories that didn't answer a reading in time: not read again (a thread may still
    /// wait for them).
    hung: HashSet<PathBuf>,
}

impl RecentStatuses {
    pub(super) fn get(&self, path: &Path) -> Option<&RecentStatus> {
        self.found.get(path)?.as_ref()
    }

    /// Called each frame the start page is drawn: takes in what was read, and reads `paths`
    /// again when the page was just shown, after a while (while the window has the focus),
    /// or when one was never read.
    pub(super) fn update(&mut self, paths: &[PathBuf], ctx: &egui::Context) {
        self.take_in();
        let frame = ctx.cumulative_frame_nr();
        let just_shown = self.seen_frame.is_none_or(|f| f + 1 < frame);
        self.seen_frame = Some(frame);
        if self.reading.is_some() {
            return;
        }
        let focused = ctx.input(|i| i.focused);
        let stale = focused && self.started.is_none_or(|t| t.elapsed() >= REFRESH_EVERY);
        let unread = paths.iter().any(|p| !self.found.contains_key(p));
        if just_shown || stale || unread {
            let wanted = self.to_read(paths, just_shown);
            self.start(wanted, ctx);
        }
        ctx.request_repaint_after(REFRESH_EVERY);
    }

    /// Which of `paths` a reading reads: not those that hung, nor one on another machine that
    /// didn't answer in time last time (until the page is shown again).
    fn to_read(&self, paths: &[PathBuf], just_shown: bool) -> Vec<PathBuf> {
        paths
            .iter()
            .filter(|p| !self.hung.contains(*p))
            .filter(|p| {
                just_shown
                    || !crate::remote::is_remote(p)
                    || self.get(p) != Some(&RecentStatus::TimedOut)
            })
            .cloned()
            .collect()
    }

    fn start(&mut self, paths: Vec<PathBuf>, ctx: &egui::Context) {
        let (tx, rx) = channel();
        for path in &paths {
            let (tx, ctx, path) = (tx.clone(), ctx.clone(), path.clone());
            std::thread::spawn(move || {
                let status = read(&path);
                let _ = tx.send((path, status));
                ctx.request_repaint();
            });
        }
        self.reading = Some(Reading {
            rx,
            waiting: paths.into_iter().collect(),
            started: Instant::now(),
        });
        self.started = Some(Instant::now());
    }

    fn take_in(&mut self) {
        let Some(reading) = &mut self.reading else {
            return;
        };
        // `None`, not reachable without connecting: nothing is shown for it.
        while let Ok((path, status)) = reading.rx.try_recv() {
            reading.waiting.remove(&path);
            self.found.insert(path, status);
        }
        if reading.waiting.is_empty() {
            self.reading = None;
        } else if reading.started.elapsed() > READ_FOR {
            for path in std::mem::take(&mut reading.waiting) {
                self.found
                    .insert(path.clone(), Some(RecentStatus::TimedOut));
                self.hung.insert(path);
            }
            self.reading = None;
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

    /// A recent folder that lost its `.git` but sits inside another repository isn't shown
    /// with that repository's branch.
    #[test]
    fn a_folder_inside_another_repository_is_not_one() {
        let dir = repo("parent");
        git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "one"]);
        let inner = dir.join("was a repo");
        let _ = std::fs::create_dir_all(&inner);
        let status = read(&inner);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(status, Some(RecentStatus::NotRepo));
    }

    #[test]
    fn timed_out_remote_ones_wait_until_the_page_is_shown_again() {
        assert_eq!(
            from_error("exec git status didn't answer in time"),
            RecentStatus::TimedOut
        );
        let remote = PathBuf::from("ssh://box/home/me/api");
        let here = PathBuf::from("/home/me/app");
        let hung = PathBuf::from("/mnt/nfs/stuck");
        let mut statuses = RecentStatuses::default();
        statuses
            .found
            .insert(remote.clone(), Some(RecentStatus::TimedOut));
        statuses
            .found
            .insert(here.clone(), Some(RecentStatus::TimedOut));
        statuses.hung.insert(hung.clone());
        let all = [remote.clone(), here.clone(), hung];
        assert_eq!(statuses.to_read(&all, false), vec![here.clone()]);
        assert_eq!(statuses.to_read(&all, true), vec![remote, here]);
        assert_eq!(RecentStatus::Failed("boom".into()).reason(), Some("boom"));
    }

    #[test]
    fn one_on_another_machine_is_not_connected_to() {
        let path = Path::new("ssh://gitcraft-never-connected.invalid/home/me/repo");
        assert_eq!(read(path), None);
    }
}
