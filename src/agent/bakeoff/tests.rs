//! Bake-off on real repositories: two attempts from one base, one a branch and one a worktree
//! with uncommitted changes; compared, then a branch assembled from both.

use std::path::{Path, PathBuf};

use super::assemble::{Plan, Take, assemble, file_hunks};
use super::{Attempt, compare};
use crate::git;

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    worktree: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
    out.unwrap_or_default()
}

/// Base: `a.txt` (five lines), `c.txt`, a `Cargo.toml`. Attempt `try-a` (a branch): line 2 of
/// `a.txt`, a new `docs/b ü.txt`, a dependency added and one bumped. Attempt `try-b` (a
/// worktree, uncommitted): line 5 of `a.txt`, `c.txt` deleted, an untracked `d.txt`.
fn fixture(name: &str) -> Fixture {
    let root =
        std::env::temp_dir().join(format!("gitcraft-bakeoff-{name}-{} ä", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("repo");
    let _ = std::fs::create_dir_all(&repo);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git_ok(&repo, args);
    }
    write(&repo.join("a.txt"), "1\n2\n3\n4\n5\n");
    write(&repo.join("c.txt"), "gone soon\n");
    write(
        &repo.join("Cargo.toml"),
        "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1.0\"\n",
    );
    git_ok(&repo, &["add", "-A"]);
    git_ok(&repo, &["commit", "-qm", "base"]);
    git_ok(&repo, &["checkout", "-q", "-b", "try-a"]);
    write(&repo.join("a.txt"), "1\nTWO\n3\n4\n5\n");
    write(&repo.join("docs/b ü.txt"), "new\n");
    write(
        &repo.join("Cargo.toml"),
        "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1.1\"\nanyhow = \"1\"\n",
    );
    git_ok(&repo, &["add", "-A"]);
    git_ok(&repo, &["commit", "-qm", "attempt a"]);
    git_ok(&repo, &["checkout", "-q", "main"]);
    let worktree = root.join("try b");
    git_ok(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "try-b",
            &worktree.display().to_string(),
            "main",
        ],
    );
    write(&worktree.join("a.txt"), "1\n2\n3\n4\nFIVE\n");
    let _ = std::fs::remove_file(worktree.join("c.txt"));
    write(&worktree.join("d.txt"), "untracked\n");
    Fixture {
        root,
        repo,
        worktree,
    }
}

fn attempts(fx: &Fixture) -> Vec<Attempt> {
    vec![
        Attempt {
            name: "try-a".to_owned(),
            rev: "try-a".to_owned(),
            worktree: None,
        },
        Attempt {
            name: "try-b".to_owned(),
            rev: "try-b".to_owned(),
            worktree: Some(fx.worktree.clone()),
        },
    ]
}

#[test]
fn attempts_are_compared_with_their_base_uncommitted_work_included() -> Result<(), String> {
    let fx = fixture("compare");
    let status_before = git_ok(&fx.worktree, &["status", "--porcelain"]);
    let cmp = compare(&fx.repo, &attempts(&fx))?;
    assert_eq!(cmp.base, git::head(&fx.repo));
    assert!(cmp.same_base);
    let [a, b] = &cmp.attempts[..] else {
        return Err("two attempts".to_owned());
    };
    let paths = |c: &super::Compared| c.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
    assert_eq!(paths(a), ["Cargo.toml", "a.txt", "docs/b ü.txt"]);
    assert_eq!(paths(b), ["a.txt", "c.txt", "d.txt"]);
    assert_eq!(a.new_deps, ["Cargo.toml: anyhow"]);
    assert!(!a.uncommitted && b.uncommitted);
    assert_eq!((b.added, b.removed), (2, 2));
    let files = cmp.files();
    assert_eq!(files[1], ("a.txt".to_owned(), vec![true, true]));
    // Looking touched nothing of the worktree's own.
    assert_eq!(
        git_ok(&fx.worktree, &["status", "--porcelain"]),
        status_before
    );
    assert!(!fx.worktree.join(".git").is_dir());
    Ok(())
}

#[test]
fn too_few_attempts_or_unrelated_ones_are_refused() {
    let fx = fixture("refuse");
    let one = &attempts(&fx)[..1];
    assert!(compare(&fx.repo, one).is_err_and(|e| e.contains("Pick 2 to 4")));
    git_ok(&fx.repo, &["checkout", "-q", "--orphan", "lonely"]);
    git_ok(
        &fx.repo,
        &["commit", "-q", "--allow-empty", "-m", "elsewhere"],
    );
    let unrelated = vec![
        attempts(&fx)[0].clone(),
        Attempt {
            name: "lonely".to_owned(),
            rev: "lonely".to_owned(),
            worktree: None,
        },
    ];
    assert!(compare(&fx.repo, &unrelated).is_err_and(|e| e.contains("no history")));
    let missing = vec![
        attempts(&fx)[0].clone(),
        Attempt {
            name: "x".to_owned(),
            rev: "no-such".to_owned(),
            worktree: None,
        },
    ];
    assert!(compare(&fx.repo, &missing).is_err_and(|e| e.contains("isn't a commit")));
}

