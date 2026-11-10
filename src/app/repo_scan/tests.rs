use super::*;
use crate::git;

fn tree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-scan-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn init(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    assert!(git::run(Some(dir), &["init", "-q"]).is_ok());
}

fn names(root: &Path, found: &Found) -> Vec<String> {
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

fn run(root: &Path, limits: ScanLimits) -> Found {
    let stop = AtomicBool::new(false);
    scan(root, limits, &stop, &mut |_| {}).unwrap_or_default()
}

#[test]
fn repositories_are_found_where_they_are_looked_for() {
    let root = tree("found");
    init(&root.join("app"));
    init(&root.join("app/plugins/inner"));
    init(&root.join("my projects/Größe 日本"));
    init(&root.join("node_modules/dep"));
    init(&root.join(".hidden/secret"));
    init(&root.join("a/b/c/d"));
    init(&root.join("a/b/c/d/e"));
    let _ = std::fs::write(root.join("a/file.txt"), "x");
    let found = run(&root, LIMITS);
    assert_eq!(
        names(&root, &found),
        vec![
            "a/b/c/d",
            "app",
            "app/plugins/inner",
            "my projects/Größe 日本"
        ]
    );
    assert_eq!(found.stopped, None);
    // Not as deep.
    let shallow = ScanLimits { depth: 1, ..LIMITS };
    assert_eq!(names(&root, &run(&root, shallow)), vec!["app"]);
    // At most so many.
    let few = ScanLimits {
        max_repos: 2,
        ..LIMITS
    };
    let cut = run(&root, few);
    assert_eq!(cut.repos.len(), 2);
    assert_eq!(cut.stopped, Some(Stopped::Repos));
    // The folder picked is itself one.
    let inner = run(&root.join("app"), LIMITS);
    assert_eq!(names(&root, &inner), vec!["app", "app/plugins/inner"]);
    let _ = std::fs::remove_dir_all(&root);
}

/// A repository named like a folder that isn't looked in (`build`, `.dotfiles`) is still
/// found; what is inside it isn't looked at.
#[test]
fn repositories_named_like_skipped_folders_are_found() {
    let root = tree("skipped-names");
    init(&root.join("build"));
    init(&root.join("tools/target"));
    init(&root.join(".dotfiles"));
    init(&root.join("vendor/inside/repo"));
    let found = run(&root, LIMITS);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        names(&root, &found),
        vec![".dotfiles", "build", "tools/target"]
    );
}

/// No more folders are kept to look in than the limit, however many a folder holds.
#[test]
fn the_folders_kept_stay_within_the_limit() {
    let root = tree("many");
    for i in 0..40 {
        let _ = std::fs::create_dir_all(root.join(format!("d{i:02}/x")));
    }
    init(&root.join("d00"));
    let mut most = 0;
    let stop = AtomicBool::new(false);
    let limits = ScanLimits {
        max_dirs: 10,
        ..LIMITS
    };
    let found = scan(&root, limits, &stop, &mut |p| most = most.max(p.folders));
    let _ = std::fs::remove_dir_all(&root);
    let found = found.unwrap_or_default();
    assert_eq!(found.stopped, Some(Stopped::Folders));
    assert!(most <= 10, "{most}");
    assert_eq!(names(&root, &found), vec!["d00"]);
}

#[test]
fn a_missing_folder_is_an_error_and_an_empty_one_finds_nothing() {
    let root = tree("empty");
    let stop = AtomicBool::new(false);
    assert_eq!(
        scan(&root, LIMITS, &stop, &mut |_| {}),
        Ok(Found::default())
    );
    let gone = scan(&root.join("gone"), LIMITS, &stop, &mut |_| {});
    assert!(gone.is_err_and(|e| e.ends_with("isn't there any more.")));
    // Stopped at once: it says so, with nothing found.
    init(&root.join("r"));
    let stopped = AtomicBool::new(true);
    assert_eq!(
        scan(&root, LIMITS, &stopped, &mut |_| {}),
        Ok(Found {
            repos: Vec::new(),
            stopped: Some(Stopped::Asked)
        })
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn links_are_not_followed() {
    let root = tree("loop");
    init(&root.join("real"));
    let _ = std::fs::create_dir_all(root.join("dir"));
    // A loop, and a second way to the repository.
    let _ = std::os::unix::fs::symlink(&root, root.join("dir/up"));
    let _ = std::os::unix::fs::symlink(root.join("real"), root.join("again"));
    let found = run(&root, LIMITS);
    assert_eq!(names(&root, &found), vec!["real"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn skipped_folders() {
    assert!(walked("src"));
    assert!(walked("my code"));
    assert!(!walked(".git"));
    assert!(!walked(".cache"));
    assert!(!walked("node_modules"));
    assert!(!walked("target"));
}

/// Adding what a scan found never pushes out what the recent list has.
#[test]
fn adding_found_ones_keeps_the_recent_list() {
    let p = |s: &str| PathBuf::from(s);
    let mut recent: Vec<PathBuf> = (0..10).map(|i| p(&format!("/r{i}"))).collect();
    let found = [p("/r3"), p("/new1"), p("/new2"), p("/new3"), p("/new4")];
    assert_eq!(add_to_recent(&mut recent, &found, 12), (2, 1, 2));
    assert_eq!(recent.len(), 12);
    assert_eq!(recent[0], p("/r0"));
    assert_eq!(recent[10..], [p("/new1"), p("/new2")]);
    assert_eq!(
        added_message(2, 1, 2),
        format!(
            "Added 2 repositories to the recent list (1 was already there; 2 repositories \
             didn't fit: it holds {MAX_RECENT}, remove some to make room)"
        )
    );
    assert_eq!(
        added_message(1, 0, 0),
        "Added 1 repository to the recent list"
    );
}
