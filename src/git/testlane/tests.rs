//! Tests over history on real repositories: results shared with git-test's notes, kept across
//! a rebase that keeps a tree, run at many commits at once, stopped, and the scratch
//! worktrees gone however the run ended.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::fixture::Repo;
use super::*;
use crate::git;

/// A branch `feature` off `main` of `n` commits, those from `bad_from` on with a file
/// `broken`; the test command fails where it is there.
fn branch_with_a_break(
    name: &str,
    n: usize,
    bad_from: usize,
) -> Result<(Repo, Vec<String>), String> {
    let repo = Repo::new(name)?;
    repo.commit("base", &[("README.md", Some("hi\n"))])?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    let mut commits = Vec::new();
    for i in 1..=n {
        let text = format!("{i}\n");
        let mut files = vec![("n.txt", Some(text.as_str()))];
        if i == bad_from {
            files.push(("broken", Some("x")));
        }
        commits.push(repo.commit(&format!("c{i}"), &files)?);
    }
    Ok((repo, commits))
}

const TEST: &str = "test ! -e broken";

fn plan(command: &str, workers: usize) -> TestPlan {
    TestPlan {
        name: DEFAULT_TEST.to_owned(),
        command: command.to_owned(),
        workers,
        retest: false,
    }
}

/// Runs `plan` over `main..feature`, collecting what it told.
fn run_branch(repo: &Repo, plan: &TestPlan) -> Result<(RunSummary, Vec<TestEvent>), String> {
    let jobs = commits_to_test(repo.path(), "main..feature")?;
    let events = Mutex::new(Vec::new());
    let tell = |e: TestEvent| {
        if let Ok(mut events) = events.lock() {
            events.push(e);
        }
    };
    let summary = run_tests(repo.path(), jobs, plan, &RunStop::default(), &tell)?;
    Ok((summary, events.into_inner().map_err(|e| e.to_string())?))
}

#[test]
fn a_branch_is_tested_commit_by_commit_and_its_worktrees_go() -> Result<(), String> {
    let (repo, commits) = branch_with_a_break("branch", 5, 3)?;
    let (summary, events) = run_branch(&repo, &plan(TEST, 2))?;
    assert_eq!((summary.good, summary.bad, summary.known), (2, 3, 0));
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);
    assert!(!summary.stopped);
    let results = read_results(repo.path(), DEFAULT_TEST)?;
    for (i, commit) in commits.iter().enumerate() {
        let want = if i + 1 >= 3 {
            Verdict::Bad
        } else {
            Verdict::Good
        };
        assert_eq!(results.get(&repo.tree(commit)?), Some(&want), "c{}", i + 1);
    }
    let started = events
        .iter()
        .filter(|e| matches!(e, TestEvent::Started { .. }))
        .count();
    assert_eq!(started, 5);
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    // The user's checkout was never touched.
    assert_eq!(repo.rev("HEAD")?, commits[4]);
    assert!(git::status(repo.path())?.is_clean());
    Ok(())
}

/// The notes are git-test's own: what gitcraft writes, `git notes --ref=tests/default show
/// <commit>^{tree}` reads as `good` / `bad`; what git-test writes (`git notes add -f -m`),
/// gitcraft reads; and the command is in `test.default.command`.
#[test]
fn the_notes_are_shared_with_git_test() -> Result<(), String> {
    let (repo, commits) = branch_with_a_break("compat", 2, 2)?;
    set_test_command(repo.path(), DEFAULT_TEST, TEST)?;
    assert_eq!(repo.git(&["config", "test.default.command"])?.trim(), TEST);
    assert_eq!(
        test_command(repo.path(), DEFAULT_TEST)?.as_deref(),
        Some(TEST)
    );
    run_branch(&repo, &plan(TEST, 1))?;
    for (commit, want) in commits.iter().zip(["good", "bad"]) {
        let said = repo.git(&[
            "notes",
            "--ref=tests/default",
            "show",
            &format!("{commit}^{{tree}}"),
        ])?;
        assert_eq!(said, format!("{want}\n"));
    }
    // As git-test writes them: on the tree, `-m good`.
    let base = repo.rev("main")?;
    repo.git(&[
        "notes",
        "--ref=tests/default",
        "add",
        "-f",
        "-m",
        "good",
        &format!("{base}^{{tree}}"),
    ])?;
    let results = read_results(repo.path(), DEFAULT_TEST)?;
    assert_eq!(results.get(&repo.tree(&base)?), Some(&Verdict::Good));
    assert_eq!(results.len(), 3);
    // Something git-test wouldn't write is left out.
    repo.git(&[
        "notes",
        "--ref=tests/default",
        "add",
        "-f",
        "-m",
        "maybe",
        &format!("{base}^{{tree}}"),
    ])?;
    assert_eq!(read_results(repo.path(), DEFAULT_TEST)?.len(), 2);
    // Forgetting them all is `git test forget-results`.
    forget_results(repo.path(), DEFAULT_TEST)?;
    assert!(read_results(repo.path(), DEFAULT_TEST)?.is_empty());
    assert!(
        repo.git(&["rev-parse", "--verify", "-q", "refs/notes/tests/default"])
            .is_err()
    );
    forget_results(repo.path(), DEFAULT_TEST)?;
    Ok(())
}

