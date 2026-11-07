//! Telling a rewrite from a move forward, and keeping (and letting go of) both versions.

use super::*;

/// A repository in a temporary folder, removed when dropped.
struct Repo(std::path::PathBuf);

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-rw-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        run(Some(&self.0), args).map(|s| s.trim().to_owned())
    }

    fn commit(&self, message: &str) -> Result<String, String> {
        std::fs::write(self.0.join("f.txt"), message).map_err(|e| e.to_string())?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "-m", message])?;
        self.git(&["rev-parse", "HEAD"])
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn moving_forward_is_no_rewrite() -> Result<(), String> {
    let repo = Repo::new("forward")?;
    let a = repo.commit("a")?;
    let b = repo.commit("b")?;
    assert_eq!(rewritten(&repo.0, &a, &b, None)?, None);
    assert_eq!(rewritten(&repo.0, &b, &b, None)?, None);
    assert_eq!(rewritten(&repo.0, "", &b, None)?, None);
    Ok(())
}

#[test]
fn an_amend_is_a_rewrite_from_its_parent() -> Result<(), String> {
    let repo = Repo::new("amend")?;
    let a = repo.commit("a")?;
    let b = repo.commit("b")?;
    repo.git(&["commit", "-q", "--amend", "-m", "b, better"])?;
    let amended = repo.git(&["rev-parse", "HEAD"])?;
    let found = rewritten(&repo.0, &b, &amended, None)?;
    assert_eq!(
        found,
        Some((Series::new(&a, &b), Series::new(&a, &amended)))
    );
    Ok(())
}

#[test]
fn a_rebase_starts_the_new_series_at_what_it_went_onto() -> Result<(), String> {
    let repo = Repo::new("onto")?;
    let base = repo.commit("base")?;
    repo.commit("main moves")?;
    let main = repo.git(&["rev-parse", "HEAD"])?;
    repo.git(&["checkout", "-q", "-b", "topic", &base])?;
    std::fs::write(repo.0.join("t.txt"), "t").map_err(|e| e.to_string())?;
    repo.git(&["add", "-A"])?;
    repo.git(&["commit", "-q", "-m", "topic"])?;
    let before = repo.git(&["rev-parse", "HEAD"])?;
    repo.git(&["rebase", "-q", "main"])?;
    let after = repo.git(&["rev-parse", "HEAD"])?;
    // Not from where the two meet, which would count main's commit as added.
    let found = rewritten(&repo.0, &before, &after, Some("refs/heads/main"))?;
    assert_eq!(
        found,
        Some((Series::new(&base, &before), Series::new(&main, &after)))
    );
    let without = rewritten(&repo.0, &before, &after, None)?;
    assert_eq!(
        without,
        Some((Series::new(&base, &before), Series::new(&base, &after)))
    );
    Ok(())
}

#[test]
fn both_versions_are_kept_listed_and_let_go_of() -> Result<(), String> {
    let repo = Repo::new("keep")?;
    let a = repo.commit("a")?;
    let b = repo.commit("b")?;
    repo.git(&["commit", "-q", "--amend", "-m", "b2"])?;
    let b2 = repo.git(&["rev-parse", "HEAD"])?;
    let (old, new) = (Series::new(&a, &b), Series::new(&a, &b2));
    let first = record_rewrite(&repo.0, "refs/heads/main", &old, &new, 1_000)?;
    // The same second again: a rewrite of its own.
    let second = record_rewrite(&repo.0, "main", &old, &new, 1_000)?;
    let detached = record_rewrite(&repo.0, "", &old, &new, 500)?;
    assert_eq!((first.time, second.time), (1_000, 1_001));
    assert_eq!(detached.branch, "HEAD");
    let list = rewrites(&repo.0)?;
    assert_eq!(list, [second.clone(), first.clone(), detached.clone()]);

    // The old commit outlives the reflog and gc, kept by the ref alone.
    repo.git(&["reflog", "expire", "--expire=now", "--all"])?;
    repo.git(&["gc", "-q", "--prune=now"])?;
    assert!(
        repo.git(&["cat-file", "-e", &format!("{b}^{{commit}}")])
            .is_ok()
    );
    // And none of it is in the graph's history.
    let history = crate::git::log(&repo.0, None, 0, 100)?;
    assert!(history.iter().all(|c| c.hash != b), "{history:?}");

    // A month on, only what is newer stays.
    let now = 1_001 + KEEP_SECS;
    assert_eq!(to_prune(&list, now), [&first, &detached]);
    assert_eq!(prune_rewrites(&repo.0, now)?, 2);
    assert_eq!(rewrites(&repo.0)?, std::slice::from_ref(&second));
    delete_rewrite(&repo.0, &second)?;
    assert!(rewrites(&repo.0)?.is_empty());
    // Deleting what is gone already is no error.
    delete_rewrite(&repo.0, &second)?;
    let left = repo.git(&["for-each-ref", "refs/gitcraft/"])?;
    assert!(left.is_empty(), "{left}");
    Ok(())
}

