//! The tab's locks through the code paths the views use: read in the background, lock and
//! unlock from the menu or the panel, the warnings and the offer after a push, against a real
//! git-lfs and a test LFS server.

use std::collections::HashSet;

use super::{LockAct, LockState, Panel, lock_label, pushes_current, unlock_offer};
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

fn set(paths: &[&str]) -> HashSet<String> {
    paths.iter().map(|p| (*p).to_owned()).collect()
}

#[test]
fn labels_say_who_and_since_when() {
    let now = 2 * 86_400;
    assert_eq!(
        lock_label(&lock("1", "a", "Ana", Some(false)), now),
        "Locked by Ana 2 days ago"
    );
    assert_eq!(
        lock_label(&lock("1", "a", "me", Some(true)), 30),
        "Locked by you just now"
    );
    let mut unknown = lock("1", "a", "", None);
    unknown.locked_at = None;
    assert_eq!(lock_label(&unknown, now), "Locked by someone");
}

#[test]
fn the_panel_filters_by_path_owner_and_mine() {
    let mut state = LockState::default();
    state.set_locks(vec![
        lock("1", "Art/Hero.psd", "Ana", Some(false)),
        lock("2", "levels/über.unity", "me", Some(true)),
        lock("3", "x.bin", "Bob", None),
    ]);
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
    assert_eq!(state.lock_on("x.bin").map(|l| l.id.as_str()), Some("3"));
    assert!(state.lock_on("y.bin").is_none());
}

#[test]
fn only_our_locks_on_pushed_files_without_changes_are_offered_back() {
    let locks = [
        lock("1", "pushed.psd", "me", Some(true)),
        lock("2", "pushed but edited again.psd", "me", Some(true)),
        lock("3", "theirs.psd", "Ana", Some(false)),
        lock("4", "unknown.psd", "?", None),
        lock("5", "locked before editing.psd", "me", Some(true)),
    ];
    let pushed = set(&[
        "pushed.psd",
        "pushed but edited again.psd",
        "theirs.psd",
        "unknown.psd",
    ]);
    let changed = set(&["pushed but edited again.psd"]);
    let offer: Vec<String> = unlock_offer(&locks, &pushed, &changed)
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(offer, ["1"]);
    assert!(unlock_offer(&locks, &set(&[]), &changed).is_empty());
}

#[test]
fn only_pushes_of_the_checked_out_branch_offer_unlocking() {
    assert!(pushes_current("Push", Some("main")));
    assert!(pushes_current("Push main to origin", Some("main")));
    assert!(pushes_current(
        "Push my branch to origin",
        Some("my branch")
    ));
    assert!(!pushes_current("Push tag v1.0", Some("main")));
    assert!(!pushes_current("Push other to origin", Some("main")));
    assert!(!pushes_current("Push mainline to origin", Some("main")));
    assert!(!pushes_current("Push main to origin", None));
    assert!(!pushes_current("Pull", Some("main")));
}

/// The banners: someone else's lock on a changed file; changed lockable files nobody holds,
/// but not new ones (nobody else can have them).
#[test]
fn banners_name_the_right_files() {
    let mut state = LockState::default();
    state.set_locks(vec![
        lock("1", "theirs.psd", "Ana", Some(false)),
        lock("2", "mine.psd", "me", Some(true)),
    ]);
    for p in ["theirs.psd", "mine.psd", "free.psd", "new.psd", "added.psd"] {
        state.lockable.insert(p.to_owned(), true);
    }
    state.lockable.insert("notes.txt".to_owned(), false);
    let file = |path: &str, code: char| git::StatusFile {
        path: path.to_owned(),
        old_path: None,
        code,
        area: git::Area::Unstaged,
    };
    let files = [
        file("theirs.psd", 'M'),
        file("mine.psd", 'M'),
        file("free.psd", 'D'),
        file("new.psd", '?'),
        file("added.psd", 'A'),
        file("notes.txt", 'M'),
        file("unasked.psd", 'M'),
    ];
    let refs: Vec<&git::StatusFile> = files.iter().collect();
    state.derive(&refs);
    let theirs: Vec<&str> = state
        .derived
        .theirs
        .iter()
        .map(|&i| state.locks[i].path.as_str())
        .collect();
    assert_eq!(theirs, ["theirs.psd"]);
    assert_eq!(state.derived.unlocked, ["free.psd"]);
    assert!(state.derived.changed.contains("notes.txt"));
}

/// Polls the tab, as its frames do, until `done` holds (or some seconds pass).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        tab.lfs_locks_tick(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn idle(tab: &RepoTab) -> bool {
    tab.lfs_locks.busy.is_none() && tab.lfs_locks.reading.is_none()
}

/// Writes `text` to a file git-lfs may have left read-only (lockable files are, until locked).
fn overwrite(file: &std::path::Path, text: &str) -> Result<(), String> {
    if let Ok(meta) = std::fs::metadata(file) {
        let mut perms = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(file, perms);
    }
    std::fs::write(file, text).map_err(|e| e.to_string())
}

