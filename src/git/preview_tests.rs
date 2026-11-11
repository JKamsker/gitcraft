//! Tests of the preview of a destructive operation against real repositories.

use std::path::{Path, PathBuf};

use super::*;

/// A new repository in a folder of its own, with an identity and no signing.
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-preview-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        run(Some(&dir), args)?;
    }
    Ok(dir)
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    run(Some(dir), args).map(|s| s.trim().to_owned())
}

/// An empty commit on what is checked out; its hash.
fn commit(dir: &Path, message: &str) -> Result<String, String> {
    git(dir, &["commit", "-q", "--allow-empty", "-m", message])?;
    git(dir, &["rev-parse", "HEAD"])
}

fn write(dir: &Path, path: &str, text: &str) -> Result<(), String> {
    std::fs::write(dir.join(path), text).map_err(|e| e.to_string())
}

fn kept(p: &Preview) -> Vec<(&str, &Kept)> {
    p.leaving
        .iter()
        .map(|c| (c.summary.as_str(), &c.kept))
        .collect()
}

fn reset_of_main(head: &str, to: &str, upstream: Option<&str>) -> PreviewAsk {
    let range = Range::new(head, Some(to));
    PreviewAsk {
        leaving: Some(range.clone()),
        moving: vec!["refs/heads/main".to_owned()],
        rewritten: Some(range),
        upstream: upstream.map(str::to_owned),
        files: LostFiles::HardReset(Some(to.to_owned())),
    }
}

