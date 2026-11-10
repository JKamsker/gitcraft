use super::*;

#[test]
fn nothing_to_offer_in_a_new_repository() -> Result<(), String> {
    let s = Scratch::new("cleanup-empty");
    git_ok(&s.0, &["init", "-q", "-b", "main"])?;
    let tab = tab_on(&s.0, Vec::new());
    assert!(tab.cleanup_candidates(1).is_empty());
    assert!(tab.branch_health.health.is_some());
    Ok(())
}

/// A branch being bisected (or rebased) in another worktree shows there as detached, so it is
/// offered; git refuses to delete it. That is found before anything is deleted: the other
/// ticked branches, here and on the remote, stay, rather than going with no undo.
#[test]
fn a_branch_in_use_in_a_worktree_stops_everything_before_it_starts() -> Result<(), String> {
    let f = fixture("cleanup-bisect")?;
    let wt = f.dir.with_file_name("bisecting");
    git_ok(&f.dir, &["branch", "busy", "main~1"])?;
    git_ok(
        &f.dir,
        &["worktree", "add", "-q", &wt.to_string_lossy(), "busy"],
    )?;
    git_ok(&wt, &["bisect", "start", "HEAD", "HEAD~2"])?;
    let tab = tab_on(&f.dir, Vec::new());
    let found = tab.cleanup_candidates(0);
    assert!(
        names(&found).contains(&"busy"),
        "shown detached, it is offered"
    );
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, _) = delete_many(&chosen, true);
    let (done, _) = run_steps(&f.dir, &steps, OpKind::plain(), None);
    assert!(done.is_err());
    for c in &found {
        assert!(
            git::resolve(&f.dir, &format!("refs/heads/{}", c.name)).is_ok(),
            "{}",
            c.name
        );
    }
    assert!(git::resolve(&f.remote, "refs/heads/fix/ünïcode-✓").is_ok());
    Ok(())
}

/// A branch made from a shared one (`checkout -b fix origin/release`) tracks it: deleting the
/// branch "on the remote" must not delete `release`. Only a remote branch of the same name, and
/// never the default branch, goes along.
#[test]
fn only_a_remote_branch_of_the_same_name_goes_along() -> Result<(), String> {
    let f = fixture("cleanup-shared")?;
    git_ok(&f.dir, &["push", "-q", "origin", "main:refs/heads/release"])?;
    git_ok(&f.dir, &["fetch", "-q", "origin"])?;
    git_ok(
        &f.dir,
        &["branch", "-q", "--track", "from-release", "origin/release"],
    )?;
    git_ok(
        &f.dir,
        &["branch", "-q", "--track", "from-main", "origin/main"],
    )?;
    let tab = tab_on(&f.dir, Vec::new());
    let found = tab.cleanup_candidates(0);
    for name in ["from-release", "from-main"] {
        let c = found.iter().find(|c| c.name == name).ok_or(name)?;
        assert_eq!(c.remote, None, "{name}");
        assert!(c.preselected(), "{name} is merged");
    }
    let chosen: Vec<&Candidate> = found.iter().collect();
    let (steps, _) = delete_many(&chosen, true);
    let (done, _) = run_steps(&f.dir, &steps, OpKind::plain(), None);
    assert!(done.is_ok(), "{done:?}");
    assert!(git::resolve(&f.remote, "refs/heads/release").is_ok());
    assert!(git::resolve(&f.remote, "refs/heads/main").is_ok());
    assert!(git::resolve(&f.remote, "refs/heads/fix/ünïcode-✓").is_err());
    Ok(())
}

/// A branch found merged, then given a commit: until a read sees the commit, it is not offered
/// as merged on the strength of the old read.
#[test]
fn a_branch_that_moved_since_the_read_is_not_merged() -> Result<(), String> {
    let f = fixture("cleanup-moved-read")?;
    let mut tab = tab_on(&f.dir, Vec::new());
    git_ok(&f.dir, &["checkout", "-q", "rebased"])?;
    commit(&f.dir, "new.txt", "new work")?;
    git_ok(&f.dir, &["checkout", "-q", "main"])?;
    // The tab's refs are read again; the health read is the old one still.
    tab.refs = git::refs(&f.dir)?;
    assert!(!names(&tab.cleanup_candidates(0)).contains(&"rebased"));
    Ok(())
}

/// The second read of an unchanged repository asks `git cherry` nothing new: its answers are
/// kept by branch and base commit.
#[test]
fn cherry_answers_are_kept_between_reads() -> Result<(), String> {
    let f = fixture("cleanup-cache")?;
    let tab = tab_on(&f.dir, Vec::new());
    let cached = tab.branch_health.cache.clone();
    assert!(
        cached.values().any(|all_in| *all_in),
        "rebased is cached as in"
    );
    // Answers that say the opposite of git: a read that used them shows it.
    let mut lying: branch_health::CherryCache = cached.into_iter().map(|(k, v)| (k, !v)).collect();
    let bases = tab.health_bases();
    let inputs = Inputs::new(&tab.refs.local, bases, &[]);
    let read = branch_health::read(&f.dir, &inputs, &mut lying)?;
    assert!(!read.merged.contains_key("rebased"));
    Ok(())
}
