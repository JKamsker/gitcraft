//! Rehearsals on real repositories: a rebase that stops on a conflict at a known commit, one
//! that goes through with a failing test, a merge with conflicts against the same branch, a
//! fast-forward, up to date, refs moved meanwhile, and the scratch worktree gone every time.

use super::*;
use crate::git::fixture::Repo;

/// `main` and `feature` off a base: `feature` has five commits, the third of them changing
/// `shared.txt`, which `main` changes too. `main` also adds `broken` when `break_main`.
fn diverged(name: &str, break_main: bool) -> Result<Repo, String> {
    let repo = Repo::new(&format!("rehearse-{name}"))?;
    repo.commit(
        "base",
        &[("shared.txt", Some("one\n")), ("README.md", Some("hi\n"))],
    )?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    for i in 1..=5 {
        let text = format!("{i}\n");
        let mut files = vec![("feature.txt", Some(text.as_str()))];
        if i == 3 {
            files.push(("shared.txt", Some("feature's\n")));
        }
        repo.commit(&format!("feature step {i}"), &files)?;
    }
    repo.git(&["checkout", "-q", "main"])?;
    let mut files = vec![("shared.txt", Some("main's\n"))];
    if break_main {
        files.push(("broken", Some("x")));
    }
    repo.commit("main changes shared", &files)?;
    repo.git(&["checkout", "-q", "feature"])?;
    Ok(repo)
}

fn spec(kind: RehearseKind, other: &str, command: Option<&str>) -> RehearseSpec {
    RehearseSpec {
        kind,
        branch: "refs/heads/feature".to_owned(),
        other: other.to_owned(),
        command: command.map(str::to_owned),
    }
}

/// Untouched: the branch, the checkout, and no worktree left.
fn untouched(repo: &Repo, tip: &str) -> Result<(), String> {
    assert_eq!(repo.rev("feature")?, tip);
    assert_eq!(repo.rev("HEAD")?, tip);
    assert!(crate::git::status(repo.path())?.is_clean());
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok(())
}

#[test]
fn a_rebase_stops_at_the_commit_that_conflicts() -> Result<(), String> {
    let repo = diverged("conflict", false)?;
    let tip = repo.rev("feature")?;
    let third = repo.rev("feature~2")?;
    let r = rehearse(
        repo.path(),
        &spec(RehearseKind::Rebase, "refs/heads/main", Some("true")),
        &RunStop::default(),
    )?;
    assert_eq!(
        r.played,
        Played::Stopped {
            step: 3,
            of: 5,
            commit: Some(third),
            summary: "feature step 3".to_owned(),
            conflicts: vec!["shared.txt".to_owned()],
        }
    );
    assert_eq!(
        r.tested,
        Some(Tested::NotRun("it stops on conflicts".to_owned()))
    );
    assert_eq!(r.branch_tip, tip);
    assert_eq!(r.other_tip, repo.rev("main")?);
    untouched(&repo, &tip)
}

#[test]
fn a_merge_reports_its_conflicts_too() -> Result<(), String> {
    let repo = diverged("merge-stop", false)?;
    let tip = repo.rev("feature")?;
    let r = rehearse(
        repo.path(),
        &spec(RehearseKind::Merge, "refs/heads/main", None),
        &RunStop::default(),
    )?;
    let Played::Stopped {
        step,
        of,
        conflicts,
        ..
    } = &r.played
    else {
        return Err(format!("not stopped: {:?}", r.played));
    };
    assert_eq!((*step, *of), (1, 1));
    assert_eq!(conflicts, &["shared.txt"]);
    assert_eq!(r.tested, None);
    untouched(&repo, &tip)
}

/// No conflict, but the test fails on the result: the report says so, with its output, and
/// the result is stored for the lane under the repository's test command.
#[test]
fn a_clean_rebase_whose_test_fails() -> Result<(), String> {
    let repo = diverged("test-fails", true)?;
    // Without the conflict: feature's third commit leaves shared.txt alone.
    repo.git(&["checkout", "-q", "-B", "feature", "main~1"])?;
    for i in 1..=4 {
        let text = format!("{i}\n");
        repo.commit(&format!("clean {i}"), &[("feature.txt", Some(&text))])?;
    }
    let tip = repo.rev("feature")?;
    let command = "echo checking; test ! -e broken";
    crate::git::set_test_command(repo.path(), crate::git::DEFAULT_TEST, command)?;
    let r = rehearse(
        repo.path(),
        &spec(RehearseKind::Rebase, "refs/heads/main", Some(command)),
        &RunStop::default(),
    )?;
    let Played::Clean {
        tree,
        commits,
        tip: result,
    } = &r.played
    else {
        return Err(format!("not clean: {:?}", r.played));
    };
    assert_eq!(*commits, 4);
    assert_ne!(result, &tip);
    assert_eq!(
        r.tested,
        Some(Tested::Ran {
            verdict: Verdict::Bad,
            code: Some(1),
            lines: vec!["checking".to_owned()],
        })
    );
    let stored = crate::git::read_results(repo.path(), crate::git::DEFAULT_TEST)?;
    assert_eq!(stored.get(tree), Some(&Verdict::Bad));
    // Rehearsed again: the stored result is taken, nothing runs.
    let again = rehearse(
        repo.path(),
        &spec(RehearseKind::Rebase, "refs/heads/main", Some(command)),
        &RunStop::default(),
    )?;
    assert_eq!(again.tested, Some(Tested::Known(Verdict::Bad)));
    // The merge gives the same files: the two can be compared.
    let merge = rehearse(
        repo.path(),
        &spec(RehearseKind::Merge, "refs/heads/main", Some(command)),
        &RunStop::default(),
    )?;
    assert_eq!(merge.played.tree(), Some(tree.as_str()));
    untouched(&repo, &tip)
}