/// A hard reset lists the commits after the target, newest first, each marked with where
/// else it is; the uncommitted tracked files, not the untracked ones; and what was pushed.
#[test]
fn a_hard_reset_lists_commits_files_and_what_was_pushed() -> Result<(), String> {
    let dir = repo("reset")?;
    let names = ["a b.txt", "ünï €.txt", "staged new.txt"];
    write(&dir, names[0], "1\n")?;
    write(&dir, names[1], "1\n")?;
    git(&dir, &["add", "-A"])?;
    let base = commit(&dir, "base")?;
    commit(&dir, "pushed")?;
    git(&dir, &["branch", "feat"])?;
    git(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"])?;
    commit(&dir, "tagged")?;
    git(&dir, &["tag", "v1"])?;
    let head = commit(&dir, "local only")?;
    write(&dir, names[0], "2\n")?;
    write(&dir, names[1], "2\n")?;
    write(&dir, names[2], "new\n")?;
    git(&dir, &["add", "--", names[2]])?;
    write(&dir, "untracked.txt", "x\n")?;

    let found = preview(
        &dir,
        &reset_of_main(&head, &base, Some("refs/remotes/origin/main")),
    )?;
    assert_eq!(
        kept(&found),
        [
            ("local only", &Kept::OnlyHere),
            ("tagged", &Kept::On("tag v1".to_owned())),
            ("pushed", &Kept::On("feat".to_owned())),
        ]
    );
    assert_eq!((found.leaving_total, found.only_here_total), (3, 1));
    assert_eq!(
        found.pushed,
        Some(Pushed {
            upstream: "origin/main".to_owned(),
            commits: 1
        })
    );
    let mut expected = names.map(str::to_owned).to_vec();
    expected.sort();
    assert_eq!(found.lost_files, expected);
    assert_eq!(found.lost_total, 3);

    // Without an upstream, or with one that is gone: nothing about pushing.
    let found = preview(&dir, &reset_of_main(&head, &base, None))?;
    assert_eq!(found.pushed, None);
    let gone = reset_of_main(&head, &base, Some("refs/remotes/origin/gone"));
    assert_eq!(preview(&dir, &gone)?.pushed, None);
    // Reset to where it is: nothing leaves.
    let found = preview(&dir, &reset_of_main(&head, &head, None))?;
    assert!(found.leaving.is_empty() && found.leaving_total == 0);
    Ok(())
}

/// Down to the first commit, and on a detached HEAD, where HEAD is what moves.
#[test]
fn reset_to_the_root_commit_and_on_a_detached_head() -> Result<(), String> {
    let dir = repo("root")?;
    let root = commit(&dir, "root")?;
    commit(&dir, "two")?;
    let head = commit(&dir, "three")?;
    let found = preview(&dir, &reset_of_main(&head, &root, None))?;
    assert_eq!(
        kept(&found),
        [("three", &Kept::OnlyHere), ("two", &Kept::OnlyHere)]
    );

    git(&dir, &["checkout", "-q", "--detach"])?;
    let detached = commit(&dir, "detached")?;
    let range = Range::new(&detached, Some(&root));
    let ask = PreviewAsk {
        leaving: Some(range.clone()),
        moving: vec!["HEAD".to_owned()],
        rewritten: Some(range),
        upstream: None,
        files: LostFiles::HardReset(None),
    };
    let found = preview(&dir, &ask)?;
    assert_eq!(
        kept(&found),
        [
            ("detached", &Kept::OnlyHere),
            ("three", &Kept::On("main".to_owned())),
            ("two", &Kept::On("main".to_owned())),
        ]
    );
    assert!(found.lost_files.is_empty());
    Ok(())
}

/// Deleting a branch: its commits HEAD hasn't, those another branch (or the one it tracks)
/// still has marked so; deleting a remote branch, the same from the remote-tracking ref.
#[test]
fn deleting_a_branch_or_a_remote_branch() -> Result<(), String> {
    let dir = repo("delete")?;
    let base = commit(&dir, "base")?;
    git(&dir, &["checkout", "-q", "-b", "feat"])?;
    commit(&dir, "shared")?;
    git(&dir, &["branch", "other"])?;
    git(&dir, &["update-ref", "refs/remotes/origin/feat", "HEAD"])?;
    let tip = commit(&dir, "feat only")?;
    git(&dir, &["checkout", "-q", "main"])?;
    let ask = PreviewAsk {
        leaving: Some(Range::new(&tip, Some(&base))),
        moving: vec!["refs/heads/feat".to_owned()],
        ..PreviewAsk::nothing()
    };
    let found = preview(&dir, &ask)?;
    assert_eq!(
        kept(&found),
        [
            ("feat only", &Kept::OnlyHere),
            ("shared", &Kept::On("other".to_owned()))
        ]
    );

    // On the remote, its commits are still on the local branch.
    let remote_tip = git(&dir, &["rev-parse", "refs/remotes/origin/feat"])?;
    let ask = PreviewAsk {
        leaving: Some(Range::new(&remote_tip, Some("HEAD"))),
        moving: vec!["refs/remotes/origin/feat".to_owned()],
        ..PreviewAsk::nothing()
    };
    let found = preview(&dir, &ask)?;
    assert_eq!(found.leaving.len(), 1);
    assert!(
        matches!(&found.leaving[0].kept, Kept::On(name) if name == "feat" || name == "other"),
        "{found:?}"
    );
    assert_eq!(found.only_here_total, 0);
    Ok(())
}

/// With HEAD unborn (a new orphan branch), a branch's whole history leaves.
#[test]
fn deleting_a_branch_while_head_is_unborn() -> Result<(), String> {
    let dir = repo("unborn")?;
    commit(&dir, "one")?;
    let tip = commit(&dir, "two")?;
    git(&dir, &["checkout", "-q", "--orphan", "fresh"])?;
    let ask = PreviewAsk {
        leaving: Some(Range::new(&tip, None)),
        moving: vec!["refs/heads/main".to_owned()],
        ..PreviewAsk::nothing()
    };
    let found = preview(&dir, &ask)?;
    assert_eq!(
        kept(&found),
        [("two", &Kept::OnlyHere), ("one", &Kept::OnlyHere)]
    );
    Ok(())
}

/// A repository with no commits: the files that would be lost are read, and asking about
/// commits fails with git's words rather than with nothing.
#[test]
fn an_empty_repository() -> Result<(), String> {
    let dir = repo("empty")?;
    write(&dir, "first file.txt", "x\n")?;
    git(&dir, &["add", "-A"])?;
    let ask = PreviewAsk {
        files: LostFiles::HardReset(None),
        ..PreviewAsk::nothing()
    };
    assert_eq!(preview(&dir, &ask)?.lost_files, ["first file.txt"]);
    let ask = PreviewAsk {
        leaving: Some(Range::new("HEAD", None)),
        ..PreviewAsk::nothing()
    };
    assert!(preview(&dir, &ask).is_err());
    Ok(())
}

/// Commits only here come after some that another branch has (a merged side branch): each is
/// still marked right.
#[test]
fn a_merged_side_branch_is_marked_commit_by_commit() -> Result<(), String> {
    let dir = repo("merge")?;
    let base = commit(&dir, "base")?;
    git(&dir, &["checkout", "-q", "-b", "side"])?;
    commit(&dir, "side 1")?;
    commit(&dir, "side 2")?;
    git(&dir, &["checkout", "-q", "main"])?;
    commit(&dir, "main 1")?;
    git(
        &dir,
        &["merge", "-q", "--no-ff", "-m", "merge side", "side"],
    )?;
    let head = commit(&dir, "main 2")?;
    let found = preview(&dir, &reset_of_main(&head, &base, None))?;
    assert_eq!(found.leaving_total, 5);
    assert_eq!(found.only_here_total, 3);
    for c in &found.leaving {
        let expected = if c.summary.starts_with("side") {
            Kept::On("side".to_owned())
        } else {
            Kept::OnlyHere
        };
        assert_eq!(c.kept, expected, "{}", c.summary);
    }
    Ok(())
}

/// Hundreds of commits leaving: the first few are listed, all are counted.
#[test]
fn a_huge_range_is_capped() -> Result<(), String> {
    let dir = repo("huge")?;
    let base = commit(&dir, "base")?;
    let tree = git(&dir, &["rev-parse", "HEAD^{tree}"])?;
    let mut tip = base.clone();
    for i in 0..365 {
        tip = git(
            &dir,
            &["commit-tree", &tree, "-p", &tip, "-m", &format!("c{i}")],
        )?;
    }
    git(&dir, &["update-ref", "refs/heads/main", &tip])?;
    git(&dir, &["branch", "half", &format!("{tip}~200")])?;
    let found = preview(&dir, &reset_of_main(&tip, &base, None))?;
    assert_eq!(found.leaving.len(), PREVIEW_SHOWN);
    assert_eq!(found.leaving_total, 365);
    assert_eq!(found.only_here_total, 200);
    assert_eq!(found.leaving[0].summary, "c364");
    assert!(found.leaving.iter().all(|c| c.kept == Kept::OnlyHere));
    Ok(())
}

/// Many changed files: the first are listed, all are counted.
#[test]
fn many_lost_files_are_capped() -> Result<(), String> {
    let dir = repo("files")?;
    for i in 0..40 {
        write(&dir, &format!("f{i:02}.txt"), "1\n")?;
    }
    git(&dir, &["add", "-A"])?;
    commit(&dir, "files")?;
    for i in 0..40 {
        write(&dir, &format!("f{i:02}.txt"), "2\n")?;
    }
    let ask = PreviewAsk {
        files: LostFiles::HardReset(None),
        ..PreviewAsk::nothing()
    };
    let found = preview(&dir, &ask)?;
    assert_eq!(found.lost_files.len(), PREVIEW_SHOWN);
    assert_eq!(found.lost_total, 40);
    assert_eq!(found.lost_files[0], "f00.txt");
    Ok(())
}

/// Untracked and ignored files a hard reset writes over (the target has a file there) are lost
/// too; others, and what is inside submodules, stay.
#[test]
fn a_hard_reset_counts_the_untracked_files_it_writes_over() -> Result<(), String> {
    let dir = repo("overwrite")?;
    write(&dir, ".gitignore", "*.log\n")?;
    std::fs::create_dir_all(dir.join("new dir")).map_err(|e| e.to_string())?;
    for path in ["precious.txt", "build.log", "new dir/inner.txt"] {
        write(&dir, path, "v1\n")?;
    }
    git(&dir, &["add", "-A"])?;
    git(&dir, &["add", "-f", "build.log"])?;
    let old = commit(&dir, "has them")?;
    git(
        &dir,
        &[
            "rm",
            "-q",
            "-r",
            "--cached",
            "precious.txt",
            "build.log",
            "new dir",
        ],
    )?;
    let head = commit(&dir, "drops them")?;
    // Still on disk, untracked (or ignored) now, and changed.
    for path in ["precious.txt", "build.log", "new dir/inner.txt"] {
        write(&dir, path, "PRECIOUS\n")?;
    }
    write(&dir, "other.txt", "stays\n")?;
    let found = preview(&dir, &reset_of_main(&head, &old, None))?;
    assert_eq!(
        found.lost_files,
        ["build.log", "new dir/inner.txt", "precious.txt"]
    );
    Ok(())
}

/// Changed files inside a submodule are not what a reset of the repository throws away.
#[test]
fn a_hard_reset_leaves_submodule_contents_out() -> Result<(), String> {
    let sub = repo("submodule-inner")?;
    commit(&sub, "inner")?;
    let dir = repo("submodule-outer")?;
    let url = sub.to_string_lossy().into_owned();
    git(
        &dir,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "-q",
            "add",
            &url,
            "sub",
        ],
    )?;
    let head = commit(&dir, "with a submodule")?;
    write(&dir.join("sub"), "dirty.txt", "x\n")?;
    git(&dir.join("sub"), &["add", "-A"])?;
    let found = preview(&dir, &reset_of_main(&head, &head, None))?;
    assert!(found.lost_files.is_empty(), "{:?}", found.lost_files);
    Ok(())
}