#[test]
fn only_the_newest_of_a_branch_are_kept() {
    let series = Series::new("a", "b");
    let list: Vec<Rewrite> = (0..KEEP_PER_BRANCH as i64 + 3)
        .rev()
        .flat_map(|time| {
            ["x", "feature/y"].map(|branch| Rewrite {
                branch: branch.to_owned(),
                time,
                old: series.clone(),
                new: series.clone(),
            })
        })
        .collect();
    let pruned = to_prune(&list, KEEP_PER_BRANCH as i64 + 3);
    assert_eq!(pruned.len(), 6);
    assert!(pruned.iter().all(|r| r.time < 3));
}

#[test]
fn branches_with_slashes_are_read_back() {
    let oid = |c: char| c.to_string().repeat(40);
    let out: String = ROLES
        .iter()
        .zip(['a', 'b', 'c', 'd'])
        .map(|(role, c)| format!("{ROOT}feature/x/12/{role}{FS}{}\n", oid(c)))
        // Missing an end, or not ours: left out.
        .chain([format!("{ROOT}other/7/old-tip{FS}{}\n", oid('e'))])
        .chain([format!("{ROOT}weird{FS}{}\n", oid('f'))])
        .collect();
    let list = parse_rewrites(&out);
    assert_eq!(
        list,
        [Rewrite {
            branch: "feature/x".to_owned(),
            time: 12,
            old: Series::new(&oid('a'), &oid('b')),
            new: Series::new(&oid('c'), &oid('d')),
        }]
    );
}

/// An interrupted record (some ends made, not all) is pruned once it is old, and a new record
/// at the same second goes beside it.
#[test]
fn what_an_interrupted_record_left_is_cleaned_up() -> Result<(), String> {
    let repo = Repo::new("partial")?;
    let a = repo.commit("a")?;
    let b = repo.commit("b")?;
    repo.git(&["update-ref", &format!("{ROOT}main/100/old-tip"), &b])?;
    assert!(rewrites(&repo.0)?.is_empty());
    let series = Series::new(&a, &b);
    let made = record_rewrite(&repo.0, "main", &series, &series, 100)?;
    assert_eq!(made.time, 101);
    // Too fresh to be taken for left over: it may still be being written.
    assert_eq!(prune_rewrites(&repo.0, 100 + PARTIAL_SECS)?, 0);
    assert_eq!(prune_rewrites(&repo.0, 101 + PARTIAL_SECS)?, 1);
    assert_eq!(rewrites(&repo.0)?, [made]);
    let left = repo.git(&["for-each-ref", &format!("{ROOT}main/100/")])?;
    assert!(left.is_empty(), "{left}");
    Ok(())
}

/// A record that fails partway takes back the refs it made.
#[test]
fn a_failed_record_leaves_nothing() -> Result<(), String> {
    let repo = Repo::new("rollback")?;
    let a = repo.commit("a")?;
    let bad = Series::new(&a, &"0".repeat(39));
    assert!(record_rewrite(&repo.0, "main", &Series::new(&a, &a), &bad, 5).is_err());
    let left = repo.git(&["for-each-ref", ROOT])?;
    assert!(left.is_empty(), "{left}");
    Ok(())
}

/// `pull --rebase` after the upstream was force-pushed: the old upstream commits were never
/// the branch's own, so the old version doesn't list them as dropped.
#[test]
fn a_pull_after_a_force_push_starts_at_the_fork_point() -> Result<(), String> {
    let up = Repo::new("fork-up")?;
    up.commit("base")?;
    let old_upstream = up.commit("U1")?;
    let clone = Repo::new("fork-clone")?;
    clone.git(&["remote", "add", "origin", &up.0.to_string_lossy()])?;
    clone.git(&["fetch", "-q", "origin"])?;
    clone.git(&["checkout", "-q", "-B", "main", "--track", "origin/main"])?;
    let before = clone.commit("L1")?;
    up.git(&["commit", "-q", "--amend", "-m", "U1 rewritten"])?;
    clone.git(&["pull", "-q", "--rebase"])?;
    let after = clone.git(&["rev-parse", "HEAD"])?;
    let upstream = clone.git(&["rev-parse", "@{upstream}"])?;
    let found = rewritten(&clone.0, &before, &after, Some("@{upstream}"))?;
    assert_eq!(
        found,
        Some((
            Series::new(&old_upstream, &before),
            Series::new(&upstream, &after)
        ))
    );
    Ok(())
}