#[test]
fn up_to_date_and_fast_forward() -> Result<(), String> {
    let repo = Repo::new("rehearse-ff")?;
    repo.commit("base", &[("a", Some("1"))])?;
    repo.git(&["branch", "feature"])?;
    let ahead = repo.commit("main moves on", &[("a", Some("2"))])?;
    repo.git(&["checkout", "-q", "feature"])?;
    let tip = repo.rev("feature")?;
    let merge = rehearse(
        repo.path(),
        &spec(RehearseKind::Merge, "refs/heads/main", None),
        &RunStop::default(),
    )?;
    assert_eq!(
        merge.played,
        Played::FastForward {
            tip: ahead.clone(),
            tree: repo.tree(&ahead)?
        }
    );
    // feature is behind main: rebasing it moves it to main, no commit replayed.
    let rebase = rehearse(
        repo.path(),
        &spec(RehearseKind::Rebase, "refs/heads/main", None),
        &RunStop::default(),
    )?;
    assert!(
        matches!(&rebase.played, Played::Clean { commits: 0, tip, .. } if *tip == ahead),
        "{:?}",
        rebase.played
    );
    // main has nothing feature lacks the other way round.
    let mut back = spec(RehearseKind::Merge, "refs/heads/feature", None);
    back.branch = "refs/heads/main".to_owned();
    let none = rehearse(repo.path(), &back, &RunStop::default())?;
    assert_eq!(none.played, Played::UpToDate);
    untouched(&repo, &tip)
}

/// Applying checks the refs first: one moved since the rehearsal is named.
#[test]
fn moved_refs_are_caught() -> Result<(), String> {
    let repo = diverged("moved", false)?;
    let r = rehearse(
        repo.path(),
        &spec(RehearseKind::Merge, "refs/heads/main", None),
        &RunStop::default(),
    )?;
    unmoved(repo.path(), &r)?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("main again", &[("other", Some("x"))])?;
    let err = unmoved(repo.path(), &r).err().unwrap_or_default();
    assert!(err.starts_with("main moved since the rehearsal"), "{err}");
    assert!(err.ends_with("rehearse again"), "{err}");
    repo.git(&["branch", "-D", "feature"])?;
    let err = unmoved(repo.path(), &r).err().unwrap_or_default();
    assert!(
        err.contains("feature moved") && err.contains("gone"),
        "{err}"
    );
    Ok(())
}

#[test]
fn unknown_refs_say_so() -> Result<(), String> {
    let repo = diverged("unknown", false)?;
    let err = rehearse(
        repo.path(),
        &spec(RehearseKind::Rebase, "refs/heads/nope", None),
        &RunStop::default(),
    )
    .err()
    .unwrap_or_default();
    assert_eq!(err, "nope isn't a commit");
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

/// Stopped while its test runs, it ends, says so, and leaves no worktree.
#[cfg(unix)]
#[test]
fn a_stopped_rehearsal_cleans_up() -> Result<(), String> {
    let repo = Repo::new("rehearse-stop")?;
    repo.commit("base", &[("a", Some("1"))])?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    repo.commit("f", &[("b", Some("1"))])?;
    repo.git(&["checkout", "-q", "main"])?;
    repo.commit("m", &[("c", Some("1"))])?;
    repo.git(&["checkout", "-q", "feature"])?;
    let tip = repo.rev("feature")?;
    let stop = RunStop::default();
    let stopping = stop.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        stopping.stop();
    });
    let result = rehearse(
        repo.path(),
        &spec(RehearseKind::Merge, "refs/heads/main", Some("sleep 30")),
        &stop,
    );
    assert_eq!(result.err().as_deref(), Some("stopped"));
    untouched(&repo, &tip)
}

#[test]
fn refs_read_as_people_name_them() {
    assert_eq!(short_ref("refs/heads/feat/ü x"), "feat/ü x");
    assert_eq!(short_ref("refs/remotes/origin/main"), "origin/main");
    assert_eq!(short_ref("abc123"), "abc123");
}
