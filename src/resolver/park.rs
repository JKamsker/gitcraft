//! A merge (cherry-pick, revert) in the middle of its conflicts put aside, like a stash, while
//! an agent resolves it: the worktree's files, its index as it is (each conflict's stages
//! too) and git's own files of the operation (`MERGE_HEAD`, `MERGE_MSG`…) are kept, and the
//! worktree goes back to its branch, free for the user to check out anything meanwhile. Brought
//! back, all of it is as it was, conflicts and what was staged included.
//!
//! What is kept: the files as a commit (parents: the commit checked out, and the one being
//! brought in) under `refs/gitcraft/parked/<id>`, which keeps it from git's garbage collection
//! and shows it in the graph; the index and the operation's files under
//! `.git/gitcraft/parked/<id>/`, with what is known of it (`parked.json`). Nothing is changed in
//! the worktree before all of it is written. Files git doesn't track stay where they are.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::snapshot;
use crate::git;

/// Where the parked merges' refs are.
pub const REF_PREFIX: &str = "refs/gitcraft/parked/";

/// git's files of an operation in progress, kept as they are.
const STATE_FILES: [&str; 8] = [
    "MERGE_HEAD",
    "MERGE_MSG",
    "MERGE_MODE",
    "MERGE_RR",
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
    git::run(Some(repo), args)
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

/// The operation in progress in `repo` that can be put aside, its commit; or why not.
fn operation(repo: &Path, gd: &Path) -> Result<(&'static str, String), String> {
    if gd.join("rebase-merge").exists() || gd.join("rebase-apply").exists() {
        return Err("A rebase in progress can't be put aside: resolve it here".to_owned());
    }
    if gd.join("sequencer").exists() {
        return Err(
            "A cherry-pick or revert of several commits can't be put aside: resolve it here"
                .to_owned(),
        );
    }
    let head = |name: &str| {
        run(repo, &["rev-parse", "-q", "--verify", name])
            .ok()
            .map(trimmed)
            .filter(|h| !h.is_empty())
    };
    if let Some(h) = head("MERGE_HEAD") {
        if std::fs::read_to_string(gd.join("MERGE_HEAD")).is_ok_and(|t| t.lines().count() > 1) {
            return Err("An octopus merge can't be put aside: resolve it here".to_owned());
        }
        return Ok(("merge", h));
    }
    if let Some(h) = head("CHERRY_PICK_HEAD") {
        return Ok(("cherry-pick", h));
    }
    if let Some(h) = head("REVERT_HEAD") {
        return Ok(("revert", h));
    }
    Err("There is no merge, cherry-pick or revert in progress to put aside".to_owned())
}

/// The tree of the worktree's tracked files as they are now (conflicted ones as the files
/// are, markers and all), written through a copy of its index: its own is left alone.
fn files_tree(repo: &Path, gd: &Path) -> Result<String, String> {
    let index = snapshot::temporary_index(repo, "park")?;
    let path = repo.join(&index);
    let result = std::fs::copy(gd.join("index"), &path)
        .map_err(|e| format!("couldn't copy the index: {e}"))
        .and_then(|_| {
            git::run_with_index(repo, &index, &["add", "-u", "--", "."])?;
            git::run_with_index(repo, &index, &["write-tree"]).map(trimmed)
        });
    snapshot::remove(repo, &index);
    result
}

/// Puts the operation in progress in the worktree at `repo` aside as `id`, `files` being its
/// conflicted files: kept as described above, then the worktree is reset to its branch.
/// Nothing is changed in it when something can't be kept.
pub fn park(repo: &Path, id: &str, files: &[String]) -> Result<Parked, String> {
    if crate::remote::is_remote(repo) {
        return Err("Only a repository on this machine can be put aside, for now".to_owned());
    }
    let gd = git_dir(repo)?;
    let (operation, theirs) = operation(repo, &gd)?;
    let head = trimmed(run(repo, &["rev-parse", "--verify", "HEAD"])?);
    let branch = run(repo, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .map(trimmed)
        .filter(|b| !b.is_empty());
    let theirs_name = run(
        repo,
        &["name-rev", "--name-only", "--no-undefined", &theirs],
    )
    .ok()
    .map(trimmed)
    .filter(|n| !n.is_empty());
    let worktree = trimmed(run(repo, &["rev-parse", "--show-toplevel"])?);
    let tree = files_tree(repo, &gd)?;
    let message = format!(
        "Parked {operation} of {}: being resolved by an agent",
        theirs_name.as_deref().unwrap_or(&theirs)
    );
    let env = [
        ("GIT_AUTHOR_NAME", "gitcraft"),
        ("GIT_AUTHOR_EMAIL", "gitcraft@localhost"),
        ("GIT_COMMITTER_NAME", "gitcraft"),
        ("GIT_COMMITTER_EMAIL", "gitcraft@localhost"),
    ];
    let snapshot = git::run_env(
        Some(repo),
        &[
            "commit-tree",
            &tree,
            "-p",
            &head,
            "-p",
            &theirs,
            "-m",
            &message,
        ],
        &env,
    )
    .map(trimmed)?;
    let parked = Parked {
        id: id.to_owned(),
        worktree,
        operation: operation.to_owned(),
        branch,
        head,
        theirs,
        theirs_name,
        snapshot,
        files: files.to_vec(),
        state_files: Vec::new(),
        at: crate::git::now(),
    };
    let kept = keep(repo, &gd, parked).inspect_err(|_| forget(repo, id))?;
    // All of it is kept: now the worktree is cleared.
    run(repo, &["reset", "--hard", "-q", "HEAD"])
        .map_err(|e| format!("Kept, but the worktree wasn't cleared ({e}): bring it back"))?;
    Ok(kept)
}

/// Writes what is kept of `parked`: its ref, the index and the operation's files, then what is
/// known of it last (a parked merge is listed once it is whole).
fn keep(repo: &Path, gd: &Path, mut parked: Parked) -> Result<Parked, String> {
    let dir = folder(repo, &parked.id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    run(
        repo,
        &["update-ref", &parked.ref_name(), &parked.snapshot, ""],
    )?;
    std::fs::copy(gd.join("index"), dir.join("index"))
        .map_err(|e| format!("couldn't keep the index: {e}"))?;
    for name in STATE_FILES {
        let from = gd.join(name);
        if from.is_file() {
            std::fs::copy(&from, dir.join(name))
                .map_err(|e| format!("couldn't keep {name}: {e}"))?;
            parked.state_files.push(name.to_owned());
        }
    }
    let text = serde_json::to_string_pretty(&parked).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("parked.json"), text)
        .map_err(|e| format!("couldn't write what was kept: {e}"))?;
    Ok(parked)
}

/// Removes what is kept of `id`: its ref and its folder.
pub fn forget(repo: &Path, id: &str) {
    let _ = run(repo, &["update-ref", "-d", &format!("{REF_PREFIX}{id}")]);
    if let Ok(dir) = folder(repo, id) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The merges put aside from the worktree at `repo`, oldest first.
pub fn list(repo: &Path) -> Result<Vec<Parked>, String> {
    let root = crate::agent::repo_file(repo, "parked")?;
    let top = trimmed(run(repo, &["rev-parse", "--show-toplevel"])?);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(Vec::new());
    };
    let mut found: Vec<Parked> = entries
        .filter_map(Result::ok)
        .filter_map(|e| std::fs::read_to_string(e.path().join("parked.json")).ok())
        .filter_map(|text| serde_json::from_str::<Parked>(&text).ok())
        .filter(|p| crate::remote::same_dir(Path::new(&p.worktree), Path::new(&top)))
        .collect();
    found.sort_by_key(|p| p.at);
    Ok(found)
}

/// Why `parked` can't be brought back into the worktree at `repo` as it is now, if it can't.
pub fn blocker(repo: &Path, parked: &Parked) -> Option<String> {
    let gd = git_dir(repo).ok()?;
    if operation(repo, &gd).is_ok() || gd.join("rebase-merge").exists() {
        return Some("Finish or abort the operation in progress first".to_owned());
    }
    let dirty = run(repo, &["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(true);
    if dirty {
        return Some("Commit or stash your changes first".to_owned());
    }
    let tip = match &parked.branch {
        Some(b) => run(
            repo,
            &[
                "rev-parse",
                "-q",
                "--verify",
                &format!("refs/heads/{b}^{{commit}}"),
            ],
        )
        .ok()
        .map(trimmed),
        None => Some(parked.head.clone()),
    };
    match tip {
        None => Some(format!(
            "{} isn't there any more",
            parked.branch.as_deref().unwrap_or("its branch")
        )),
        Some(tip) if tip != parked.head => Some(format!(
            "{} moved since it was put aside: put it back at {} to bring this back",
            parked.branch.as_deref().unwrap_or("HEAD"),
            parked.head.get(..8).unwrap_or(&parked.head)
        )),
        Some(_) => None,
    }
}

/// Brings `parked` back into the worktree at `repo`: its branch checked out, its files, index
/// and operation as they were. What is kept of it is removed only once it is all back.
pub fn bring_back(repo: &Path, parked: &Parked) -> Result<(), String> {
    if let Some(why) = blocker(repo, parked) {
        return Err(why);
    }
    let dir = folder(repo, &parked.id)?;
    if !dir.join("index").is_file() {
        return Err("What was kept of it is missing".to_owned());
    }
    let current = run(repo, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .map(trimmed);
    match &parked.branch {
        Some(b) if current.as_deref() != Some(b.as_str()) => {
            run(repo, &["checkout", "-q", b])?;
        }
        None if trimmed(run(repo, &["rev-parse", "HEAD"])?) != parked.head => {
            run(repo, &["checkout", "-q", "--detach", &parked.head])?;
        }
        _ => {}
    }
    let gd = git_dir(repo)?;
    let tree = format!("{}^{{tree}}", parked.snapshot);
    run(repo, &["read-tree", "-u", "--reset", &tree])?;
    std::fs::copy(dir.join("index"), gd.join("index"))
        .map_err(|e| format!("couldn't put the index back: {e}"))?;
    for name in &parked.state_files {
        std::fs::copy(dir.join(name), gd.join(name))
            .map_err(|e| format!("couldn't put {name} back: {e}"))?;
    }
    // The index's file times are of then: git looks at the files again.
    let _ = git::run_code(Some(repo), &["update-index", "-q", "--refresh"], &[0, 1]);
    forget(repo, &parked.id);
    Ok(())
}

#[cfg(test)]
mod tests;
