//! The card in a real tab, driven as the window draws it: an operation fails on a lock, the
//! card offers to remove it, and, confirmed, the operation runs again, works, and the card
//! is gone. (Not a
//! stash: `git stash` fails on a lock without a word.)

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::super::{Action, Refresh, RepoTab, ops};
use crate::git;

/// Frames until `done` holds (a while at most).
fn until(harness: &mut Harness<'_, RepoTab>, done: impl Fn(&RepoTab) -> bool) {
    let start = Instant::now();
    while !done(harness.state()) && start.elapsed() < Duration::from_secs(30) {
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    let tab = harness.state();
    assert!(
        done(tab),
        "never happened: status {:?}, card {:?}",
        tab.status,
        tab.error_card.as_ref().map(|c| c.stage().clone())
    );
}

#[test]
fn a_lock_explained_removed_and_the_file_staged() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft tab lock ö-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        git::run(Some(&dir), args)?;
    }
    std::fs::write(dir.join("a.txt"), "1\n").map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["add", "a.txt"])?;
    git::run(Some(&dir), &["commit", "-q", "-m", "one"])?;
    std::fs::write(dir.join("a.txt"), "2\n").map_err(|e| e.to_string())?;
    let lock = dir.join(".git/index.lock");

    let tab = RepoTab::new_deferred(dir.clone(), &egui::Context::default());
    let mut harness = Harness::builder()
        .with_size([1300.0, 800.0])
        .build_ui_state(
            {
                // The app's fonts and theme first, as the window has them.
                let mut styled = false;
                move |ui, tab: &mut RepoTab| {
                    if !std::mem::replace(&mut styled, true) {
                        crate::theme::install_fonts(ui.ctx());
                        crate::theme::apply(
                            ui.ctx(),
                            &crate::theme::ThemeId::default(),
                            &crate::theme::Tweaks::default(),
                        );
                        return;
                    }
                    tab.ui(ui);
                }
            },
            tab,
        );
    until(&mut harness, |tab| !tab.opening() && tab.settled());

    std::fs::write(&lock, "").map_err(|e| e.to_string())?;
    harness.state_mut().queue(Action::Ops {
        label: "Stage".to_owned(),
        ops: ops(&[&["add", "--", "a.txt"]]),
        refresh: Refresh::Status,
    });
    until(&mut harness, |tab| {
        tab.settled()
            && tab
                .error_card
                .as_ref()
                .is_some_and(|c| c.offered().is_some())
    });
    harness.get_by_label("Remove stale lock").click();
    // The card grows by the question: its panel settles over a frame or two.
    for _ in 0..3 {
        harness.step();
    }
    // Asked first: nothing removed yet.
    assert!(lock.exists());
    assert!(harness.query_by_label("Cancel").is_some());
    harness.get_by_label("Remove stale lock").click();
    until(&mut harness, |tab| {
        tab.error_card
            .as_ref()
            .is_some_and(|c| matches!(c.stage(), crate::error_card::Stage::Done { .. }))
    });
    assert!(!lock.exists());
    for _ in 0..3 {
        harness.step();
    }
    harness.get_by_label("Try again").click();
    until(&mut harness, |tab| {
        tab.settled() && tab.work.staged.len() == 1
    });
    // Fixed, with nothing to undo: the card went with the retry.
    assert!(harness.state().error_card.is_none());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