/// A rebase that leaves a commit's files as they were keeps its result: nothing is run again.
#[test]
fn results_survive_a_rebase_that_keeps_the_trees() -> Result<(), String> {
    let (repo, _) = branch_with_a_break("rebase", 3, 9)?;
    let (first, _) = run_branch(&repo, &plan(TEST, 2))?;
    assert_eq!((first.good, first.known), (3, 0));
    // Every message reworded: new commits, same trees.
    repo.git(&[
        "-c",
        "sequence.editor=true",
        "rebase",
        "-q",
        "--exec",
        "git commit -q --amend -m reworded",
        "main",
    ])?;
    let (again, events) = run_branch(&repo, &plan("false", 2))?;
    assert_eq!((again.good, again.bad, again.known), (3, 0, 3));
    assert!(events.iter().all(|e| matches!(
        e,
        TestEvent::Done {
            end: TestEnd::Known(_),
            ..
        }
    )));
    // Asked to, it runs them again.
    let mut retest = plan("false", 2);
    retest.retest = true;
    let (third, _) = run_branch(&repo, &retest)?;
    assert_eq!((third.good, third.bad, third.known), (0, 3, 0));
    Ok(())
}

/// Two commits with the same tree are tested once.
#[test]
fn a_tree_is_tested_once() -> Result<(), String> {
    let repo = Repo::new("dup")?;
    repo.commit("base", &[("a", Some("1"))])?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    repo.commit("change", &[("a", Some("2"))])?;
    repo.commit("back", &[("a", Some("1"))])?;
    repo.commit("again", &[("a", Some("2"))])?;
    let (summary, _) = run_branch(&repo, &plan("true", 3))?;
    assert_eq!(summary.good, 2);
    Ok(())
}

/// The command runs at the top of the worktree with git's own variables for it unset: one
/// that makes a repository of its own gets that one, not the worktree's.
#[test]
fn the_command_runs_clear_of_the_worktree_s_git_variables() -> Result<(), String> {
    let (repo, _) = branch_with_a_break("env", 1, 9)?;
    let command = "test -f n.txt && test -z \"$GIT_DIR\" && d=$(mktemp -d) && \
                   git -C \"$d\" init -q && test \"$(git -C \"$d\" rev-parse --git-dir)\" = .git; \
                   s=$?; rm -rf \"$d\"; exit $s";
    let (summary, events) = run_branch(&repo, &plan(command, 1))?;
    let said: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            TestEvent::Done { lines, .. } => Some(lines.join("\n")),
            _ => None,
        })
        .collect();
    assert_eq!(summary.good, 1, "{said:?}");
    Ok(())
}

/// What the command printed comes with its result, its last lines only.
#[test]
fn the_last_lines_come_with_the_result() -> Result<(), String> {
    let (repo, _) = branch_with_a_break("lines", 1, 9)?;
    let command = "i=0; while [ $i -lt 300 ]; do echo \"line $i ü\"; i=$((i+1)); done; exit 3";
    let (summary, events) = run_branch(&repo, &plan(command, 1))?;
    assert_eq!(summary.bad, 1);
    let Some(TestEvent::Done { lines, end, .. }) = events.last() else {
        return Err(format!("no end: {events:?}"));
    };
    assert_eq!(
        end,
        &TestEnd::Ran {
            verdict: Verdict::Bad,
            code: Some(3)
        }
    );
    assert_eq!(lines.len(), KEPT_LINES);
    assert_eq!(lines.last().map(String::as_str), Some("line 299 ü"));
    Ok(())
}

/// Stopped, the commands going end, nothing is stored for them, and the worktrees go.
#[cfg(unix)]
#[test]
fn a_stopped_run_stores_nothing_and_cleans_up() -> Result<(), String> {
    let (repo, _) = branch_with_a_break("stop", 4, 9)?;
    let jobs = commits_to_test(repo.path(), "main..feature")?;
    let stop = RunStop::default();
    let stopping = stop.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        stopping.stop();
    });
    let started = Instant::now();
    let summary = run_tests(repo.path(), jobs, &plan("sleep 30", 2), &stop, &|_| {})?;
    assert!(started.elapsed() < Duration::from_secs(25));
    assert!(summary.stopped);
    assert_eq!((summary.good, summary.bad), (0, 0));
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);
    assert!(read_results(repo.path(), DEFAULT_TEST)?.is_empty());
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok(())
}

