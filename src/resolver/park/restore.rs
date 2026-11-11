use super::*;

/// Why `parked` can't be brought back into the worktree at `repo` as it is now, if it can't.
#[allow(dead_code)] // Preserve the preflight API for callers and recovery fixtures.
pub fn blocker(repo: &Path, parked: &Parked) -> Option<String> {
    let _lease = match locking::acquire(repo, &parked.id) {
        Ok(lease) => lease,
        Err(e) => return Some(e),
    };
    blocker_locked(repo, parked)
}

fn blocker_locked(repo: &Path, parked: &Parked) -> Option<String> {
    let dir = discovery::locate(repo, &parked.id).ok()?;
    if matches!(
        journal::latest(&dir),
        Ok(Some(
            journal::Phase::Restoring
                | journal::Phase::Restored
                | journal::Phase::Cleanup
                | journal::Phase::Uncertain
        ))
    ) {
        return None; // bring_back performs byte-bound restart checks before writing.
    }
    blocker_clean(repo, parked)
}

fn blocker_clean(repo: &Path, parked: &Parked) -> Option<String> {
    let dir = match discovery::locate(repo, &parked.id) {
        Ok(dir) => dir,
        Err(e) => return Some(e),
    };
    if let Err(e) = journal::latest(&dir) {
        return Some(format!("{e}; recovery archive: {}", dir.display()));
    }
    let gd = git_dir(repo).ok()?;
    if gd.join("index.lock").exists() {
        return Some(format!(
            "Git index is locked; {}",
            transaction::guidance(&dir)
        ));
    }
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
/// stops it before anything changes. Partial failures retain a byte-bound copy plan or
/// require explicit recovery; no automatic reset overwrites uncertain work.
pub fn bring_back(repo: &Path, parked: &Parked) -> Result<(), String> {
    let _lease = locking::acquire(repo, &parked.id)?;
    let dir = discovery::locate(repo, &parked.id)?;
    transaction::identity(&dir, parked)?;
    transaction::validate_required(&dir, parked)?;
    let gd = git_dir(repo)?;
    let phase = journal::latest(&dir)?;
    if matches!(
        phase,
        Some(
            journal::Phase::Restoring
                | journal::Phase::Restored
                | journal::Phase::Cleanup
                | journal::Phase::Uncertain
        )
    ) {
        if verified(repo, &gd, &dir, parked)? {
            sync_live(repo, &gd, parked)?;
            journal::record(&dir, journal::Phase::Restored)?;
            return journal::retire(repo, &parked.id);
        }
        if dir.join("restore-plan.json").is_file() {
            transaction::resume(repo, &gd, &dir, parked)?;
            if !verified(repo, &gd, &dir, parked)? {
                return Err(transaction::guidance(&dir));
            }
            sync_live(repo, &gd, parked)?;
            journal::record(&dir, journal::Phase::Restored)?;
            return journal::retire(repo, &parked.id);
        }
        // No copy plan: only a provably clean worktree may retry the Git step.
        if blocker_clean(repo, parked).is_some() {
            return Err(transaction::guidance(&dir));
        }
    }
    if let Some(why) = blocker_locked(repo, parked) {
        return Err(why);
    }
    let dir = discovery::locate(repo, &parked.id)?;
    if !dir.join("index").is_file() {
        return Err("What was kept of it is missing".to_owned());
    }
    let current = run(repo, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .map(trimmed);
    durability::capture_untouched(repo, &gd, &dir, parked)?;
    // Ignored files where the branch's go stop it too: git would write over those.
    journal::record(&dir, journal::Phase::Restoring)?;
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
        return Err(format!(
            "Git restore interrupted ({e}); {}",
            transaction::guidance(&dir)
        ));
    }
    let gd = git_dir(repo)?;
    match restore(repo, &gd, &dir, parked) {
        Ok(()) => {
            journal::record(&dir, journal::Phase::Restored)?;
            journal::record(&dir, journal::Phase::Cleanup)?;
            journal::retire(repo, &parked.id)
        }
        Err(e) => Err(format!("{e}; {}", transaction::guidance(&dir))),
    }
}

/// The first file or folder on disk where a file of `to` that `from` hasn't goes (or a file
/// where one of its folders goes), if one is. The worktree being `from`'s, whatever is there
/// is the user's, untracked or ignored, but for a file of `from`'s itself (where `to` has a
/// folder): git replaces that.
pub(super) fn in_the_way(repo: &Path, from: &str, to: &str) -> Result<Option<String>, String> {
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
pub(super) fn restore(repo: &Path, gd: &Path, dir: &Path, parked: &Parked) -> Result<(), String> {
    transaction::prepare(repo, gd, dir, parked)?;
    transaction::resume(repo, gd, dir, parked)?;
    if !verified(repo, gd, dir, parked)? {
        return Err(transaction::guidance(dir));
    }
    sync_live(repo, gd, parked)
}

pub(super) fn original_checkout(repo: &Path, parked: &Parked) -> Result<bool, String> {
    let branch = run(repo, &["symbolic-ref", "-q", "--short", "HEAD"])
        .ok()
        .map(trimmed);
    Ok(branch == parked.branch && trimmed(run(repo, &["rev-parse", "HEAD"])?) == parked.head)
}

pub(super) fn verified(
    repo: &Path,
    gd: &Path,
    dir: &Path,
    parked: &Parked,
) -> Result<bool, String> {
    transaction::identity(dir, parked)?;
    Ok(![
        "rebase-merge",
        "rebase-apply",
        "sequencer",
        "BISECT_LOG",
        "index.lock",
    ]
    .iter()
    .any(|p| gd.join(p).exists())
        && original_checkout(repo, parked)?
        && unchanged(repo, gd, dir, &parked.snapshot)
        && durability::restored_attributes(repo, gd, dir, parked)?)
}

pub(super) fn sync_live(repo: &Path, gd: &Path, parked: &Parked) -> Result<(), String> {
    let dir = discovery::locate(repo, &parked.id)?;
    let tracked = run(repo, &["ls-files", "--cached", "-z"])?;
    let changed = run(
        repo,
        &[
            "diff-tree",
            "-r",
            "-z",
            "--name-only",
            &parked.head,
            &parked.snapshot,
        ],
    )?;
    let paths = tracked
        .split('\0')
        .chain(changed.split('\0'))
        .chain(parked.raw.iter().map(String::as_str));
    for path in paths.filter(|p| !p.is_empty()) {
        let path = repo.join(crate::mcp::args::repo_path(path)?);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file()) {
            let relative = path.strip_prefix(repo).map_err(|e| e.to_string())?;
            let key = format!("live/false/{}", relative.to_string_lossy());
            durability::sync_live_file(&dir, &key, &path)?;
        }
        let mut parent = path.parent().ok_or("Missing parent")?;
        while !parent.is_dir() {
            parent = parent.parent().ok_or("Missing parent")?;
        }
        journal::sync_ancestors(parent)?;
    }
    for name in std::iter::once("index").chain(parked.state_files.iter().map(String::as_str)) {
        durability::sync_live_file(&dir, &format!("live/true/{name}"), &gd.join(name))?;
    }
    journal::sync_ancestors(gd)?;
    effects::boundary("live-restore-synced")
}
