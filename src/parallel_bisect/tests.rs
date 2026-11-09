//! Parallel bisects of real temporary repositories with a known first bad commit, run as the
//! tab runs them (through `start`), with what they leave behind checked after.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::git;

/// A scratch folder of its own for the test `name`.
fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-pb-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A path as `sh` (also Git for Windows' own) takes it in a command.
fn sh_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// A repository in `dir` with `n` commits on a line, `c0` the oldest. Commit `i` changes
/// `n.txt` and holds the marker files `files(i)` (`bug`, `skip`, `slow`), only those.
struct Repo {
    dir: PathBuf,
    commits: Vec<String>,
}

impl Repo {
    fn new(
        dir: &Path,
        n: usize,
        files: &dyn Fn(usize) -> Vec<&'static str>,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| git::run(Some(dir), args);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            git(args)?;
        }
        let mut commits = Vec::new();
        for i in 0..n {
            std::fs::write(dir.join("n.txt"), format!("{i}\n")).map_err(|e| e.to_string())?;
            let wanted = files(i);
            for marker in ["bug", "skip", "slow"] {
                let path = dir.join(marker);
                if wanted.contains(&marker) {
                    std::fs::write(&path, "x\n").map_err(|e| e.to_string())?;
                } else if path.exists() {
                    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
                }
            }
            git(&["add", "-A"])?;
            git(&["commit", "-q", "-m", &format!("c{i}")])?;
            commits.push(git(&["rev-parse", "HEAD"])?.trim().to_owned());
        }
        Ok(Repo {
            dir: dir.to_path_buf(),
            commits,
        })
    }

    /// A repository where commits from `culprit` on have the bug.
    fn with_culprit(dir: &Path, n: usize, culprit: usize) -> Result<Self, String> {
        Self::new(dir, n, &|i| if i >= culprit { vec!["bug"] } else { vec![] })
    }

    /// A bisect of all of it: the newest commit bad, the oldest good.
    fn setup(&self, command: &str, parallel: usize) -> Setup {
        Setup {
            repo: self.dir.clone(),
            bad: self.commits.last().cloned().unwrap_or_default(),
            good: self.commits.first().cloned().into_iter().collect(),
            skip: Vec::new(),
            command: command.to_owned(),
            machine: Machine::Here,
            repo_there: None,
            parallel,
            timeout: Duration::from_secs(60),
        }
    }

    /// Nothing of a bisect is left: no worktree but the repository's own, no folder, no ref.
    fn assert_clean(&self) -> Result<(), String> {
        assert_clean(&self.dir)
    }
}

fn assert_clean(dir: &Path) -> Result<(), String> {
    let list = git::run(Some(dir), &["worktree", "list", "--porcelain"])?;
    let worktrees = list.lines().filter(|l| l.starts_with("worktree ")).count();
    assert_eq!(worktrees, 1, "{list}");
    let (base, _) = workspace::base_for(dir, &Machine::Here)?;
    assert!(!base.exists(), "{} is still there", base.display());
    let refs = git::run(Some(dir), &["for-each-ref", "refs/gitcraft-bisect/"])?;
    assert!(refs.trim().is_empty(), "{refs}");
    Ok(())
}

/// The test the repositories are bisected with: bad where `bug` is, skipped where `skip` is,
/// hanging where `slow` is; each tree it tests written to `log`, to count.
fn command(log: &Path) -> String {
    format!(
        "git rev-parse HEAD^{{tree}} >> '{}'; [ -e skip ] && exit 125; \
         if [ -e slow ]; then sleep 6789 & sleeper=$!; echo \"$sleeper\" >> '{}'; \
         wait \"$sleeper\"; fi; test ! -e bug",
        sh_path(log),
        sh_path(&log.with_extension("pids"))
    )
}

/// Bisects as `setup` says, with `cache`, until it ends (calling `meanwhile` with each event,
/// and the handle); how it ended and what it said on the way.
fn bisect_with(
    setup: Setup,
    cache: &TreeCache,
    meanwhile: &mut dyn FnMut(&Event, &Handle),
) -> (Outcome, Vec<Event>) {
    let handle = start(setup, cache.clone(), || {});
    let mut seen = Vec::new();
    let until = Instant::now() + Duration::from_secs(240);
    while Instant::now() < until {
        for event in handle.events() {
            meanwhile(&event, &handle);
            if let Event::Done(outcome) = &event {
                let outcome = outcome.clone();
                seen.push(event);
                return (outcome, seen);
            }
            seen.push(event);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    (Outcome::Failed("the bisect never ended".to_owned()), seen)
}

fn bisect(setup: Setup) -> (Outcome, Vec<Event>) {
    bisect_with(setup, &TreeCache::default(), &mut |_, _| {})
}

/// The tests that really ran (not answered from the cache).
fn tests_run(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Tested { cached: false, .. }))
        .count()
}

