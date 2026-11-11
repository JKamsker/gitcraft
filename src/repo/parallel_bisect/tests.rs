//! The tab's parallel bisect driven as the window drives it: actions in, then frames' polling
//! until the bisect it started has ended.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{ParallelAct, marks::RowMark};
use crate::git::{self, Mark};
use crate::parallel_bisect::{Machine, Outcome, Setup, Verdict};
use crate::repo::bisect::BisectAct;
use crate::repo::{Action, Dialog, RepoTab};

/// A repository of `n` commits, each writing its number to `f`, with `bug` from `bad_from` on.
fn culprit_repo(name: &str, n: usize, bad_from: usize) -> Result<(PathBuf, Vec<String>), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-tab-pb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
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

/// The test command: bad where `f` says `bug`, hanging when `slow` is set.
const TEST: &str = "! grep -q bug f";

/// Polls the tab until `done` says so.
fn until(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        tab.poll(ctx);
        if done(tab) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Err("the tab never got there".to_owned())
}

fn open(dir: &Path, ctx: &egui::Context) -> Result<RepoTab, String> {
    let mut tab = RepoTab::new(dir.to_path_buf(), ctx);
    until(&mut tab, ctx, |t| t.settled() && t.history_gen > 0)?;
    Ok(tab)
}

fn setup(dir: &Path, bad: &str, good: &str, command: &str, parallel: usize) -> Setup {
    Setup {
        repo: dir.to_path_buf(),
        bad: bad.to_owned(),
        good: vec![good.to_owned()],
        skip: Vec::new(),
        command: command.to_owned(),
        machine: Machine::Here,
        repo_there: None,
        parallel,
        timeout: Duration::from_secs(60),
    }
}

fn ended(tab: &RepoTab) -> bool {
    tab.parallel.view().is_some_and(|v| !v.running())
}

fn outcome(tab: &RepoTab) -> Option<Outcome> {
    tab.parallel.view()?.outcome.clone()
}

