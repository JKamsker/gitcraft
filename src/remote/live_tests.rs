//! Against a real machine, when one is named (they are skipped otherwise):
//! - `GITCRAFT_TEST_SSH=<host>:<repo path there>`, e.g. `build:~/repos/x`: reads a repository
//!   there, as a tab does. Nothing in it is changed.
//! - `GITCRAFT_TEST_WSL=<distro>`: makes a scratch repository in that distribution's `/tmp`
//!   and works in it (stages, commits, rebases), then removes it.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::*;
use crate::git;

/// Everything the tab reads on opening a repository, timed.
fn read_like_a_tab(repo: &Path) -> Result<(), String> {
    let t = Instant::now();
    let timed = |what: &str, at: Instant| eprintln!("  {what}: {:?}", at.elapsed());
    let at = Instant::now();
    let refs = git::refs(repo)?;
    timed("refs", at);
    let at = Instant::now();
    let status = git::status(repo)?;
    timed("status", at);
    let at = Instant::now();
    let stashes = git::stashes(repo)?;
    timed("stashes", at);
    let at = Instant::now();
    let worktrees = git::worktrees(repo, None)?;
    timed("worktrees", at);
    let at = Instant::now();
    let mut log = git::LogStream::start(repo, None, 0)?;
    let page = log.next_page(git::LOG_PAGE)?;
    timed("log page", at);
    // The next page follows on from it, as one log of both would have them.
    let at = Instant::now();
    let more = log.next_page(10)?;
    timed("next log page", at);
    let whole = git::log(repo, None, 0, page.len() + more.len())?;
    let paged: Vec<&str> = page.iter().chain(&more).map(|c| c.hash.as_str()).collect();
    let at_once: Vec<&str> = whole.iter().map(|c| c.hash.as_str()).collect();
    assert_eq!(paged, at_once);
    let at = Instant::now();
    let files = git::tree_files(repo, "HEAD")?;
    timed("tree", at);
    eprintln!(
        "{}: {} commits, {} files, {} changed, {} stashes, {} worktrees, head {} in {:?}",
        repo.display(),
        page.len(),
        files.len(),
        status.changed_files(),
        stashes.len(),
        worktrees.len(),
        refs.head,
        t.elapsed()
    );
    assert!(!page.is_empty());
    assert!(worktrees.iter().any(|w| w.current), "{worktrees:?}");
    // A file of the tree, from the index and from disk.
    let first = files.first().ok_or("no files")?;
    let at_head = git::file_at(repo, Some("HEAD"), first)?;
    let on_disk = git::file_at(repo, None, first);
    eprintln!(
        "{first}: {} bytes, on disk {:?}",
        at_head.len(),
        on_disk.as_ref().map(Vec::len)
    );
    // Many at once, as background loads do: more than there are shells.
    let t = Instant::now();
    std::thread::scope(|s| {
        let runs: Vec<_> = (0..20)
            .map(|_| s.spawn(|| git::run(Some(repo), &["rev-parse", "HEAD"])))
            .collect();
        for r in runs {
            let out = r.join().map_err(|_| "panicked".to_owned())??;
            assert_eq!(out.trim(), refs.head);
        }
        Ok::<(), String>(())
    })?;
    eprintln!("20 side by side in {:?}", t.elapsed());
    let t = Instant::now();
    for _ in 0..10 {
        git::run(Some(repo), &["rev-parse", "HEAD"])?;
    }
    eprintln!("one round trip: {:?}", t.elapsed() / 10);
    Ok(())
}