/// A commit that can't be checked out fails on its own; the rest is tested, and the
/// worktrees go.
#[test]
fn a_commit_that_cannot_be_tested_fails_alone() -> Result<(), String> {
    let (repo, commits) = branch_with_a_break("fail", 2, 9)?;
    let mut jobs = commits_to_test(repo.path(), "main..feature")?;
    jobs.insert(
        0,
        TestJob {
            commit: "0".repeat(40),
            tree: "1".repeat(40),
        },
    );
    let summary = run_tests(
        repo.path(),
        jobs,
        &plan("true", 1),
        &RunStop::default(),
        &|_| {},
    )?;
    assert_eq!(summary.good, 2);
    assert_eq!(summary.failed.len(), 1);
    assert!(
        summary.failed[0].1.contains("could not check out"),
        "{:?}",
        summary.failed
    );
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.rev("HEAD")?, commits[1]);
    Ok(())
}

#[test]
fn ranges_are_checked() -> Result<(), String> {
    let (repo, commits) = branch_with_a_break("range", 2, 9)?;
    assert!(commits_to_test(repo.path(), "  ").is_err());
    assert!(commits_to_test(repo.path(), "feature..feature")?.is_empty());
    let one = commits_to_test(repo.path(), "feature")?;
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].commit, commits[1]);
    let both = commits_to_test(repo.path(), "main..feature")?;
    assert_eq!(
        both.iter().map(|j| j.commit.clone()).collect::<Vec<_>>(),
        commits
    );
    assert!(commits_to_test(repo.path(), "no-such-branch..feature").is_err());
    // An empty range runs nothing, and makes no worktree.
    let summary = run_tests(
        repo.path(),
        Vec::new(),
        &plan("true", 4),
        &RunStop::default(),
        &|_| {},
    )?;
    assert_eq!(summary, RunSummary::default());
    Ok(())
}

/// More commits than a run takes is refused with the reason, before anything runs.
#[test]
fn a_huge_range_is_refused() -> Result<(), String> {
    let repo = Repo::new("huge")?;
    repo.commit("base", &[("a", Some("0"))])?;
    // Commits made in one go, with `commit-tree`, each on the last.
    let tree = repo.tree("HEAD")?;
    let mut tip = repo.rev("HEAD")?;
    for i in 0..=MAX_COMMITS {
        tip = repo
            .git(&["commit-tree", &tree, "-p", &tip, "-m", &format!("c{i}")])?
            .trim()
            .to_owned();
    }
    repo.git(&["branch", "feature", &tip])?;
    let err = commits_to_test(repo.path(), "main..feature")
        .err()
        .unwrap_or_default();
    assert!(err.contains("more than 1000"), "{err}");
    Ok(())
}

/// A scratch worktree left by a gitcraft that is gone is removed when the next is made.
#[test]
fn worktrees_left_by_a_crash_are_swept() -> Result<(), String> {
    let (repo, _) = branch_with_a_break("sweep", 1, 9)?;
    let common = common_dir(repo.path())?;
    // A process that can't be running: pid numbers stop well before this.
    let left = common.join(SCRATCH_DIR).join("test-4000000000-0");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        &left.to_string_lossy(),
        "main",
    ])?;
    assert_eq!(repo.worktree_count()?, 2);
    // Hidden from the worktrees the window lists.
    assert_eq!(git::worktree_list(repo.path(), None)?.len(), 1);
    let scratch = Scratch::add_empty(repo.path(), "test", "main")?;
    assert_eq!(repo.worktree_count()?, 2);
    scratch.remove()?;
    assert_eq!(repo.worktree_count()?, 1);
    // Dropped without being removed, it goes too.
    drop(Scratch::add_empty(repo.path(), "test", "main")?);
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

/// git knowing nobody, the notes are written as gitcraft.
#[test]
fn notes_are_written_without_an_identity() -> Result<(), String> {
    let (repo, commits) = branch_with_a_break("ident", 1, 9)?;
    repo.git(&["config", "--unset", "user.name"])?;
    repo.git(&["config", "--unset", "user.email"])?;
    let tree = repo.tree(&commits[0])?;
    let fallback = !has_committer(repo.path());
    write_result(repo.path(), DEFAULT_TEST, &tree, Verdict::Good, fallback)?;
    assert_eq!(
        read_results(repo.path(), DEFAULT_TEST)?.get(&tree),
        Some(&Verdict::Good)
    );
    Ok(())
}
