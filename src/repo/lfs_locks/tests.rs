//! The tab's locks through the code paths the views use: read in the background, lock and
//! unlock from the menu or the panel, the warnings and the offer after a push, against a real
//! git-lfs and a test LFS server.

use std::collections::HashSet;

use super::{LockAct, LockState, Panel, lock_label, unlock_offer};
use crate::git::{self, LfsLock, lfs_test_server::lfs_repo};
use crate::repo::{Action, RepoTab};

fn lock(id: &str, path: &str, owner: &str, ours: Option<bool>) -> LfsLock {
    LfsLock {
        id: id.to_owned(),
        path: path.to_owned(),
        owner: owner.to_owned(),
        locked_at: Some(0),
        ours,
    }
}

#[test]
fn labels_say_who_and_since_when() {
    let now = 2 * 86_400;
    assert_eq!(
        lock_label(&lock("1", "a", "Ana", Some(false)), now),
        "Locked by Ana · 2 d"
    );
    assert_eq!(
        lock_label(&lock("1", "a", "me", Some(true)), now),
        "Locked by you · 2 d"
    );
    let mut unknown = lock("1", "a", "", None);
    unknown.locked_at = None;
    assert_eq!(lock_label(&unknown, now), "Locked by someone");
}

#[test]
fn the_panel_filters_by_path_owner_and_mine() {
    let state = LockState {
        locks: vec![
            lock("1", "Art/Hero.psd", "Ana", Some(false)),
            lock("2", "levels/über.unity", "me", Some(true)),
            lock("3", "x.bin", "Bob", None),
        ],
        ..Default::default()
    };
    let ids = |panel: &Panel| -> Vec<String> {
        state.shown(panel).iter().map(|l| l.id.clone()).collect()
    };
    let mut panel = Panel::default();
    assert_eq!(ids(&panel), ["1", "2", "3"]);
    panel.filter = "  hero ".to_owned();
    assert_eq!(ids(&panel), ["1"]);
    panel.filter = "bob".to_owned();
    assert_eq!(ids(&panel), ["3"]);
    panel.filter = "ÜBER".to_owned();
    assert_eq!(ids(&panel), ["2"]);
    panel.filter.clear();
    panel.only_mine = true;
    assert_eq!(ids(&panel), ["2"]);
}

#[test]
fn only_our_locks_on_files_without_changes_are_offered_back() {
    let locks = [
        lock("1", "pushed.psd", "me", Some(true)),
        lock("2", "still editing.psd", "me", Some(true)),
        lock("3", "theirs.psd", "Ana", Some(false)),
        lock("4", "unknown.psd", "?", None),
    ];
    let changed: HashSet<String> = ["still editing.psd".to_owned()].into();
    let offer: Vec<String> = unlock_offer(&locks, &changed)
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(offer, ["1"]);
    assert!(unlock_offer(&[], &changed).is_empty());
}

/// Polls the tab until `done` holds (or a few seconds pass).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn idle(tab: &RepoTab) -> bool {
    tab.lfs_locks.busy.is_none() && tab.lfs_locks.reading.is_none()
}