#[test]
fn a_repository_over_ssh_reads_like_a_local_one() -> Result<(), String> {
    let Ok(spec) = std::env::var("GITCRAFT_TEST_SSH") else {
        return Ok(());
    };
    let (host, path) = spec
        .split_once(':')
        .ok_or("GITCRAFT_TEST_SSH=<host>:<path>")?;
    let t = Instant::now();
    let root = git::toplevel(&encode(&Host::Ssh(host.to_owned()), path))?;
    eprintln!(
        "connected and found {} in {:?}",
        root.display(),
        t.elapsed()
    );
    let target = parse(&root).ok_or("not remote")?;
    assert!(target.path.starts_with('/'), "{}", target.path);
    read_like_a_tab(&root)?;
    // A file written for git there comes back as written.
    let there = host_temp_file(&target, "live.txt", b"one\ntwo\n")?;
    let back = fs::read(&encode(&target.host, &there)).map_err(|e| e.to_string())?;
    assert_eq!(back, b"one\ntwo\n");
    // A folder that isn't there says so, rather than "not a repository".
    let gone = encode(&target.host, "/no/such/folder/here");
    let err = git::toplevel(&gone).err().unwrap_or_default();
    assert!(err.contains("isn't a folder"), "{err}");
    Ok(())
}

#[test]
fn a_repository_in_wsl_can_be_worked_in() -> Result<(), String> {
    let Ok(distro) = std::env::var("GITCRAFT_TEST_WSL") else {
        return Ok(());
    };
    let host = Host::Wsl(distro);
    let dir = format!("/tmp/gitcraft-live-{}", std::process::id());
    let repo = encode(&host, &dir);
    let result = work_in(&repo);
    let _ = exec(
        &host,
        &Request {
            script: format!("rm -rf {}", sh_quote(&dir)),
            ..Request::default()
        },
    );
    result
}

/// Output a command leaves being written after it ends (a hook's background job) neither
/// spills into the next answer nor makes a shell run anything twice.
fn late_output_stays_out(host: &Host) -> Result<(), String> {
    let run = |script: &str| {
        exec(
            host,
            &Request {
                script: script.to_owned(),
                ..Request::default()
            },
        )
    };
    let first = run("printf a; (sleep 0.3; printf bbbbbbbbbbbb) &")?;
    assert!(first.stdout.starts_with(b"a"), "{first:?}");
    std::thread::sleep(std::time::Duration::from_millis(600));
    // Every shell of the pool, to reach the one that ran it.
    for _ in 0..8 {
        assert_eq!(run("printf next")?.stdout, b"next");
    }
    Ok(())
}

/// A command that hangs is stopped at its deadline, there too, and the shells go on.
fn hung_commands_are_stopped(host: &Host) -> Result<(), String> {
    let marker = format!("{}.{}", std::process::id() % 1000 + 100, 25);
    let hung = exec(
        host,
        &Request {
            script: format!("exec sleep {marker}"),
            timeout: Some(std::time::Duration::from_secs(2)),
            ..Request::default()
        },
    );
    let err = hung.err().unwrap_or_default();
    assert!(err.contains("was stopped"), "{err}");
    // Stopped in the background, within a few seconds.
    std::thread::sleep(std::time::Duration::from_secs(4));
    let left = exec(
        host,
        &Request {
            script: format!("ps -eo args | grep -c '^sleep {marker}$' || true"),
            ..Request::default()
        },
    )?;
    assert_eq!(String::from_utf8_lossy(&left.stdout).trim(), "0");
    // What it started started something in turn (a hook running a program): stopped as well.
    let deep = format!("{}.{}", std::process::id() % 1000 + 100, 75);
    let hung = exec(
        host,
        &Request {
            script: format!("sh -c 'sleep {deep}; true'; true"),
            timeout: Some(std::time::Duration::from_secs(2)),
            ..Request::default()
        },
    );
    assert!(hung.is_err());
    std::thread::sleep(std::time::Duration::from_secs(4));
    let left = exec(
        host,
        &Request {
            script: format!("ps -eo args | grep -c '^sleep {deep}$' || true"),
            ..Request::default()
        },
    )?;
    assert_eq!(String::from_utf8_lossy(&left.stdout).trim(), "0");
    Ok(())
}

