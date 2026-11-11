use super::*;

pub(super) fn marker_sizes(repo: &Path, paths: &[&str]) -> Result<Vec<usize>, String> {
    let input: String = paths.iter().map(|p| format!("{p}\0")).collect();
    let out = git::run_input(
        Some(repo),
        &["check-attr", "-z", "--stdin", "conflict-marker-size"],
        &input,
    )?;
    // `<path>\0conflict-marker-size\0<value>\0` for each, in their order.
    let fields: Vec<&str> = out.split('\0').collect();
    let sizes: Vec<usize> = fields
        .as_chunks::<3>()
        .0
        .iter()
        .map(|[_, _, value]| value.parse().ok().filter(|n| *n > 0).unwrap_or(MARKER_SIZE))
        .collect();
    if sizes.len() != paths.len() {
        return Err("git didn't say how long the conflict markers are".to_owned());
    }
    Ok(sizes)
}

/// The operation `repo` is in the middle of, its commit and how to replay it in a worktree of
/// its own: `merge`, `cherry-pick`, `revert`, or a rebase's commit (picked again).
pub(super) fn in_progress(repo: &Path) -> Option<(&'static str, String, Vec<&'static str>)> {
    let head = |name: &str| {
        git::run(Some(repo), &["rev-parse", "-q", "--verify", name])
            .ok()
            .and_then(|out| out.lines().next().map(|l| l.trim().to_owned()))
            .filter(|h| !h.is_empty())
    };
    if let Some(h) = head("MERGE_HEAD") {
        // Histories without a common commit were let be merged, or there would be no merge.
        let merge = vec![
            "merge",
            "--no-commit",
            "--no-ff",
            "-q",
            "--allow-unrelated-histories",
        ];
        return Some(("merge", h, merge));
    }
    if let Some(h) = head("CHERRY_PICK_HEAD") {
        return Some(("cherry-pick", h, vec!["cherry-pick", "--no-commit"]));
    }
    if let Some(h) = head("REVERT_HEAD") {
        return Some(("revert", h, vec!["revert", "--no-commit"]));
    }
    if let Some(h) = head("REBASE_HEAD") {
        return Some(("rebase", h, vec!["cherry-pick", "--no-commit"]));
    }
    None
}

/// The branch, tag or other name of `commit`, if git knows one.
pub(super) fn name_of(repo: &Path, commit: &str) -> Option<String> {
    git::run(
        Some(repo),
        &["name-rev", "--name-only", "--no-undefined", commit],
    )
    .ok()
    .map(|n| n.trim().to_owned())
    .filter(|n| !n.is_empty())
}

pub(super) fn sides(repo: &Path) -> (Sides, Option<Vec<String>>) {
    let ours = git::run(Some(repo), &["rev-parse", "HEAD"])
        .map(|h| h.trim().to_owned())
        .unwrap_or_default();
    let ours_name = git::run(Some(repo), &["symbolic-ref", "-q", "--short", "HEAD"])
        .map(|b| b.trim().to_owned())
        .unwrap_or_else(|_| "detached HEAD".to_owned());
    let Some((operation, theirs, replay)) = in_progress(repo) else {
        let sides = Sides {
            ours,
            ours_name,
            ..Sides::default()
        };
        return (sides, None);
    };
    let mut replay: Vec<String> = replay.iter().map(|w| (*w).to_owned()).collect();
    if let Some(parent) = (operation != "merge")
        .then(|| mainline(repo, &theirs))
        .flatten()
    {
        replay.extend(["-m".to_owned(), parent]);
    }
    replay.push(theirs.clone());
    let sides = Sides {
        operation: operation.to_owned(),
        theirs_name: name_of(repo, &theirs),
        theirs: Some(theirs),
        ours,
        ours_name,
    };
    (sides, Some(replay))
}

// Whether `path` of the checkout at `top` is reached without a link: none of its folders, nor
// it, is one (an agent could link a file of the checkout to a secret elsewhere, to have it
// applied and committed). `Ok(false)` when it, or a folder on its way, isn't there.
