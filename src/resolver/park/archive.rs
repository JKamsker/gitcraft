use super::*;

/// Writes all that is kept of the operation in progress into `dir` (made for it): its index
/// (first, the files' trees are made from that copy), the files' commit and its ref, the
/// changed files' bytes, the operation's files, and last what is known of it (a parked merge
/// is listed once it is whole).
pub(super) fn keep(
    repo: &Path,
    gd: &Path,
    dir: &Path,
    id: &str,
    operation: &str,
    theirs: &str,
    files: &[String],
) -> Result<Parked, String> {
    durability::copy(&gd.join("index"), &dir.join("index"), dir, "index")
        .map_err(|e| format!("couldn't keep the index: {e}"))?;
    effects::boundary("archive-index-copied")?;
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
            durability::copy(&file, &to, dir, &format!("raw/{path}"))
                .map_err(|e| format!("couldn't keep {path}: {e}"))?;
            effects::boundary("archive-raw-copied")?;
            parked.raw.push(path);
        }
    }
    for name in STATE_FILES {
        let from = gd.join(name);
        if from.is_file() {
            durability::copy(&from, &dir.join(name), dir, name)
                .map_err(|e| format!("couldn't keep {name}: {e}"))?;
            effects::boundary("archive-metadata-copied")?;
            parked.state_files.push(name.to_owned());
        }
    }
    let text = serde_json::to_string_pretty(&parked).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("parked.json"), text)
        .map_err(|e| format!("couldn't write what was kept: {e}"))?;
    effects::boundary("archive-manifest-written")?;
    Ok(parked)
}

/// The tracked files whose bytes are kept, including filtered files that appear clean.
/// Only regular files are copied; Git retains symlinks in the snapshot tree.
pub(super) fn raw_paths(
    repo: &Path,
    parked: &Parked,
    files: &[String],
) -> Result<Vec<String>, String> {
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
