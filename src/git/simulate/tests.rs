//! The simulation against real repositories: what it says an operation gives is what running
//! the operation gives (trees, the commits left on no ref, the commit and files that
//! conflict), on every engine, and the repository is exactly as it was after it.

use std::path::{Path, PathBuf};

use super::*;
use crate::git::run;

/// A repository in the temporary folder, removed when dropped.
pub(crate) struct TestRepo {
    pub(crate) dir: PathBuf,
}

impl TestRepo {
    pub(crate) fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-sim-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Self { dir };
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Ana"],
            &["config", "user.email", "ana@example.com"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    pub(crate) fn git(&self, args: &[&str]) -> Result<String, String> {
        run(Some(&self.dir), args)
    }

    pub(crate) fn rev(&self, rev: &str) -> Result<String, String> {
        Ok(self.git(&["rev-parse", rev])?.trim().to_owned())
    }

    /// Writes `files` (name, contents) and commits them as `message`; returns the commit.
    pub(crate) fn commit(&self, files: &[(&str, &str)], message: &str) -> Result<String, String> {
        for (name, text) in files {
            std::fs::write(self.dir.join(name), text).map_err(|e| e.to_string())?;
            self.git(&["add", "--", name])?;
        }
        self.git(&["commit", "-q", "--allow-empty", "-m", message])?;
        self.rev("HEAD")
    }

    /// Everything a simulation must leave as it was: refs, HEAD, the index, the working tree,
    /// the reflogs, the worktrees and the config.
    pub(crate) fn snapshot(&self) -> Result<String, String> {
        let mut out = String::new();
        out += &self.git(&["for-each-ref", "--format=%(refname) %(objectname)"])?;
        out += &self
            .git(&["symbolic-ref", "-q", "HEAD"])
            .unwrap_or_default();
        out += &self.rev("HEAD")?;
        out += &self.git(&["status", "--porcelain=v2", "--untracked-files=all"])?;
        out += &self.git(&["worktree", "list", "--porcelain"])?;
        let git_dir = self.dir.join(".git");
        for file in ["index", "config", "FETCH_HEAD", "ORIG_HEAD", "packed-refs"] {
            let bytes = std::fs::read(git_dir.join(file)).unwrap_or_default();
            out += &format!("{file}: {:x}\n", md5::compute(bytes));
        }
        let mut logs = Vec::new();
        collect_files(&git_dir.join("logs"), &mut logs);
        logs.sort();
        for log in logs {
            let bytes = std::fs::read(&log).unwrap_or_default();
            out += &format!("{}: {:x}\n", log.display(), md5::compute(bytes));
        }
        let mut work = Vec::new();
        collect_files(&self.dir, &mut work);
        work.retain(|p| !p.starts_with(&git_dir));
        work.sort();
        for file in work {
            let bytes = std::fs::read(&file).unwrap_or_default();
            out += &format!("{}: {:x}\n", file.display(), md5::compute(bytes));
        }
        Ok(out)
    }
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The git these tests run with.
fn real_version(repo: &TestRepo) -> Result<GitVersion, String> {
    GitVersion::of(&repo.dir)
}

/// The engines a replay can run on, each as the version that picks it here. `git replay` only
/// when the git running the tests has it.
fn versions(real: GitVersion) -> Vec<(Engine, GitVersion)> {
    let mut list = vec![
        (Engine::Scratch, GitVersion(2, 30, 0)),
        (Engine::MergeTree, GitVersion(2, 41, 0)),
    ];
    if real.has_replay() {
        list.push((Engine::Replay, real));
    }
    list
}

/// main: base, then `a` changed twice. feature, from base: `b` and `c` added.
fn diverged(name: &str) -> Result<(TestRepo, String), String> {
    let repo = TestRepo::new(name)?;
    let base = repo.commit(&[("a", "1\n")], "base")?;
    repo.commit(&[("a", "2\n")], "main one")?;
    repo.commit(&[("a", "3\n")], "main two")?;
    repo.git(&["switch", "-q", "-c", "feature", &base])?;
    repo.commit(&[("b", "b\n")], "feature one")?;
    repo.commit(&[("c", "c\n")], "feature two")?;
    Ok((repo, base))
}

fn no_cancel() -> bool {
    false
}

/// A rebase worked out on each engine gives the tree a real `git rebase` gives, and the same
/// number of commits; the repository is untouched by it.
#[test]
fn a_rebase_simulated_on_every_engine_is_what_git_rebase_does() -> Result<(), String> {
    let (repo, _) = diverged("rebase")?;
    let real = real_version(&repo)?;
    let picks = rebase_picks(&repo.dir, "refs/heads/main", "refs/heads/feature")?;
    assert_eq!(picks.len(), 2);
    let before = repo.snapshot()?;
    let mut results = Vec::new();
    for (engine, version) in versions(real) {
        let sim = replay(
            &repo.dir,
            "refs/heads/main",
            ("refs/heads/main", "refs/heads/feature"),
            &picks,
            version,
            &no_cancel,
        )?;
        assert_eq!(sim.engine, engine);
        assert_eq!(
            repo.snapshot()?,
            before,
            "{engine:?} changed the repository"
        );
        results.push(sim);
    }
    repo.git(&["rebase", "-q", "main"])?;
    let tree = repo.rev("feature^{tree}")?;
    let count = repo.git(&["rev-list", "--count", "main..feature"])?;
    for sim in results {
        assert!(sim.stop.is_none(), "{:?}", sim.engine);
        assert_eq!(sim.steps.last().map(|s| s.tree.clone()), Some(tree.clone()));
        let kept = sim.steps.iter().filter(|s| s.copy.is_some()).count();
        assert_eq!(kept.to_string(), count.trim(), "{:?}", sim.engine);
        assert_eq!(sim.onto, repo.rev("main")?);
    }
    Ok(())
}

/// A commit whose change upstream has already is left out, as `git rebase` leaves it; one
/// that only becomes empty on the way is dropped.
#[test]
fn commits_already_upstream_are_left_out_or_dropped() -> Result<(), String> {
    let repo = TestRepo::new("upstream")?;
    let base = repo.commit(&[("a", "1\n")], "base")?;
    repo.commit(&[("b", "b\n")], "main adds b")?;
    repo.commit(&[("x", "x\n")], "main adds x")?;
    repo.git(&["switch", "-q", "-c", "feature", &base])?;
    // The same change as main's, so a cherry-pick of it: left out.
    repo.commit(&[("b", "b\n")], "feature adds b too")?;
    repo.commit(&[("c", "c\n")], "feature adds c")?;
    let picks = rebase_picks(&repo.dir, "refs/heads/main", "refs/heads/feature")?;
    assert_eq!(picks, [repo.rev("feature")?]);
    // Replayed from base onto main with every commit (as `pull --rebase` from a fork point
    // does): the first becomes empty and is dropped.
    let all = vec![repo.rev("feature~1")?, repo.rev("feature")?];
    let real = real_version(&repo)?;
    for (engine, version) in versions(real) {
        let sim = replay(
            &repo.dir,
            "main",
            (&base, "refs/heads/feature"),
            &all,
            version,
            &no_cancel,
        )?;
        assert_eq!(sim.steps.len(), 2, "{engine:?}");
        assert_eq!(sim.steps[0].copy, None, "{engine:?}");
        assert!(sim.steps[1].copy.is_some(), "{engine:?}");
    }
    repo.git(&["rebase", "-q", "main"])?;
    let sim = super::picks::stepwise_only(&repo.dir, "main", &picks)?;
    assert_eq!(
        sim.steps.last().map(|s| s.tree.clone()),
        Some(repo.rev("feature^{tree}")?)
    );
    Ok(())
}

/// A rebase that conflicts stops at the commit `git rebase` stops at, with the same files.
#[test]
fn a_conflicting_rebase_stops_where_git_rebase_stops() -> Result<(), String> {
    let repo = TestRepo::new("conflict")?;
    let base = repo.commit(&[("a", "1\n"), ("b", "1\n")], "base")?;
    repo.commit(&[("b", "main\n")], "main changes b")?;
    repo.git(&["switch", "-q", "-c", "feature", &base])?;
    repo.commit(&[("c", "c\n")], "feature adds c")?;
    let second = repo.commit(&[("b", "feature\n")], "feature changes b")?;
    repo.commit(&[("d", "d\n")], "feature adds d")?;
    let picks = rebase_picks(&repo.dir, "main", "feature")?;
    let before = repo.snapshot()?;
    let real = real_version(&repo)?;
    let mut stops = Vec::new();
    for (engine, version) in versions(real) {
        let sim = replay(
            &repo.dir,
            "main",
            ("main", "refs/heads/feature"),
            &picks,
            version,
            &no_cancel,
        )?;
        assert_eq!(
            repo.snapshot()?,
            before,
            "{engine:?} changed the repository"
        );
        stops.push((engine, sim.steps.len(), sim.stop));
    }
    let rebased = repo.git(&["rebase", "main"]);
    let stopped_at = repo.rev("REBASE_HEAD")?;
    let files = repo.git(&["diff", "--name-only", "--diff-filter=U"])?;
    repo.git(&["rebase", "--abort"])?;
    assert!(rebased.is_err());
    assert_eq!(stopped_at, second);
    for (engine, done, stop) in stops {
        let stop = stop.ok_or(format!("{engine:?} found no conflict"))?;
        assert_eq!(done, 1, "{engine:?}");
        assert_eq!(
            (stop.step, stop.commit.as_str()),
            (1, second.as_str()),
            "{engine:?}"
        );
        assert_eq!(stop.files, files.lines().collect::<Vec<_>>(), "{engine:?}");
    }
    Ok(())
}

/// A merge worked out in memory, or in a scratch clone, has the tree a real merge has, or
/// the same files in conflict.
#[test]
fn a_merge_simulated_is_what_git_merge_does() -> Result<(), String> {
    let (repo, _) = diverged("merge")?;
    let before = repo.snapshot()?;
    let real = real_version(&repo)?;
    let clean = [
        merge(&repo.dir, "feature", "main", real)?,
        merge(&repo.dir, "feature", "main", GitVersion(2, 37, 0))?,
    ];
    assert_eq!(repo.snapshot()?, before);
    assert_eq!(
        (clean[0].engine, clean[1].engine),
        (Engine::MergeTree, Engine::Scratch)
    );
    repo.git(&["merge", "-q", "--no-ff", "--no-edit", "main"])?;
    let tree = repo.rev("HEAD^{tree}")?;
    for m in &clean {
        assert!(m.conflicts.is_empty());
        assert_eq!(m.tree, tree, "{:?}", m.engine);
    }
    // Both change `a`: a conflict in it, on either engine.
    repo.commit(&[("a", "feature\n")], "feature changes a")?;
    repo.git(&["switch", "-q", "main"])?;
    repo.commit(&[("a", "main again\n")], "main changes a")?;
    let before = repo.snapshot()?;
    let conflicted = [
        merge(&repo.dir, "main", "feature", real)?,
        merge(&repo.dir, "main", "feature", GitVersion(2, 30, 0))?,
    ];
    assert_eq!(repo.snapshot()?, before);
    let merged = repo.git(&["merge", "--no-edit", "feature"]);
    let files = repo.git(&["diff", "--name-only", "--diff-filter=U"])?;
    repo.git(&["merge", "--abort"])?;
    assert!(merged.is_err());
    for m in &conflicted {
        assert_eq!(
            m.conflicts,
            files.lines().collect::<Vec<_>>(),
            "{:?}",
            m.engine
        );
    }
    Ok(())
}

/// The commits a hard reset leaves on no ref are those git can no longer reach after it; one
/// still on another branch, or stashed, isn't among them.
#[test]
fn the_commits_left_on_no_ref_are_those_git_loses() -> Result<(), String> {
    let repo = TestRepo::new("reach")?;
    let base = repo.commit(&[("a", "1\n")], "base")?;
    repo.commit(&[("a", "2\n")], "also on keep")?;
    repo.git(&["branch", "keep"])?;
    let lost = repo.commit(&[("a", "3\n")], "only on main")?;
    let old = repo.rev("HEAD")?;
    let moved = ["refs/heads/main".to_owned(), "HEAD".to_owned()];
    let sim = unreachable(
        &repo.dir,
        std::slice::from_ref(&old),
        std::slice::from_ref(&base),
        &moved,
        100,
    )?;
    assert_eq!(sim, std::slice::from_ref(&lost));
    repo.git(&["reset", "-q", "--hard", &base])?;
    let real = repo.git(&["rev-list", &old, "--not", "--all"])?;
    assert_eq!(real.lines().collect::<Vec<_>>(), [lost.as_str()]);
    Ok(())
}

/// Asked to stop, a replay stops with [`CANCELLED`] between commits.
#[test]
fn a_cancelled_replay_stops() -> Result<(), String> {
    let (repo, _) = diverged("cancel")?;
    let picks = rebase_picks(&repo.dir, "main", "feature")?;
    for version in [GitVersion(2, 41, 0), GitVersion(2, 30, 0)] {
        let sim = replay(
            &repo.dir,
            "main",
            ("main", "refs/heads/feature"),
            &picks,
            version,
            &|| true,
        );
        assert_eq!(sim, Err(CANCELLED.to_owned()), "{version}");
    }
    Ok(())
}

/// A scratch clone made from a linked worktree is of the repository.
#[test]
fn a_scratch_replay_works_from_a_worktree_and_leaves_nothing() -> Result<(), String> {
    let (repo, _) = diverged("worktree")?;
    let wt = repo
        .dir
        .with_file_name(format!("gitcraft-sim-worktree-wt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wt);
    repo.git(&["worktree", "add", "-q", &wt.to_string_lossy(), "main"])?;
    let picks = rebase_picks(&wt, "main", "feature")?;
    let before = repo.snapshot()?;
    let sim = replay(
        &wt,
        "main",
        ("main", "refs/heads/feature"),
        &picks,
        GitVersion(2, 30, 0),
        &no_cancel,
    );
    let after = repo.snapshot()?;
    let _ = repo.git(&["worktree", "remove", "--force", &wt.to_string_lossy()]);
    let _ = std::fs::remove_dir_all(&wt);
    assert_eq!(sim?.steps.len(), 2);
    assert_eq!(after, before);
    Ok(())
}
