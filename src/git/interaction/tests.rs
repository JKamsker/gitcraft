//! Searches on real repositories: two branches that pass alone and fail together, the pair
//! found; a flaky test told apart; branches that pass together, fail alone, or don't apply
//! together; a budget; and the scratch worktree gone every time.

use std::sync::Mutex;

use super::*;
use crate::git::fixture::Repo;

/// `a` and `b` off `main`, five commits each: `a`'s third adds `flag`, `b`'s fourth adds
/// `uses-flag`; the others add files of their own.
fn two_branches(name: &str) -> Result<Repo, String> {
    let repo = Repo::new(&format!("interaction-{name}"))?;
    repo.commit("base", &[("README.md", Some("hi\n"))])?;
    for (branch, special, file) in [("a", 3, "flag"), ("b", 4, "uses-flag")] {
        repo.git(&["checkout", "-q", "-b", branch, "main"])?;
        for i in 1..=5 {
            let own = format!("{branch}{i}.txt");
            let path = if i == special { file } else { own.as_str() };
            repo.commit(&format!("{branch} step {i}"), &[(path, Some("x\n"))])?;
        }
    }
    repo.git(&["checkout", "-q", "main"])?;
    Ok(repo)
}

/// Fails when both files are there.
const BOTH: &str = "! { test -e flag && test -e uses-flag; }";

fn spec(command: &str) -> InteractionSpec {
    InteractionSpec {
        a: "a".to_owned(),
        b: "b".to_owned(),
        command: command.to_owned(),
        budget: DEFAULT_BUDGET,
        reruns: 1,
    }
}

fn find(repo: &Repo, spec: &InteractionSpec) -> Result<(Interaction, Vec<SearchProgress>), String> {
    let told = Mutex::new(Vec::new());
    let tell = |p: SearchProgress| {
        if let Ok(mut told) = told.lock() {
            told.push(p);
        }
    };
    let found = find_interaction(repo.path(), spec, &RunStop::default(), &tell)?;
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok((found, told.into_inner().map_err(|e| e.to_string())?))
}

#[test]
fn finds_the_two_commits_that_fail_together() -> Result<(), String> {
    let repo = two_branches("pair")?;
    let (found, told) = find(&repo, &spec(BOTH))?;
    let Found::Together { commits, minimal } = &found.found else {
        return Err(format!("{:?}", found.found));
    };
    assert!(minimal);
    let names: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(names, ["a step 3", "b step 4"]);
    assert_eq!(commits[0].commit, repo.rev("a~2")?);
    assert_eq!(commits[1].side, Side::B);
    assert_eq!(found.changes.len(), 10);
    assert_eq!(found.base, repo.rev("main")?);
    // Every failure was run twice (reruns: 1).
    assert!(
        found.tests > told.len(),
        "{} tests, {} told",
        found.tests,
        told.len()
    );
    assert!(found.flaky.is_empty());
    // The user's checkout is untouched.
    assert_eq!(repo.rev("HEAD")?, repo.rev("main")?);
    Ok(())
}

/// A failure that doesn't repeat is not believed.
#[test]
fn a_flaky_failure_is_told_apart() -> Result<(), String> {
    let repo = two_branches("flaky")?;
    let counter = repo.path().with_extension("count");
    let _ = std::fs::remove_file(&counter);
    // Fails on the first run, passes on the next, and so on.
    let command = format!(
        "f='{}'; n=$(cat \"$f\" 2>/dev/null || echo 0); echo $((n+1)) > \"$f\"; [ $((n % 2)) -eq 1 ]",
        counter.display()
    );
    let (found, _) = find(&repo, &spec(&command))?;
    let _ = std::fs::remove_file(&counter);
    assert_eq!(found.found, Found::Flaky);
    assert_eq!(found.flaky.len(), 1);
    assert_eq!(found.flaky[0].len(), 10);
    Ok(())
}

