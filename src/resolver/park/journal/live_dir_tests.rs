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
    let result = sync_each(dirs.iter().map(PathBuf::as_path), |dir: &Path| {
        seen.push(dir.to_path_buf());
        Ok(())
    });
    assert_eq!(result, Ok(()));
    let repo = Path::new("repo");
    let expected = [
        repo.join("src").join("deep"),
        repo.join("src"),
        repo.to_path_buf(),
        PathBuf::new(),
        repo.join("vendor"),
    ];
    assert_eq!(seen, expected);
}

#[test]
fn a_live_directory_without_entry_rights_is_skipped_only_on_windows() {
    let dirs = dirs();
    let vendor = dirs[2].clone();
    let mut seen = Vec::new();
    let result = sync_each(dirs.iter().map(PathBuf::as_path), |dir: &Path| {
        seen.push(dir.to_path_buf());
        if dir == vendor {
            Err(Error::from(ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    });
    assert_eq!(result.is_ok(), cfg!(windows), "{result:?}");
    assert_eq!(seen.last(), Some(&vendor));
}

#[test]
fn a_live_directory_held_by_another_process_or_failing_otherwise_stops_the_restore() {
    for error in [Error::from_raw_os_error(32), Error::from(ErrorKind::Other)] {
        let dirs = dirs();
        let src = dirs[1].clone();
        let kind = error.kind();
        let code = error.raw_os_error();
        let mut seen = Vec::new();
        let result = sync_each(dirs.iter().map(PathBuf::as_path), |dir: &Path| {
            seen.push(dir.to_path_buf());
            if dir != src {
                return Ok(());
            }
            Err(code.map_or_else(|| Error::from(kind), Error::from_raw_os_error))
        });
        // src is an ancestor of the first directory (tolerated on Windows for a sharing
        // violation only), then requested itself: that request must fail.
        let error = result.err().unwrap_or_default();
        assert!(error.starts_with("sync repo"), "{code:?}: {error}");
        assert!(!seen.contains(&dirs[2]), "nothing after the failure");
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
