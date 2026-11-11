use super::*;

pub(super) fn hookless(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut all = vec![
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "rerere.enabled=false",
    ];
    all.extend_from_slice(args);
    git::run(Some(dir), &all)
}

/// The files the index of the worktree at `dir` has in conflict.
pub(super) fn unmerged(dir: &Path) -> Result<Vec<String>, String> {
    let out = git::run(Some(dir), &["diff", "--name-only", "-z", "--diff-filter=U"])?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Replays the operation in progress (`replay`, see [`sides`]) in the scratch worktree at `dir`.
/// It conflicts there too (that is why it is replayed): what matters is that the rest of it is
/// in place. One git refused outright is an error: the agent would be told of an operation that
/// isn't there.
pub(super) fn replay(dir: &Path, replay: &[String]) -> Result<(), String> {
    let args: Vec<&str> = replay.iter().map(String::as_str).collect();
    match hookless(dir, &args) {
        Ok(_) => Ok(()),
        Err(_) if !unmerged(dir)?.is_empty() => Ok(()),
        Err(e) => Err(format!(
            "The operation couldn't be replayed in the agent's worktree: {e}"
        )),
    }
}

/// The files the replay left in conflict in the scratch worktree at `dir` that the agent isn't
/// given: put there as they are in the checkout at `repo`, the user's resolution included, and
/// marked resolved there when they are in the checkout. The agent sees what the user decided.
pub(super) fn take_resolved(repo: &Path, dir: &Path, given: &[Conflicted]) -> Result<(), String> {
    let still = unmerged(repo)?;
    let mut resolved = Vec::new();
    for path in unmerged(dir)? {
        if given.iter().any(|f| f.path == path) {
            continue;
        }
        // One that isn't a plain file in the checkout stays as the replay left it.
        let Some(bytes) = checked(&path).ok().and_then(|p| read(repo, p).ok()) else {
            continue;
        };
        write(dir, &path, bytes.as_deref())?;
        if !still.contains(&path) {
            resolved.push(path);
        }
    }
    if resolved.is_empty() {
        return Ok(());
    }
    let mut add = vec!["--literal-pathspecs", "add", "-A", "--"];
    add.extend(resolved.iter().map(String::as_str));
    hookless(dir, &add).map(|_| ())
}

/// The parent a cherry-pick or revert of `commit` of `repo` goes by (`-m`), when it is a merge:
/// the one git kept for the sequence in progress, else the first (as nearly always).
pub(super) fn mainline(repo: &Path, commit: &str) -> Option<String> {
    let parents = git::run(Some(repo), &["rev-list", "--parents", "-n", "1", commit]).ok()?;
    if parents.split_whitespace().count() < 3 {
        return None;
    }
    let kept = git::run(Some(repo), &["rev-parse", "--git-path", "sequencer/opts"])
        .ok()
        .and_then(|opts| {
            git::run(
                Some(repo),
                &["config", "-f", opts.trim(), "--get", "options.mainline"],
            )
            .ok()
        })
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()));
    Some(kept.unwrap_or_else(|| "1".to_owned()))
}
