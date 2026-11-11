//! The panel through the tab: opened on a real Cargo repository, it lists the package and its
//! report, a second package can be picked, and a stale report never replaces the latest.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::PackageAct;
use crate::repo::RepoTab;

fn git(dir: &Path, args: &[&str]) {
    let out = crate::git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
}

fn write(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(file, text);
}

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft package tab-ü {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "t"]);
    git(&dir, &["config", "user.email", "t@t"]);
    git(&dir, &["config", "commit.gpgSign", "false"]);
    write(
        &dir,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/two\"]\n\n[package]\nname = \"gitcraft-tab-one\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\n",
    );
    write(&dir, "src/lib.rs", "\n");
    write(&dir, "debug.log", "noise\n");
    write(
        &dir,
        "crates/two/Cargo.toml",
        "[package]\nname = \"gitcraft-tab-two\"\nversion = \"0.2.0\"\nedition = \"2021\"\nlicense = \"MIT\"\n",
    );
    write(&dir, "crates/two/src/lib.rs", "\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "start"]);
    dir
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(120);
    while !done(tab) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        tab.poll(ctx);
    }
}

fn reported(tab: &RepoTab) -> bool {
    tab.package.as_ref().is_some_and(|v| v.report.is_some())
}

#[test]
fn the_panel_lists_each_package_and_reports_the_one_picked() {
    let dir = project();
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.package_act(PackageAct::Open, &ctx);
    wait(&mut tab, &ctx, reported);
    assert!(tab.package.is_some(), "the panel closed");
    let Some(view) = &tab.package else { return };
    assert!(
        matches!(view.packages, Some(Ok(_))),
        "packages: {:?}",
        view.packages
    );
    let Some(Ok(list)) = &view.packages else {
        return;
    };
    let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["gitcraft-tab-one", "gitcraft-tab-two"]);
    assert!(
        matches!(view.report, Some(Ok(_))),
        "report: {:?}",
        view.report
    );
    let Some(Ok(report)) = &view.report else {
        return;
    };
    assert!(report.files.iter().any(|(f, _)| f == "src/lib.rs"));
    assert!(report.accidental.iter().any(|n| n.path == "debug.log"));
    // The second package; a report asked for before it is dropped when it comes in.
    tab.package_act(PackageAct::Select(1), &ctx);
    let stale = tab.package.as_ref().map_or(0, |v| v.request);
    tab.package_act(PackageAct::Select(1), &ctx);
    assert!(tab.package.as_ref().is_some_and(|v| v.request == stale + 1));
    wait(&mut tab, &ctx, reported);
    assert!(tab.package.is_some(), "the panel closed");
    let Some(view) = &tab.package else { return };
    assert_eq!(view.selected, 1);
    assert!(
        matches!(view.report, Some(Ok(_))),
        "report: {:?}",
        view.report
    );
    let Some(Ok(report)) = &view.report else {
        return;
    };
    assert!(report.files.iter().any(|(f, _)| f == "Cargo.toml"));
    assert!(!report.files.iter().any(|(f, _)| f.contains("debug.log")));
    // The registry is asked once the files are in (refused offline, still said).
    wait(&mut tab, &ctx, |t| {
        t.package.as_ref().is_some_and(|v| v.published.is_some())
    });
    assert!(tab.package.as_ref().is_some_and(|v| v.published.is_some()));
    // Refresh reads it all again and keeps the package picked.
    tab.package_act(PackageAct::Refresh, &ctx);
    assert!(tab.package.as_ref().is_some_and(|v| v.report.is_none()));
    wait(&mut tab, &ctx, reported);
    assert!(tab.package.as_ref().is_some_and(|v| v.selected == 1));
    tab.package_act(PackageAct::Close, &ctx);
    assert!(tab.package.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
