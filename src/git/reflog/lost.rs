//! Commits no ref reaches any more, whether a reflog still has them or not: those `git fsck
//! --unreachable --no-reflogs` finds. It reads every object of the repository, which takes a while on a big
//! one, so it can be cancelled.

use std::io::{BufRead, Read};
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::git::{END_OPTS, NO_SIGNATURE, UTF8, run};

/// A commit found unreachable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LostCommit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    /// Committer time (when it was last made or rewritten), Unix seconds.
    pub time: i64,
    /// The same, as a local date: `2024-05-01 14:03`.
    pub date: String,
    pub summary: String,
}

/// What the error of a cancelled scan says.
pub const CANCELLED: &str = "Cancelled";

/// Stops a running [`lost_commits`] from another thread: the git it runs is ended, and it
/// returns [`CANCELLED`].
#[derive(Clone, Default)]
pub struct Cancel(Arc<Mutex<CancelState>>);

#[derive(Default)]
struct CancelState {
    cancelled: bool,
    /// The `git fsck` running, while it runs.
    child: Option<Child>,
}

impl Cancel {
    pub fn cancel(&self) {
        let mut state = self.lock();
        state.cancelled = true;
        if let Some(child) = &mut state.child {
            let _ = child.kill();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// A panic elsewhere while it was held leaves nothing half-done here.
    fn lock(&self) -> MutexGuard<'_, CancelState> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(CANCELLED.to_owned())
        } else {
            Ok(())
        }
    }
}

/// How many commits one `git log --no-walk` is asked about: their hashes stay well within the
/// shortest command line (Windows': 32 767 characters).
const CHUNK: usize = 200;

/// Every commit that no ref (branch, tag, stash…) and no index reaches, newest first; a reflog
/// having it doesn't count. These are what a `reset --hard`, a deleted branch or a rebase left
/// behind, also once the reflog has expired (or was never there), and dropped stashes, until
/// `git gc` prunes them.
pub fn lost_commits(repo: &Path, cancel: &Cancel) -> Result<Vec<LostCommit>, String> {
    let hashes = unreachable_commits(repo, cancel)?;
    let mut found = Vec::with_capacity(hashes.len());
    for chunk in hashes.chunks(CHUNK) {
        cancel.check()?;
        found.extend(describe(repo, chunk)?);
    }
    found.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.hash.cmp(&b.hash)));
    Ok(found)
}

/// The hashes of the unreachable commits, as `git fsck` lists them.
fn unreachable_commits(repo: &Path, cancel: &Cancel) -> Result<Vec<String>, String> {
    cancel.check()?;
    let args = [
        "fsck",
        // Only what reaches what: not every blob's contents checked, many times quicker.
        "--connectivity-only",
        "--unreachable",
        "--no-reflogs",
        "--no-progress",
    ];
    let mut child = super::super::command::spawn_piped(repo, &args)?;
    let stdout = child.stdout.take().ok_or("could not read git fsck")?;
    let err = child.stderr.take().map(|mut stderr| {
        // Unread, a full pipe would stop git.
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });
    {
        let mut state = cancel.lock();
        if state.cancelled {
            let _ = child.kill();
        }
        state.child = Some(child);
    }
    let mut hashes = Vec::new();
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        hashes.extend(unreachable_commit(&line).map(str::to_owned));
    }
    let child = cancel.lock().child.take();
    let status = child.map(|mut c| c.wait());
    let said = err
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    cancel.check()?;
    match status {
        Some(Ok(status)) if status.success() => Ok(hashes),
        // fsck also fails on a corrupt object: what it found unreachable is still found.
        Some(Ok(_)) if !hashes.is_empty() => Ok(hashes),
        Some(Ok(status)) => Err(failed(&said, &status.to_string())),
        Some(Err(e)) => Err(format!("could not run git fsck: {e}")),
        None => Err("could not run git fsck".to_owned()),
    }
}

/// What a failed fsck said, or how it ended when it said nothing.
fn failed(said: &str, status: &str) -> String {
    let said = said.trim();
    if said.is_empty() {
        format!("git fsck failed ({status})")
    } else {
        said.to_owned()
    }
}

/// The hash of an `unreachable commit <hash>` line of `git fsck`; other objects (blobs, trees,
/// tags) are no commit to get back.
fn unreachable_commit(line: &str) -> Option<&str> {
    let hash = line.trim().strip_prefix("unreachable commit ")?.trim();
    (!hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit())).then_some(hash)
}

/// How many fields [`describe`]'s format has, each ended by a NUL.
const FIELDS: usize = 6;

/// The commits `hashes`, read in one `git log`.
fn describe(repo: &Path, hashes: &[String]) -> Result<Vec<LostCommit>, String> {
    let mut args = vec![
        "log",
        "--no-walk=unsorted",
        NO_SIGNATURE,
        UTF8,
        "--date=format-local:%Y-%m-%d %H:%M",
        "--format=%H%x00%P%x00%an%x00%ct%x00%cd%x00%s%x00",
        END_OPTS,
    ];
    args.extend(hashes.iter().map(String::as_str));
    args.push("--");
    let out = run(Some(repo), &args)?;
    let fields: Vec<&str> = out.split('\0').collect();
    Ok(fields
        .as_chunks::<FIELDS>()
        .0
        .iter()
        .filter_map(parse_commit)
        .collect())
}

fn parse_commit(record: &[&str; FIELDS]) -> Option<LostCommit> {
    let [hash, parents, author, time, date, summary] = record.map(|f| f.trim_start_matches('\n'));
    if hash.is_empty() {
        return None;
    }
    Some(LostCommit {
        hash: hash.to_owned(),
        parents: parents.split_whitespace().map(str::to_owned).collect(),
        author: author.to_owned(),
        time: time.parse().unwrap_or(0),
        date: date.to_owned(),
        summary: summary.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unreachable_commits_are_taken() {
        let hash = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            unreachable_commit(&format!("unreachable commit {hash}")),
            Some(hash)
        );
        assert_eq!(
            unreachable_commit(&format!("unreachable blob {hash}")),
            None
        );
        assert_eq!(unreachable_commit(&format!("dangling commit {hash}")), None);
        assert_eq!(unreachable_commit("unreachable commit "), None);
        assert_eq!(unreachable_commit("unreachable commit zz"), None);
        assert_eq!(unreachable_commit("Checking connectivity"), None);
    }

    #[test]
    fn a_failure_says_what_git_said() {
        assert_eq!(failed("  fatal: bad  \n", "exit status: 128"), "fatal: bad");
        assert_eq!(
            failed("", "exit status: 2"),
            "git fsck failed (exit status: 2)"
        );
    }

    #[test]
    fn a_cancelled_scan_starts_no_git() {
        let cancel = Cancel::default();
        cancel.cancel();
        let found = lost_commits(Path::new("/nonexistent-gitcraft-repo"), &cancel);
        assert_eq!(found, Err(CANCELLED.to_owned()));
    }
}
