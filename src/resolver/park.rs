//! A merge (cherry-pick, revert) in the middle of its conflicts put aside, like a stash, while
//! an agent resolves it: the worktree's files, its index as it is (each conflict's stages
//! too) and git's own files of the operation (`MERGE_HEAD`, `MERGE_MSG`…) are kept, and the
//! worktree goes back to its branch, free for the user to check out anything meanwhile. Brought
//! back, all of it is as it was, conflicts and what was staged included.
//!
//! What is kept: the files as a commit (parents: the commit checked out, the one being brought
//! in, and, when it differs, a commit of what was staged) under `refs/gitcraft/parked/<id>`,
//! which keeps it and what was staged from git's garbage collection; the index, the
//! operation's files and the changed files' own bytes (as no filter changed them) under
//! `.git/gitcraft/parked/<id>/`, with what is known of it (`parked.json`).
//!
//! Recovery retains an archive across partial failures: nothing in the worktree changes
//! before all of it is written and checked against the worktree once more; what git can't keep
//! exactly (files marked assume-unchanged or left out of a sparse checkout, submodules changed)
//! isn't put aside at all; neither putting it aside nor bringing it back writes over a file git
//! doesn't track (it stops, and says which). Partial restores resume only against
//! verified before/after bytes; uncertain states require explicit recovery. Files git
//! doesn't track stay where they are throughout.

#[path = "park/discovery.rs"]
mod discovery;
#[path = "park/effects.rs"]
mod effects;
#[path = "park/locking.rs"]
mod locking;
pub use discovery::recovery_location;
#[path = "park/bytes.rs"]
mod bytes;
#[path = "park/durability.rs"]
mod durability;
#[path = "park/journal.rs"]
mod journal;
#[path = "park/publication.rs"]
mod publication;
#[path = "park/transaction.rs"]
mod transaction;
use bytes::*;
#[path = "park/guards.rs"]
mod guards;
use guards::*;
#[path = "park/restore.rs"]
mod restore;
use restore::*;
#[allow(unused_imports)] // Compatibility re-export.
pub use restore::{blocker, bring_back};
#[path = "park/archive.rs"]
mod archive;
use archive::*;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::snapshot;
use crate::git;

/// Where the parked merges' refs are.
pub const REF_PREFIX: &str = "refs/gitcraft/parked/";

/// git's files of an operation in progress, kept as they are. (`MERGE_RR` isn't: the reset
/// that clears the worktree clears what rerere recorded of the conflicts with it.)
const STATE_FILES: [&str; 8] = [
    "MERGE_HEAD",
    "MERGE_MSG",
    "MERGE_MODE",
    "MERGE_AUTOSTASH",
    "AUTO_MERGE",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "SQUASH_MSG",
];

/// A merge put aside.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Parked {
    pub id: String,
    /// The worktree it was put aside from (its top folder, as git names it).
    pub worktree: String,
    /// "merge", "cherry-pick" or "revert".
    pub operation: String,
    /// The branch checked out then (`None`: a detached HEAD).
    pub branch: Option<String>,
    /// The commit checked out then.
    pub head: String,
    /// The commit being brought in, and what names it.
    pub theirs: String,
    pub theirs_name: Option<String>,
    /// The commit of the worktree's files.
    pub snapshot: String,
    /// The conflicted files then.
    pub files: Vec<String>,
    /// How many unmerged entries the index had: it has as many once back.
    pub unmerged: usize,
    /// The tracked regular files kept byte for byte (under `raw/`).
    pub raw: Vec<String>,
    /// git's files of the operation that were kept (of [`STATE_FILES`]).
    pub state_files: Vec<String>,
    /// When, in seconds since 1970.
    pub at: i64,
}

impl Parked {
    /// "merge of dev", "cherry-pick of 1a2b3c4d".
    pub fn what(&self) -> String {
        let theirs = self
            .theirs_name
            .clone()
            .unwrap_or_else(|| self.theirs.get(..8).unwrap_or(&self.theirs).to_owned());
        format!("{} of {theirs}", self.operation)
    }

    pub fn ref_name(&self) -> String {
        format!("{REF_PREFIX}{}", self.id)
    }
}

fn run(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = git::run(Some(repo), args)?;
    effects::boundary(&format!("git:{}", args.join(" ")))?;
    Ok(output)
}

