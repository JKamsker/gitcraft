// Remote regression coverage.

/// A clone of a bare `origin`, with main pushed, and another clone that pushes to it.
struct Remote {
    _origin: TestRepo,
    other: TestRepo,
    here: TestRepo,
}

fn remote(name: &str) -> Result<Remote, String> {
    let origin = TestRepo::new(&format!("{name}-origin"))?;
    origin.git(&["config", "core.bare", "true"])?;
    let url = origin.dir.join(".git").to_string_lossy().into_owned();
    let here = TestRepo::new(name)?;
    here.commit(&[("a", "1\n")], "base")?;
    here.git(&["remote", "add", "origin", &url])?;
    here.git(&["push", "-q", "-u", "origin", "main"])?;
    let other = TestRepo::new(&format!("{name}-other"))?;
    other.git(&["remote", "add", "origin", &url])?;
    other.git(&["fetch", "-q", "origin"])?;
    other.git(&["reset", "-q", "--hard", "origin/main"])?;
    // Fetching with these must still change no ref.
    here.git(&["config", "fetch.prune", "true"])?;
    here.git(&["config", "remote.origin.tagOpt", ""])?;
    Ok(Remote {
        _origin: origin,
        other,
        here,
    })
}

/// A pull's ghost run fetches what the remote has (into no ref) and shows each mode's result:
/// a fast-forward, the merge (with its tree), the rebase; as pulling then does.
#[test]
fn a_pull_ghost_is_what_pulling_does() -> Result<(), String> {
    let r = remote("ghost-pull")?;
    r.other
        .commit(&[("b", "theirs\n")], "pushed by someone else")?;
    r.other.git(&["tag", "v1"])?;
    r.other.git(&["push", "-q", "origin", "main", "v1"])?;
    let theirs = r.other.rev("HEAD")?;
    // Only the remote has the new commit: a hover never asks it (the network could hang or
    // ask for a key), so the preview is of what was last fetched, and says so.
    let unfetched = ghost(&r.here, &GhostOp::Pull(PullMode::FastForwardOnly))?;
    assert!(unfetched.moves.is_empty());
    assert!(unfetched.notes.iter().any(|n| n.contains("last fetch")));
    let missing = r
        .here
        .git(&["cat-file", "-e", &format!("{theirs}^{{commit}}")]);
    assert!(missing.is_err(), "the preview fetched");
    r.here.git(&["fetch", "-q", "origin"])?;
    let ff = ghost(&r.here, &GhostOp::Pull(PullMode::FastForwardOnly))?;
    assert_eq!(moved_to(&ff, "refs/heads/main"), Some(theirs.clone()));
    // Diverged: each mode as it would go.
    r.here.commit(&[("c", "mine\n")], "mine")?;
    let refused = ghost(&r.here, &GhostOp::Pull(PullMode::FastForwardOnly))?;
    assert!(refused.refused.is_some());
    let merged = ghost(&r.here, &GhostOp::Pull(PullMode::Merge))?;
    let rebased = ghost(&r.here, &GhostOp::Pull(PullMode::Rebase))?;
    let old = r.here.rev("HEAD")?;
    r.here.git(&["pull", "-q", "--no-rebase", "--no-edit"])?;
    let merge_tree = r.here.rev("HEAD^{tree}")?;
    r.here.git(&["reset", "-q", "--hard", &old])?;
    r.here.git(&["pull", "-q", "--rebase"])?;
    let rebase_tree = r.here.rev("HEAD^{tree}")?;
    let files: Vec<&str> = merged
        .files
        .iter()
        .flatten()
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(files, ["b"]);
    assert_eq!(merged.created[0].parents, [old.clone(), theirs.clone()]);
    assert_eq!(merged.tree, Some(merge_tree));
    assert_eq!(rebased.tree, Some(rebase_tree.clone()));
    let tip = moved_to(&rebased, "refs/heads/main").ok_or("main doesn't move")?;
    assert_eq!(r.here.rev(&format!("{tip}^{{tree}}"))?, rebase_tree);
    assert_eq!(lost(&rebased), really_lost(&r.here, &old)?);
    Ok(())
}

/// A force push's ghost run lists what leaves the remote's branch, with who made it.
#[test]
fn a_force_push_ghost_lists_what_leaves_the_remote() -> Result<(), String> {
    let r = remote("ghost-push")?;
    r.other.commit(&[("b", "theirs\n")], "pushed by Bo")?;
    r.other.git(&["config", "user.name", "Bo"])?;
    r.other
        .git(&["commit", "-q", "--amend", "--reset-author", "--no-edit"])?;
    r.other.git(&["push", "-q", "origin", "main"])?;
    r.here.git(&["fetch", "-q", "origin"])?;
    let theirs = r.here.rev("origin/main")?;
    r.here.commit(&[("c", "mine\n")], "mine")?;
    let o = ghost(
        &r.here,
        &GhostOp::ForcePush {
            branch: "main".to_owned(),
        },
    )?;
    assert!(o.from_remote);
    assert_eq!(
        moved_to(&o, "refs/remotes/origin/main"),
        Some(r.here.rev("main")?)
    );
    assert_eq!(lost(&o), std::slice::from_ref(&theirs));
    assert_eq!(o.lost[0].author, "Bo");
    r.here.git(&[
        "push",
        "-q",
        &format!("--force-with-lease=refs/heads/main:{theirs}"),
        "origin",
        "main",
    ])?;
    assert_eq!(really_lost(&r.here, &theirs)?, [theirs]);
    Ok(())
}
