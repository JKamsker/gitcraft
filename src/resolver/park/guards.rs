use super::*;

/// Why the worktree is in the middle of something a merge can't be brought back into, if it is.
pub(super) fn busy_with(gd: &Path) -> Option<&'static str> {
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
pub(super) fn operation(repo: &Path, gd: &Path) -> Result<(&'static str, String), String> {
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
pub(super) fn sparse(repo: &Path) -> bool {
    run(repo, &["config", "--bool", "core.sparseCheckout"]).is_ok_and(|v| v.trim() == "true")
}

/// Why the worktree's state can't be kept exactly, if it can't: files git is told not to look
/// at (assume-unchanged, or left out of a sparse checkout), whose changes would be lost, and
/// submodules changed (or with changes of their own), whose own worktrees aren't kept.
pub(super) fn unkeepable(repo: &Path) -> Option<String> {
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
