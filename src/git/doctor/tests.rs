//! The doctor's reads and writes against real repositories in temporary folders.

use std::path::{Path, PathBuf};

use super::config::{self, Scope};
use super::flags::{self, Flag, Sparse};
use crate::git::run;

/// A fresh repository named after `name`, with one commit of `files`.
pub(crate) fn repo(name: &str, files: &[(&str, &[u8])]) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-doctor-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(&dir, args)?;
    }
    for (path, bytes) in files {
        write(&dir.join(path), bytes)?;
    }
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "base"])?;
    Ok(dir)
}

/// Writes `bytes` to `file`, making its folders.
pub(crate) fn write(file: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, bytes).map_err(|e| format!("{}: {e}", file.display()))
}

/// Runs git in `dir`, saying which command failed.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    run(Some(dir), args).map_err(|e| format!("git {args:?}: {e}"))
}

/// `args` as the borrowed list `git` takes.
pub(crate) fn borrowed(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[test]
fn every_scope_shows_with_its_file_and_the_last_one_wins() -> Result<(), String> {
    let dir = repo("config-scopes", &[])?;
    let result = (|| {
        // A file of this test's own, read through an include as a user's would be.
        let included = dir.join("included config");
        write(&included, b"[pull]\n\trebase = merges\n")?;
        git(&dir, &["config", "pull.rebase", "false"])?;
        git(
            &dir,
            &["config", "include.path", &included.to_string_lossy()],
        )?;
        git(&dir, &["config", "user.name", "Zoë Ünïcode"])?;
        git(&dir, &["config", "core.fsmonitor", "true"])?;
        // Never listed, whatever it holds.
        git(&dir, &["config", "credential.helper", "store"])?;
        let all = config::origins(&dir)?;
        let rebase = config::values_of(&all, "pull.rebase");
        assert!(
            rebase.iter().filter(|o| o.scope == Scope::Local).count() >= 2,
            "{rebase:?}"
        );
        // The include comes after the line that names it: it wins.
        let winner = (*rebase.last().ok_or("no value")?).clone();
        assert_eq!(winner.shown_value(), "merges");
        assert!(
            winner
                .file
                .as_deref()
                .is_some_and(|f| f.ends_with("included config")),
            "{winner:?}"
        );
        let name = config::values_of(&all, "user.name");
        assert_eq!(name.last().map(|o| o.shown_value()), Some("Zoë Ünïcode"));
        assert!(all.iter().all(|o| !o.key.starts_with("credential")));

        // Removed from the included file, the repository's own value applies again.
        git(&dir, &borrowed(&config::unset_args(&winner)?))?;
        let all = config::origins(&dir)?;
        let rebase = config::values_of(&all, "pull.rebase");
        assert_eq!(rebase.last().map(|o| o.shown_value()), Some("false"));

        // Set at the repository's scope, in the repository's own file.
        git(
            &dir,
            &borrowed(&config::set_args(Scope::Local, "pull.rebase", "true")?),
        )?;
        let all = config::origins(&dir)?;
        let last = config::values_of(&all, "pull.rebase")
            .last()
            .map(|o| (o.scope, o.shown_value().to_owned()));
        assert_eq!(last, Some((Scope::Local, "true".to_owned())));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn worktree_scope_and_other_worktrees() -> Result<(), String> {
    let dir = repo("config-worktrees", &[("a.txt", b"a\n")])?;
    let other = PathBuf::from(format!("{} other wt", dir.display()));
    let _ = std::fs::remove_dir_all(&other);
    let result = (|| {
        git(&dir, &["config", "extensions.worktreeConfig", "true"])?;
        git(
            &dir,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "side",
                &other.to_string_lossy(),
            ],
        )?;
        git(&other, &["config", "--worktree", "pull.ff", "only"])?;
        let seen = config::in_worktrees(&[dir.clone(), other.clone()], "pull.ff");
        assert_eq!(seen.len(), 2);
        let local = seen[0].1.clone()?;
        let there = seen[1].1.clone()?.ok_or("not set in the other worktree")?;
        // Unless this machine's own config sets it: then not from the worktree.
        assert!(local.is_none_or(|o| o.scope != Scope::Worktree));
        assert_eq!(
            (there.scope, there.shown_value()),
            (Scope::Worktree, "only")
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&other);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn flags_are_listed_and_cleared() -> Result<(), String> {
    let dir = repo(
        "flags",
        &[
            ("plain.txt", b"a\n"),
            ("assumed file.txt", b"a\n"),
            ("skipped/ü.txt", b"a\n"),
            ("both.txt", b"a\n"),
        ],
    )?;
    let result = (|| {
        git(
            &dir,
            &[
                "update-index",
                "--assume-unchanged",
                "--",
                "assumed file.txt",
            ],
        )?;
        git(
            &dir,
            &["update-index", "--skip-worktree", "--", "skipped/ü.txt"],
        )?;
        // One flag per run: given both options, update-index takes only the last.
        git(
            &dir,
            &["update-index", "--assume-unchanged", "--", "both.txt"],
        )?;
        git(&dir, &["update-index", "--skip-worktree", "--", "both.txt"])?;
        let (list, sparse) = flags::flagged(&dir)?;
        assert_eq!(sparse, Sparse::Off);
        let flag_of = |p: &str| list.iter().find(|f| f.path == p).map(|f| f.flag);
        assert_eq!(flag_of("assumed file.txt"), Some(Flag::AssumeUnchanged));
        assert_eq!(flag_of("skipped/ü.txt"), Some(Flag::SkipWorktree));
        assert_eq!(flag_of("both.txt"), Some(Flag::Both));
        assert_eq!(flag_of("plain.txt"), None);

        // A change to an assumed-unchanged file doesn't show; once cleared, it does.
        write(&dir.join("assumed file.txt"), b"changed\n")?;
        assert!(!git(&dir, &["status", "--porcelain"])?.contains("assumed"));
        for op in flags::clear_ops(&list) {
            git(&dir, &borrowed(&op))?;
        }
        let (after, _) = flags::flagged(&dir)?;
        assert!(after.is_empty(), "{after:?}");
        let status = git(&dir, &["status", "--porcelain"])?;
        assert!(status.contains("assumed file.txt"), "{status}");
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn sparse_checkout_entries_are_not_manual_flags() -> Result<(), String> {
    let dir = repo(
        "flags-sparse",
        &[
            ("top.txt", b"a\n"),
            ("kept/a.txt", b"a\n"),
            ("left out/b.txt", b"b\n"),
        ],
    )?;
    let result = (|| {
        git(&dir, &["sparse-checkout", "set", "--cone", "kept"])?;
        // A manual flag inside the cone stays one.
        git(
            &dir,
            &["update-index", "--assume-unchanged", "--", "kept/a.txt"],
        )?;
        let (list, sparse) = flags::flagged(&dir)?;
        assert_eq!(sparse, Sparse::Cone(vec!["kept".to_owned()]));
        let flag_of = |p: &str| list.iter().find(|f| f.path == p).map(|f| f.flag);
        assert_eq!(flag_of("left out/b.txt"), Some(Flag::Sparse));
        assert_eq!(flag_of("kept/a.txt"), Some(Flag::AssumeUnchanged));
        assert_eq!(flag_of("top.txt"), None);
        // Clearing never touches what sparse-checkout left out.
        let ops = flags::clear_ops(&list);
        assert!(!ops.iter().flatten().any(|a| a == "left out/b.txt"));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn sparse_files_with_both_flags_and_quoted_folders() -> Result<(), String> {
    let dir = repo(
        "flags-sparse-both",
        &[
            ("top.txt", b"a\n"),
            ("in é dir/a.txt", b"a\n"),
            ("out/b.txt", b"b\n"),
        ],
    )?;
    let result = (|| {
        git(&dir, &["sparse-checkout", "set", "--cone", "in é dir"])?;
        // assume-unchanged on top of sparse-checkout's skip-worktree.
        git(
            &dir,
            &["update-index", "--assume-unchanged", "--", "out/b.txt"],
        )?;
        let (list, sparse) = flags::flagged(&dir)?;
        // Read unquoted: the folder's files are checked out.
        assert_eq!(sparse, Sparse::Cone(vec!["in é dir".to_owned()]));
        let flag_of = |p: &str| list.iter().find(|f| f.path == p).map(|f| f.flag);
        assert_eq!(flag_of("out/b.txt"), Some(Flag::AssumeOnSparse));
        assert_eq!(flag_of("in é dir/a.txt"), None);
        // Clearing it takes only assume-unchanged off: it stays left out, not deleted.
        for op in flags::clear_ops(&list) {
            git(&dir, &borrowed(&op))?;
        }
        let (after, _) = flags::flagged(&dir)?;
        let b = after.iter().find(|f| f.path == "out/b.txt").map(|f| f.flag);
        assert_eq!(b, Some(Flag::Sparse));
        assert_eq!(git(&dir, &["status", "--porcelain"])?, "");

        // Patterns instead of cone: every skip-worktree file is sparse-checkout's.
        git(&dir, &["sparse-checkout", "set", "--no-cone", "/top.txt"])?;
        let (list, sparse) = flags::flagged(&dir)?;
        assert_eq!(sparse, Sparse::Patterns);
        assert!(list.iter().all(|f| f.flag == Flag::Sparse), "{list:?}");
        assert!(flags::clear_ops(&list).is_empty());
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn setting_a_key_a_file_holds_twice_replaces_both() -> Result<(), String> {
    let dir = repo("config-twice", &[])?;
    let result = (|| {
        git(&dir, &["config", "--add", "pull.rebase", "true"])?;
        git(&dir, &["config", "--add", "pull.rebase", "false"])?;
        git(
            &dir,
            &borrowed(&config::set_args(Scope::Local, "pull.rebase", "merges")?),
        )?;
        let all = config::origins(&dir)?;
        let local: Vec<_> = config::values_of(&all, "pull.rebase")
            .into_iter()
            .filter(|o| o.scope == Scope::Local)
            .map(|o| o.shown_value().to_owned())
            .collect();
        assert_eq!(local, ["merges"]);
        assert!(!config::worktree_config(&dir));
        git(&dir, &["config", "extensions.worktreeConfig", "true"])?;
        assert!(config::worktree_config(&dir));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_folder_that_is_no_repository_says_so() {
    let dir = std::env::temp_dir().join(format!("gitcraft-doctor-none-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    // Gone: `config --list` would read the other files fine outside a repository.
    let missing = dir.join("gone");
    let origins = config::origins(&missing);
    let flags = flags::flagged(&missing);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(origins.is_err_and(|e| e.starts_with("Could not read the configuration")));
    assert!(flags.is_err_and(|e| e.starts_with("Could not list the tracked files")));
}
