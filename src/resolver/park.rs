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
//! Nothing of the user's is lost on the way, whatever fails: nothing in the worktree changes
//! before all of it is written and checked against the worktree once more; what git can't keep
//! exactly (files marked assume-unchanged or left out of a sparse checkout, submodules changed)
//! isn't put aside at all; neither putting it aside nor bringing it back writes over a file git
//! doesn't track (it stops, and says which), and when anything fails half way the worktree
//! goes back to its branch, clean, and the merge stays put aside to try again. Files git
//! doesn't track stay where they are throughout.

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

/// Why the worktree is in the middle of something a merge can't be brought back into, if it is.
fn busy_with(gd: &Path) -> Option<&'static str> {
    let any = |names: &[&str]| names.iter().any(|n| gd.join(n).exists());
    if any(&["rebase-merge", "rebase-apply"]) {
        Some("a rebase")
    } else if any(&["sequencer"]) {
        Some("a cherry-pick or revert of several commits")
    } else if any(&["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD"]) {
        Some("a merge, cherry-pick or revert")
    } else if any(&["BISECT_LOG"]) {
        Some("a bisect")
    } else {
        None
    }
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

/// Whether sparse checkout is on: it would leave the merge's files out on the way back.
fn sparse(repo: &Path) -> bool {
    run(repo, &["config", "--bool", "core.sparseCheckout"]).is_ok_and(|v| v.trim() == "true")
}

/// Why the worktree's state can't be kept exactly, if it can't: files git is told not to look
/// at (assume-unchanged, or left out of a sparse checkout), whose changes would be lost, and
/// submodules changed (or with changes of their own), whose own worktrees aren't kept.
fn unkeepable(repo: &Path) -> Option<String> {
    if sparse(repo) {
        return Some(
            "A merge in a sparse checkout can't be put aside safely: resolve it here".to_owned(),
        );
    }
    let listed = match run(repo, &["ls-files", "-v", "-z"]) {
        Ok(listed) => listed,
        Err(e) => return Some(format!("Could not check index flags: {e}")),
    };
    let hidden: Vec<&str> = listed
        .split('\0')
        .filter(|l| l.starts_with(|c: char| c.is_ascii_lowercase() || c == 'S'))
        .filter_map(|l| l.get(2..))
        .take(3)
        .collect();
    if !hidden.is_empty() {
        return Some(format!(
            "Files git is told not to look at (assume-unchanged or sparse), such as {}, can't \
             be put aside safely: resolve it here",
            hidden.join(", ")
        ));
    }
    // A gitlink's mode among the fields before the path (not in a file's name or an id).
    let submodule = |out: Result<String, String>| {
        out.is_ok_and(|o| {
            o.lines().any(|l| {
                l.split('\t').next().is_some_and(|fields| {
                    fields
                        .split_whitespace()
                        .any(|f| f.trim_start_matches(':') == "160000")
                })
            })
        })
    };
    let all = "--ignore-submodules=none";
    if submodule(run(repo, &["ls-files", "-u"]))
        || submodule(run(repo, &["diff-index", "--cached", all, "HEAD"]))
        || submodule(run(repo, &["diff-files", all]))
    {
        return Some(
            "A merge with submodules changed (or changes in one) can't be put aside: resolve it \
             here"
                .into(),
        );
    }
    None
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
        && raw_unchanged(repo, dir)
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
    let kept = keep(repo, &gd, &dir, id, operation, &theirs, files);
    let parked = match kept {
        Ok(parked) => parked,
        Err(e) => {
            forget(repo, id);
            return Err(e);
        }
    };
    // Last look: the user (an editor saving) changed nothing since it was kept.
    if !unchanged(repo, &gd, &dir, &parked.snapshot) {
        forget(repo, id);
        return Err("Files changed while the merge was put aside: try again".to_owned());
    }
    // The reset writes the branch's files back over whatever is in their way: one the merge
    // deleted that is on disk again, untracked or ignored, stops it.
    match in_the_way(repo, &parked.snapshot, &parked.head) {
        Ok(None) => {}
        Ok(Some(path)) => {
            forget(repo, id);
            return Err(format!(
                "{path} isn't tracked, but its branch has it: move it, or resolve the merge here"
            ));
        }
        Err(e) => {
            forget(repo, id);
            return Err(e);
        }
    }
    // All of it is kept: now the worktree is cleared.
    if let Err(e) = run(repo, &["reset", "--hard", "-q", "HEAD"]) {
        // Nothing changed: nothing need be kept. Anything did: it stays put aside, whole.
        if unchanged(repo, &gd, &dir, &parked.snapshot) {
            forget(repo, id);
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
    std::fs::write(dir.join("clean-baseline.json"), bytes).map_err(|e| e.to_string())?;
    Ok(parked)
}

#[derive(Serialize, Deserialize)]
struct CleanFile {
    path: String,
    object: String,
    bytes: Vec<u8>,
}

fn head_files(repo: &Path) -> Result<Vec<(String, String)>, String> {
    run(repo, &["ls-tree", "-r", "-z", "HEAD"])?
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let (fields, path) = entry.split_once('\t')?;
            let mut fields = fields.split_whitespace();
            let mode = fields.next()?;
            if !matches!(mode, "100644" | "100755") {
                return None;
            }
            fields.next()?;
            Some(
                crate::mcp::args::repo_path(path)
                    .map(|path| (path.to_owned(), fields.next().unwrap_or("").to_owned())),
            )
        })
        .collect()
}

