//! The test lane driven as the window drives it: Test commits… from a commit's menu, the
//! dots as the results come in, a reload after a rebase that keeps the trees, Stop, and a
//! command changed with its old results forgotten.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::banner::progress_text;
use super::output::{Tone, end_text};
use super::*;
use crate::git::TestEnd;
use crate::git::fixture::Repo;

/// `main` with a base commit, `feature` of `n` commits on it, `broken` from `bad_from` on,
/// checked out.
fn branch_repo(name: &str, n: usize, bad_from: usize) -> Result<(Repo, Vec<String>), String> {
    let repo = Repo::new(&format!("lane-{name}"))?;
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

fn open(repo: &Repo, ctx: &egui::Context) -> Result<RepoTab, String> {
    let mut tab = RepoTab::new(repo.path().to_path_buf(), ctx);
    tab.test_lane.visible = true;
    settle(&mut tab, ctx)?;
    Ok(tab)
}

/// Polls the tab until what it started is in: no read, no run, no lane read left.
fn settle(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(90);
    while Instant::now() < until {
        tab.poll(ctx);
        let lane = &tab.test_lane;
        if tab.settled() && lane.shown == lane.reads && !lane.running() && tab.history_gen > 0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Err("the tab never settled".to_owned())
}

/// What the Test commits… dialog would start, for the commits up to `tip`.
fn dialog_start(tab: &RepoTab, tip: Option<&str>) -> Result<StartTests, String> {
    match tab.test_dialog(tip) {
        Action::Dialog(Dialog::TestCommits { start, .. }) => Ok(start),
        _ => Err("no Test commits dialog".to_owned()),
    }
}

fn start(tab: &mut RepoTab, ctx: &egui::Context, start: StartTests) -> Result<(), String> {
    tab.apply(Action::TestLane(LaneAct::Start(start)), ctx);
    settle(tab, ctx)
}

fn dot_of(tab: &RepoTab, repo: &Repo, commit: &str) -> Result<Option<DotState>, String> {
    Ok(tab.test_lane.dot(&repo.tree(commit)?))
}

#[test]
fn testing_a_branch_fills_the_lane() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, commits) = branch_repo("fill", 4, 3)?;
    let mut tab = open(&repo, &ctx)?;
    let mut asked = dialog_start(&tab, None)?;
    assert_eq!(asked.range, "main..feature");
    assert_eq!(asked.workers, DEFAULT_WORKERS);
    assert!(asked.command.is_empty());
    asked.command = "test ! -e broken".to_owned();
    start(&mut tab, &ctx, asked)?;
    for (i, commit) in commits.iter().enumerate() {
        let want = if i + 1 >= 3 {
            DotState::Bad
        } else {
            DotState::Good
        };
        assert_eq!(dot_of(&tab, &repo, commit)?, Some(want), "c{}", i + 1);
    }
    // The base wasn't in the range: no dot.
    assert_eq!(dot_of(&tab, &repo, "main")?, None);
    let run = tab.test_lane.run.as_ref().ok_or("no run")?;
    assert_eq!((run.good, run.bad, run.failed), (2, 2, 0));
    assert_eq!(
        run.first_bad.as_ref().map(|(_, c)| c.as_str()),
        Some(commits[2].as_str())
    );
    assert_eq!(
        progress_text(run),
        "Tested main..feature with `test ! -e broken`: 4 done · 2 passed · 2 failed"
    );
    let (said, bad) = tab.status.as_deref().cloned().ok_or("nothing said")?;
    assert!(bad);
    assert_eq!(
        said,
        "Tested 4 commits of main..feature: 2 passed, 2 failed"
    );
    // The command is the repository's, as git-test keeps it, and the lane shows from now on.
    assert_eq!(tab.test_lane.command(), Some("test ! -e broken"));
    assert!(Settings::get(&ctx).show_test_lane);
    // What a failure printed is there to see.
    let tree = repo.tree(&commits[2])?;
    tab.apply(Action::TestLane(LaneAct::ShowOutput(tree.clone())), &ctx);
    assert_eq!(tab.test_lane.output_open.as_deref(), Some(tree.as_str()));
    let kept = tab.test_lane.outputs.get(&tree).ok_or("no output kept")?;
    assert_eq!(
        end_text(&kept.end),
        ("Failed with exit code 1".to_owned(), Tone::Bad)
    );
    tab.apply(Action::TestLane(LaneAct::Close), &ctx);
    assert!(tab.test_lane.run.is_none());
    Ok(())
}