/// Deleting a branch here and on the remote: commits someone pushed there that the branch
/// here hasn't leave too.
#[test]
fn deleting_both_lists_what_only_the_remote_branch_has() -> Result<(), String> {
    let dir = repo("both")?;
    let base = commit(&dir, "base")?;
    git(&dir, &["checkout", "-q", "-b", "feat"])?;
    let local = commit(&dir, "f1")?;
    let remote = commit(&dir, "remote only")?;
    git(&dir, &["update-ref", "refs/remotes/origin/feat", &remote])?;
    git(&dir, &["reset", "-q", "--hard", &local])?;
    git(&dir, &["checkout", "-q", "main"])?;
    let ask = PreviewAsk::delete(
        &[&local, &remote],
        &base,
        vec![
            "refs/heads/feat".to_owned(),
            "refs/remotes/origin/feat".to_owned(),
        ],
    );
    let found = preview(&dir, &ask)?;
    // Made in the same second: in either order.
    let mut leaving = kept(&found);
    leaving.sort_by_key(|(summary, _)| *summary);
    assert_eq!(
        leaving,
        [("f1", &Kept::OnlyHere), ("remote only", &Kept::OnlyHere)]
    );
    Ok(())
}

/// `origin/HEAD` only points at a remote branch: deleting that branch leaves its commits on no
/// ref, whatever the symref says.
#[test]
fn origin_head_keeps_nothing() -> Result<(), String> {
    let dir = repo("origin-head")?;
    let base = commit(&dir, "base")?;
    let tip = commit(&dir, "on the remote")?;
    git(&dir, &["update-ref", "refs/remotes/origin/dev", &tip])?;
    git(
        &dir,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/dev",
        ],
    )?;
    git(&dir, &["reset", "-q", "--hard", &base])?;
    let ask = PreviewAsk::delete(&[&tip], &base, vec!["refs/remotes/origin/dev".to_owned()]);
    let found = preview(&dir, &ask)?;
    assert_eq!(kept(&found), [("on the remote", &Kept::OnlyHere)]);
    Ok(())
}