#[test]
fn a_branch_is_assembled_from_a_file_of_one_attempt_and_hunks_of_both() -> Result<(), String> {
    let fx = fixture("assemble");
    let cmp = compare(&fx.repo, &attempts(&fx))?;
    let trees: Vec<String> = cmp.attempts.iter().map(|a| a.tree.clone()).collect();
    assert_eq!(
        file_hunks(&fx.repo, &cmp.base, &trees[1], "a.txt")?.len(),
        1
    );
    let head_before = git::head(&fx.repo);
    let plan = Plan {
        base: cmp.base.clone(),
        trees,
        names: vec!["try-a".to_owned(), "try-b".to_owned()],
        picks: vec![
            ("docs/b ü.txt".to_owned(), Take::File(0)),
            ("a.txt".to_owned(), Take::Hunks(vec![(0, 0), (1, 0)])),
            ("c.txt".to_owned(), Take::File(1)),
            ("d.txt".to_owned(), Take::Hunks(vec![(1, 0)])),
        ],
        branch: "best-of".to_owned(),
    };
    let commit = assemble(&fx.repo, &plan)?;
    assert_eq!(git_ok(&fx.repo, &["rev-parse", "best-of"]).trim(), commit);
    let show = |path: &str| git::run(Some(&fx.repo), &["show", &format!("best-of:{path}")]);
    assert_eq!(show("a.txt")?, "1\nTWO\n3\n4\nFIVE\n");
    assert_eq!(show("docs/b ü.txt")?, "new\n");
    assert_eq!(show("d.txt")?, "untracked\n");
    assert!(show("c.txt").is_err(), "deleted as attempt b did");
    // What wasn't picked stays as the base has it.
    assert_eq!(
        show("Cargo.toml")?,
        git_ok(&fx.repo, &["show", "main:Cargo.toml"])
    );
    let message = git_ok(&fx.repo, &["log", "-1", "--format=%B", "best-of"]);
    assert!(
        message.contains("a.txt: 2 hunk(s) from try-a, try-b"),
        "{message}"
    );
    // Nothing checked out moved, and the user's index is as it was.
    assert_eq!(git::head(&fx.repo), head_before);
    assert_eq!(git_ok(&fx.repo, &["status", "--porcelain"]), "");
    // Again under the same name: refused.
    let again = assemble(&fx.repo, &plan).err().unwrap_or_default();
    assert!(again.contains("exists already"), "{again}");
    let bad = Plan {
        branch: "bad name..".to_owned(),
        ..plan.clone()
    };
    assert!(assemble(&fx.repo, &bad).is_err_and(|e| e.contains("can't be a branch name")));
    let none = Plan {
        branch: "empty".to_owned(),
        picks: Vec::new(),
        ..plan
    };
    assert!(assemble(&fx.repo, &none).is_err_and(|e| e.contains("at least one")));
    Ok(())
}