/// The dots are kept by tree: after the branch is rebased with new messages (in a terminal,
/// say), a reload shows them again without running anything.
#[test]
fn a_rebase_keeping_the_trees_keeps_the_dots() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, _) = branch_repo("rebase", 3, 2)?;
    let mut tab = open(&repo, &ctx)?;
    let mut asked = dialog_start(&tab, None)?;
    asked.command = "test ! -e broken".to_owned();
    start(&mut tab, &ctx, asked)?;
    repo.git(&[
        "-c",
        "sequence.editor=true",
        "rebase",
        "-q",
        "--exec",
        "git commit -q --amend -m reworded",
        "main",
    ])?;
    // A new tab, as after a restart: nothing in memory.
    let tab = open(&repo, &ctx)?;
    let rebased = commits_of(&repo, "main..feature")?;
    assert_eq!(rebased.len(), 3);
    let dots: Vec<_> = rebased
        .iter()
        .map(|c| dot_of(&tab, &repo, c))
        .collect::<Result<_, _>>()?;
    assert_eq!(
        dots,
        [
            Some(DotState::Good),
            Some(DotState::Bad),
            Some(DotState::Bad)
        ]
    );
    Ok(())
}

fn commits_of(repo: &Repo, range: &str) -> Result<Vec<String>, String> {
    Ok(repo
        .git(&["rev-list", "--reverse", range])?
        .lines()
        .map(str::to_owned)
        .collect())
}

/// Testing up to a commit of the menu tests from the base to it.
#[test]
fn testing_up_to_a_commit_starts_at_the_base() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, commits) = branch_repo("upto", 3, 9)?;
    let tab = open(&repo, &ctx)?;
    let asked = dialog_start(&tab, Some(&commits[1]))?;
    assert_eq!(asked.range, format!("main..{}", commits[1]));
    Ok(())
}

/// Stop from the banner ends the commands; nothing is kept for them, the scratch worktrees go.
#[cfg(unix)]
#[test]
fn stop_ends_the_run() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, commits) = branch_repo("stop", 3, 9)?;
    let mut tab = open(&repo, &ctx)?;
    let mut asked = dialog_start(&tab, None)?;
    asked.command = "sleep 30".to_owned();
    tab.apply(Action::TestLane(LaneAct::Start(asked)), &ctx);
    // Until the tests are going.
    let until = Instant::now() + Duration::from_secs(30);
    while tab
        .test_lane
        .run
        .as_ref()
        .is_none_or(|r| r.running.is_empty())
    {
        if Instant::now() > until {
            return Err("the tests never started".to_owned());
        }
        tab.poll(&ctx);
        std::thread::sleep(Duration::from_millis(15));
    }
    assert_eq!(dot_of(&tab, &repo, &commits[0])?, Some(DotState::Running));
    tab.apply(Action::TestLane(LaneAct::Stop), &ctx);
    settle(&mut tab, &ctx)?;
    let (said, _) = tab.status.as_deref().cloned().ok_or("nothing said")?;
    assert!(
        said.starts_with("Testing main..feature stopped after 0 commits"),
        "{said}"
    );
    for commit in &commits {
        assert_eq!(dot_of(&tab, &repo, commit)?, None);
    }
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok(())
}