/// Locking from a file's menu, someone else's lock on a changed file, the lockable file nobody
/// holds, and unlocking what a push carried: the way the views ask for them.
#[test]
fn locks_in_the_tab() -> Result<(), String> {
    let Some((dir, server)) = lfs_repo("tab")? else {
        return Ok(());
    };
    let bare = dir.with_extension("bare");
    let _ = std::fs::remove_dir_all(&bare);
    git::run(None, &["init", "-q", "--bare", &bare.to_string_lossy()])?;
    git::run(
        Some(&dir),
        &["remote", "add", "origin", &bare.to_string_lossy()],
    )?;
    git::track_lockable(&dir, "*.psd")?;
    for name in ["mine.psd", "their art.psd", "ünlocked.psd", "kept.psd"] {
        std::fs::write(dir.join(name), "v1").map_err(|e| e.to_string())?;
    }
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "art"])?;
    // The test server stores no objects: the pre-push hook that uploads them is skipped.
    git::run(Some(&dir), &["branch", "-M", "main"])?;
    git::run(
        Some(&dir),
        &["push", "-q", "--no-verify", "-u", "origin", "main"],
    )?;
    server.lock_for_someone_else("their art.psd");

    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| !t.loading && t.lfs);
    assert!(tab.lfs, "the tab sees LFS");
    wait(&mut tab, &ctx, |t| idle(t) && t.lfs_locks.read_ok);
    let theirs = tab.lfs_locks.lock_on("their art.psd").cloned();
    assert_eq!(theirs.as_ref().map(|l| l.theirs()), Some(true));

    let both = vec!["mine.psd".to_owned(), "kept.psd".to_owned()];
    tab.apply(Action::LfsLocks(LockAct::Lock(both)), &ctx);
    wait(&mut tab, &ctx, |t| {
        idle(t) && t.lfs_locks.lock_on("kept.psd").is_some()
    });
    assert_eq!(
        tab.lfs_locks.lock_on("mine.psd").map(|l| l.ours),
        Some(Some(true)),
        "{:?}",
        tab.status
    );

    // Changed: one someone else holds, one nobody does.
    for name in ["their art.psd", "ünlocked.psd"] {
        overwrite(&dir.join(name), "v2")?;
    }
    tab.reload_status(&ctx);
    wait(&mut tab, &ctx, |t| {
        t.lfs_locks.is_lockable("ünlocked.psd") && !t.lfs_locks.derived.unlocked.is_empty()
    });
    assert!(tab.lfs_locks.is_lockable("their art.psd"));
    assert!(
        !tab.lfs_locks.lockable.contains_key("mine.psd"),
        "unchanged: not asked"
    );
    assert_eq!(tab.lfs_locks.derived.unlocked, ["ünlocked.psd"]);
    assert_eq!(tab.lfs_locks.derived.theirs.len(), 1);

    // A push carrying mine.psd: its lock is offered back, kept.psd's (locked, not pushed) isn't.
    overwrite(&dir.join("mine.psd"), "v2")?;
    git::run(Some(&dir), &["commit", "-qm", "mine", "--", "mine.psd"])?;
    git::run(Some(&dir), &["push", "-q", "--no-verify", "origin", "main"])?;
    tab.reload_status(&ctx);
    let before = tab.work_read;
    wait(&mut tab, &ctx, |t| t.work_read != before);
    tab.locks_after_push("Push tag v1", &ctx);
    tab.locks_after_push("Push", &ctx);
    wait(&mut tab, &ctx, |t| idle(t) && !t.lfs_locks.offer.is_empty());
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
    wait(&mut tab, &ctx, |t| {
        idle(t) && t.lfs_locks.lock_on("mine.psd").is_none()
    });
    assert!(tab.lfs_locks.lock_on("mine.psd").is_none());
    let mut held = server.paths();
    held.sort();
    assert_eq!(held, ["kept.psd", "their art.psd"]);

    // Someone else's, refused without force: the status and the panel say why.
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
    assert!(tab.lfs_locks.job_error.is_some());
    tab.apply(
        Action::LfsLocks(LockAct::Unlock {
            locks: pair,
            force: true,
        }),
        &ctx,
    );
    wait(&mut tab, &ctx, |t| {
        idle(t) && t.lfs_locks.lock_on("their art.psd").is_none()
    });
    assert_eq!(server.paths(), ["kept.psd"]);

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
    let _ = std::fs::remove_dir_all(&bare);
    Ok(())
}

/// With background fetching turned off, or a server without locks, the locks aren't read in
/// the background; otherwise at first and every few minutes.
#[test]
fn when_locks_are_read_in_the_background() {
    use super::{REREAD, reads_due};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let long_ago = now.checked_sub(REREAD + Duration::from_secs(1));
    assert!(reads_due(true, false, None));
    assert!(!reads_due(true, false, Some(now)));
    assert!(reads_due(true, false, long_ago));
    assert!(!reads_due(false, false, None));
    assert!(!reads_due(true, true, None));
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
    tab.lfs_locks_tick(&ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.lfs_locks.reading.is_none());
}