fn trimmed(out: String) -> String {
    out.trim_end_matches(['\n', '\r']).to_owned()
}

/// The worktree's own git folder (where its index and `MERGE_HEAD` are).
fn git_dir(repo: &Path) -> Result<PathBuf, String> {
    run(repo, &["rev-parse", "--absolute-git-dir"]).map(|d| PathBuf::from(trimmed(d)))
}

/// The folder a parked merge's files are kept in.
fn folder(repo: &Path, id: &str) -> Result<PathBuf, String> {
    crate::agent::repo_file(repo, &format!("parked/{id}"))
}

/// The trees of the worktree's tracked files, written through a copy of the index at `index`
/// (it is left alone): what is staged (conflicted files as they are on disk), then the files
/// as they are now (conflicted ones markers and all).
fn trees(repo: &Path, index: &Path) -> Result<(String, String), String> {
    let temporary = snapshot::temporary_index(repo, "park")?;
    let path = repo.join(&temporary);
    let with_index = |args: &[&str]| git::run_with_index(repo, &temporary, args);
    let result = std::fs::copy(index, &path)
        .map_err(|e| format!("couldn't copy the index: {e}"))
        .and_then(|_| {
            let unmerged = with_index(&["ls-files", "-u", "-z"])?;
            let mut paths: Vec<&str> = unmerged
                .split('\0')
                .filter_map(|e| e.split_once('\t').map(|(_, p)| p))
                .collect();
            paths.dedup();
            let input: String = paths.iter().map(|p| format!("{p}\0")).collect();
            let call = git::Call {
                env: vec![("GIT_INDEX_FILE", temporary.clone())],
                stdin: Some(input.into_bytes()),
                ..git::Call::default()
            };
            let args = ["update-index", "--remove", "-z", "--stdin"];
            git::run_call(Some(repo), &args, call)?;
            let staged = with_index(&["write-tree"]).map(trimmed)?;
            with_index(&["add", "-u", "--", ":/"])?;
            Ok((staged, with_index(&["write-tree"]).map(trimmed)?))
        });
    snapshot::remove(repo, &temporary);
    result
}

/// Whether the worktree is still as it was kept in `dir`, as `snapshot`: its index and files.
fn unchanged(repo: &Path, gd: &Path, dir: &Path, snapshot: &str) -> bool {
    let files = trees(repo, &gd.join("index")).ok().map(|(_, files)| files);
    let kept = run(repo, &["rev-parse", &format!("{snapshot}^{{tree}}")])
        .ok()
        .map(trimmed);
    std::fs::read(gd.join("index")).ok() == std::fs::read(dir.join("index")).ok()
        && files.is_some()
        && files == kept
        && STATE_FILES.iter().all(|name| {
            match (std::fs::read(gd.join(name)), std::fs::read(dir.join(name))) {
                (Ok(a), Ok(b)) => a == b,
                (Err(a), Err(b)) => {
                    a.kind() == std::io::ErrorKind::NotFound
                        && b.kind() == std::io::ErrorKind::NotFound
                }
                _ => false,
            }
        })
        && raw_unchanged(repo, dir)
        && serde_json::from_slice::<Parked>(
            &std::fs::read(dir.join("parked.json")).unwrap_or_default(),
        )
        .is_ok_and(|p| durability::restored_attributes(repo, gd, dir, &p).unwrap_or(false))
}

/// Filters can map distinct worktree bytes to the same tree. Check the saved bytes too.
fn raw_unchanged(repo: &Path, dir: &Path) -> bool {
    let Some(parked) = std::fs::read(dir.join("parked.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Parked>(&b).ok())
    else {
        return false;
    };
    let Ok(paths) = run(repo, &["ls-files", "--cached", "-z"]) else {
        return false;
    };
    let raw: std::collections::BTreeSet<&str> = parked.raw.iter().map(String::as_str).collect();
    let mut tracked: std::collections::BTreeSet<&str> =
        paths.split('\0').filter(|p| !p.is_empty()).collect();
    tracked.extend(raw.iter().copied());
    tracked.into_iter().all(|path| {
        if raw.contains(path) {
            crate::safe_read::read_inside(repo, path, u64::MAX - 1).is_some_and(|bytes| {
                std::fs::read(dir.join("raw").join(path)).is_ok_and(|kept| kept == bytes)
            })
        } else {
            !std::fs::symlink_metadata(repo.join(path)).is_ok_and(|m| m.is_file())
        }
    })
}

/// Takes `dir` for a merge put aside: none is there yet, or it isn't written over.
fn reserve(dir: &Path) -> Result<(), String> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("couldn't make {}: {e}", parent.display()))?;
    }
    std::fs::create_dir(dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => format!("{} is put aside already", dir.display()),
        _ => format!("couldn't make {}: {e}", dir.display()),
    })
}

