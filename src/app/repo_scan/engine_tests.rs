//! The shared walk's bounds and its cheap repository checks, on real folders.

use super::*;
use crate::git;
use std::path::Path;
use std::time::Duration;

fn tree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-engine-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    assert!(git::run(Some(dir), &["init", "-q"]).is_ok());
}

fn rel(root: &Path, found: &Found) -> Vec<String> {
    found
        .repos
        .iter()
        .map(|p| {
            p.strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn at_depth(root: &Path, depth: usize) -> Found {
    let stop = AtomicBool::new(false);
    let limits = ScanLimits {
        depth,
        ..ROOT_LIMITS
    };
    walk(root, limits, &stop, &mut |_| {}).unwrap_or_default()
}

/// The issue's example: two repositories as children, one at depth 2, the root itself one.
#[test]
fn depth_decides_what_a_root_finds() {
    let root = tree("depth");
    init(&root.join("one"));
    init(&root.join("two"));
    init(&root.join("group/three"));
    assert_eq!(rel(&root, &at_depth(&root, 0)), Vec::<String>::new());
    assert_eq!(rel(&root, &at_depth(&root, 1)), ["one", "two"]);
    assert_eq!(
        rel(&root, &at_depth(&root, 2)),
        ["group/three", "one", "two"]
    );
    init(&root);
    assert_eq!(rel(&root, &at_depth(&root, 0)), [""]);
    assert_eq!(rel(&root, &at_depth(&root, 1)), ["", "one", "two"]);
    let _ = std::fs::remove_dir_all(&root);
}

/// Linked worktrees (a `.git` file) are found; malformed `.git` entries are not.
#[test]
fn git_files_and_malformed_git_entries() {
    let root = tree("gitfile");
    let main = root.join("main");
    init(&main);
    let _ = git::run(
        Some(&main),
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ],
    );
    let linked = root.join("linked");
    assert!(
        git::run(
            Some(&main),
            &["worktree", "add", "-q", &linked.to_string_lossy()]
        )
        .is_ok()
    );
    // A `.git` folder with nothing in it, a `.git` file naming nothing, one naming a folder
    // that isn't there, and binary noise.
    let _ = std::fs::create_dir_all(root.join("empty-git/.git"));
    let _ = std::fs::create_dir_all(root.join("noise"));
    let _ = std::fs::write(root.join("noise/.git"), [0u8, 159, 146, 150]);
    let _ = std::fs::create_dir_all(root.join("blank"));
    let _ = std::fs::write(root.join("blank/.git"), "gitdir:   \n");
    let _ = std::fs::create_dir_all(root.join("dangling"));
    let _ = std::fs::write(root.join("dangling/.git"), "gitdir: ../nowhere/.git\n");
    let _ = std::fs::create_dir_all(root.join("plain/src"));
    let found = at_depth(&root, 1);
    assert_eq!(rel(&root, &found), ["linked", "main"]);
    assert!(gitfile_target(&linked, &linked.join(".git")).is_some());
    let _ = std::fs::remove_dir_all(&root);
}

/// A folder of many entries stops the walk at the entry budget, holding no more than the
/// folders it may look in: an honest partial result.
#[test]
fn huge_fan_out_stops_within_its_bounds() {
    let root = tree("fanout");
    for i in 0..600 {
        let _ = std::fs::create_dir(root.join(format!("d{i:04}")));
    }
    for i in 0..400 {
        let _ = std::fs::write(root.join(format!("f{i:04}.txt")), "");
    }
    init(&root.join("d0000"));
    let stop = AtomicBool::new(false);
    let limits = ScanLimits {
        max_entries: 300,
        ..NEARBY_LIMITS
    };
    let mut most = 0;
    let found = walk(&root, limits, &stop, &mut |e| {
        if let ScanEvent::Progress(p) = e {
            most = most.max(p.folders);
        }
    });
    let found = found.unwrap_or_default();
    assert_eq!(found.stopped, Some(Stopped::Entries));
    assert!(most <= NEARBY_LIMITS.max_dirs, "{most}");
    // Without the entry budget, the folder budget cuts in and what is kept is in name order.
    let found = walk(&root, NEARBY_LIMITS, &stop, &mut |_| {}).unwrap_or_default();
    assert_eq!(found.stopped, Some(Stopped::Folders));
    assert_eq!(rel(&root, &found), ["d0000"]);
    let _ = std::fs::remove_dir_all(&root);
}

/// The time budget and cancellation end a walk with what it had; repositories are told one by
/// one as they are found.
#[test]
fn budget_cancellation_and_events() {
    let root = tree("budget");
    init(&root.join("a"));
    init(&root.join("b"));
    let stop = AtomicBool::new(false);
    let none = ScanLimits {
        budget: Some(Duration::ZERO),
        ..ROOT_LIMITS
    };
    let found = walk(&root, none, &stop, &mut |_| {}).unwrap_or_default();
    assert_eq!(found.stopped, Some(Stopped::Time));
    let mut told = Vec::new();
    let found = walk(&root, ROOT_LIMITS, &stop, &mut |e| {
        if let ScanEvent::Repo(p) = e {
            told.push(p.to_owned());
        }
    })
    .unwrap_or_default();
    assert_eq!(found.stopped, None);
    told.sort();
    assert_eq!(told, found.repos);
    stop.store(true, Ordering::Relaxed);
    let found = walk(&root, ROOT_LIMITS, &stop, &mut |_| {}).unwrap_or_default();
    assert_eq!(found.stopped, Some(Stopped::Asked));
    let _ = std::fs::remove_dir_all(&root);
}

/// A folder that can't be read is passed over; a root that can't be read is an error.
#[cfg(unix)]
#[test]
fn unreadable_folders_are_passed_over() {
    use std::os::unix::fs::PermissionsExt;
    let root = tree("unreadable");
    init(&root.join("ok"));
    let locked = root.join("locked");
    let _ = std::fs::create_dir_all(locked.join("inner"));
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000));
    let found = at_depth(&root, 2);
    let stop = AtomicBool::new(false);
    let direct = walk(&locked, ROOT_LIMITS, &stop, &mut |_| {});
    let _ = std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755));
    assert_eq!(rel(&root, &found), ["ok"]);
    // Root (uid 0) reads anything: only an ordinary user sees the refusal.
    if let Err(e) = direct {
        assert!(e.contains("isn't allowed"), "{e}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
