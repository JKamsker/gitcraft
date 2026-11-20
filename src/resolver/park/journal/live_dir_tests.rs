use super::ancestor_tests::{Answer, paths, recorded};
use super::*;
use std::io::{Error, ErrorKind};

fn dirs() -> [PathBuf; 3] {
    let repo = Path::new("repo");
    [
        repo.join("src").join("deep"),
        repo.join("src"),
        repo.join("vendor"),
    ]
}

#[test]
fn live_directories_flush_children_first_and_shared_ancestors_once() {
    let mut seen = Vec::new();
    let dirs = dirs();
    let sync = recorded(&mut seen, |_| Answer::Flushed);
    assert_eq!(sync_each(dirs.iter().map(PathBuf::as_path), sync), Ok(()));
    let repo = Path::new("repo");
    let expected = [
        (repo.join("src").join("deep"), true),
        (repo.join("src"), false),
        (repo.to_path_buf(), false),
        (PathBuf::new(), false),
        (repo.join("vendor"), true),
    ];
    assert_eq!(seen, expected);
}

#[test]
fn a_live_directory_without_entry_rights_is_passed_over_to_its_parent_on_windows() {
    // A lone denied live directory: on Windows its parent is still flushed.
    let vendor = Path::new("repo").join("vendor");
    let mut seen = Vec::new();
    let sync = recorded(&mut seen, |dir| {
        if dir == vendor {
            Answer::OpenFails(Error::from(ErrorKind::PermissionDenied))
        } else {
            Answer::Flushed
        }
    });
    let result = sync_each([vendor.as_path()], sync);
    assert_eq!(result.is_ok(), cfg!(windows), "{result:?}");
    if cfg!(windows) {
        let expected = [vendor.clone(), PathBuf::from("repo"), PathBuf::new()];
        assert_eq!(paths(&seen), expected);
    } else {
        assert_eq!(paths(&seen), [vendor]);
    }
}

#[test]
fn a_live_flush_refused_after_opening_is_never_passed_over() {
    let vendor = Path::new("repo").join("vendor");
    let mut seen = Vec::new();
    let sync = recorded(&mut seen, |dir| {
        if dir == vendor {
            Answer::FlushFails(Error::from(ErrorKind::PermissionDenied))
        } else {
            Answer::Flushed
        }
    });
    let error = sync_each([vendor.as_path()], sync)
        .err()
        .unwrap_or_default();
    assert!(
        error.contains("vendor") && error.contains("flush"),
        "{error}"
    );
}

#[test]
fn a_live_directory_held_by_another_process_or_failing_otherwise_stops_the_restore() {
    for n in 0..2 {
        let dirs = dirs();
        let src = dirs[1].clone();
        let mut seen = Vec::new();
        let sync = recorded(&mut seen, |dir| match (dir == src, n) {
            // Still held after the patient retries of the requested directory.
            (true, 0) => Answer::OpenFails(Error::from_raw_os_error(32)),
            (true, _) => Answer::OpenFails(Error::from(ErrorKind::Other)),
            (false, _) => Answer::Flushed,
        });
        // src is an ancestor of the first directory (tolerated on Windows for a sharing
        // violation only), then requested itself: that request must fail.
        let error = sync_each(dirs.iter().map(PathBuf::as_path), sync)
            .err()
            .unwrap_or_default();
        assert!(error.starts_with("sync repo"), "case {n}: {error}");
        assert!(
            !paths(&seen).contains(&dirs[2]),
            "nothing after the failure"
        );
    }
}

#[test]
fn real_sibling_directories_flush_through_a_shared_parent() -> Result<(), Box<dyn std::error::Error>>
{
    let root = std::env::temp_dir().join(format!("gitcraft-live-dirs-{}", std::process::id()));
    let (a, b) = (root.join("a/deep"), root.join("b"));
    std::fs::create_dir_all(&a)?;
    std::fs::create_dir_all(&b)?;
    let result = sync_ancestors_of([a.as_path(), b.as_path()]);
    std::fs::remove_dir_all(&root)?;
    result?;
    Ok(())
}
