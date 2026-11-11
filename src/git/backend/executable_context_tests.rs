use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn lookup_requires_same_git_before_any_relative_path_entry() -> std::io::Result<()> {
    let root =
        std::env::temp_dir().join(format!("gitcraft-native-git-path-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let first = root.join("first");
    let second = root.join("second");
    let unexecutable = root.join("unexecutable");
    let links = root.join("links");
    for dir in [&first, &second, &unexecutable, &links] {
        std::fs::create_dir_all(dir)?;
    }
    for (dir, mode) in [(&first, 0o755), (&second, 0o755), (&unexecutable, 0o644)] {
        let git = dir.join("git");
        // A marker: the helper must inspect metadata, never execute this file.
        std::fs::write(&git, "This file must never execute")?;
        std::fs::set_permissions(git, std::fs::Permissions::from_mode(mode))?;
    }
    let selected = first.join("git");
    let paths = |dirs: &[&Path]| std::env::join_paths(dirs).map_err(std::io::Error::other);
    let expected = Some(std::fs::canonicalize(&selected)?);
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[&first, &second])?),
        expected
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[&second, &first])?),
        None
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[&unexecutable, &first])?),
        expected
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[Path::new("."), &first])?),
        None
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[&first, Path::new("")])?),
        expected
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[&first, Path::new("relative")])?),
        expected
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[Path::new(""), &first])?),
        None
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), std::ffi::OsStr::new("")),
        None
    );
    assert_eq!(
        git_executable_context_matches(None, &paths(&[&first])?),
        None
    );
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[&links])?),
        None
    );
    symlink(&selected, links.join("git"))?;
    assert_eq!(
        git_executable_context_matches(Some(&selected), &paths(&[&links, &second])?),
        expected
    );
    std::fs::remove_dir_all(root)
}