/// Locking from a file's menu, someone else's lock on a changed file, the lockable file nobody
/// holds, and unlocking what was pushed: the way the views ask for them.
#[test]
fn locks_in_the_tab() -> Result<(), String> {
    let Some((dir, server)) = lfs_repo("tab")? else {
        return Ok(());
    };
    git::track_lockable(&dir, "*.psd")?;
    for name in ["mine.psd", "their art.psd", "ünlocked.psd"] {
        std::fs::write(dir.join(name), "v1").map_err(|e| e.to_string())?;
    }
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "art"])?;
    server.lock_for_someone_else("their art.psd");

    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| !t.loading && t.lfs);
    assert!(tab.lfs, "the tab sees LFS");

    let mut acts = Vec::new();
    tab.lfs_locks_ui(&ctx, &mut acts);
    wait(&mut tab, &ctx, idle);
    let theirs = tab.lfs_locks.lock_on("their art.psd").cloned();
    assert_eq!(theirs.as_ref().map(|l| l.theirs()), Some(true));

    tab.apply(Action::LfsLocks(LockAct::Lock("mine.psd".to_owned())), &ctx);
    wait(&mut tab, &ctx, idle);
    assert_eq!(
        tab.lfs_locks.lock_on("mine.psd").map(|l| l.ours),
        Some(Some(true)),
        "{:?}",
        tab.status
    );

    // Changed: one someone else holds, one nobody does. (Lockable files are read-only until
    // locked, as git-lfs leaves them.)
    for name in ["their art.psd", "ünlocked.psd"] {
        let file = dir.join(name);
        if let Ok(meta) = std::fs::metadata(&file) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(&file, perms);
        }
        std::fs::write(&file, "v2").map_err(|e| e.to_string())?;
    }
    tab.reload_status(&ctx);
    let before = tab.work_read;
    wait(&mut tab, &ctx, |t| {
        t.work_read != before && t.work.unstaged.len() == 2
    });
    tab.lfs_locks_ui(&ctx, &mut acts);
    wait(&mut tab, &ctx, |t| t.lfs_locks.is_lockable("ünlocked.psd"));
    assert!(tab.lfs_locks.is_lockable("their art.psd"));
    assert!(
        !tab.lfs_locks.is_lockable("mine.psd"),
        "unchanged: not asked"
    );

    // A push went through: our lock on a file with nothing left to push is offered back.
    tab.locks_after_push(&ctx);
    wait(&mut tab, &ctx, idle);
    let offered: Vec<String> = tab.lfs_locks.offer.iter().map(|l| l.path.clone()).collect();
    assert_eq!(offered, ["mine.psd"]);
    let locks = tab
        .lfs_locks
        .offer
        .iter()
        .map(|l| (l.id.clone(), l.path.clone()))
        .collect();
    tab.apply(
        Action::LfsLocks(LockAct::Unlock {
            locks,
            force: false,
        }),
        &ctx,
    );
    assert!(tab.lfs_locks.offer.is_empty(), "taken off at once");
    wait(&mut tab, &ctx, idle);
    assert!(tab.lfs_locks.lock_on("mine.psd").is_none());
    assert_eq!(server.paths(), ["their art.psd"]);

    // Someone else's, refused without force: the status says why.
    let theirs = theirs.ok_or("no lock of theirs")?;
    let pair = vec![(theirs.id.clone(), theirs.path.clone())];
    tab.apply(
        Action::LfsLocks(LockAct::Unlock {
            locks: pair.clone(),
            force: false,
        }),
        &ctx,
    );
    wait(&mut tab, &ctx, idle);
    let (status, failed) = tab.status.clone().unwrap_or_default();
    assert!(
        failed && status.starts_with("Unlock their art.psd failed: "),
        "{status}"
    );
    tab.apply(
        Action::LfsLocks(LockAct::Unlock {
            locks: pair,
            force: true,
        }),
        &ctx,
    );
    wait(&mut tab, &ctx, idle);
    assert!(server.paths().is_empty());

    // A pattern made lockable from the panel lands in .gitattributes.
    tab.apply(Action::LfsLocks(LockAct::OpenPanel), &ctx);
    tab.apply(
        Action::LfsLocks(LockAct::TrackLockable("*.fbx".to_owned())),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        idle(t) && t.lfs_locks.patterns.len() == 2
    });
    assert_eq!(tab.lfs_locks.patterns, ["*.psd", "*.fbx"]);
    tab.apply(Action::LfsLocks(LockAct::ClosePanel), &ctx);
    assert!(tab.lfs_locks.panel.is_none());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A repository without LFS never asks a server for locks.
#[test]
fn no_lfs_no_locks_read() {
    let dir = std::env::temp_dir().join(format!("gitcraft-nolfs-locks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let _ = git::run(Some(&dir), &["init", "-q"]);
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let mut acts = Vec::new();
    tab.lfs_locks_ui(&ctx, &mut acts);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.lfs_locks.reading.is_none());
    assert!(acts.is_empty());
}