fn work_in(repo: &PathBuf) -> Result<(), String> {
    let host = parse(repo).ok_or("not remote")?.host;
    late_output_stays_out(&host)?;
    hung_commands_are_stopped(&host)?;
    let git = |args: &[&str]| git::run(Some(repo), args);
    git::init(repo)?;
    git(&["config", "user.name", "Live Test"])?;
    git(&["config", "user.email", "live@example.invalid"])?;
    write(&repo.join("a.txt"), b"1\n2\n3\n").map_err(|e| e.to_string())?;
    git(&["add", "a.txt"])?;
    git(&["commit", "-m", "first\n\nwith a body: it's \"quoted\""])?;
    assert_eq!(git::toplevel(repo)?, *repo);
    // A staged line, as the diff view stages it: through a patch file there.
    write(&repo.join("a.txt"), b"1\nX\n3\n").map_err(|e| e.to_string())?;
    let file = git::status(repo)?
        .unstaged
        .into_iter()
        .next()
        .ok_or("no change seen")?;
    let patch = git::work_diff(repo, &file, git::DiffOptions::default())?;
    let saved = git::save_patch_for(repo, &patch)?;
    git(&["apply", "--cached", &saved])?;
    assert_eq!(git(&["diff", "--cached", "--name-only"])?.trim(), "a.txt");
    git(&["commit", "-m", "second"])?;
    // A name with a `\` in it (Linux allows one): read as itself.
    write(&join(repo, r"a\b.txt"), b"odd\n").map_err(|e| e.to_string())?;
    git(&["add", "--", r"a\b.txt"])?;
    assert_eq!(git::file_at(repo, None, r"a\b.txt")?, b"odd\n");
    assert_eq!(git::file_at(repo, Some(""), r"a\b.txt")?, b"odd\n");
    git(&["commit", "-q", "-m", "odd name"])?;
    // And a repository whose own folder has one: found as itself.
    let odd = join(repo, r"sub\repo");
    git::init(&odd)?;
    assert_eq!(git::toplevel(&odd)?, odd);
    assert!(parse(&odd).is_some_and(|t| t.path.ends_with(r"/sub\repo")));
    // `.gitignore` there, read and written there.
    assert!(git::add_to_gitignore(repo, "build/")?);
    let ignore = read_to_string(&repo.join(".gitignore")).map_err(|e| e.to_string())?;
    assert_eq!(ignore, "build/\n");
    read_like_a_tab(repo)?;
    // The working tree's file, read as the tab reads it, is what was written there.
    assert_eq!(git::file_at(repo, None, "a.txt")?, b"1\nX\n3\n");
    snapshot_there(repo)?;
    Ok(())
}

/// A snapshot made there: its copy of the index made and removed there, the files untouched.
fn snapshot_there(repo: &Path) -> Result<(), String> {
    let git = |args: &[&str]| git::run(Some(repo), args);
    write(&repo.join("a.txt"), b"snap\n").map_err(|e| e.to_string())?;
    write(&repo.join("new file.txt"), b"new\n").map_err(|e| e.to_string())?;
    let status = git(&["status", "--porcelain", "-z"])?;
    let made = git::snapshot(repo, "live", true)?.ok_or("nothing to snapshot")?;
    git(&["stash", "store", "-m", &made.message, &made.commit])?;
    assert_eq!(git(&["status", "--porcelain", "-z"])?, status);
    let git_dir = git(&["rev-parse", "--absolute-git-dir"])?;
    let host = parse(repo).ok_or("not remote")?.host;
    let listed = exec(
        &host,
        &Request {
            script: format!("ls -a -- {}", sh_quote(git_dir.trim())),
            ..Request::default()
        },
    )?;
    let left = String::from_utf8_lossy(&listed.stdout);
    assert!(!left.contains("gitcraft-snapshot"), "{left}");
    assert_eq!(
        git(&["ls-tree", "-r", "--name-only", "stash@{0}^3"])?.trim(),
        "new file.txt"
    );
    git(&["stash", "drop", "-q"])?;
    Ok(())
}
