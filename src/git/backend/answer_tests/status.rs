//! Status.

use super::*;

/// Check native porcelain status through staged, unstaged, and ignored changes.
#[test]
pub(super) fn porcelain_status_matches_git() -> Result<(), String> {
    let repo = Repo::new("answer-status")?;
    assert!(same(repo.path(), STATUS));
    repo.commit(
        "base",
        &[
            ("both", Some("one\n")),
            ("gone", Some("gone\n")),
            ("cached", Some("cached\n")),
            ("space ü.txt", Some("hello\n")),
            (".gitignore", Some("*.ignored\n")),
        ],
    )?;
    assert!(same(repo.path(), STATUS));
    std::fs::write(repo.path().join("both"), "two\n").map_err(|e| e.to_string())?;
    std::fs::remove_file(repo.path().join("gone")).map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("space ü.txt"), "changed\n").map_err(|e| e.to_string())?;
    std::fs::create_dir_all(repo.path().join("sub/empty")).map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("sub/new ü"), "new\n").map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("skip.ignored"), "ignored\n").map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join(".git/info/exclude"), "excluded\n")
        .map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("excluded"), "ignored\n").map_err(|e| e.to_string())?;
    assert!(same(repo.path(), STATUS));
    repo.git(&["add", "both"])?;
    std::fs::write(repo.path().join("both"), "three\n").map_err(|e| e.to_string())?;
    assert!(same(repo.path(), STATUS));
    repo.git(&["rm", "--cached", "cached"])?;
    assert!(same(repo.path(), STATUS));
    repo.git(&["config", "core.autocrlf", "true"])?;
    std::fs::write(repo.path().join("space ü.txt"), "hello\r\n").map_err(|e| e.to_string())?;
    assert!(same(repo.path(), STATUS));
    repo.git(&["add", "sub/new ü"])?;
    left_to_git(repo.path(), &[STATUS]); // possible rename
    repo.git(&["reset", "--hard", "HEAD"])?;
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        repo.git(&["update-index", flag, "both"])?;
        left_to_git(repo.path(), &[STATUS]);
        repo.git(&[
            "update-index",
            "--no-assume-unchanged",
            "--no-skip-worktree",
            "both",
        ])?;
    }
    std::fs::write(repo.path().join("intent"), "new\n").map_err(|e| e.to_string())?;
    repo.git(&["add", "-N", "intent"])?;
    left_to_git(repo.path(), &[STATUS]);
    repo.git(&["reset", "--hard", "HEAD"])?;
    // Git accepts both spellings and case-insensitive values for copy detection.
    for value in ["copies", "copy", "CoPy"] {
        repo.git(&["config", "status.renames", value])?;
        left_to_git(repo.path(), &[STATUS]);
    }
    repo.git(&["config", "--unset", "status.renames"])?;
    repo.git(&["config", "diff.renames", "copy"])?;
    left_to_git(repo.path(), &[STATUS]);
    Ok(())
}

/// Verify file-mode parity and fallback for submodules, nested repos, and conflicts.
#[test]
pub(super) fn porcelain_status_modes_and_fallbacks() -> Result<(), String> {
    let repo = Repo::new("answer-status-modes")?;
    let base = repo.commit("base", &[("a", Some("one\n"))])?;
    repo.git(&["update-index", "--chmod=+x", "a"])?;
    assert!(same(repo.path(), STATUS));
    let blob = repo.rev("HEAD:a")?;
    repo.git(&["update-index", "--cacheinfo", &format!("120000,{blob},a")])?;
    assert!(same(repo.path(), STATUS));
    repo.git(&["reset", "--hard", "HEAD"])?;
    std::fs::write(repo.path().join(".gitmodules"), "").map_err(|e| e.to_string())?;
    left_to_git(repo.path(), &[STATUS]);
    std::fs::remove_file(repo.path().join(".gitmodules")).map_err(|e| e.to_string())?;
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{base},sub"),
    ])?;
    left_to_git(repo.path(), &[STATUS]);
    repo.git(&["reset", "--hard", "HEAD"])?;
    std::fs::create_dir_all(repo.path().join("nested")).map_err(|e| e.to_string())?;
    assert_eq!(git(&repo.path().join("nested"), &["init", "-q"]).code, 0);
    left_to_git(repo.path(), &[STATUS]);
    std::fs::remove_dir_all(repo.path().join("nested")).map_err(|e| e.to_string())?;
    repo.git(&["switch", "-q", "-c", "side"])?;
    repo.commit("side", &[("a", Some("side\n"))])?;
    repo.git(&["switch", "-q", "main"])?;
    repo.commit("main", &[("a", Some("main\n"))])?;
    assert_ne!(git(repo.path(), &["merge", "side"]).code, 0);
    left_to_git(repo.path(), &[STATUS]);
    Ok(())
}