/// A branch whose name looks like a tag's or a remote branch's is named as the branch it is.
#[test]
fn a_branch_named_like_a_tag_is_still_a_branch() -> Result<(), String> {
    let dir = repo("odd-names")?;
    let base = commit(&dir, "base")?;
    let head = commit(&dir, "kept")?;
    git(&dir, &["branch", "tags/v9"])?;
    let found = preview(&dir, &reset_of_main(&head, &base, None))?;
    assert_eq!(kept(&found), [("kept", &Kept::On("tags/v9".to_owned()))]);
    Ok(())
}

#[test]
fn rebase_preview_names_rewritten_commits_and_already_pushed_history() -> Result<(), String> {
    let dir = repo("rebase-pushed")?;
    let base = commit(&dir, "base")?;
    let head = commit(&dir, "published work")?;
    git(&dir, &["update-ref", "refs/remotes/origin/main", &head])?;
    let found = preview(
        &dir,
        &PreviewAsk::rebase(
            "main",
            &head,
            &base,
            Some("refs/remotes/origin/main".into()),
        ),
    )?;
    assert_eq!(found.leaving_total, 1);
    assert_eq!(found.leaving[0].summary, "published work");
    assert_eq!(found.pushed.as_ref().map(|p| p.commits), Some(1));
    assert_eq!(found.lost_total, 0);
    let root = preview(
        &dir,
        &PreviewAsk::rebase("main", &head, "", Some("refs/remotes/origin/main".into())),
    )?;
    assert_eq!(root.leaving_total, 2);
    assert_eq!(root.pushed.as_ref().map(|p| p.commits), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