/// No tree was tested twice.
fn assert_each_tree_once(log: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(log).map_err(|e| e.to_string())?;
    let mut trees: Vec<&str> = text.lines().collect();
    let all = trees.len();
    trees.sort_unstable();
    trees.dedup();
    assert_eq!(trees.len(), all, "a tree was tested twice:\n{text}");
    Ok(())
}

/// Check only this fixture's sleepers: other bisect tests can run at the same time.
/// SIGKILL delivery is asynchronous, and an orphaned zombie has already stopped.
#[cfg(unix)]
fn sleepers_stopped(log: &Path) -> Result<bool, String> {
    let pids = std::fs::read_to_string(log.with_extension("pids")).map_err(|e| e.to_string())?;
    let pids = pids
        .lines()
        .map(|pid| pid.parse::<u32>().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    assert!(!pids.is_empty(), "no slow test started");
    let pids = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pids])
            .output()
            .map_err(|e| e.to_string())?;
        // ps exits 1 when none of the selected processes exist.
        if !matches!(out.status.code(), Some(0 | 1)) {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }
        if String::from_utf8_lossy(&out.stdout)
            .lines()
            .all(|state| state.trim().starts_with('Z'))
        {
            return Ok(true);
        }
        if Instant::now() >= until {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn finds_the_first_bad_commit_with_any_parallelism() -> Result<(), String> {
    let dir = scratch("parallelism")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 40, 23)?;
    for k in [1, 2, 4, 8] {
        let log = dir.join(format!("trees-{k}"));
        let (outcome, events) = bisect(repo.setup(&command(&log), k));
        assert_eq!(
            outcome,
            Outcome::FirstBad(repo.commits[23].clone()),
            "k {k}: {events:?}"
        );
        assert_each_tree_once(&log)?;
        // Never more at once than asked for.
        for e in &events {
            if let Event::Round { testing, .. } = e {
                assert!(testing.len() <= k, "k {k}: {testing:?}");
            }
        }
        // Fewer tests than there are commits.
        assert!(tests_run(&events) < 25, "k {k}: {}", tests_run(&events));
        repo.assert_clean()?;
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// More at once takes fewer rounds.
#[test]
fn more_at_once_takes_fewer_rounds() -> Result<(), String> {
    let dir = scratch("rounds")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 64, 41)?;
    let rounds = |k| -> Result<usize, String> {
        let (outcome, events) = bisect(repo.setup(&command(&dir.join(format!("t{k}"))), k));
        assert_eq!(outcome, Outcome::FirstBad(repo.commits[41].clone()));
        Ok(events
            .iter()
            .filter(|e| matches!(e, Event::Round { .. }))
            .count())
    };
    let (one, eight) = (rounds(1)?, rounds(8)?);
    // log2(64) = 6 steps and the two ends one by one, against about log9(64) = 2 rounds.
    assert!(one >= 7, "{one}");
    assert!(eight <= 4, "{eight}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A second bisect with the same command answers every tree from the cache.
#[test]
fn trees_tested_before_are_not_tested_again() -> Result<(), String> {
    let dir = scratch("cache")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 30, 11)?;
    let cache = TreeCache::default();
    let setup = repo.setup(&command(&dir.join("trees")), 4);
    let (first, events) = bisect_with(setup.clone(), &cache, &mut |_, _| {});
    assert_eq!(first, Outcome::FirstBad(repo.commits[11].clone()));
    assert!(tests_run(&events) > 0);
    let (again, events) = bisect_with(setup.clone(), &cache, &mut |_, _| {});
    assert_eq!(again, first);
    assert_eq!(tests_run(&events), 0, "{events:?}");
    // Another command is another cache.
    let other = Setup {
        command: format!("{} ", setup.command) + "&& true",
        ..setup
    };
    let (_, events) = bisect_with(other, &cache, &mut |_, _| {});
    assert!(tests_run(&events) > 0);
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Commits with the same tree (a revert and its revert) are tested once between them.
#[test]
fn a_tree_shared_by_commits_is_tested_once() -> Result<(), String> {
    let dir = scratch("same-tree")?;
    // The bug comes in at c9.
    let repo = Repo::new(&dir.join("repo"), 14, &|i| {
        if i >= 9 { vec!["bug"] } else { vec![] }
    })?;
    let git = |args: &[&str]| git::run(Some(&repo.dir), args);
    // Two more commits on top whose trees are those of c12 and c13: same answers.
    git(&["revert", "--no-edit", "HEAD"])?;
    git(&["revert", "--no-edit", "HEAD"])?;
    let tip = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    let log = dir.join("trees");
    let setup = Setup {
        bad: tip,
        ..repo.setup(&command(&log), 8)
    };
    let (outcome, _) = bisect(setup);
    assert_eq!(outcome, Outcome::FirstBad(repo.commits[9].clone()));
    assert_each_tree_once(&log)?;
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_skipped_first_bad_commit_leaves_a_choice() -> Result<(), String> {
    let dir = scratch("skip")?;
    let repo = Repo::new(&dir.join("repo"), 30, &|i| match i {
        17 => vec!["bug", "skip"],
        i if i > 17 => vec!["bug"],
        6 => vec!["skip"],
        _ => vec![],
    })?;
    for k in [1, 3] {
        let (outcome, _) = bisect(repo.setup(&command(&dir.join(format!("t{k}"))), k));
        assert_eq!(
            outcome,
            Outcome::OneOf(vec![repo.commits[18].clone(), repo.commits[17].clone()]),
            "k {k}"
        );
    }
    repo.assert_clean()?;
    // A commit marked skipped beforehand (git bisect's own skips) isn't tested.
    let log = dir.join("pre-skipped");
    let setup = Setup {
        skip: vec![repo.commits[17].clone()],
        ..repo.setup(&command(&log), 2)
    };
    let tree = git::run(
        Some(&repo.dir),
        &["rev-parse", &format!("{}^{{tree}}", repo.commits[17])],
    )?;
    let (outcome, _) = bisect(setup);
    assert!(matches!(outcome, Outcome::OneOf(_)), "{outcome:?}");
    let tested = std::fs::read_to_string(&log).map_err(|e| e.to_string())?;
    assert!(!tested.contains(tree.trim()), "{tested}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A test that runs past its limit is stopped, and counts as a skip.
#[test]
fn a_test_past_its_limit_is_stopped_and_skipped() -> Result<(), String> {
    let dir = scratch("timeout")?;
    let repo = Repo::new(&dir.join("repo"), 12, &|i| match i {
        7 => vec!["bug", "slow"],
        i if i > 7 => vec!["bug"],
        _ => vec![],
    })?;
    let setup = Setup {
        timeout: Duration::from_secs(3),
        ..repo.setup(&command(&dir.join("trees")), 4)
    };
    let started = Instant::now();
    let (outcome, events) = bisect(setup);
    assert_eq!(
        outcome,
        Outcome::OneOf(vec![repo.commits[8].clone(), repo.commits[7].clone()])
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Tested { verdict: Verdict::TimedOut, hash, .. } if *hash == repo.commits[7]
    )));
    assert!(started.elapsed() < Duration::from_secs(120));
    #[cfg(unix)]
    assert!(
        sleepers_stopped(&dir.join("trees"))?,
        "the test that timed out still runs"
    );
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_test_that_fails_on_both_ends_is_reported() -> Result<(), String> {
    let dir = scratch("ends")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 10, 5)?;
    for k in [1, 4] {
        let (outcome, _) = bisect(repo.setup("exit 1", k));
        assert!(
            matches!(&outcome, Outcome::GoodEndFails { hash, .. } if *hash == repo.commits[0]),
            "k {k}: {outcome:?}"
        );
        repo.assert_clean()?;
        let (outcome, _) = bisect(repo.setup("echo fine", k));
        assert!(
            matches!(&outcome, Outcome::BadEndPasses { hash, output } if *hash == repo.commits[9] && output.contains("fine")),
            "k {k}: {outcome:?}"
        );
        repo.assert_clean()?;
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// 128 and more stops the bisect, as with `git bisect run`; the worktrees go all the same.
#[test]
fn an_exit_code_of_128_or_more_stops_it() -> Result<(), String> {
    let dir = scratch("crash")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 10, 5)?;
    let (outcome, _) = bisect(repo.setup("echo broken; exit 200", 3));
    assert!(
        matches!(&outcome, Outcome::Failed(e) if e.contains("200") && e.contains("broken")),
        "{outcome:?}"
    );
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A crash (128 or more) on a commit between the ends counts as a skip, with a warning: a bisect
/// one commit at a time might never have tested it. (Here c7, just before the first bad c8, is
/// needed: the answer is a choice of the two.)
#[test]
fn a_crash_between_the_ends_is_a_skip() -> Result<(), String> {
    let dir = scratch("crash-between")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 10, 8)?;
    let command = "[ \"$(cat n.txt)\" = 7 ] && exit 139; test ! -e bug";
    for k in [1, 10] {
        let (outcome, events) = bisect(repo.setup(command, k));
        let either = vec![repo.commits[8].clone(), repo.commits[7].clone()];
        assert_eq!(outcome, Outcome::OneOf(either), "k {k}");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Warning(w) if w.contains("139")))
        );
    }
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Cancelling stops every test running and removes the worktrees.
#[test]
fn cancelling_stops_the_tests_and_removes_the_worktrees() -> Result<(), String> {
    let dir = scratch("cancel")?;
    let repo = Repo::new(&dir.join("repo"), 20, &|_| vec!["slow"])?;
    let started = Instant::now();
    let mut child_started = false;
    let (outcome, _) = bisect_with(
        repo.setup(&command(&dir.join("trees")), 4),
        &TreeCache::default(),
        &mut |event, handle| {
            if matches!(event, Event::Round { .. }) {
                // Round is emitted before workspace setup and child dispatch. Wait
                // for this fixture's actual start marker, even under suite load.
                let deadline = Instant::now() + Duration::from_secs(30);
                while Instant::now() < deadline {
                    if std::fs::read_to_string(dir.join("trees.pids"))
                        .is_ok_and(|text| text.lines().any(|line| !line.trim().is_empty()))
                    {
                        child_started = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                // Cancel even if startup failed, so the fixture always joins its work.
                handle.cancel();
            }
        },
    );
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(
        child_started,
        "no owned slow child reached its start marker"
    );
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "{:?}",
        started.elapsed()
    );
    #[cfg(unix)]
    assert!(
        sleepers_stopped(&dir.join("trees"))?,
        "a cancelled test still runs"
    );
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Completed durable owners and a released claim let the next claimant recover worktrees
/// and generation refs; a still-held claim prevents takeover regardless of PID or time.
#[test]
fn a_crashed_bisect_is_cleared_up_by_the_next() -> Result<(), String> {
    let dir = scratch("leftovers")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 10, 4)?;
    let leave_behind = || -> Result<workspace::Workspace, String> {
        let ws = workspace::Workspace::open(&repo.dir, &Machine::Here, None)?;
        let (_, slot) = ws.slot(1);
        let limit = Duration::from_secs(60);
        let added = ws.run_git(
            &repo.dir,
            &["worktree", "add", "-q", "--detach", &slot, &repo.commits[2]],
            limit,
        )?;
        assert_eq!(added.code, Some(0), "{:?}", added.stderr);
        let authority = ws.authority(crate::remote::workspace_authority::FenceMode::Mutation)?;
        let reference = format!("refs/gitcraft-bisect/{}/sent-0", authority.generation);
        let published = ws.run_git(
            &repo.dir,
            &["update-ref", &reference, &repo.commits[3]],
            limit,
        )?;
        assert_eq!(published.code, Some(0), "{:?}", published.stderr);
        let mut records = 0;
        for entry in std::fs::read_dir(&authority.root).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|ext| ext == "owner") {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                assert_eq!(value["v"], 3);
                assert_eq!(value["generation"], authority.generation);
                assert_eq!(crate::remote::recover_owner(&path)?, bytes);
                records += 1;
            }
        }
        assert!(records >= 2, "real operations must publish durable owners");
        Ok(ws)
    };
    let ws = leave_behind()?;
    let base = ws.base.clone();
    let (outcome, _) = bisect(repo.setup(&command(&dir.join("t1")), 2));
    assert!(
        matches!(&outcome, Outcome::Failed(e) if e.contains("another workspace claimant")),
        "{outcome:?}"
    );
    assert!(base.join("bisect-1").exists());
    ws.leave_completed_fixture()?;
    workspace::clear_leftovers(&repo.dir);
    repo.assert_clean()?;
    let ws = leave_behind()?;
    ws.leave_completed_fixture()?;
    let (outcome, events) = bisect(repo.setup(&command(&dir.join("t2")), 2));
    assert_eq!(
        outcome,
        Outcome::FirstBad(repo.commits[4].clone()),
        "{events:?}"
    );
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A legacy timestamp/PID file cannot authorize cleanup, even if it looks ancient.
#[test]
fn legacy_crash_evidence_is_refused_and_retained() -> Result<(), String> {
    let dir = scratch("legacy-leftovers")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 10, 4)?;
    let (base, _) = workspace::base_for(&repo.dir, &Machine::Here)?;
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let slot = sh_path(&base.join("bisect-1"));
    git::run(
        Some(&repo.dir),
        &["worktree", "add", "-q", "--detach", &slot, &repo.commits[2]],
    )?;
    let reference = "refs/gitcraft-bisect/sent-0";
    git::run(
        Some(&repo.dir),
        &["update-ref", reference, &repo.commits[3]],
    )?;
    let evidence = b"gitcraft 1 1000\n";
    std::fs::write(base.join("owner"), evidence).map_err(|e| e.to_string())?;
    let (outcome, _) = bisect(repo.setup(&command(&dir.join("trees")), 2));
    assert!(matches!(outcome, Outcome::Failed(e) if e.contains("unresolved legacy evidence")));
    workspace::clear_leftovers(&repo.dir);
    assert_eq!(
        std::fs::read(base.join("owner")).map_err(|e| e.to_string())?,
        evidence
    );
    assert!(base.join("bisect-1").exists());
    assert_eq!(
        git::run(Some(&repo.dir), &["rev-parse", reference])?.trim(),
        repo.commits[3]
    );
    // Only this fixture's completed Git operations created these artifacts; no owners
    // or processes are inferred from the deliberately unauthenticated legacy text.
    git::run(Some(&repo.dir), &["worktree", "remove", "--force", &slot])?;
    git::run(
        Some(&repo.dir),
        &["update-ref", "-d", reference, &repo.commits[3]],
    )?;
    std::fs::remove_file(base.join("owner")).map_err(|e| e.to_string())?;
    std::fs::remove_dir(&base).map_err(|e| e.to_string())?;
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Clearing up never forgets the user's own worktrees, not even one whose folder is away (an
/// unmounted drive): only the bisect's.
#[test]
fn clearing_up_keeps_the_user_s_worktrees() -> Result<(), String> {
    let dir = scratch("own-worktrees")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 8, 4)?;
    let away = dir.join("away");
    git::run(
        Some(&repo.dir),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &sh_path(&away),
            &repo.commits[1],
        ],
    )?;
    std::fs::rename(&away, dir.join("moved")).map_err(|e| e.to_string())?;
    let (outcome, _) = bisect(repo.setup("test ! -e bug", 3));
    assert_eq!(outcome, Outcome::FirstBad(repo.commits[4].clone()));
    let list = git::run(Some(&repo.dir), &["worktree", "list", "--porcelain"])?;
    assert!(list.contains("away"), "{list}");
    assert_eq!(list.matches("worktree ").count(), 2, "{list}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Bisecting in another repository (another machine's, here one on this machine named as
/// such): the commits are sent there first, all of them, or only what it lacks.
#[test]
fn the_commits_are_sent_to_another_repository() -> Result<(), String> {
    let dir = scratch("send")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 16, 9)?;
    let elsewhere = dir.join("elsewhere");
    git::run(None, &["init", "-q", &sh_path(&elsewhere)])?;
    let setup = Setup {
        repo_there: Some(sh_path(&elsewhere)),
        ..repo.setup(&command(&dir.join("t1")), 4)
    };
    let (outcome, _) = bisect(setup.clone());
    assert_eq!(outcome, Outcome::FirstBad(repo.commits[9].clone()));
    assert_clean(&elsewhere)?;
    repo.assert_clean()?;
    // A clone that has the good end: only the rest is sent.
    let clone = dir.join("clone");
    git::run(
        None,
        &["clone", "-q", &sh_path(&repo.dir), &sh_path(&clone)],
    )?;
    git::run(Some(&clone), &["reset", "-q", "--hard", &repo.commits[0]])?;
    git::run(
        Some(&repo.dir),
        &["commit", "-q", "--allow-empty", "-m", "newer"],
    )?;
    let newer = git::run(Some(&repo.dir), &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let setup = Setup {
        bad: newer,
        repo_there: Some(sh_path(&clone)),
        ..setup
    };
    let (outcome, _) = bisect(setup);
    assert_eq!(outcome, Outcome::FirstBad(repo.commits[9].clone()));
    assert_clean(&clone)?;
    // A folder that isn't a repository is said to be one.
    let setup = Setup {
        repo_there: Some(sh_path(&dir.join("nothing"))),
        ..repo.setup("true", 2)
    };
    let (outcome, _) = bisect(setup);
    assert!(
        matches!(&outcome, Outcome::Failed(e) if e.contains("not a git repository")),
        "{outcome:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A bisect across merges finds the same commit as `git bisect run` does.
#[test]
fn finds_what_git_bisect_run_finds_across_merges() -> Result<(), String> {
    let dir = scratch("merges")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 3, 99)?;
    let git = |args: &[&str]| git::run(Some(&repo.dir), args);
    // Two branches off c2, each a few commits, merged; the bug comes in on the second.
    let mut hashes = Vec::new();
    for (branch, from_bug) in [("one", 99), ("two", 2)] {
        git(&["checkout", "-q", "-b", branch, &repo.commits[2]])?;
        for i in 0..5 {
            std::fs::write(repo.dir.join(format!("{branch}.txt")), format!("{i}\n"))
                .map_err(|e| e.to_string())?;
            if i >= from_bug {
                std::fs::write(repo.dir.join("bug"), "x\n").map_err(|e| e.to_string())?;
            }
            git(&["add", "-A"])?;
            git(&["commit", "-q", "-m", &format!("{branch}{i}")])?;
            hashes.push(git(&["rev-parse", "HEAD"])?.trim().to_owned());
        }
    }
    git(&["checkout", "-q", "one"])?;
    git(&["merge", "-q", "--no-edit", "two"])?;
    git(&["commit", "-q", "--allow-empty", "-m", "after"])?;
    let tip = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    // What git itself finds.
    git(&["bisect", "start", &tip, &repo.commits[0]])?;
    let found = git(&["bisect", "run", "sh", "-c", "test ! -e bug"])?;
    git(&["bisect", "reset"])?;
    let want = hashes[7].clone();
    // Newer gits put the term in quotes: "is the first 'bad' commit".
    let said = found
        .lines()
        .find(|l| l.contains("is the first") && l.contains("bad"))
        .unwrap_or_default();
    assert!(said.starts_with(&want), "{found}");
    for k in [1, 2, 4, 8] {
        let setup = Setup {
            bad: tip.clone(),
            ..repo.setup("test ! -e bug", k)
        };
        let (outcome, _) = bisect(setup);
        assert_eq!(outcome, Outcome::FirstBad(want.clone()), "k {k}");
    }
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// On another machine, when one is named (skipped otherwise): `GITCRAFT_TEST_BISECT_SSH=<host>`
/// or `GITCRAFT_TEST_BISECT_WSL=<distro>`. A repository here is bisected there, its commits
/// sent to the copy gitcraft keeps there; then a run is cancelled. Nothing is left there but
/// that copy.
#[test]
fn bisects_on_another_machine() -> Result<(), String> {
    use crate::remote::Host;
    let host = match (
        std::env::var("GITCRAFT_TEST_BISECT_SSH"),
        std::env::var("GITCRAFT_TEST_BISECT_WSL"),
    ) {
        (Ok(ssh), _) => Host::Ssh(ssh),
        (_, Ok(wsl)) => Host::Wsl(wsl),
        _ => return Ok(()),
    };
    let dir = scratch("elsewhere")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 40, 29)?;
    let machine = Machine::There(host);
    for k in [1, 4, 8] {
        let setup = Setup {
            machine: machine.clone(),
            ..repo.setup("test ! -e bug", k)
        };
        let started = Instant::now();
        let (outcome, _) = bisect(setup);
        eprintln!("k {k}: {outcome:?} in {:?}", started.elapsed());
        assert_eq!(outcome, Outcome::FirstBad(repo.commits[29].clone()));
    }
    let slow = Setup {
        machine: machine.clone(),
        ..repo.setup("sleep 6789", 4)
    };
    let (outcome, _) = bisect_with(slow, &TreeCache::default(), &mut |event, handle| {
        if matches!(event, Event::Round { .. }) {
            std::thread::sleep(Duration::from_secs(3));
            handle.cancel();
        }
    });
    assert_eq!(outcome, Outcome::Cancelled);
    // Workspace deletion is allowed only after every supervisor's stop/reap receipt.
    let Machine::There(host) = &machine else {
        return Ok(());
    };
    let req = crate::remote::Request {
        script: "for d in ~/.cache/gitcraft/bisect/*/; do case $d in */run-*/) echo \"left: $d\";; esac; done".to_owned(),
        ..crate::remote::Request::default()
    };
    let left = crate::remote::exec(host, &req)?;
    assert_eq!(String::from_utf8_lossy(&left.stdout).trim(), "");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