fn unmerged(repo: &Path) -> Result<usize, String> {
    run(repo, &["ls-files", "-u"]).map(|o| o.lines().count())
}

/// Puts the operation in progress in the worktree at `repo` aside as `id`, `files` being its
/// conflicted files: kept as described above, then the worktree is reset to its branch.
/// Nothing is changed in it when something can't be kept, and nothing is left kept then.
pub fn park(repo: &Path, id: &str, files: &[String]) -> Result<Parked, String> {
    if crate::remote::is_remote(repo) {
        return Err("Only a repository on this machine can be put aside, for now".to_owned());
    }
    let _lease = locking::acquire(repo, id)?;
    let gd = git_dir(repo)?;
    let (operation, theirs) = operation(repo, &gd)?;
    if let Some(why) = unkeepable(repo) {
        return Err(why);
    }
    // A split index points into a shared one git may remove meanwhile: kept whole.
    let split = std::fs::read_dir(&gd).is_ok_and(|d| {
        d.filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("sharedindex."))
    });
    if split {
        run(repo, &["update-index", "--no-split-index"])?;
    }
    let dir = folder(repo, id)?;
    // Its folder first, new: what is cleaned up on failure is this one's only.
    reserve(&dir)?;
    effects::boundary("archive-reserved")?;
    journal::record(&dir, journal::Phase::Archiving)?;
    let kept = keep(repo, &gd, &dir, id, operation, &theirs, files);
    let parked = match kept {
        Ok(parked) => parked,
        Err(e) => {
            forget_locked(repo, id);
            return Err(e);
        }
    };
    // Last look: the user (an editor saving) changed nothing since it was kept.
    if !unchanged(repo, &gd, &dir, &parked.snapshot) {
        forget_locked(repo, id);
        return Err("Files changed while the merge was put aside: try again".to_owned());
    }
    // The reset writes the branch's files back over whatever is in their way: one the merge
    // deleted that is on disk again, untracked or ignored, stops it.
    match in_the_way(repo, &parked.snapshot, &parked.head) {
        Ok(None) => {}
        Ok(Some(path)) => {
            forget_locked(repo, id);
            return Err(format!(
                "{path} isn't tracked, but its branch has it: move it, or resolve the merge here"
            ));
        }
        Err(e) => {
            forget_locked(repo, id);
            return Err(e);
        }
    }
    journal::sync_tree(&dir)?;
    journal::sync_ancestors(&dir)?;
    effects::boundary("archive-reachable")?;
    journal::record(&dir, journal::Phase::Archived)?;
    if !unchanged(repo, &gd, &dir, &parked.snapshot) {
        return Err(format!(
            "Files changed while syncing the archive; retained at {}",
            dir.display()
        ));
    }
    journal::record(&dir, journal::Phase::Clearing)?;
    // All of it is durably kept: now the worktree is cleared.
    if let Err(e) = run(repo, &["reset", "--hard", "-q", "HEAD"]) {
        // Nothing changed: nothing need be kept. Anything did: it stays put aside, whole.
        if unchanged(repo, &gd, &dir, &parked.snapshot) {
            forget_locked(repo, id);
            return Err(format!(
                "The worktree couldn't be cleared ({e}): resolve it here"
            ));
        }
        return Err(format!(
            "The worktree was cleared only in part ({e}); the merge is put aside, whole: bring \
             it back, or drop it to keep its files on a branch"
        ));
    }
    // Record the exact clean worktree bytes left by reset, including filter-hidden bytes.
    // Failure leaves the archive intact; restore then uses the conservative canonical guard.
    let baseline = clean_baseline(repo)?;
    let bytes = serde_json::to_vec(&baseline).map_err(|e| e.to_string())?;
    journal::atomic_write(&dir.join("clean-baseline.json"), &bytes)?;
    journal::sync_tree(&dir)?;
    journal::record(&dir, journal::Phase::Parked)?;
    Ok(parked)
}