/// A new command forgets the old one's results when asked; kept, they show on.
#[test]
fn a_new_command_can_forget_the_old_results() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, commits) = branch_repo("forget", 2, 9)?;
    let mut tab = open(&repo, &ctx)?;
    let mut asked = dialog_start(&tab, None)?;
    asked.command = "true".to_owned();
    start(&mut tab, &ctx, asked)?;
    assert_eq!(dot_of(&tab, &repo, &commits[0])?, Some(DotState::Good));
    // Only the first commit tested again, with a command that fails: the second's result
    // is forgotten.
    let mut again = dialog_start(&tab, None)?;
    assert_eq!(again.command, "true");
    again.command = "false".to_owned();
    again.range = commits[0].clone();
    again.forget = true;
    start(&mut tab, &ctx, again)?;
    assert_eq!(dot_of(&tab, &repo, &commits[0])?, Some(DotState::Bad));
    assert_eq!(dot_of(&tab, &repo, &commits[1])?, None);
    assert_eq!(
        git::test_command(repo.path(), git::DEFAULT_TEST)?.as_deref(),
        Some("false")
    );
    Ok(())
}

/// A range that isn't one says so, in the banner and the status line.
#[test]
fn a_bad_range_says_why() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, _) = branch_repo("badrange", 1, 9)?;
    let mut tab = open(&repo, &ctx)?;
    let mut asked = dialog_start(&tab, None)?;
    asked.command = "true".to_owned();
    asked.range = "no-such..feature".to_owned();
    start(&mut tab, &ctx, asked)?;
    let (said, bad) = tab.status.as_deref().cloned().ok_or("nothing said")?;
    assert!(bad);
    assert!(
        said.starts_with("Couldn't test no-such..feature:"),
        "{said}"
    );
    assert!(matches!(
        tab.test_lane.run.as_ref().and_then(|r| r.ended.as_ref()),
        Some(Err(_))
    ));
    Ok(())
}

#[test]
fn messages_say_how_a_run_went() {
    let summary = |good, bad, stopped| RunSummary {
        good,
        bad,
        stopped,
        ..RunSummary::default()
    };
    assert_eq!(
        ended_message("a..b", &Ok(summary(1, 0, false))),
        ("Tested 1 commit of a..b: 1 passed".to_owned(), false)
    );
    assert_eq!(
        ended_message("a..b", &Ok(summary(0, 2, true))),
        (
            "Testing a..b stopped after 2 commits: 0 passed, 2 failed".to_owned(),
            true
        )
    );
    let mut failed = summary(3, 0, false);
    failed.failed.push(("t".into(), "e".into()));
    assert_eq!(
        ended_message("x", &Ok(failed)).0,
        "Tested 3 commits of x: 3 passed, 1 couldn't be tested"
    );
    assert_eq!(
        ended_message("x", &Err("boom".into())),
        ("Couldn't test x: boom".to_owned(), true)
    );
}

#[test]
fn dots_say_what_they_mean() {
    let tree = "0123456789abcdef";
    assert_eq!(
        dot_tip(DotState::Good, Some("cargo test"), tree, true),
        "Passed `cargo test` · tree 01234567\nClick to see what it printed"
    );
    assert!(dot_tip(DotState::Bad, None, tree, false).starts_with("Failed the test · tree"));
    assert!(dot_tip(DotState::Bad, None, tree, false).contains("shared with git-test"));
    assert_eq!(
        dot_tip(DotState::Queued, None, tree, false),
        "Waiting to be tested · tree 01234567"
    );
    assert_eq!(
        end_text(&TestEnd::Ran {
            verdict: Verdict::Bad,
            code: None
        })
        .0,
        "Failed: ended by a signal"
    );
}

