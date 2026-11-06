//! The overview against real repositories: here through git's commands one by one, and through
//! the script a repository elsewhere is asked with, run here by `sh`, which must read the same.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::*;
use crate::git::run;

/// A path as git printed it, here.
fn plain(p: &str) -> PathBuf {
    PathBuf::from(p)
}

fn scratch(name: &str) -> Result<PathBuf, String> {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-overview-{}-{n}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    run(Some(dir), args)
}

fn init(dir: &Path) -> Result<(), String> {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Tëst"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(dir, args)?;
    }
    Ok(())
}

fn commit(dir: &Path, file: &str, text: &str) -> Result<(), String> {
    std::fs::write(dir.join(file), text).map_err(|e| e.to_string())?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", text]).map(|_| ())
}

/// A repository with some of everything: a clone of a bare one, with commits not pushed,
/// a branch never pushed, one whose remote branch is gone, a stash, a worktree with changes
/// (in a folder with spaces and letters outside ASCII), and an uncommitted file.
fn busy_repo() -> Result<(PathBuf, PathBuf), String> {
    let root = scratch("busy")?;
    let origin = root.join("origin.git");
    git(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"])?;
    let repo = root.join("wörk repo");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    init(&repo)?;
    git(
        &repo,
        &["remote", "add", "origin", &origin.to_string_lossy()],
    )?;
    commit(&repo, "a.txt", "one")?;
    git(&repo, &["push", "-q", "-u", "origin", "main"])?;
    // Two commits on main not pushed.
    commit(&repo, "a.txt", "two")?;
    commit(&repo, "a.txt", "three")?;
    // A branch pushed, then deleted on the remote.
    git(&repo, &["branch", "gone-branch"])?;
    git(&repo, &["push", "-q", "-u", "origin", "gone-branch"])?;
    git(&repo, &["push", "-q", "origin", "--delete", "gone-branch"])?;
    git(&repo, &["fetch", "-q", "--prune"])?;
    // A branch never pushed, one commit past what the remote has.
    git(&repo, &["checkout", "-q", "-b", "feature/ünïcode"])?;
    commit(&repo, "b.txt", "local only")?;
    git(&repo, &["checkout", "-q", "main"])?;
    // A stash.
    std::fs::write(repo.join("a.txt"), "stashed").map_err(|e| e.to_string())?;
    git(&repo, &["stash", "push", "-q", "-m", "half done ✓"])?;
    // A worktree with two changed files.
    let wt = root.join("other tree ß");
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "side", &wt.to_string_lossy()],
    )?;
    std::fs::write(wt.join("a.txt"), "changed").map_err(|e| e.to_string())?;
    std::fs::write(wt.join("new file.txt"), "new").map_err(|e| e.to_string())?;
    // An uncommitted file here.
    std::fs::write(repo.join("untracked ✓.txt"), "x").map_err(|e| e.to_string())?;
    Ok((repo, wt))
}

#[test]
fn finds_all_the_work_that_is_only_here() -> Result<(), String> {
    let (repo, wt) = busy_repo()?;
    let o = overview(&repo, Duration::from_secs(60))?;
    assert_eq!(o.branch.as_deref(), Some("main"));
    assert_eq!(o.upstream, "origin/main");
    assert_eq!((o.ahead, o.behind), (2, 0));
    assert_eq!(o.changed, 1);
    assert!(o.has_remote);
    assert!(o.last_fetch.is_some(), "fetched just now");
    assert!(!o.head.is_empty());
    let mut unpushed: Vec<(&str, Option<&str>, u32)> = o
        .unpushed
        .iter()
        .map(|u| (u.branch.as_str(), u.upstream.as_deref(), u.commits))
        .collect();
    unpushed.sort();
    // `side` is main's commits, already counted on main; it's on no remote, so it counts too.
    assert_eq!(
        unpushed,
        vec![
            ("feature/ünïcode", None, 3),
            ("main", Some("origin/main"), 2),
            ("side", None, 2),
        ]
    );
    assert_eq!(o.stashes, vec!["half done ✓".to_owned()]);
    assert_eq!(o.gone, vec!["gone-branch".to_owned()]);
    assert_eq!(o.dirty_worktrees.len(), 1);
    assert_eq!(o.dirty_worktrees[0].changes, 2);
    let found = std::fs::canonicalize(&o.dirty_worktrees[0].path).map_err(|e| e.to_string())?;
    let made = std::fs::canonicalize(&wt).map_err(|e| e.to_string())?;
    assert_eq!(found, made);
    Ok(())
}

#[test]
fn a_clean_pushed_repository_has_no_local_work() -> Result<(), String> {
    let root = scratch("clean")?;
    git(&root, &["init", "-q", "--bare", "-b", "main", "o.git"])?;
    let repo = root.join("r");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    init(&repo)?;
    git(&repo, &["remote", "add", "origin", "../o.git"])?;
    commit(&repo, "a", "one")?;
    git(&repo, &["push", "-q", "-u", "origin", "main"])?;
    let o = overview(&repo, Duration::from_secs(60))?;
    assert!(
        o.unpushed.is_empty() && o.stashes.is_empty() && o.gone.is_empty(),
        "{o:?}"
    );
    assert_eq!(o.changed, 0);
    assert_eq!((o.ahead, o.behind), (0, 0));
    // A push doesn't write FETCH_HEAD: never fetched.
    assert_eq!(o.last_fetch, None);
    Ok(())
}