/// Removes what is kept of `id`: its ref and its folder.
#[allow(dead_code)] // Explicit recovery API, also exercised by the recording harness.
pub fn forget(repo: &Path, id: &str) {
    let Ok(_lease) = locking::acquire(repo, id) else {
        return;
    };
    forget_locked(repo, id);
}

fn forget_locked(repo: &Path, id: &str) {
    let _ = run(repo, &["update-ref", "-d", &format!("{REF_PREFIX}{id}")]);
    if let Ok(dir) = folder(repo, id) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Active parked merges and retained completed recovery archives for `repo`, oldest first.
pub fn list(repo: &Path) -> Result<Vec<Parked>, String> {
    let top = trimmed(run(repo, &["rev-parse", "--show-toplevel"])?);
    let ids = discovery::identities(repo)?;
    // A snapshot can name a source which is retired before its lock is acquired.
    // Resolve and read under the same stable lease used by foreground operations.
    let mut recovered = Vec::new();
    for id in ids {
        let _lease = locking::acquire(repo, &id)?;
        let Some(parked) = discovery::manifest(repo, &id)? else {
            continue;
        };
        if crate::remote::same_dir(Path::new(&parked.worktree), Path::new(&top))
            && journal::recover_locked(repo, &parked)?
        {
            recovered.push(parked);
        }
    }
    let mut found = recovered;
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found.dedup_by(|a, b| a == b);
    found.sort_by_key(|p| p.at);
    Ok(found)
}

/// Drops `parked` without bringing it back, its files kept on a branch of their own (named
/// after it): nothing of it is lost. The branch's name.
pub fn drop_to_branch(repo: &Path, parked: &Parked) -> Result<String, String> {
    let _lease = locking::acquire(repo, &parked.id)?;
    let dir = discovery::locate(repo, &parked.id)?;
    transaction::identity(&dir, parked)?;
    let name = match journal::latest(&dir)? {
        Some(journal::Phase::Dropping { branch }) => branch,
        _ => {
            let short = parked.id.get(..9).unwrap_or(&parked.id);
            let base = format!("parked/{}-{short}", parked.operation);
            let mut name = base.clone();
            let mut n = 2;
            while run(
                repo,
                &["rev-parse", "-q", "--verify", &format!("refs/heads/{name}")],
            )
            .is_ok()
            {
                name = format!("{base}-{n}");
                n += 1;
            }
            journal::sync_tree(&dir)?;
            journal::record(
                &dir,
                journal::Phase::Dropping {
                    branch: name.clone(),
                },
            )?;
            name
        }
    };
    let reference = format!("refs/heads/{name}");
    match run(repo, &["rev-parse", "-q", "--verify", &reference]) {
        Ok(tip) if tip.trim_end_matches(['\n', '\r']) == parked.snapshot => {}
        Ok(_) => return Err(format!("Recovery branch {name} moved; archive retained")),
        Err(_) => {
            run(repo, &["update-ref", &reference, &parked.snapshot, ""])?;
        }
    }
    journal::retire(repo, &parked.id)?;
    let archive = crate::agent::repo_file(repo, &format!("parked-recovery/{}", parked.id))?;
    let description = format!(
        "Original byte-exact files and index: {}",
        archive.join("parked").display()
    );
    run(
        repo,
        &[
            "config",
            &format!("branch.{name}.description"),
            &description,
        ],
    )?;
    Ok(name)
}

#[cfg(all(test, not(recovery_recording)))]
mod tests;

#[cfg(recovery_recording)]
#[path = "park/checks/transitions.rs"]
mod recording_checks;

#[cfg(recovery_recording)]
#[path = "park/checks/regressions.rs"]
mod recording_regressions;

#[cfg(recovery_recording)]
#[path = "park/checks/discovery.rs"]
mod recording_discovery;

#[cfg(recovery_recording)]
#[path = "park/checks/readonly.rs"]
mod recording_readonly;
