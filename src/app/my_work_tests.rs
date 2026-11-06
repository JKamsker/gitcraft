//! The panel's own state against real repositories: looked at through the same queue the window
//! uses, a branch pushed from it, and the repository looked at again afterwards.

use std::time::{Duration, Instant};

use super::*;
use crate::git;

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
}

fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-panel-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A clone of a bare repository with one commit pushed and one not, on a branch never pushed.
fn repo_with_work(root: &Path, name: &str) -> Result<PathBuf, String> {
    run(
        root,
        &["init", "-q", "--bare", "-b", "main", &format!("{name}.git")],
    )?;
    let repo = root.join(name);
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "T"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["remote", "add", "origin", &format!("../{name}.git")],
    ] {
        run(&repo, args)?;
    }
    std::fs::write(repo.join("a"), "1").map_err(|e| e.to_string())?;
    run(&repo, &["add", "-A"])?;
    run(&repo, &["commit", "-qm", "one"])?;
    run(&repo, &["push", "-q", "-u", "origin", "main"])?;
    run(
        &repo,
        &["checkout", "-q", "-b", "topic ✓".replace(' ', "-").as_str()],
    )?;
    std::fs::write(repo.join("a"), "2").map_err(|e| e.to_string())?;
    run(&repo, &["commit", "-qam", "two"])?;
    Ok(repo)
}

fn settle(w: &mut MyWork, ctx: &egui::Context) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        w.poll(ctx);
        let pushing = w.pushes.values().any(Option::is_none);
        if !w.busy() && !pushing {
            return Ok(());
        }
        if Instant::now() > until {
            return Err("the panel never settled".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn finds_work_pushes_it_and_looks_again() -> Result<(), String> {
    let root = scratch("push")?;
    let repo = repo_with_work(&root, "app")?;
    let clean = root.join("clean");
    std::fs::create_dir_all(&clean).map_err(|e| e.to_string())?;
    run(&clean, &["init", "-q"])?;
    let gone = root.join("gone");
    let ctx = egui::Context::default();
    let mut w = MyWork::default();
    w.look(vec![
        repo.clone(),
        clean.clone(),
        gone.clone(),
        repo.clone(),
    ]);
    assert_eq!(w.waiting.len(), 3, "asked once each");
    settle(&mut w, &ctx)?;
    let work = match w.seen.get(&repo) {
        Some(Ok(o)) => my_work::work_of(o),
        other => return Err(format!("not looked at: {other:?}")),
    };
    assert_eq!(
        work,
        vec![Work::Unpushed {
            branch: "topic-✓".into(),
            upstream: None,
            commits: 1
        }]
    );
    assert!(matches!(w.seen.get(&clean), Some(Ok(o)) if my_work::work_of(o).is_empty()));
    assert!(matches!(w.seen.get(&gone), Some(Err(_))));

    w.push(repo.clone(), "topic-✓".into(), false, &ctx);
    settle(&mut w, &ctx)?;
    // Settled after the push, then again after the look it starts.
    settle(&mut w, &ctx)?;
    assert_eq!(
        w.pushes.get(&(repo.clone(), "topic-✓".into())),
        Some(&Some(Ok(())))
    );
    assert!(
        matches!(w.seen.get(&repo), Some(Ok(o)) if my_work::work_of(o).is_empty()),
        "{:?}",
        w.seen.get(&repo)
    );
    Ok(())
}

#[test]
fn a_refused_push_says_why_and_the_work_stays() -> Result<(), String> {
    let root = scratch("refused")?;
    let repo = repo_with_work(&root, "app")?;
    // Its remote is gone: the push can't reach it.
    std::fs::remove_dir_all(root.join("app.git")).map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    let mut w = MyWork::default();
    w.push(repo.clone(), "topic-✓".into(), false, &ctx);
    settle(&mut w, &ctx)?;
    let result = w.pushes.get(&(repo.clone(), "topic-✓".into())).cloned();
    assert!(
        matches!(&result, Some(Some(Err(e))) if !e.is_empty()),
        "{result:?}"
    );
    assert!(w.waiting.is_empty(), "no look after a failed push");
    Ok(())
}

#[test]
fn another_machine_isnt_reached_unasked() {
    let here = PathBuf::from("/home/me/x");
    let far = PathBuf::from("ssh://gitcraft-test-never-connected/x");
    assert_eq!(reachable(&[here.clone(), far]), vec![here]);
}

#[test]
fn counts_read_in_words() {
    assert_eq!(repo_count(1), "1 repository");
    assert_eq!(repo_count(4), "4 repositories");
}
