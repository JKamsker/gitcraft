// Fixtures. Included to preserve libtest shard names.
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