#[test]
fn a_repository_without_commits_or_remote() -> Result<(), String> {
    let repo = scratch("empty")?;
    init(&repo)?;
    let o = overview(&repo, Duration::from_secs(60))?;
    assert_eq!(o.branch.as_deref(), Some("main"));
    assert_eq!(o.head, "");
    assert!(!o.has_remote);
    assert!(o.unpushed.is_empty());
    // Without a remote nothing is counted as unpushed: there is nowhere to push.
    commit(&repo, "a", "one")?;
    let o = overview(&repo, Duration::from_secs(60))?;
    assert!(o.unpushed.is_empty());
    Ok(())
}

#[test]
fn a_detached_head_has_no_branch() -> Result<(), String> {
    let repo = scratch("detached")?;
    init(&repo)?;
    commit(&repo, "a", "one")?;
    git(&repo, &["checkout", "-q", "--detach"])?;
    let o = overview(&repo, Duration::from_secs(60))?;
    assert_eq!(o.branch, None);
    assert_eq!(o.upstream, "");
    Ok(())
}

#[test]
fn a_folder_that_isnt_a_repository_says_so() -> Result<(), String> {
    let dir = scratch("not-a-repo")?;
    // Not inside the repository the tests run in, either.
    let err = overview(&dir, Duration::from_secs(60))
        .err()
        .unwrap_or_default();
    if !run(Some(&dir), &["rev-parse", "--git-dir"]).is_ok() {
        assert!(err.contains("not a git repository"), "{err}");
    }
    let missing = dir.join("nothing here");
    assert!(overview(&missing, Duration::from_secs(60)).is_err());
    Ok(())
}

/// Branches past [`MAX_COUNTED_BRANCHES`] aren't counted: a repository with hundreds of
/// local branches still answers with one command per counted branch.
#[test]
fn only_so_many_branches_are_counted() -> Result<(), String> {
    let root = scratch("many")?;
    git(&root, &["init", "-q", "--bare", "-b", "main", "o.git"])?;
    let repo = root.join("r");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    init(&repo)?;
    git(&repo, &["remote", "add", "origin", "../o.git"])?;
    commit(&repo, "a", "one")?;
    git(&repo, &["push", "-q", "-u", "origin", "main"])?;
    commit(&repo, "a", "two")?;
    for i in 0..MAX_COUNTED_BRANCHES + 5 {
        git(&repo, &["branch", &format!("b{i:03}")])?;
    }
    let o = overview(&repo, Duration::from_secs(120))?;
    let local_only = o.unpushed.iter().filter(|u| u.upstream.is_none()).count();
    assert_eq!(local_only, MAX_COUNTED_BRANCHES);
    Ok(())
}

#[test]
fn sections_are_read_back_whatever_their_bytes() {
    let mut bytes = Vec::new();
    for (code, key, out) in [
        (0, "status", &b"?? a\0"[..]),
        (1, "wt:/home/me/a b", &b"fatal: x\n"[..]),
        (0, "stashes", &b"\x1e not a header\n"[..]),
    ] {
        bytes.extend(format!("\x1e{code} {} {key}\n", out.len()).as_bytes());
        bytes.extend(out);
    }
    let read = split_sections(&bytes);
    assert_eq!(read.len(), 3);
    assert_eq!(read[1].key, "wt:/home/me/a b");
    assert_eq!(read[1].code, 1);
    assert_eq!(read[2].out, b"\x1e not a header\n");
    // Cut short: what is whole is kept.
    let cut = &bytes[..bytes.len() - 3];
    assert_eq!(split_sections(cut).len(), 2);
    assert!(split_sections(b"").is_empty());
    assert!(split_sections(b"\x1egarbage\n").is_empty());
}

#[test]
fn no_status_is_an_error_not_a_clean_repository() {
    assert!(parse(&[], plain).is_err());
    let failed = [Section {
        key: "status".into(),
        code: 128,
        out: b"fatal: not a git repository\n".to_vec(),
    }];
    assert_eq!(
        parse(&failed, plain).err().as_deref(),
        Some("fatal: not a git repository")
    );
}

/// The script a repository elsewhere is asked with, run here by `sh`: it reads the same as the
/// commands run one by one.
#[cfg(unix)]
#[test]
fn the_script_for_another_machine_reads_the_same() -> Result<(), String> {
    let (repo, _) = busy_repo()?;
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(script())
        .current_dir(&repo)
        .env("LANGUAGE", "C")
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let there = parse(&split_sections(&out.stdout), plain)?;
    let mut here = overview(&repo, Duration::from_secs(60))?;
    let mut there = there;
    for o in [&mut here, &mut there] {
        o.unpushed.sort_by(|a, b| a.branch.cmp(&b.branch));
    }
    assert_eq!(here, there);
    Ok(())
}