#[test]
fn hunks_that_clash_are_refused_and_no_branch_is_made() -> Result<(), String> {
    let fx = fixture("clash");
    // Attempt b changes line 2 too.
    write(&fx.worktree.join("a.txt"), "1\nzwei\n3\n4\n5\n");
    let cmp = compare(&fx.repo, &attempts(&fx))?;
    let plan = Plan {
        base: cmp.base.clone(),
        trees: cmp.attempts.iter().map(|a| a.tree.clone()).collect(),
        names: vec!["a".to_owned(), "b".to_owned()],
        picks: vec![("a.txt".to_owned(), Take::Hunks(vec![(0, 0), (1, 0)]))],
        branch: "clash".to_owned(),
    };
    let err = assemble(&fx.repo, &plan).err().unwrap_or_default();
    assert!(err.contains("change the same lines"), "{err}");
    assert!(
        git::run(
            Some(&fx.repo),
            &["rev-parse", "--verify", "-q", "refs/heads/clash"]
        )
        .is_err()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_build_leaves_its_exit_status_for_the_grid() -> Result<(), String> {
    use super::build::{Built, read_status, script, status_file};
    let fx = fixture("build");
    let status = status_file(&fx.repo, &fx.worktree)?;
    let there = super::build::for_sh(&status);
    for (command, expected) in [
        ("test -f a.txt", Built::Passed),
        ("echo x\nexit 3", Built::Failed(3)),
    ] {
        let full = format!(
            "gitcraft_say() {{ printf '%s\\n' \"$*\"; }}\n{}",
            script(command, &there)
        );
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", &full])
            .current_dir(&fx.worktree)
            .output()
            .map_err(|e| e.to_string())?;
        assert!(out.status.success());
        let text = std::fs::read_to_string(&status).ok();
        assert_eq!(read_status(text.as_deref()), expected);
    }
    assert_eq!(read_status(None), Built::Running);
    Ok(())
}

/// What the review found: a rename, names that look like pathspec magic, a user's
/// `diff.interHunkContext`, an executable new file, a file deleted hunk by hunk, an attempt
/// that started from a newer commit.
#[test]
fn renames_odd_names_config_and_modes_are_assembled_right() -> Result<(), String> {
    let fx = fixture("odd");
    let repo = &fx.repo;
    git_ok(repo, &["config", "diff.interHunkContext", "5"]);
    git_ok(repo, &["checkout", "-q", "-b", "odd", "main"]);
    git_ok(repo, &["mv", "c.txt", "moved.txt"]);
    write(&repo.join(":memo"), "m\n");
    write(&repo.join("run.sh"), "#!/bin/sh\necho hi\n");
    // Two changes three lines apart: one hunk each, whatever interHunkContext says.
    write(&repo.join("a.txt"), "ONE\n2\n3\n4\nFIVE\n");
    git_ok(repo, &["add", "-A"]);
    git_ok(repo, &["update-index", "--chmod=+x", "run.sh"]);
    git_ok(repo, &["commit", "-qm", "odd"]);
    // The file on disk isn't executable, which its index entry now says it is.
    git_ok(repo, &["checkout", "-q", "-f", "main"]);
    let pair = vec![
        attempts(&fx)[0].clone(),
        Attempt {
            name: "odd".to_owned(),
            rev: "odd".to_owned(),
            worktree: None,
        },
    ];
    let cmp = compare(repo, &pair)?;
    let odd = &cmp.attempts[1];
    let status = |path: &str| odd.files.iter().find(|f| f.path == path).map(|f| f.status);
    assert_eq!(status("c.txt"), Some('D'), "a rename is the old file gone");
    assert_eq!(status("moved.txt"), Some('A'));
    assert_eq!(status(":memo"), Some('A'));
    let trees: Vec<String> = cmp.attempts.iter().map(|a| a.tree.clone()).collect();
    assert_eq!(file_hunks(repo, &cmp.base, &trees[1], "a.txt")?.len(), 2);
    let plan = Plan {
        base: cmp.base.clone(),
        trees,
        names: vec!["try-a".to_owned(), "odd".to_owned()],
        picks: vec![
            (":memo".to_owned(), Take::File(1)),
            ("moved.txt".to_owned(), Take::File(1)),
            ("c.txt".to_owned(), Take::Hunks(vec![(1, 0)])),
            ("run.sh".to_owned(), Take::Hunks(vec![(1, 0)])),
            ("a.txt".to_owned(), Take::Hunks(vec![(1, 1)])),
        ],
        branch: "odd-best".to_owned(),
    };
    assemble(repo, &plan)?;
    let tree = git_ok(repo, &["ls-tree", "-r", "odd-best"]);
    assert!(tree.contains("\t:memo\n"), "{tree}");
    assert!(tree.contains("\tmoved.txt\n"), "{tree}");
    assert!(
        !tree.contains("\tc.txt\n"),
        "deleted, as every hunk of its deletion was taken: {tree}"
    );
    assert!(
        tree.contains("100755 blob") && tree.contains("\trun.sh\n"),
        "{tree}"
    );
    assert_eq!(
        git_ok(repo, &["show", "odd-best:a.txt"]),
        "1\n2\n3\n4\nFIVE\n"
    );

    // An attempt from a newer commit: not the same base, and that attempt is named.
    git_ok(
        repo,
        &["commit", "-q", "--allow-empty", "-m", "upstream moved"],
    );
    git_ok(repo, &["branch", "late"]);
    let three = vec![
        attempts(&fx)[0].clone(),
        attempts(&fx)[1].clone(),
        Attempt {
            name: "late".to_owned(),
            rev: "late".to_owned(),
            worktree: None,
        },
    ];
    let cmp = compare(repo, &three)?;
    assert!(!cmp.same_base);
    assert!(cmp.attempts.iter().any(|a| a.forked_elsewhere));
    Ok(())
}