#[test]
fn branches_that_pass_together_have_nothing_to_find() -> Result<(), String> {
    let repo = two_branches("pass")?;
    let (found, _) = find(&repo, &spec("true"))?;
    assert_eq!(found.found, Found::PassTogether);
    assert_eq!(found.tests, 1);
    Ok(())
}

#[test]
fn a_branch_that_fails_alone_is_named() -> Result<(), String> {
    let repo = two_branches("alone")?;
    let (found, _) = find(&repo, &spec("! test -e flag"))?;
    assert_eq!(found.found, Found::FailsAlone(Side::A));
    let (found, _) = find(&repo, &spec("! test -e uses-flag"))?;
    assert_eq!(found.found, Found::FailsAlone(Side::B));
    Ok(())
}

#[test]
fn branches_that_conflict_say_so() -> Result<(), String> {
    let repo = Repo::new("interaction-conflict")?;
    repo.commit("base", &[("f", Some("0\n"))])?;
    for branch in ["a", "b"] {
        repo.git(&["checkout", "-q", "-b", branch, "main"])?;
        repo.commit(branch, &[("f", Some(&format!("{branch}\n")))])?;
    }
    let (found, _) = find(&repo, &spec("true"))?;
    assert_eq!(found.found, Found::DontApply);
    Ok(())
}

#[test]
fn a_budget_cuts_the_search_short() -> Result<(), String> {
    let repo = two_branches("budget")?;
    let mut tight = spec(BOTH);
    tight.budget = 2;
    tight.reruns = 0;
    let (found, _) = find(&repo, &tight)?;
    let Found::Together { commits, minimal } = &found.found else {
        return Err(format!("{:?}", found.found));
    };
    assert!(!minimal);
    assert!(commits.len() > 2);
    assert!(commits.iter().any(|c| c.summary == "a step 3"));
    Ok(())
}

/// Results are shared with the lane when the command is the repository's: a second search
/// runs no test.
#[test]
fn results_are_shared_with_the_test_lane() -> Result<(), String> {
    let repo = two_branches("shared")?;
    crate::git::set_test_command(repo.path(), crate::git::DEFAULT_TEST, BOTH)?;
    let (first, _) = find(&repo, &spec(BOTH))?;
    assert!(first.tests > 0);
    let (again, _) = find(&repo, &spec(BOTH))?;
    assert_eq!(again.tests, 0);
    assert_eq!(again.found, first.found);
    // `a` alone passed: its tip's tree has a result for the lane.
    let results = crate::git::read_results(repo.path(), crate::git::DEFAULT_TEST)?;
    assert_eq!(results.get(&repo.tree("a")?), Some(&Verdict::Good));
    Ok(())
}

#[test]
fn what_cannot_be_searched_says_why() -> Result<(), String> {
    let repo = two_branches("refuse")?;
    let mut s = spec(BOTH);
    s.b = "nope".to_owned();
    assert_eq!(
        find(&repo, &s).err().as_deref(),
        Some("nope isn't a commit")
    );
    let mut same = spec(BOTH);
    same.b = "main".to_owned();
    let err = find(&repo, &same).err().unwrap_or_default();
    assert!(err.contains("main has no commits of its own"), "{err}");
    // A merge on a branch.
    repo.git(&["checkout", "-q", "b"])?;
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge", "a~4"])?;
    let err = find(&repo, &spec(BOTH)).err().unwrap_or_default();
    assert!(err.contains("merge commits"), "{err}");
    assert_eq!(
        find(&repo, &spec("  ")).err().as_deref(),
        Some("Type the command to test with")
    );
    Ok(())
}

/// Stopped part way, it says so and leaves no worktree.
#[cfg(unix)]
#[test]
fn a_stopped_search_cleans_up() -> Result<(), String> {
    let repo = two_branches("stop")?;
    let stop = RunStop::default();
    let stopping = stop.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        stopping.stop();
    });
    let result = find_interaction(repo.path(), &spec("sleep 30"), &stop, &|_| {});
    assert_eq!(result.err().as_deref(), Some("stopped"));
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok(())
}
