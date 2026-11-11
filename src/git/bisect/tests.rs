//! Bisects of real repositories with a known culprit, driven through what the window uses:
//! marked by hand, with skips, started in a terminal and picked up, run with a command, and
//! reset to where they began.

use std::path::{Path, PathBuf};

use super::*;
use crate::git::{self, Area, Ended, InProgress, StatusFile};

/// A repository of `n` commits `c1`…`cn` on `main`, each writing its number to `f`, those
/// from `bad_from` on with `bug` under it. Gives its folder and the commits, oldest first.
fn culprit_repo(name: &str, n: usize, bad_from: usize) -> Result<(PathBuf, Vec<String>), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-bisect-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        // `f` as written on every system: the test commands read its first line.
        &["config", "core.autocrlf", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    let mut hashes = Vec::new();
    for i in 1..=n {
        let bug = if i >= bad_from { "bug\n" } else { "" };
        std::fs::write(dir.join("f"), format!("{i}\n{bug}")).map_err(|e| e.to_string())?;
        git::run(Some(&dir), &["add", "f"])?;
        git::run(Some(&dir), &["commit", "-q", "-m", &format!("c{i}")])?;
        hashes.push(git::head(&dir));
    }
    Ok((dir, hashes))
}

/// The commit number in `f` as it is checked out.
fn checked_out_number(dir: &Path) -> Result<usize, String> {
    let text = std::fs::read_to_string(dir.join("f")).map_err(|e| e.to_string())?;
    text.lines()
        .next()
        .and_then(|l| l.parse().ok())
        .ok_or_else(|| format!("no number in f: {text}"))
}

/// Whether the commit checked out has the bug.
fn has_bug(dir: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(dir.join("f")).map_err(|e| e.to_string())?;
    Ok(text.contains("bug"))
}

fn state(dir: &Path) -> Result<Bisect, String> {
    bisect_state(dir)?.ok_or_else(|| "no bisect going on".to_owned())
}

/// Tests each commit it is given as a person would, until the bisect ends; `skip` says which
/// numbers can't be tested. Gives the outcome and how many tests it took.
fn test_by_hand(dir: &Path, skip: &[usize]) -> Result<(Outcome, usize), String> {
    for tests in 0..64 {
        let bisect = state(dir)?;
        if let Some(outcome) = bisect.outcome {
            return Ok((outcome, tests));
        }
        assert!(bisect.progress.is_some(), "no progress: {bisect:?}");
        let mark = if skip.contains(&checked_out_number(dir)?) {
            Mark::Skip
        } else if has_bug(dir)? {
            Mark::Bad
        } else {
            Mark::Good
        };
        bisect_mark(dir, &bisect, mark, None)?;
    }
    Err("the bisect never ended".to_owned())
}

fn current_branch(dir: &Path) -> Result<String, String> {
    git::run(Some(dir), &["symbolic-ref", "--short", "HEAD"]).map(|b| b.trim().to_owned())
}

