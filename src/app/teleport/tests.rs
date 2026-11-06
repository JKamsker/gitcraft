//! The dialog driven in a window: a teleport to the loopback SSH host (whose shells are `sh`
//! here), from the tab's repository to a clone found there, opens a tab on its worktree, and
//! the window's thread never waits for the other machine.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::Place;
use crate::app::{App, Tab};
use crate::remote::{self, Host};
use crate::teleport::Machine;

type H<'a> = egui_kittest::Harness<'a, App>;

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

/// A repository with a commit and uncommitted work, and a clone of it.
fn repositories(root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let src = root.join("source");
    std::fs::create_dir_all(&src).map_err(|e| e.to_string())?;
    git(&src, &["init", "-q", "-b", "main"])?;
    for (key, value) in [
        ("user.name", "t"),
        ("user.email", "t@t"),
        ("commit.gpgSign", "false"),
    ] {
        git(&src, &["config", key, value])?;
    }
    std::fs::write(src.join("a.txt"), "one\n").map_err(|e| e.to_string())?;
    git(&src, &["add", "a.txt"])?;
    git(&src, &["commit", "-qm", "one"])?;
    git(
        &src,
        &["remote", "add", "origin", "https://example.invalid/app.git"],
    )?;
    let target = root.join("target");
    git(
        root,
        &[
            "clone",
            "-q",
            &src.to_string_lossy(),
            &target.to_string_lossy(),
        ],
    )?;
    git(
        &target,
        &["remote", "set-url", "origin", "https://example.invalid/app"],
    )?;
    git(&src, &["checkout", "-q", "-b", "work"])?;
    std::fs::write(src.join("a.txt"), "one\ntwo\n").map_err(|e| e.to_string())?;
    std::fs::write(src.join("new.txt"), "new\n").map_err(|e| e.to_string())?;
    Ok((src, target))
}

/// Frames as the window draws them, until `done` or `ms` have gone by; whether it was done.
fn run_until(harness: &mut H<'_>, ms: u64, done: &dyn Fn(&App) -> bool) -> bool {
    let until = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < until {
        harness.step();
        if done(harness.state()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_teleport_from_the_dialog_opens_its_worktree() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("gitcraft-tp-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let root = std::fs::canonicalize(&root).map_err(|e| e.to_string())?;
    let (src, target) = repositories(&root)?;
    let host = Host::Ssh(remote::LOOPBACK.to_owned());
    let there = remote::encode(&host, &target.to_string_lossy());

    let source = src.clone();
    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| App::new(cc, Some(source)));
    let refused_before = remote::refused_on_ui_thread();
    let opened =
        |app: &App| matches!(app.tabs.get(app.active), Some(Tab::Repo(r)) if r.path == src);
    assert!(
        run_until(&mut harness, 20_000, &opened),
        "the source never opened"
    );

    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        // A clone the app knows of there, as a recent repository.
        app.recent.push(there.clone());
        app.start_teleport(src.clone(), &ctx);
    }
    let read = |app: &App| {
        app.teleport
            .as_ref()
            .is_some_and(|f| f.info.ready().is_some())
    };
    assert!(
        run_until(&mut harness, 20_000, &read),
        "the source was never read"
    );
    let machine = Machine::There(host.clone());
    {
        let app = harness.state_mut();
        let form = app.teleport.as_mut().ok_or("no dialog")?;
        assert!(form.machines().contains(&machine));
        form.pick(machine, &ctx);
    }
    let found = |app: &App| {
        app.teleport
            .as_ref()
            .is_some_and(|f| f.clones.ready().is_some_and(|c| !c.is_empty()))
    };
    assert!(
        run_until(&mut harness, 20_000, &found),
        "no clone was found there"
    );
    let folder = {
        let app = harness.state_mut();
        let form = app.teleport.as_mut().ok_or("no dialog")?;
        assert_eq!(form.place, Place::Found(0));
        assert_eq!(form.branch, "work");
        let folder = form.folder.clone();
        form.start(&ctx);
        folder
    };
    assert_eq!(folder, format!("{}-work", target.to_string_lossy()));
    let landed = remote::encode(&host, &folder);
    let in_tab = |app: &App| {
        app.teleport.is_none()
            && app
                .tabs
                .iter()
                .any(|t| matches!(t, Tab::Repo(r) if r.path == landed))
    };
    let arrived = run_until(&mut harness, 60_000, &in_tab);
    let error = harness
        .state()
        .teleport
        .as_ref()
        .and_then(|f| f.error.clone());
    assert!(arrived, "no tab opened on the worktree: {error:?}");
    let status = git(
        Path::new(&folder),
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    assert_eq!(status, " M a.txt\n?? new.txt\n");
    assert_eq!(
        remote::refused_on_ui_thread() - refused_before,
        0,
        "commands asked of the other machine from the window's thread"
    );
    drop(harness);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}
