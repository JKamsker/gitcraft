//! Commits no ref reaches any more, whether a reflog still has them or not: those `git fsck
//! --unreachable --no-reflogs` finds. It reads every object of the repository, which takes a while on a big
//! one, so it can be cancelled.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Read};
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::git::{END_OPTS, NO_SIGNATURE, UTF8, run};

/// How long what git said is waited for once it has ended: over SSH, a proxy ssh started can
/// keep the error output open after it.
const SAID_WITHIN: Duration = Duration::from_secs(2);

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
    /// A dropped stash (`git stash` makes a merge of HEAD and the index, "WIP on main: …"):
    /// applied back as a stash, not made a branch of.
    pub stash: bool,
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

/// How many commits one `git log --no-walk` is asked about: their hashes (SHA-256's 64
/// characters included) stay within the shortest command line (Windows': 32 767 characters).
const CHUNK: usize = 400;

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
    cancel.check()?;
    let live = live_stashes(repo);
    found.retain(|c| !live.contains(&c.hash));
    let mut found = settle_stashes(found);
    found.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.hash.cmp(&b.hash)));
    Ok(found)
}

/// Whether a commit looks like what `git stash` makes: a merge of HEAD with the index (and
/// the untracked files), its message `WIP on main: …`, or `On main: …` when given one. Only
/// its index commit tells it apart from a merge so named (see [`settle_stashes`]).
fn is_stash(summary: &str, parents: usize) -> bool {
    (2..=3).contains(&parents) && (summary.starts_with("WIP on ") || summary.starts_with("On "))
}

/// Marks as dropped stashes only those whose second parent is their index commit (`index on
/// main: …`, lost with them), and leaves out the commits that are only part of one (that
/// index commit, and its `untracked files on main: …` one): applying the stash brings them
/// back.
fn settle_stashes(mut found: Vec<LostCommit>) -> Vec<LostCommit> {
    let summaries: HashMap<String, String> = found
        .iter()
        .map(|c| (c.hash.clone(), c.summary.clone()))
        .collect();
    for c in &mut found {
        c.stash = c.stash
            && c.parents
                .get(1)
                .and_then(|p| summaries.get(p))
                .is_some_and(|s| s.starts_with("index on "));
    }
    let parts: HashSet<String> = found
        .iter()
        .filter(|c| c.stash)
        .flat_map(|c| c.parents.iter().skip(1).cloned())
        .collect();
    found.retain(|c| !parts.contains(&c.hash));
    found
}

/// The stashes there are (`stash@{1}` on are only in `refs/stash`'s reflog, which the scan
/// doesn't count) and their index and untracked commits: none of them is lost. Empty when
/// there are none, or they can't be read.
fn live_stashes(repo: &Path) -> HashSet<String> {
    let args = [
        "log",
        "--walk-reflogs",
        NO_SIGNATURE,
        "--format=%H %P",
        END_OPTS,
        "refs/stash",
        "--",
    ];
    run(Some(repo), &args)
        .map(|out| out.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
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
    let (said_tx, said) = std::sync::mpsc::channel();
    if let Some(mut stderr) = child.stderr.take() {
        // Unread, a full pipe would stop git.
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            let _ = said_tx.send(text);
        });
    }
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
    let said = said.recv_timeout(SAID_WITHIN).unwrap_or_default();
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
        stash: is_stash(summary, parents.split_whitespace().count()),
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
    fn dropped_stashes_are_told_apart_and_their_parts_left_out() {
        assert!(is_stash("WIP on main: 1234567 Fix", 2));
        assert!(is_stash("On fünf: my message", 3));
        assert!(!is_stash("WIP on main: looks like one", 1));
        assert!(!is_stash("Merge branch 'x'", 2));
        let commit = |hash: &str, parents: &[&str], summary: &str| LostCommit {
            hash: hash.to_owned(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            author: String::new(),
            time: 0,
            date: String::new(),
            summary: summary.to_owned(),
            stash: is_stash(summary, parents.len()),
        };
        let found = vec![
            commit("s", &["h", "i", "u"], "WIP on main: h base"),
            commit("i", &["h"], "index on main: h base"),
            commit("u", &[], "untracked files on main: h base"),
            commit("x", &["h"], "lost work"),
            // A merge named like a stash, whose second parent is no index commit.
            commit("m", &["h", "y"], "On second thought, merge topic"),
            commit("y", &["h"], "topic work"),
        ];
        let kept: Vec<String> = settle_stashes(found).into_iter().map(|c| c.hash).collect();
        assert_eq!(kept, ["s", "x", "m", "y"]);
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
