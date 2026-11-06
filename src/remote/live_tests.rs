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

fn work_in(repo: &PathBuf) -> Result<(), String> {
    late_output_stays_out(&parse(repo).ok_or("not remote")?.host)?;
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
    // `.gitignore` there, read and written there.
    assert!(git::add_to_gitignore(repo, "build/")?);
    let ignore = read_to_string(&repo.join(".gitignore")).map_err(|e| e.to_string())?;
    assert_eq!(ignore, "build/\n");
    read_like_a_tab(repo)
}
