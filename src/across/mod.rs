//! Across repositories: what looks over many repositories at once, wherever they are (here, in
//! WSL, over SSH). The window's parts that show it are in [`crate::app`].

pub mod runner;
pub mod workspaces;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use runner::Limits;

/// How many repositories are looked at side by side: in all, and on one other machine, whose
/// few shells kept open (`remote::pool`: six for reads) an open tab needs too.
pub const LOOKS: Limits = Limits {
    total: 6,
    per_group: 2,
};

/// How many repositories are fetched side by side; on one other machine at most two of the
/// three shells kept for network operations, so a tab's push still gets one.
pub const FETCHES: Limits = Limits {
    total: 4,
    per_group: 2,
};

/// How long a look at one repository may take there: a huge one's first `status` is slow.
/// Elsewhere it is stopped there after this.
pub const LOOK_FOR: Duration = Duration::from_secs(60);

/// How long a fetch of one repository may take there (stopped there after it, elsewhere).
pub const FETCH_FOR: Duration = Duration::from_secs(600);

/// What a job may wait on top of its own time: a shell of a busy machine (`remote::pool`
/// waits two minutes at most), or connecting to it.
const SLACK: Duration = Duration::from_secs(150);

/// How long the window waits for a look before saying it got no answer.
pub const LOOK_WAIT: Duration = LOOK_FOR.saturating_add(SLACK);

/// How long the window waits for a fetch.
pub const FETCH_WAIT: Duration = FETCH_FOR.saturating_add(SLACK);

/// The group a repository's jobs are limited by: its machine.
pub fn machine_key(repo: &Path) -> String {
    crate::remote::parse(repo).map_or_else(String::new, |t| t.host.label())
}

/// What runs now, as (what, repository): a job given up on by the window (no answer in time)
/// keeps its place until it really ends, so the same isn't started twice in one repository
/// (two fetches would fight over its lock).
fn running() -> &'static Mutex<HashSet<(&'static str, PathBuf)>> {
    static RUNNING: OnceLock<Mutex<HashSet<(&'static str, PathBuf)>>> = OnceLock::new();
    RUNNING.get_or_init(Mutex::default)
}

/// Runs `job` for `repo` unless a `what` of it still runs (then says so).
pub fn once_at_a_time<R>(
    what: &'static str,
    repo: &Path,
    job: impl FnOnce() -> Result<R, String>,
) -> Result<R, String> {
    let key = (what, repo.to_owned());
    let fresh = running()
        .lock()
        .is_ok_and(|mut set| set.insert(key.clone()));
    if !fresh {
        return Err(format!(
            "the last {what} of it is still running; try again once it ends"
        ));
    }
    let result = job();
    if let Ok(mut set) = running().lock() {
        set.remove(&key);
    }
    result
}

/// Looks at `repo` (see [`crate::git::overview`]), not twice at once.
pub fn look(repo: &Path) -> Result<crate::git::Overview, String> {
    once_at_a_time("look", repo, || crate::git::overview(repo, LOOK_FOR))
}

/// Fetches every remote of `repo`, asking nobody for a password (a workspace's fetch runs
/// unattended, like the background fetch): a remote that needs one fails with git's message.
pub fn fetch(repo: &Path) -> Result<(), String> {
    once_at_a_time("fetch", repo, || {
        crate::git::run_unattended_within(
            repo,
            &["fetch", "--all", "--prune", "--quiet"],
            FETCH_FOR,
        )
        .map(|_| ())
    })
}

/// A few words for a row saying why a look or fetch failed; the whole reason goes in its
/// tooltip.
pub fn short_reason(err: &str) -> &'static str {
    let e = err.to_lowercase();
    if crate::git::is_auth_error(err) || e.contains("permission denied") {
        "needs a sign-in"
    } else if e.contains("no answer within") || e.contains("didn't finish within") {
        "timed out"
    } else if e.contains("still running") {
        "still busy"
    } else if e.contains("isn't a folder")
        || e.contains("no such file")
        || e.contains("cannot change to")
    {
        "folder not found"
    } else if e.contains("not a git repository") {
        "not a repository"
    } else if e.contains("could not resolve host") || e.contains("connection") {
        "can't connect"
    } else {
        "failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_job_never_runs_twice_at_once() {
        let repo = Path::new("/tmp/gitcraft-once-at-a-time");
        let inner = once_at_a_time("look", repo, || {
            once_at_a_time("look", repo, || Ok(()))
                .err()
                .ok_or_else(|| "ran twice".to_owned())
        });
        assert!(inner.is_ok_and(|e| e.contains("still running")));
        // Another kind of job, or the same once the first ended, runs.
        let other = once_at_a_time("look", repo, || once_at_a_time("fetch", repo, || Ok(1)));
        assert_eq!(other, Ok(1));
    }

    #[test]
    fn reasons_in_a_few_words() {
        assert_eq!(
            short_reason("fatal: cannot change to '/x': No such file or directory"),
            "folder not found"
        );
        assert_eq!(
            short_reason("/x isn't a folder on SSH · h"),
            "folder not found"
        );
        assert_eq!(
            short_reason("fatal: Authentication failed for 'https://x'"),
            "needs a sign-in"
        );
        assert_eq!(
            short_reason("no answer within 3 minutes; it may still be running"),
            "timed out"
        );
        assert_eq!(
            short_reason("fatal: not a git repository"),
            "not a repository"
        );
        assert_eq!(short_reason("something else"), "failed");
    }

    #[test]
    fn repositories_are_grouped_by_machine() {
        assert_eq!(machine_key(Path::new("/a")), machine_key(Path::new("/b")));
        assert_eq!(machine_key(Path::new("ssh://h/x")), "SSH · h");
        assert_ne!(
            machine_key(Path::new("ssh://h/x")),
            machine_key(Path::new("ssh://g/x"))
        );
    }
}