#[test]
fn a_parallel_bisect_from_a_commit_finds_the_culprit_and_shows_it() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("menu", 30, 12)?;
    let ctx = egui::Context::default();
    let result = (|| {
        let mut tab = open(&dir, &ctx)?;
        // From the first commit's menu: the dialog opens, it good and HEAD bad.
        let from_menu = tab.parallel_from(&hashes[0]);
        tab.apply(from_menu, &ctx);
        assert!(matches!(tab.dialog, Some(Dialog::ParallelBisect(_))));
        tab.dialog = None;
        let head_before = git::head(&dir);
        tab.apply(
            Action::ParallelBisect(ParallelAct::Start(Box::new(setup(
                &dir, "HEAD", &hashes[0], TEST, 4,
            )))),
            &ctx,
        );
        assert!(tab.parallel.running());
        // What may still be the culprit shows while it runs.
        until(&mut tab, &ctx, |t| {
            t.parallel_row(&hashes[20]).is_some() || ended(t)
        })?;
        until(&mut tab, &ctx, ended)?;
        let culprit = hashes[11].clone();
        assert_eq!(outcome(&tab), Some(Outcome::FirstBad(culprit.clone())));
        assert_eq!(tab.parallel_row(&culprit), Some(RowMark::FirstBad));
        // The rows tested say how; nothing is left "may still be".
        let tested = hashes
            .iter()
            .filter(|h| matches!(tab.parallel_row(h), Some(RowMark::Tested(_))))
            .count();
        assert!(tested >= 2, "{tested}");
        assert!(
            hashes
                .iter()
                .all(|h| tab.parallel_row(h) != Some(RowMark::Left))
        );
        assert_eq!(
            tab.parallel_row(&hashes[0]),
            Some(RowMark::Tested(Verdict::Good))
        );
        // Selected, said, and the checkout untouched.
        let selected = tab
            .commits
            .get(tab.selected.ok_or("nothing selected")?)
            .map(|c| c.hash.clone());
        assert_eq!(selected, Some(culprit));
        assert!(
            tab.status
                .as_deref()
                .is_some_and(|(s, failed)| s.contains("first bad commit") && !failed)
        );
        assert_eq!(git::head(&dir), head_before);
        // Closed, the graph shows nothing of it any more.
        tab.apply(Action::ParallelBisect(ParallelAct::Close), &ctx);
        assert!(tab.parallel.view().is_none());
        assert!(tab.parallel_row(&hashes[11]).is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// Started from a bisect's marks, its answer marked back in the bisect ends that one too.
#[test]
fn a_bisect_s_marks_run_in_parallel_and_its_answer_goes_back() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("marks", 20, 7)?;
    let ctx = egui::Context::default();
    let result = (|| {
        let mut tab = open(&dir, &ctx)?;
        tab.apply(
            Action::Bisect(BisectAct::Start {
                bad: "HEAD".to_owned(),
                good: Some(hashes[0].clone()),
            }),
            &ctx,
        );
        until(&mut tab, &ctx, |t| {
            t.settled() && t.bisect.state().is_some_and(|b| b.progress.is_some())
        })?;
        let Some(Action::ParallelBisect(ParallelAct::Open { bad, good, .. })) =
            tab.parallel_from_bisect()
        else {
            return Err("the bisect's marks offer no parallel bisect".to_owned());
        };
        assert_eq!(bad, hashes[19]);
        assert_eq!(good, vec![hashes[0].clone()]);
        let start = setup(&dir, &bad, &good[0], TEST, 3);
        tab.apply(
            Action::ParallelBisect(ParallelAct::Start(Box::new(start))),
            &ctx,
        );
        until(&mut tab, &ctx, ended)?;
        assert_eq!(outcome(&tab), Some(Outcome::FirstBad(hashes[6].clone())));
        tab.apply(Action::ParallelBisect(ParallelAct::MarkInBisect), &ctx);
        until(&mut tab, &ctx, |t| {
            t.settled() && t.bisect.state().is_some_and(|b| b.outcome.is_some())
        })?;
        let found = tab.bisect.state().and_then(|b| b.outcome.clone());
        assert_eq!(found, Some(git::Outcome::FirstBad(hashes[6].clone())));
        assert_eq!(tab.bisect.mark_of(&hashes[6]), Some(Mark::Bad));
        git::bisect_reset(&dir)?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn cancelling_from_the_banner_ends_it() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("cancel", 12, 5)?;
    let ctx = egui::Context::default();
    let result = (|| {
        let mut tab = open(&dir, &ctx)?;
        let slow = setup(&dir, "HEAD", &hashes[0], "sleep 30", 2);
        tab.apply(
            Action::ParallelBisect(ParallelAct::Start(Box::new(slow))),
            &ctx,
        );
        until(&mut tab, &ctx, |t| {
            t.parallel.view().is_some_and(|v| v.round > 0)
        })?;
        let asked = Instant::now();
        tab.apply(Action::ParallelBisect(ParallelAct::Cancel), &ctx);
        until(&mut tab, &ctx, ended)?;
        assert_eq!(outcome(&tab), Some(Outcome::Cancelled));
        assert!(
            asked.elapsed() < Duration::from_secs(25),
            "{:?}",
            asked.elapsed()
        );
        // Nothing tested shows as testing any more.
        assert!(
            hashes
                .iter()
                .all(|h| tab.parallel_row(h) != Some(RowMark::Testing))
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_good_end_that_fails_says_so() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("good-fails", 8, 3)?;
    let ctx = egui::Context::default();
    let result = (|| {
        let mut tab = open(&dir, &ctx)?;
        let start = setup(&dir, "HEAD", &hashes[0], "echo nope; exit 1", 4);
        tab.apply(
            Action::ParallelBisect(ParallelAct::Start(Box::new(start))),
            &ctx,
        );
        until(&mut tab, &ctx, ended)?;
        assert!(matches!(outcome(&tab), Some(Outcome::GoodEndFails { .. })));
        let (said, failed) = tab.status.as_deref().cloned().ok_or("nothing said")?;
        assert!(
            failed && said.contains("the good end already fails"),
            "{said}"
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