fn clean_baseline(repo: &Path) -> Result<Vec<CleanFile>, String> {
    head_files(repo)?
        .into_iter()
        .map(|(path, object)| {
            let bytes =
                crate::safe_read::read_inside(repo, &path, u64::MAX - 1).ok_or_else(|| {
                    format!("Could not record clean bytes of {path}; merge remains put aside")
                })?;
            Ok(CleanFile {
                path,
                object,
                bytes,
            })
        })
        .collect()
}

/// Git status compares filtered content. Prove the raw bytes safe before any checkout,
/// read-tree, rollback or raw archive write can overwrite a clean-filter-hidden edit.
fn clean_bytes_guard(repo: &Path, parked: &Parked) -> Result<(), String> {
    let baseline: Vec<CleanFile> =
        std::fs::read(folder(repo, &parked.id)?.join("clean-baseline.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
    for (path, object) in head_files(repo)? {
        let actual = crate::safe_read::read_inside(repo, &path, u64::MAX - 1);
        let expected = match baseline
            .iter()
            .find(|f| f.path == path && f.object == object)
        {
            Some(file) => file.bytes.clone(),
            None => git::run_bytes(
                Some(repo),
                &["cat-file", "--filters", &format!("HEAD:{path}")],
            )?,
        };
        if actual.as_deref() != Some(expected.as_slice()) {
            return Err(format!(
                "{path} has raw bytes changed outside Git's filtered status; save them elsewhere before bringing the merge back"
            ));
        }
    }
    Ok(())
}

/// Writes all that is kept of the operation in progress into `dir` (made for it): its index
/// (first, the files' trees are made from that copy), the files' commit and its ref, the
/// changed files' bytes, the operation's files, and last what is known of it (a parked merge
/// is listed once it is whole).
fn keep(
    repo: &Path,
    gd: &Path,
    dir: &Path,
    id: &str,
    operation: &str,
    theirs: &str,
    files: &[String],
) -> Result<Parked, String> {
    std::fs::copy(gd.join("index"), dir.join("index"))
        .map_err(|e| format!("couldn't keep the index: {e}"))?;
    let (staged, tree) = trees(repo, &dir.join("index"))?;
    let head = trimmed(run(repo, &["rev-parse", "--verify", "HEAD"])?);
    let branch = run(repo, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .map(trimmed)
        .filter(|b| !b.is_empty());
    let theirs_name = run(repo, &["name-rev", "--name-only", "--no-undefined", theirs])
        .ok()
        .map(trimmed)
        .filter(|n| !n.is_empty());
    let message = format!(
        "Parked {operation} of {}: being resolved by an agent",
        theirs_name.as_deref().unwrap_or(theirs)
    );
    let env = [
        ("GIT_AUTHOR_NAME", "gitcraft"),
        ("GIT_AUTHOR_EMAIL", "gitcraft@localhost"),
        ("GIT_COMMITTER_NAME", "gitcraft"),
        ("GIT_COMMITTER_EMAIL", "gitcraft@localhost"),
    ];
    let commit = |args: &[&str]| git::run_env(Some(repo), args, &env).map(trimmed);
    // What was staged, when the files differ from it: a parent of the files' commit, kept
    // from garbage collection meanwhile and on the branch the merge is dropped to.
    let staged = if staged == tree {
        None
    } else {
        let what = format!("Staged when the {operation} was parked");
        Some(commit(&["commit-tree", &staged, "-p", &head, "-m", &what])?)
    };
    let mut args = vec!["commit-tree", &tree, "-p", &head, "-p", theirs];
    if let Some(staged) = &staged {
        args.extend(["-p", staged]);
    }
    args.extend(["-m", &message]);
    let snapshot = commit(&args)?;
    let mut parked = Parked {
        id: id.to_owned(),
        worktree: trimmed(run(repo, &["rev-parse", "--show-toplevel"])?),
        operation: operation.to_owned(),
        branch,
        head,
        theirs: theirs.to_owned(),
        theirs_name,
        snapshot,
        files: files.to_vec(),
        unmerged: unmerged(repo)?,
        raw: Vec::new(),
        state_files: Vec::new(),
        at: crate::git::now(),
    };
    run(
        repo,
        &["update-ref", &parked.ref_name(), &parked.snapshot, ""],
    )?;
    // The changed files as they are on disk: a filter (line endings, LFS, one cleaning
    // notebooks) going through git both ways may not give them back byte for byte.
    for path in raw_paths(repo, &parked, files)? {
        let file = repo.join(&path);
        let plain = std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_file());
        if plain {
            let to = dir.join("raw").join(&path);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&file, &to).map_err(|e| format!("couldn't keep {path}: {e}"))?;
            parked.raw.push(path);
        }
    }
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

/// The tracked files whose bytes are kept, including filtered files that appear clean.
/// Only regular files are copied; Git retains symlinks in the snapshot tree.
fn raw_paths(repo: &Path, parked: &Parked, files: &[String]) -> Result<Vec<String>, String> {
    let args = [
        "diff-tree",
        "-r",
        "-z",
        "--name-only",
        "--no-renames",
        "--diff-filter=d",
        &parked.head,
        &parked.snapshot,
    ];
    let changed = run(repo, &args)?;
    let mut paths = files
        .iter()
        .map(|p| crate::mcp::args::repo_path(p).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    paths.extend(
        changed
            .split('\0')
            .filter(|p| !p.is_empty() && crate::mcp::args::repo_path(p).is_ok())
            .map(str::to_owned),
    );
    // A clean filter can hide edits even in files whose filtered tree equals HEAD.
    paths.extend(
        run(repo, &["ls-files", "--cached", "-z"])?
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(|p| crate::mcp::args::repo_path(p).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?,
    );
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
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
    if let Some(what) = busy_with(&gd) {
        return Some(format!("Finish or abort {what} in progress first"));
    }
    if sparse(repo) {
        return Some(
            "Turn sparse checkout off first (git sparse-checkout disable): it would leave the \
             merge's files out"
                .to_owned(),
        );
    }
    if let Some(why) = unkeepable(repo) {
        return Some(why);
    }
    let dirty = run(repo, &["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(true);
    if dirty {
        return Some("Commit or stash your changes first".to_owned());
    }
    if let Err(why) = clean_bytes_guard(repo, parked) {
        return Some(why);
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
            "{} isn't there any more: drop it to keep its files on a branch",
            parked.branch.as_deref().unwrap_or("its branch")
        )),
        Some(tip) if tip != parked.head => Some(format!(
            "{} moved since it was put aside: put it back at {}, or drop it to keep its files \
             on a branch",
            parked.branch.as_deref().unwrap_or("HEAD"),
            parked.head.get(..8).unwrap_or(&parked.head)
        )),
        Some(_) => None,
    }
}

/// Brings `parked` back into the worktree at `repo`: its branch checked out, its files, index
/// and operation as they were. Files git doesn't track are never written over: one in the way
/// stops it before anything changes. Anything failing after that puts the worktree back on its
/// branch, clean, and the merge stays put aside. What is kept of it is removed only once it is
/// all back and checked.
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
    // Ignored files where the branch's go stop it too: git would write over those.
    let checkout = ["checkout", "-q", "--no-overwrite-ignore"];
    match &parked.branch {
        Some(b) if current.as_deref() != Some(b.as_str()) => {
            run(repo, &[&checkout[..], &[b.as_str()]].concat())?;
        }
        // Detached it was, detached it is again, even where a branch is at its commit.
        None if current.is_some() || trimmed(run(repo, &["rev-parse", "HEAD"])?) != parked.head => {
            run(repo, &[&checkout[..], &["--detach", &parked.head]].concat())?;
        }
        _ => {}
    }
    // Anything on disk where a file of the merge goes that the branch doesn't have, ignored
    // files included (git would write over those): it stops here, nothing written.
    if let Some(path) = in_the_way(repo, &parked.head, &parked.snapshot)? {
        return Err(format!(
            "Files git doesn't track are where the merge's files go ({path}): move them, then \
             bring it back"
        ));
    }
    // From its commit to its files, as a merge of the two: git stops too, writing nothing,
    // when a file it doesn't track is in the way.
    let tree = format!("{}^{{tree}}", parked.snapshot);
    if let Err(e) = run(repo, &["read-tree", "-m", "-u", &parked.head, &tree]) {
        // Whatever git wrote before it stopped goes: the worktree was clean, and files git
        // doesn't track stay as they are.
        let cleaned = match run(repo, &["reset", "--hard", "-q", "HEAD"]) {
            Ok(_) => String::new(),
            Err(r) => format!("; the worktree couldn't be cleaned either ({})", r.trim()),
        };
        let why = match e.lines().find(|l| l.contains("would be")) {
            Some(l) => format!(
                "Files git doesn't track are where the merge's files go: move them, then bring \
                 it back ({})",
                l.trim()
            ),
            None => format!("It couldn't be brought back ({})", e.trim()),
        };
        return Err(format!("{why}{cleaned}; it is still put aside"));
    }
    let gd = git_dir(repo)?;
    match restore(repo, &gd, &dir, parked) {
        Ok(()) => {
            forget(repo, &parked.id);
            Ok(())
        }
        Err(e) => {
            // Back to the branch, clean: the merge is still put aside, whole, to try again.
            for name in &parked.state_files {
                let _ = std::fs::remove_file(gd.join(name));
            }
            let _ = run(repo, &["reset", "--hard", "-q", "HEAD"]);
            Err(format!(
                "It couldn't be brought back ({e}); it is still put aside"
            ))
        }
    }
}

/// The first file or folder on disk where a file of `to` that `from` hasn't goes (or a file
/// where one of its folders goes), if one is. The worktree being `from`'s, whatever is there
/// is the user's, untracked or ignored, but for a file of `from`'s itself (where `to` has a
/// folder): git replaces that.
fn in_the_way(repo: &Path, from: &str, to: &str) -> Result<Option<String>, String> {
    let added = run(
        repo,
        &[
            "diff-tree",
            "-r",
            "-z",
            "--name-only",
            "--no-renames",
            "--diff-filter=A",
            from,
            to,
        ],
    )?;
    for path in added.split('\0').filter(|p| !p.is_empty()) {
        let mut at = repo.to_path_buf();
        let parts: Vec<&str> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            at.push(part);
            let Ok(meta) = std::fs::symlink_metadata(&at) else {
                break;
            };
            let there = parts[..=i].join("/");
            // The file itself there (anything), or a non-folder where a folder of it goes.
            if i + 1 == parts.len() {
                // A folder of tracked files only (where `to` has a file): git replaces it.
                let others = ["ls-files", "-o", "-z", "--", &format!(":(literal){there}")];
                if meta.is_dir() && run(repo, &others).is_ok_and(|o| o.is_empty()) {
                    break;
                }
                return Ok(Some(there));
            }
            if !meta.is_dir() {
                let tracked = run(repo, &["cat-file", "-e", &format!("{from}:{there}")]).is_ok();
                if tracked {
                    break;
                }
                return Ok(Some(there));
            }
        }
    }
    Ok(None)
}

/// Puts the kept index, the operation's files and the changed files' bytes back, and
/// checks that the conflicts are all there.
fn restore(repo: &Path, gd: &Path, dir: &Path, parked: &Parked) -> Result<(), String> {
    // The index through its lock, as git writes it: no git running meanwhile reads half of it.
    let lock = gd.join("index.lock");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|_| "git is busy in this worktree (its index is locked)".to_owned())?;
    let bytes = std::fs::read(dir.join("index")).map_err(|e| format!("the index kept: {e}"));
    let written = bytes.and_then(|b| file.write_all(&b).map_err(|e| e.to_string()));
    drop(file);
    let moved = written.and_then(|()| {
        std::fs::rename(&lock, gd.join("index")).map_err(|e| format!("the index: {e}"))
    });
    if let Err(e) = moved {
        let _ = std::fs::remove_file(&lock);
        return Err(e);
    }
    for name in &parked.state_files {
        std::fs::copy(dir.join(name), gd.join(name)).map_err(|e| format!("{name}: {e}"))?;
    }
    for path in &parked.raw {
        let path = crate::mcp::args::repo_path(path)?;
        std::fs::copy(dir.join("raw").join(path), repo.join(path))
            .map_err(|e| format!("{path}: {e}"))?;
    }
    // The index's file times are of then: git looks at the files again.
    let _ = git::run_code(Some(repo), &["update-index", "-q", "--refresh"], &[0, 1]);
    let back = unmerged(repo)?;
    if back != parked.unmerged {
        return Err(format!(
            "{back} conflicted entries came back of {}",
            parked.unmerged
        ));
    }
    run(repo, &["status", "--porcelain"]).map(|_| ())
}

/// Drops `parked` without bringing it back, its files kept on a branch of their own (named
/// after it): nothing of it is lost. The branch's name.
pub fn drop_to_branch(repo: &Path, parked: &Parked) -> Result<String, String> {
    let short = parked.id.get(..9).unwrap_or(&parked.id);
    let mut name = format!("parked/{}-{short}", parked.operation);
    let mut n = 2;
    while run(
        repo,
        &["rev-parse", "-q", "--verify", &format!("refs/heads/{name}")],
    )
    .is_ok()
    {
        name = format!("parked/{}-{short}-{n}", parked.operation);
        n += 1;
    }
    run(repo, &["branch", &name, &parked.snapshot])?;
    // A branch contains Git's filtered tree, not necessarily the original bytes.
    // Keep the complete raw archive outside the parked-list namespace before forgetting.
    let archive = crate::agent::repo_file(repo, &format!("parked-recovery/{}", parked.id))?;
    reserve(&archive)?;
    if let Err(e) = std::fs::rename(folder(repo, &parked.id)?, archive.join("parked")) {
        let _ = std::fs::remove_dir(&archive);
        return Err(format!(
            "The branch {name} exists, but raw recovery could not be archived: {e}"
        ));
    }
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
    forget(repo, &parked.id);
    Ok(name)
}

#[cfg(test)]
mod tests;