#[test]
fn marking_by_hand_finds_the_culprit_and_reset_goes_back() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("by-hand", 40, 23)?;
    let result = (|| {
        assert!(bisect_state(&dir)?.is_none());
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        let first = state(&dir)?;
        assert_eq!(first.start_branch(), Some("main"));
        assert_eq!(
            first.marks.bad.as_deref(),
            hashes.last().map(String::as_str)
        );
        assert_eq!(first.marks.good, [hashes[0].clone()]);
        assert_eq!(first.mark_of(&hashes[0]), Some(Mark::Good));
        assert_eq!(first.current, git::head(&dir));
        // What git itself says when it starts: 39 candidates.
        let progress = first.progress.ok_or("no progress")?;
        assert_eq!(progress.steps, steps_left(39));
        let (outcome, tests) = test_by_hand(&dir, &[])?;
        assert_eq!(outcome, Outcome::FirstBad(hashes[22].clone()));
        // Halving 39 candidates: at most 6 tests (the estimate is git's rough one).
        assert!(tests <= 6, "{tests} tests");
        let ended = state(&dir)?;
        assert_eq!(ended.progress, None);
        bisect_reset(&dir)?;
        assert!(bisect_state(&dir)?.is_none());
        assert_eq!(current_branch(&dir)?, "main");
        assert_eq!(git::head(&dir), hashes[39]);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn the_steps_left_shrink_as_commits_are_marked() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("steps", 64, 50)?;
    let result = (|| {
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        let mut last = u32::MAX;
        let mut tests = 0;
        while let Some(progress) = state(&dir)?.progress {
            tests += 1;
            assert!(tests <= 8, "64 commits take at most 7 tests");
            assert!(progress.steps <= last, "{} after {last}", progress.steps);
            last = progress.steps;
            let bisect = state(&dir)?;
            let mark = if has_bug(&dir)? {
                Mark::Bad
            } else {
                Mark::Good
            };
            bisect_mark(&dir, &bisect, mark, None)?;
        }
        assert_eq!(
            state(&dir)?.outcome,
            Some(Outcome::FirstBad(hashes[49].clone()))
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn skipping_around_the_culprit_leaves_the_commits_it_could_be() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("skip", 12, 6)?;
    let result = (|| {
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        let (outcome, _) = test_by_hand(&dir, &[5, 6, 7])?;
        let Outcome::OnlySkipped(possible) = outcome else {
            return Err(format!("not only skipped ones left: {outcome:?}"));
        };
        assert!(possible.contains(&hashes[5]), "{possible:?}");
        let bisect = state(&dir)?;
        assert!(!bisect.marks.skipped.is_empty());
        for skipped in &bisect.marks.skipped {
            assert_eq!(bisect.mark_of(skipped), Some(Mark::Skip));
        }
        bisect_reset(&dir)?;
        assert_eq!(current_branch(&dir)?, "main");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Skipping a commit that isn't the one being tested, by its hash, as the graph's menu does.
#[test]
fn a_commit_is_marked_by_its_hash() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("by-hash", 10, 4)?;
    let result = (|| {
        bisect_start(&dir, &hashes[9], None)?;
        let bisect = state(&dir)?;
        // No good commit yet: nothing to count.
        assert_eq!(bisect.progress, None);
        assert_eq!(bisect.outcome, None);
        bisect_mark(&dir, &bisect, Mark::Good, Some(&hashes[1]))?;
        let bisect = state(&dir)?;
        assert_eq!(bisect.marks.good, [hashes[1].clone()]);
        assert!(bisect.progress.is_some());
        bisect_mark(&dir, &bisect, Mark::Skip, Some(&hashes[5]))?;
        assert_eq!(state(&dir)?.mark_of(&hashes[5]), Some(Mark::Skip));
        assert!(bisect_mark(&dir, &bisect, Mark::Bad, Some("no-such-commit")).is_err());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Started in a terminal, with terms of its own and half done: picked up as it is and
/// finished from here.
#[test]
fn a_bisect_started_in_a_terminal_is_picked_up() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("terminal", 20, 13)?;
    let result = (|| {
        let terminal = |args: &[&str]| git::run(Some(&dir), args);
        terminal(&["bisect", "start", "--term-new=broken", "--term-old=fine"])?;
        terminal(&["bisect", "broken", "HEAD"])?;
        let waiting = state(&dir)?;
        assert_eq!(waiting.terms.bad, "broken");
        assert_eq!(waiting.word(Mark::Good), "fine");
        assert_eq!(waiting.progress, None);
        terminal(&["bisect", "fine", &hashes[0]])?;
        // One test made there.
        let mark = if has_bug(&dir)? { "broken" } else { "fine" };
        terminal(&["bisect", mark])?;
        let picked_up = state(&dir)?;
        assert_eq!(picked_up.marks.good.len(), 1 + usize::from(mark == "fine"));
        let (outcome, _) = test_by_hand(&dir, &[])?;
        assert_eq!(outcome, Outcome::FirstBad(hashes[12].clone()));
        bisect_reset(&dir)?;
        assert_eq!(current_branch(&dir)?, "main");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A bisect is a worktree's own: another worktree of the repository doesn't see it.
#[test]
fn a_bisect_belongs_to_its_worktree() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("worktree", 8, 5)?;
    let other = dir.with_file_name(format!(
        "{}-other",
        dir.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&other);
    let result = (|| {
        let other_path = other.to_string_lossy().into_owned();
        git::run(
            Some(&dir),
            &["worktree", "add", "-q", "-b", "side", &other_path],
        )?;
        bisect_start(&other, "HEAD", Some(&hashes[0]))?;
        assert!(bisect_state(&dir)?.is_none());
        assert_eq!(state(&other)?.start_branch(), Some("side"));
        let (outcome, _) = test_by_hand(&other, &[])?;
        assert_eq!(outcome, Outcome::FirstBad(hashes[4].clone()));
        bisect_reset(&other)?;
        assert_eq!(current_branch(&other)?, "side");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&other);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// What a test command for `bisect run` exits with: 125 (skip) for the numbers in `skip`, 1
/// with the bug, 0 without.
fn test_command(skip: &[usize]) -> String {
    let skips = skip
        .iter()
        .map(|n| format!("[ \"$n\" = {n} ] && exit 125;"))
        .collect::<String>();
    format!(
        "n=$(head -n 1 f); echo \"testing $n\"; {skips} if grep -q bug f; then exit 1; fi; exit 0"
    )
}

fn run_bisect(dir: &Path, command: &str) -> Result<(Ended, Vec<String>), String> {
    let args = bisect_run_args(command);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut lines = Vec::new();
    let ended = git::stream(dir, &args, &git::Stopper::default(), |l| lines.push(l))?;
    Ok((ended, lines))
}

#[test]
fn a_run_with_a_script_finds_the_culprit() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("run", 30, 17)?;
    let result = (|| {
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        // Skips the commits it can't test (125).
        let (ended, lines) = run_bisect(&dir, &test_command(&[8, 12]))?;
        assert_eq!(ended.code, Some(0), "{lines:#?}");
        assert!(!ended.stopped);
        // Its output came through, the command's own too.
        assert!(
            lines.iter().any(|l| l.starts_with("testing ")),
            "{lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l.contains(" is the first ")),
            "{lines:#?}"
        );
        let bisect = state(&dir)?;
        assert_eq!(bisect.outcome, Some(Outcome::FirstBad(hashes[16].clone())));
        assert!(!bisect.marks.skipped.is_empty() || !bisect.marks.good.is_empty());
        bisect_reset(&dir)?;
        assert_eq!(current_branch(&dir)?, "main");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_run_left_with_only_skipped_commits_says_what_it_could_be() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("run-skip", 10, 6)?;
    let result = (|| {
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        let (ended, lines) = run_bisect(&dir, &test_command(&[5, 6, 7]))?;
        // git says it can't go on.
        assert_ne!(ended.code, Some(0), "{lines:#?}");
        let Some(Outcome::OnlySkipped(possible)) = state(&dir)?.outcome else {
            return Err(format!("not only skipped ones left: {lines:#?}"));
        };
        assert!(possible.contains(&hashes[5]), "{possible:?}");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A command that exits with 128 or more stops the run, and the bisect stays where it was.
#[test]
fn a_run_that_fails_leaves_the_bisect_going() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("run-fail", 10, 6)?;
    let result = (|| {
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        let (ended, lines) = run_bisect(&dir, "exit 200")?;
        assert_ne!(ended.code, Some(0), "{lines:#?}");
        let bisect = state(&dir)?;
        assert_eq!(bisect.outcome, None);
        assert!(bisect.progress.is_some());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Stopping a run ends the command it runs; the bisect itself stays, to go on or stop.
#[cfg(unix)]
#[test]
fn a_run_can_be_stopped() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("run-stop", 10, 6)?;
    let result = (|| {
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        let stopper = git::Stopper::default();
        let stopping = stopper.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            stopping.stop();
        });
        let args = bisect_run_args("sleep 41");
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let started = std::time::Instant::now();
        let ended = git::stream(&dir, &args, &stopper, |_| {})?;
        assert!(ended.stopped);
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
        assert!(bisect_state(&dir)?.is_some());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn reset_goes_back_to_the_branch_it_began_on() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("reset", 6, 3)?;
    let result = (|| {
        git::run(Some(&dir), &["checkout", "-q", "-b", "topic", &hashes[4]])?;
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        assert_ne!(git::head(&dir), hashes[4]);
        assert!(current_branch(&dir).is_err(), "not detached");
        bisect_reset(&dir)?;
        assert_eq!(current_branch(&dir)?, "topic");
        assert_eq!(git::head(&dir), hashes[4]);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_start_needs_two_different_commits() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("start", 4, 2)?;
    let result = (|| {
        assert!(bisect_start(&dir, "HEAD", Some(&hashes[3])).is_err());
        assert!(bisect_start(&dir, "no-such-branch", None).is_err());
        assert!(bisect_start(&dir, "--help", None).is_err());
        assert!(bisect_state(&dir)?.is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn file(path: &str, area: Area) -> StatusFile {
    StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'M',
        area,
    }
}

#[test]
fn a_start_is_refused_with_changes_or_an_operation_in_the_way() {
    let clean = WorkStatus::default();
    assert_eq!(start_blocker(&clean, false), None);
    // Untracked files go along with every checkout.
    let untracked = WorkStatus {
        unstaged: vec![file("new.txt", Area::Untracked)],
        ..WorkStatus::default()
    };
    assert_eq!(start_blocker(&untracked, false), None);
    let blocked = [
        WorkStatus {
            unstaged: vec![file("a.txt", Area::Unstaged)],
            ..WorkStatus::default()
        },
        WorkStatus {
            staged: vec![file("a.txt", Area::Staged)],
            ..WorkStatus::default()
        },
        WorkStatus {
            conflicted: vec![file("a.txt", Area::Conflicted)],
            ..WorkStatus::default()
        },
        WorkStatus {
            in_progress: Some(InProgress::Rebase),
            ..WorkStatus::default()
        },
    ];
    for work in blocked {
        assert!(start_blocker(&work, false).is_some(), "{work:?}");
    }
    assert!(
        start_blocker(&clean, true).is_some_and(|why| why.contains("already")),
        "a second bisect"
    );
    let merging = WorkStatus {
        in_progress: Some(InProgress::Merge),
        ..WorkStatus::default()
    };
    assert_eq!(
        start_blocker(&merging, false).as_deref(),
        Some("A merge is in progress: finish or abort it first.")
    );
}

/// The branch it began on deleted meanwhile: reset says so and still ends the bisect.
#[test]
fn reset_ends_the_bisect_even_when_its_branch_is_gone() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("reset-gone", 6, 3)?;
    let result = (|| {
        git::run(Some(&dir), &["checkout", "-q", "-b", "gone"])?;
        bisect_start(&dir, "HEAD", Some(&hashes[0]))?;
        git::run(Some(&dir), &["update-ref", "-d", "refs/heads/gone"])?;
        let said = bisect_reset(&dir);
        assert!(
            said.as_ref().is_err_and(|e| e.contains("where it was")),
            "{said:?}"
        );
        assert!(bisect_state(&dir)?.is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A good commit marked after the bad one leaves git nothing to test: the state still reads,
/// with no progress, so the marks show and it can be stopped.
#[test]
fn marks_git_can_t_bisect_between_still_read() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("contradiction", 10, 5)?;
    let result = (|| {
        bisect_start(&dir, &hashes[3], Some(&hashes[0]))?;
        let bisect = state(&dir)?;
        // git takes the mark, then says it can't go on.
        assert!(bisect_mark(&dir, &bisect, Mark::Good, Some(&hashes[9])).is_err());
        let bisect = state(&dir)?;
        assert_eq!(bisect.mark_of(&hashes[9]), Some(Mark::Good));
        assert_eq!(bisect.progress, None);
        assert_eq!(bisect.outcome, None);
        bisect_reset(&dir)?;
        assert!(bisect_state(&dir)?.is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