/// Results come as they finish, the stored ones first: the banner's first failure is still the
/// oldest commit that failed, and two commits with one tree count once.
#[test]
fn the_first_failure_is_the_oldest_whatever_finishes_first() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, _) = branch_repo("order", 1, 9)?;
    let mut tab = open(&repo, &ctx)?;
    let start = dialog_start(&tab, None)?;
    tab.test_lane.runs = 7;
    tab.test_lane.run = Some(LaneRun {
        id: 7,
        range: start.range,
        command: "x".into(),
        total: None,
        queued: HashSet::new(),
        running: HashSet::new(),
        good: 0,
        bad: 0,
        failed: 0,
        first_bad: None,
        order: HashMap::new(),
        stop: RunStop::default(),
        started: Instant::now(),
        ended: None,
    });
    let trees = ["t1", "t2", "t3", "t2"].map(str::to_owned).to_vec();
    tab.tests_listed(7, trees);
    let done = |commit: &str, tree: &str, verdict| TestEvent::Done {
        commit: commit.into(),
        tree: tree.into(),
        end: TestEnd::Ran {
            verdict,
            code: Some(1),
        },
        lines: Vec::new(),
    };
    tab.tests_event(7, done("c3", "t3", Verdict::Bad));
    tab.tests_event(7, done("c2", "t2", Verdict::Bad));
    tab.tests_event(7, done("c1", "t1", Verdict::Good));
    let run = tab.test_lane.run.as_ref().ok_or("no run")?;
    assert_eq!(run.total, Some(3));
    assert_eq!(run.first_bad.as_ref().map(|(_, c)| c.as_str()), Some("c2"));
    assert!(progress_text(run).contains("3 of 3 done"));
    // An event of another run says nothing about this one.
    tab.tests_event(6, done("c0", "t0", Verdict::Bad));
    assert_eq!(tab.test_lane.dot("t0"), None);
    Ok(())
}

/// Results already stored are said apart from those tested.
#[test]
fn stored_results_are_not_counted_as_tested() {
    let summary = RunSummary {
        good: 3,
        bad: 1,
        known: 2,
        ..RunSummary::default()
    };
    assert_eq!(
        ended_message("a..b", &Ok(summary)).0,
        "Tested 2 commits of a..b (2 had a result already): 3 passed, 1 failed"
    );
    let cleanup = RunSummary {
        good: 1,
        cleanup: Some("busy".into()),
        ..RunSummary::default()
    };
    let (said, bad) = ended_message("x", &Ok(cleanup));
    assert!(bad);
    assert!(
        said.ends_with("Couldn't remove a scratch worktree: busy"),
        "{said}"
    );
}

/// Writing a result moves the notes ref: that alone reloads nothing.
#[test]
fn notes_written_do_not_reload_the_tab() {
    use crate::repo::watch::{FsChange, fs_change};
    let git = std::path::Path::new("/r/.git");
    assert_eq!(fs_change(&git.join("refs/notes/tests/default"), &[]), None);
    assert_eq!(
        fs_change(&git.join("refs/heads/main"), &[]),
        Some(FsChange::Git)
    );
}

/// A command stored with spaces around it is the same command: its results stay.
#[test]
fn a_stored_command_with_spaces_is_the_same() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (repo, commits) = branch_repo("spaces", 1, 9)?;
    repo.git(&["config", "test.default.command", "  true  "])?;
    assert_eq!(
        git::test_command(repo.path(), git::DEFAULT_TEST)?.as_deref(),
        Some("true")
    );
    let tree = repo.tree(&commits[0])?;
    git::write_result(repo.path(), git::DEFAULT_TEST, &tree, Verdict::Good, false)?;
    let mut tab = open(&repo, &ctx)?;
    let mut asked = dialog_start(&tab, None)?;
    assert_eq!(asked.command, "true");
    asked.forget = true;
    start(&mut tab, &ctx, asked)?;
    let (said, _) = tab.status.as_deref().cloned().ok_or("nothing said")?;
    assert!(said.contains("1 had a result already"), "{said}");
    assert_eq!(dot_of(&tab, &repo, &commits[0])?, Some(DotState::Good));
    Ok(())
}
