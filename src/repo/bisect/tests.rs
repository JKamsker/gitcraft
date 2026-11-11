//! The tab's bisect driven as the window drives it: actions in, then frames' polling until
//! the work they started in the background is in.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::BisectAct;
use crate::git::{self, Mark, Outcome};
use crate::repo::{Action, RepoTab};

/// A repository of `n` commits, each writing its number to `f`, with `bug` from `bad_from` on.
fn culprit_repo(name: &str, n: usize, bad_from: usize) -> Result<(PathBuf, Vec<String>), String> {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-tab-bisect-{name}-{}", std::process::id()));
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

/// A tab on `dir`, read.
fn open(dir: &Path, ctx: &egui::Context) -> Result<RepoTab, String> {
    let mut tab = RepoTab::new(dir.to_path_buf(), ctx);
    settle(&mut tab, ctx)?;
    Ok(tab)
}

/// Polls the tab until what it started is in: no read, operation, run or bisect read left.
fn settle(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(60);
    while Instant::now() < until {
        tab.poll(ctx);
        let bisect_read = tab.bisect.shown == tab.bisect.reads;
        if tab.settled() && bisect_read && !tab.bisect.running() && tab.history_gen > 0 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    Err("the tab never settled".to_owned())
}

fn act(tab: &mut RepoTab, ctx: &egui::Context, action: BisectAct) -> Result<(), String> {
    tab.apply(Action::Bisect(action), ctx);
    settle(tab, ctx)
}

/// The commit shown in the graph's selection.
fn selected(tab: &RepoTab) -> Option<String> {
    tab.commits.get(tab.selected?).map(|c| c.hash.clone())
}

fn has_bug(dir: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(dir.join("f")).map_err(|e| e.to_string())?;
    Ok(text.contains("bug"))
}

/// Good or Bad from the banner for each commit checked out, as a person would, until the
/// bisect ends.
fn test_by_hand(tab: &mut RepoTab, ctx: &egui::Context, dir: &Path) -> Result<(), String> {
    for _ in 0..32 {
        let Some(bisect) = tab.bisect.state() else {
            return Err("the bisect went away".to_owned());
        };
        if bisect.outcome.is_some() {
            return Ok(());
        }
        let mark = if has_bug(dir)? { Mark::Bad } else { Mark::Good };
        act(tab, ctx, BisectAct::Mark { mark, rev: None })?;
        if let Some((said, true)) = tab.status.as_deref() {
            return Err(said.clone());
        }
    }
    Err("the bisect never ended".to_owned())
}

#[test]
fn a_bisect_from_the_banner_finds_the_culprit_shows_it_and_finishes() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("banner", 24, 9)?;
    let ctx = egui::Context::default();
    let result = (|| {
        let mut tab = open(&dir, &ctx)?;
        assert!(tab.bisect.state().is_none());
        // From the first commit's menu: it is good, HEAD bad.
        act(
            &mut tab,
            &ctx,
            BisectAct::Start {
                bad: "HEAD".to_owned(),
                good: Some(hashes[0].clone()),
            },
        )?;
        let started = tab.bisect.state().ok_or("no bisect after starting one")?;
        assert!(started.progress.is_some());
        assert_eq!(tab.bisect.mark_of(&hashes[0]), Some(Mark::Good));
        assert_eq!(tab.bisect.mark_of(&hashes[23]), Some(Mark::Bad));
        test_by_hand(&mut tab, &ctx, &dir)?;
        let culprit = hashes[8].clone();
        let outcome = tab.bisect.state().and_then(|b| b.outcome.clone());
        assert_eq!(outcome, Some(Outcome::FirstBad(culprit.clone())));
        assert!(tab.bisect.is_culprit(&culprit));
        // Its details are open.
        assert_eq!(selected(&tab), Some(culprit));
        act(&mut tab, &ctx, BisectAct::Reset)?;
        assert!(tab.bisect.state().is_none());
        assert_eq!(tab.refs.current.as_deref(), Some("main"));
        assert_eq!(tab.refs.head, hashes[23]);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_bisect_started_in_a_terminal_shows_on_reload_and_goes_on_from_here() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("terminal", 16, 11)?;
    let ctx = egui::Context::default();
    let result = (|| {
        let mut tab = open(&dir, &ctx)?;
        for args in [
            &["bisect", "start"][..],
            &["bisect", "bad"],
            &["bisect", "good", &hashes[0]],
        ] {
            git::run(Some(&dir), args)?;
        }
        // As the watcher does for a change in the git folder.
        tab.reload(&ctx);
        settle(&mut tab, &ctx)?;
        let picked_up = tab.bisect.state().ok_or("the bisect wasn't picked up")?;
        assert_eq!(picked_up.start_branch(), Some("main"));
        assert!(picked_up.progress.is_some());
        test_by_hand(&mut tab, &ctx, &dir)?;
        let outcome = tab.bisect.state().and_then(|b| b.outcome.clone());
        assert_eq!(outcome, Some(Outcome::FirstBad(hashes[10].clone())));
        act(&mut tab, &ctx, BisectAct::Reset)?;
        assert_eq!(tab.refs.current.as_deref(), Some("main"));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_run_from_the_banner_streams_its_output_and_finds_the_culprit() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("run", 20, 14)?;
    let ctx = egui::Context::default();
    let result = (|| {
        let mut tab = open(&dir, &ctx)?;
        act(
            &mut tab,
            &ctx,
            BisectAct::Start {
                bad: hashes[19].clone(),
                good: Some(hashes[0].clone()),
            },
        )?;
        let command = "n=$(head -n 1 f); echo \"checking $n\"; [ \"$n\" = 5 ] && exit 125; \
                       if grep -q bug f; then exit 1; fi";
        act(&mut tab, &ctx, BisectAct::Run(command.to_owned()))?;
        let run = tab.bisect.run.as_ref().ok_or("no run")?;
        let lines: Vec<String> = run
            .lines
            .lock()
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default();
        assert!(
            lines.iter().any(|l| l.starts_with("checking ")),
            "{lines:#?}"
        );
        assert!(matches!(&run.ended, Some(Ok(e)) if e.code == Some(0)));
        let outcome = tab.bisect.state().and_then(|b| b.outcome.clone());
        assert_eq!(outcome, Some(Outcome::FirstBad(hashes[13].clone())));
        assert_eq!(
            tab.status
                .as_deref()
                .map(|(s, failed)| (s.as_str(), *failed)),
            Some(("Bisect run: found the first bad commit", false))
        );
        // The command is offered again.
        assert_eq!(tab.bisect.last_command, command);
        act(&mut tab, &ctx, BisectAct::CloseLog)?;
        assert!(tab.bisect.run.is_none());
        act(&mut tab, &ctx, BisectAct::Reset)?;
        assert!(tab.bisect.state().is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_start_with_uncommitted_changes_is_refused_with_why() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("dirty", 4, 3)?;
    let ctx = egui::Context::default();
    let result = (|| {
        std::fs::write(dir.join("f"), "changed\n").map_err(|e| e.to_string())?;
        let mut tab = open(&dir, &ctx)?;
        assert!(tab.bisect_blocker().is_some());
        act(
            &mut tab,
            &ctx,
            BisectAct::Start {
                bad: "HEAD".to_owned(),
                good: Some(hashes[0].clone()),
            },
        )?;
        assert!(tab.bisect.state().is_none());
        let (said, failed) = tab.status.as_deref().cloned().ok_or("nothing said")?;
        assert!(failed && said.contains("uncommitted changes"), "{said}");
        assert!(git::bisect_state(&dir)?.is_none());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A bisect that ended before the tab opened shows its end, but the selection stays where
/// the tab puts it: only an end found while the tab watches selects the culprit.
#[test]
fn opening_on_a_finished_bisect_keeps_the_selection() -> Result<(), String> {
    let (dir, hashes) = culprit_repo("finished", 8, 3)?;
    let ctx = egui::Context::default();
    let result = (|| {
        // Use Git's own shell via the same builtin as the banner's run action. An
        // external `sh` on PATH need not be Git for Windows' shell.
        let run = git::bisect_run_args("! grep -q bug f");
        let run: Vec<&str> = run.iter().map(String::as_str).collect();
        for args in [
            &["bisect", "start", &hashes[7], &hashes[0]][..],
            run.as_slice(),
        ] {
            // A broken fixture must fail with its command instead of hanging the
            // whole Windows shard before the tab's own bounded settle loop starts.
            git::run_unattended_within(&dir, args, Duration::from_secs(60))
                .map_err(|e| format!("finished bisect fixture {args:?}: {e}"))?;
        }
        let tab = open(&dir, &ctx)?;
        let outcome = tab.bisect.state().and_then(|b| b.outcome.clone());
        assert_eq!(outcome, Some(Outcome::FirstBad(hashes[2].clone())));
        assert_ne!(selected(&tab), Some(hashes[2].clone()));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
