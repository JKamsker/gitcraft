//! Stashing picked files, only what is staged, and snapshots, on real repositories, through
//! what the tab runs: the arguments, the background job, then undo and redo as the history
//! keeps them.

#![allow(clippy::expect_used)] // Invalid synthetic push fixtures must fail their regression tests.

use std::path::{Path, PathBuf};

use super::*;
use crate::repo::UndoEntry;
use crate::repo::operations::run_steps;

/// A repository in a fresh folder, removed when dropped, with a commit of files of every kind:
/// names with spaces and accents, a binary file, a file of several lines to stage in part.
struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The files with their bytes, the staged diff and the unstaged one.
type State = (Vec<(String, Vec<u8>)>, String, String);

const PART: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-partial-stash-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        repo.write("a.txt", b"a\n")?;
        repo.write("sp ace.txt", b"space\n")?;
        repo.write("\u{fc}n\u{ef}c\u{f6}d\u{e9} \u{6587}.txt", b"unicode\n")?;
        repo.write("old.txt", b"old\n")?;
        repo.write("del.txt", b"del\n")?;
        repo.write("bin.dat", &[0, 1, 2, 3, 255, 0, 7])?;
        repo.write("part.txt", PART.as_bytes())?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "-m", "base"])?;
        Ok(repo)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.0), args).map_err(|e| format!("git {args:?}: {e}"))
    }

    fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(self.0.join(name), bytes).map_err(|e| format!("{name}: {e}"))
    }

    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap_or_default()
    }

    fn status(&self) -> Result<WorkStatus, String> {
        git::status(&self.0)
    }

    fn stashes(&self) -> usize {
        git::stashes(&self.0).map(|s| s.len()).unwrap_or(0)
    }

    /// Everything a user can see of the working tree and the index: every file's bytes
    /// (`.git` aside), what is staged and what isn't.
    fn state(&self) -> Result<State, String> {
        let mut files = Vec::new();
        walk(&self.0, &self.0, &mut files);
        files.sort();
        let staged = self.git(&["diff", "--cached", "--binary", "--full-index"])?;
        let unstaged = self.git(&["diff", "--binary", "--full-index"])?;
        Ok((files, staged, unstaged))
    }

    /// Changes `part.txt` at its start and its end, the start staged and the end not.
    fn stage_in_part(&self) -> Result<(), String> {
        self.write("part.txt", PART.replace("one", "ONE").as_bytes())?;
        self.git(&["add", "part.txt"])?;
        self.write(
            "part.txt",
            PART.replace("one", "ONE")
                .replace("seven", "SEVEN")
                .as_bytes(),
        )
    }

    fn stash(&self, mode: StashMode, untracked: bool) -> (Result<String, String>, OpKind) {
        let request = StashRequest {
            mode,
            message: "wip ü".to_owned(),
            untracked,
        };
        let head = git::head(&self.0);
        if request.mode == StashMode::Snapshot {
            return run_snapshot(&self.0, &request.message, untracked, head);
        }
        let staged = request.mode == StashMode::Staged;
        let work = match self.status() {
            Ok(work) => work,
            Err(e) => return (Err(e), OpKind::plain()),
        };
        match push_args(&request, &work) {
            Ok(args) => run_push(&self.0, args, head, staged),
            Err(e) => (Err(e), OpKind::plain()),
        }
    }

    /// Runs the undo (or the redo) of `entry` as the history does, returning the entry back.
    fn undo(&self, entry: UndoEntry) -> Result<UndoEntry, String> {
        let (result, kind) = run_steps(&self.0, &entry.undo.clone(), OpKind::Undo(entry), None);
        result.map_err(|e| format!("undo: {e}"))?;
        match kind {
            OpKind::Undo(e) => Ok(e),
            other => Err(format!("undo came back as {other:?}")),
        }
    }

    fn redo(&self, entry: UndoEntry) -> Result<UndoEntry, String> {
        let (result, kind) = run_steps(&self.0, &entry.redo.clone(), OpKind::Redo(entry), None);
        result.map_err(|e| format!("redo: {e}"))?;
        match kind {
            OpKind::Redo(e) => Ok(e),
            other => Err(format!("redo came back as {other:?}")),
        }
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push((
                rel.to_string_lossy().into_owned(),
                std::fs::read(&path).unwrap_or_default(),
            ));
        }
    }
}

/// The undo history's entry for an operation that ended as `kind`.
fn entry(kind: OpKind) -> Result<UndoEntry, String> {
    match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Ok(UndoEntry {
            id: 0,
            agent: None,
            label: "stash".to_owned(),
            undo,
            redo,
            heads: None,
        }),
        other => Err(format!("not undoable: {other:?}")),
    }
}

const UNICODE: &str = "\u{fc}n\u{ef}c\u{f6}d\u{e9} \u{6587}.txt";
const NEW: &str = "n\u{e9}w file \u{1f600}.txt";

#[test]
fn picked_files_are_stashed_and_the_others_stay() -> Result<(), String> {
    let repo = Repo::new("files")?;
    repo.write("a.txt", b"a changed, not picked\n")?;
    repo.write("sp ace.txt", b"space changed\n")?;
    repo.write(UNICODE, b"unicode changed\n")?;
    repo.write("bin.dat", &[9, 0, 9, 0, 255])?;
    let _ = std::fs::remove_file(repo.path().join("del.txt"));
    repo.write(NEW, b"new\n")?;
    repo.write("other new.txt", b"not picked\n")?;
    repo.stage_in_part()?;
    let before = repo.state()?;
    let picked: Vec<String> = ["sp ace.txt", UNICODE, "bin.dat", "del.txt", NEW, "part.txt"]
        .map(str::to_owned)
        .to_vec();
    // Picked twice (staged and unstaged rows of one file) is one path.
    let mut twice = picked.clone();
    twice.push("part.txt".to_owned());
    let (result, kind) = repo.stash(StashMode::Files(twice), false);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(repo.stashes(), 1);
    let left = repo.status()?;
    let mut changed: Vec<&str> = left.unstaged.iter().map(|f| f.path.as_str()).collect();
    changed.sort_unstable();
    assert_eq!(changed, ["a.txt", "other new.txt"]);
    assert!(left.staged.is_empty());
    assert_eq!(repo.read("part.txt"), PART.as_bytes());
    assert_eq!(repo.read("bin.dat"), [0, 1, 2, 3, 255, 0, 7]);
    let stashed = repo.git(&[
        "-c",
        "core.quotePath=false",
        "stash",
        "show",
        "--include-untracked",
        "--name-only",
        "stash@{0}",
    ])?;
    for p in &picked {
        assert!(stashed.lines().any(|l| l == p), "{p} not in {stashed}");
    }
    // Undone: every byte back, the staging of part.txt too; redone and undone again.
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.stashes(), 0);
    let redone = repo.redo(undone)?;
    assert_eq!(repo.stashes(), 1);
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn a_staged_rename_or_deletion_is_refused_before_anything_happens() -> Result<(), String> {
    let repo = Repo::new("files-refused")?;
    repo.git(&["mv", "old.txt", "new.txt"])?;
    repo.git(&["rm", "-q", "del.txt"])?;
    repo.write("a.txt", b"changed\n")?;
    let before = repo.state()?;
    let work = repo.status()?;
    let rename = files_problem(&["new.txt".to_owned()], &work).unwrap_or_default();
    assert!(rename.contains("rename"), "{rename}");
    let deletion = files_problem(&["del.txt".to_owned()], &work).unwrap_or_default();
    assert!(deletion.contains("deletion"), "{deletion}");
    for path in ["new.txt", "del.txt"] {
        let (result, _) = repo.stash(
            StashMode::Files(vec![path.to_owned(), "a.txt".to_owned()]),
            false,
        );
        assert!(result.is_err());
    }
    assert!(
        files_problem(&["a.txt".to_owned()], &work)
            .is_some_and(|p| p.contains("staged outside the selection"))
    );
    assert_eq!(repo.stashes(), 0);
    assert_eq!(repo.state()?, before);
    Ok(())
}

/// A rename made in the working tree (not staged) is a deletion and a new file: picked, both go.
#[test]
fn an_unstaged_rename_is_stashed_as_its_two_files() -> Result<(), String> {
    let repo = Repo::new("files-rename")?;
    std::fs::rename(
        repo.path().join("old.txt"),
        repo.path().join("re named.txt"),
    )
    .map_err(|e| e.to_string())?;
    let before = repo.state()?;
    let files = vec!["old.txt".to_owned(), "re named.txt".to_owned()];
    let (result, kind) = repo.stash(StashMode::Files(files), false);
    assert!(result.is_ok(), "{result:?}");
    assert!(repo.status()?.is_clean());
    assert_eq!(repo.read("old.txt"), b"old\n");
    repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn files_with_no_changes_left_stash_nothing() -> Result<(), String> {
    let repo = Repo::new("files-nothing")?;
    let picked = vec!["a.txt".to_owned()];
    // Picked while changed, back as it was by the time it runs.
    let work = {
        repo.write("a.txt", b"x\n")?;
        let w = repo.status()?;
        repo.write("a.txt", b"a\n")?;
        w
    };
    let request = StashRequest {
        mode: StashMode::Files(picked),
        message: String::new(),
        untracked: false,
    };
    let args = push_args(&request, &work).map_err(|e| e.to_string())?;
    let (result, kind) = run_push(repo.path(), args, git::head(repo.path()), false);
    assert_eq!(
        result.err().as_deref(),
        Some("there are no changes to stash")
    );
    assert!(matches!(kind, OpKind::Normal(Undo::None, _)));
    assert_eq!(repo.stashes(), 0);
    // Not changed at all: refused before git is asked.
    let (result, _) = repo.stash(StashMode::Files(vec!["a.txt".to_owned()]), false);
    assert!(result.is_err_and(|e| e.contains("no changes")));
    Ok(())
}

#[test]
fn only_what_is_staged_is_stashed_and_comes_back_on_undo() -> Result<(), String> {
    let repo = Repo::new("staged")?;
    if git::staged_stash_support(repo.path()) != Ok(Ok(())) {
        return Ok(());
    }
    repo.stage_in_part()?;
    repo.git(&["mv", "old.txt", "new name ü.txt"])?;
    repo.git(&["rm", "-q", "del.txt"])?;
    repo.write(NEW, b"staged new\n")?;
    repo.git(&["add", "--", NEW])?;
    repo.write("sp ace.txt", b"unstaged only\n")?;
    repo.write("untracked.txt", b"stays\n")?;
    let before = repo.state()?;
    let (result, kind) = repo.stash(StashMode::Staged, true);
    assert!(result.is_ok(), "{result:?}");
    let left = repo.status()?;
    assert!(left.staged.is_empty(), "{:?}", left.staged);
    // The unstaged changes stay: the end of part.txt, sp ace.txt, the untracked file.
    assert_eq!(
        repo.read("part.txt"),
        PART.replace("seven", "SEVEN").as_bytes()
    );
    assert_eq!(repo.read("sp ace.txt"), b"unstaged only\n");
    assert_eq!(repo.read("untracked.txt"), b"stays\n");
    assert_eq!(repo.read("old.txt"), b"old\n");
    assert_eq!(repo.read("del.txt"), b"del\n");
    // Undone with the unstaged changes still there: everything as it was, staging too.
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.stashes(), 0);
    let redone = repo.redo(undone)?;
    assert_eq!(repo.stashes(), 1);
    assert!(repo.status()?.staged.is_empty());
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn a_staged_binary_file_that_git_cant_take_out_leaves_no_stash() -> Result<(), String> {
    let repo = Repo::new("staged-binary")?;
    if git::staged_stash_support(repo.path()) != Ok(Ok(())) {
        return Ok(());
    }
    repo.write("bin.dat", &[5, 0, 5, 0, 5])?;
    repo.git(&["add", "bin.dat"])?;
    repo.write("a.txt", b"unstaged\n")?;
    let before = repo.state()?;
    let (result, kind) = repo.stash(StashMode::Staged, false);
    match result {
        // Newer gits may manage it: then it is an ordinary stash.
        Ok(_) => {
            repo.undo(entry(kind)?)?;
            assert_eq!(repo.state()?, before);
        }
        Err(e) => {
            assert!(e.starts_with("nothing was stashed"), "{e}");
            assert_eq!(repo.stashes(), 0);
            assert_eq!(repo.state()?, before);
        }
    }
    Ok(())
}

#[test]
fn nothing_staged_is_refused() -> Result<(), String> {
    let repo = Repo::new("staged-nothing")?;
    repo.write("a.txt", b"unstaged\n")?;
    let (result, _) = repo.stash(StashMode::Staged, false);
    assert_eq!(result.err().as_deref(), Some("nothing is staged"));
    // Unstaged since it was looked at: git says so, and it is said plainly.
    let work = {
        repo.git(&["add", "a.txt"])?;
        let w = repo.status()?;
        repo.git(&["reset", "-q"])?;
        w
    };
    let request = StashRequest {
        mode: StashMode::Staged,
        message: String::new(),
        untracked: false,
    };
    if git::staged_stash_support(repo.path()) == Ok(Ok(())) {
        let args = push_args(&request, &work).map_err(|e| e.to_string())?;
        let (result, _) = run_push(repo.path(), args, git::head(repo.path()), true);
        assert_eq!(result.err().as_deref(), Some("nothing is staged"));
    }
    assert_eq!(repo.stashes(), 0);
    Ok(())
}

#[test]
fn a_snapshot_leaves_every_byte_where_it_was() -> Result<(), String> {
    let repo = Repo::new("snapshot")?;
    repo.stage_in_part()?;
    repo.git(&["mv", "old.txt", "moved.txt"])?;
    let _ = std::fs::remove_file(repo.path().join("del.txt"));
    repo.write("bin.dat", &[1, 0, 1, 0])?;
    repo.write(UNICODE, b"changed\n")?;
    repo.write(NEW, b"untracked\n")?;
    std::fs::create_dir_all(repo.path().join("new dir/deeper")).map_err(|e| e.to_string())?;
    repo.write("new dir/deeper/x y.txt", b"deep\n")?;
    repo.write(".gitignore", b"*.log\n")?;
    repo.write("ignored.log", b"ignored\n")?;
    let before = repo.state()?;
    let index = std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?;
    let (result, kind) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(repo.state()?, before);
    assert_eq!(
        std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?,
        index,
        "the index file changed"
    );
    // No file of its own left behind in .git.
    let leftovers: Vec<String> = std::fs::read_dir(repo.path().join(".git"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("gitcraft-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    // In the stash list like any other, named as asked, on HEAD.
    let list = git::stashes(repo.path()).map_err(|e| e.to_string())?;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].message, "wip ü");
    assert_eq!(list[0].base, git::head(repo.path()));
    // Untracked files in its third parent, ignored ones not.
    let untracked = repo.git(&[
        "-c",
        "core.quotePath=false",
        "ls-tree",
        "-r",
        "--name-only",
        "stash@{0}^3",
    ])?;
    let mut untracked: Vec<&str> = untracked.lines().collect();
    untracked.sort_unstable();
    assert_eq!(untracked, [".gitignore", "new dir/deeper/x y.txt", NEW]);
    // Popped elsewhere, it is the very state it saved.
    let entry_ = entry(kind)?;
    repo.git(&["stash", "push", "-q", "--include-untracked"])?;
    repo.git(&["stash", "drop", "-q"])?;
    // (The one just pushed dropped: the snapshot is the newest again.)
    repo.git(&["stash", "apply", "-q", "--index"])?;
    assert_eq!(repo.state()?, before);
    // Undone: the stash goes, the files stay; redone: it comes back.
    let undone = repo.undo(entry_)?;
    assert_eq!(repo.stashes(), 0);
    assert_eq!(repo.state()?, before);
    let redone = repo.redo(undone)?;
    assert_eq!(repo.stashes(), 1);
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn a_snapshot_without_untracked_files_leaves_them_out() -> Result<(), String> {
    let repo = Repo::new("snapshot-tracked")?;
    repo.write("a.txt", b"changed\n")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, false);
    assert!(result.is_ok(), "{result:?}");
    let parents = repo.git(&["log", "-1", "--format=%P", "stash@{0}"])?;
    assert_eq!(parents.split_whitespace().count(), 2);
    assert_eq!(repo.read(NEW), b"untracked\n");
    Ok(())
}

#[test]
fn a_snapshot_of_only_untracked_files_is_made_on_head() -> Result<(), String> {
    let repo = Repo::new("snapshot-untracked")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, false);
    assert!(result.is_err_and(|e| e.contains("Include untracked")));
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let list = git::stashes(repo.path()).map_err(|e| e.to_string())?;
    assert_eq!(list.len(), 1);
    // Its tree is HEAD's, its index HEAD's: popping it brings back just the new file.
    let head_tree = repo.git(&["rev-parse", "HEAD^{tree}"])?;
    assert_eq!(repo.git(&["rev-parse", "stash@{0}^{tree}"])?, head_tree);
    assert_eq!(repo.git(&["rev-parse", "stash@{0}^2^{tree}"])?, head_tree);
    std::fs::remove_file(repo.path().join(NEW)).map_err(|e| e.to_string())?;
    repo.git(&["stash", "pop", "-q", "--index"])?;
    assert_eq!(repo.read(NEW), b"untracked\n");
    assert!(repo.status()?.staged.is_empty());
    Ok(())
}

#[test]
fn a_snapshot_of_nothing_says_so() -> Result<(), String> {
    let repo = Repo::new("snapshot-nothing")?;
    let (result, kind) = repo.stash(StashMode::Snapshot, true);
    assert_eq!(
        result.err().as_deref(),
        Some("there are no changes to save")
    );
    assert!(matches!(kind, OpKind::Normal(Undo::None, _)));
    assert_eq!(repo.stashes(), 0);
    Ok(())
}

#[test]
fn a_snapshot_needs_a_first_commit() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-snapshot-unborn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let repo = Repo(dir);
    repo.git(&["init", "-q"])?;
    repo.write("a.txt", b"a\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_err_and(|e| e.contains("first commit")));
    Ok(())
}

/// Hundreds of untracked files, more than one command line takes, all go in the snapshot.
#[test]
fn a_snapshot_takes_many_untracked_files() -> Result<(), String> {
    let repo = Repo::new("snapshot-many")?;
    std::fs::create_dir_all(repo.path().join("many")).map_err(|e| e.to_string())?;
    let long = "l".repeat(120);
    for i in 0..900 {
        repo.write(&format!("many/{long} {i}.txt"), format!("{i}\n").as_bytes())?;
    }
    let before = repo.state()?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let stored = repo.git(&["ls-tree", "-r", "--name-only", "stash@{0}^3"])?;
    assert_eq!(stored.lines().count(), 900);
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn everything_stashes_untracked_files_only_when_asked() -> Result<(), String> {
    let repo = Repo::new("everything")?;
    repo.write("a.txt", b"changed\n")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, kind) = repo.stash(StashMode::Everything, false);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(repo.read(NEW), b"untracked\n");
    assert_eq!(repo.read("a.txt"), b"a\n");
    repo.undo(entry(kind)?)?;
    let (result, _) = repo.stash(StashMode::Everything, true);
    assert!(result.is_ok(), "{result:?}");
    assert!(repo.status()?.is_clean());
    Ok(())
}

#[test]
fn the_arguments_are_what_git_stash_push_takes() -> Result<(), String> {
    let work = WorkStatus {
        unstaged: vec![
            git::StatusFile {
                path: "-n.txt".to_owned(),
                old_path: None,
                code: '?',
                area: git::Area::Untracked,
            },
            git::StatusFile {
                path: "[ab].txt".to_owned(),
                old_path: None,
                code: 'M',
                area: git::Area::Unstaged,
            },
        ],
        ..WorkStatus::default()
    };
    let request = |mode, message: &str, untracked| StashRequest {
        mode,
        message: message.to_owned(),
        untracked,
    };
    let files = StashMode::Files(vec!["[ab].txt".to_owned(), "-n.txt".to_owned()]);
    assert_eq!(
        push_args(&request(files, " -m ", false), &work).map_err(|e| e.to_string())?,
        [
            "stash",
            "push",
            "--include-untracked",
            "-m",
            "-m",
            "--",
            ":(literal)-n.txt",
            ":(literal)[ab].txt"
        ]
    );
    assert_eq!(
        push_args(&request(StashMode::Everything, "", true), &work).map_err(|e| e.to_string())?,
        ["stash", "push", "--include-untracked"]
    );
    assert_eq!(
        push_args(&request(StashMode::Everything, "", false), &work).map_err(|e| e.to_string())?,
        ["stash", "push"]
    );
    assert!(push_args(&request(StashMode::Staged, "", false), &work).is_err());
    assert!(
        push_args(
            &request(StashMode::Everything, "", true),
            &WorkStatus::default()
        )
        .is_err()
    );
    assert!(push_args(&request(StashMode::Files(Vec::new()), "", true), &work).is_err());
    Ok(())
}

#[test]
fn picks_are_forgotten_once_their_files_have_no_changes() -> Result<(), String> {
    let mut picks = StashPicks::default();
    picks.toggle("a");
    picks.toggle("b");
    picks.toggle("b");
    picks.toggle("c");
    let work = WorkStatus {
        staged: vec![git::StatusFile {
            path: "c".to_owned(),
            old_path: None,
            code: 'M',
            area: git::Area::Staged,
        }],
        ..WorkStatus::default()
    };
    picks.keep_changed(&work);
    assert_eq!(picks.picked.iter().collect::<Vec<_>>(), ["c"]);
    Ok(())
}

#[test]
fn too_many_picked_files_for_one_command_line_are_refused() {
    let paths: Vec<String> = (0..400)
        .map(|i| format!("{}/{i}.txt", "d".repeat(80)))
        .collect();
    let work = WorkStatus {
        unstaged: paths
            .iter()
            .map(|p| git::StatusFile {
                path: p.clone(),
                old_path: None,
                code: 'M',
                area: git::Area::Unstaged,
            })
            .collect(),
        ..WorkStatus::default()
    };
    let problem = files_problem(&paths, &work).unwrap_or_default();
    assert!(problem.contains("too many"), "{problem}");
    assert_eq!(files_problem(&paths[..50], &work), None);
}

/// A repository inside this one is left out of a snapshot, as `git stash -u` leaves it out.
#[test]
fn a_snapshot_leaves_out_a_nested_repository() -> Result<(), String> {
    let repo = Repo::new("snapshot-nested")?;
    let nested = repo.path().join("nested");
    std::fs::create_dir_all(&nested).map_err(|e| e.to_string())?;
    git::run(Some(&nested), &["init", "-q"])?;
    std::fs::write(nested.join("x.txt"), b"x\n").map_err(|e| e.to_string())?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let stored = repo.git(&[
        "-c",
        "core.quotePath=false",
        "ls-tree",
        "-r",
        "--name-only",
        "stash@{0}^3",
    ])?;
    assert_eq!(stored.trim(), NEW);
    Ok(())
}

/// Stashing everything without untracked files when only those changed is refused up front.
#[test]
fn everything_without_untracked_files_needs_tracked_changes() -> Result<(), String> {
    let repo = Repo::new("everything-untracked-only")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Everything, false);
    assert!(result.is_err_and(|e| e.contains("only untracked")));
    assert_eq!(repo.stashes(), 0);
    Ok(())
}

/// Undone, more staged elsewhere, redone: the stash now holds more than the undo's patch, so the
/// undo refuses instead of putting back part of it and dropping the rest.
#[test]
fn a_redone_staged_stash_holding_more_is_not_undone_by_the_old_patch() -> Result<(), String> {
    let repo = Repo::new("staged-redo-more")?;
    if git::staged_stash_support(repo.path()) != Ok(Ok(())) {
        return Ok(());
    }
    repo.write("a.txt", b"staged\n")?;
    repo.git(&["add", "a.txt"])?;
    let (result, kind) = repo.stash(StashMode::Staged, false);
    assert!(result.is_ok(), "{result:?}");
    let undone = repo.undo(entry(kind)?)?;
    // In the terminal meanwhile: something else staged too.
    repo.write("sp ace.txt", b"also staged\n")?;
    repo.git(&["add", "sp ace.txt"])?;
    let redone = repo.redo(undone)?;
    let (result, _) = run_steps(
        repo.path(),
        &redone.undo,
        OpKind::Undo(redone.clone()),
        None,
    );
    assert!(result.is_err(), "the old patch was applied");
    // Nothing lost: the stash holds both, and pops back whole.
    assert_eq!(repo.stashes(), 1);
    repo.git(&["stash", "pop", "-q", "--index"])?;
    assert_eq!(repo.read("sp ace.txt"), b"also staged\n");
    assert_eq!(repo.read("a.txt"), b"staged\n");
    Ok(())
}

/// In a sparse checkout, an untracked file outside the cone goes in the snapshot, as `git stash
/// --include-untracked` takes it.
#[test]
fn a_snapshot_takes_untracked_files_outside_a_sparse_cone() -> Result<(), String> {
    let repo = Repo::new("snapshot-sparse")?;
    std::fs::create_dir_all(repo.path().join("in")).map_err(|e| e.to_string())?;
    repo.write("in/kept.txt", b"in\n")?;
    repo.git(&["add", "-A"])?;
    repo.git(&["commit", "-q", "-m", "in"])?;
    repo.git(&["sparse-checkout", "set", "--cone", "in"])?;
    std::fs::create_dir_all(repo.path().join("out")).map_err(|e| e.to_string())?;
    repo.write("out/new.txt", b"out\n")?;
    repo.write("in/kept.txt", b"changed\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let stored = repo.git(&["ls-tree", "-r", "--name-only", "stash@{0}^3"])?;
    assert_eq!(stored.trim(), "out/new.txt");
    Ok(())
}

#[test]
fn stash_job_waits_in_the_shared_worktree_mutation_lane() -> Result<(), String> {
    use std::sync::mpsc::channel;
    use std::time::Duration;
    let repo = Repo::new("mutation-lane")?;
    let peer = tempfile::tempdir().map_err(|e| e.to_string())?;
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "lane-peer",
        peer.path().to_str().ok_or("peer path")?,
    ])?;
    let ctx = egui::Context::default();
    let blocker = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    let mut tab = RepoTab::new_deferred(peer.path().to_owned(), &ctx);
    assert_eq!(
        crate::tasks::mutation_schedule_key(blocker.tx.repo.clone()),
        crate::tasks::mutation_schedule_key(tab.tx.repo.clone())
    );
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel::<()>();
    crate::repo::messages::spawn_mutation(
        &blocker.tx,
        &ctx,
        |error| done("lane blocker", Err(error), OpKind::plain()),
        move || {
            let _ = started_tx.send(());
            let _ = release_rx.recv();
            done("lane blocker", Ok(String::new()), OpKind::plain())
        },
    );
    started_rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    let (ran_tx, ran_rx) = channel();
    tab.stash_job("recorded stash lane", &ctx, move |_| {
        let _ = ran_tx.send(());
        (Ok("recorded stash finished".into()), OpKind::plain())
    });
    assert!(
        crate::tasks::pool()?.queued_priority_for(&tab.tx.cancel)
            == Some(crate::tasks::Priority::Background)
    );
    assert!(ran_rx.try_recv().is_err());
    drop(release_tx);
    ran_rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    let msg = tab
        .rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    assert!(
        matches!(&msg, Msg::OpDone { label, result: Ok(_), .. } if label == "recorded stash lane")
    );
    tab.handle(msg, &ctx);
    assert!(tab.busy_op.is_none());
    Ok(())
}

#[test]
fn filesystem_alias_guard_uses_repository_entries_and_preserves_distinct_case() -> Result<(), String>
{
    let repo = Repo::new("filesystem-alias-guard")?;
    // Two names for one owned file model filesystem aliases without mounts or a
    // destructive stash. -ef also handles true case aliases on the target filesystem.
    repo.write("A", b"excluded bytes\n")?;
    std::fs::hard_link(repo.path().join("A"), repo.path().join("alias"))
        .map_err(|e| e.to_string())?;
    let work = WorkStatus {
        staged: vec![git::StatusFile {
            path: "A".into(),
            old_path: None,
            code: 'D',
            area: git::Area::Staged,
        }],
        unstaged: vec![git::StatusFile {
            path: "alias".into(),
            old_path: None,
            code: '?',
            area: git::Area::Untracked,
        }],
        ..Default::default()
    };
    assert!(check_filesystem_overlaps(repo.path(), &work, false, &[]).is_err());
    assert!(check_filesystem_overlaps(repo.path(), &work, true, &[]).is_ok());
    assert!(check_filesystem_overlaps(repo.path(), &work, true, &["alias"]).is_err());
    std::fs::create_dir(repo.path().join("directory")).map_err(|e| e.to_string())?;
    let mut nested = work.clone();
    nested.staged[0].path = "directory/removed".into();
    assert!(check_filesystem_overlaps(repo.path(), &nested, false, &["directory"]).is_err());
    assert!(check_filesystem_overlaps(repo.path(), &work, false, &["unrelated"]).is_err()); // ordinary alias still excluded
    let mut distinct = work.clone();
    distinct.unstaged.clear();
    repo.write("a", b"other bytes\n")?;
    let case_alias = same_file::is_same_file(repo.path().join("A"), repo.path().join("a"))
        .map_err(|e| e.to_string())?;
    assert_eq!(
        check_filesystem_overlaps(repo.path(), &distinct, false, &["a"]).is_err(),
        case_alias
    );
    // Paths remain original quoted data, including shell metacharacters.
    distinct.staged[0].path = "quote'$(never-run)".into();
    let script = filesystem_overlap_script(&distinct, true, &["quote'$(never-run)/child"])?;
    assert!(script.contains("'\\''"));
    assert!(script.contains("./quote"));
    assert!(check_filesystem_overlaps(repo.path(), &distinct, true, &["unrelated"]).is_ok());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unresolved_excluded_symlinks_refuse_normal_push_and_redo() -> Result<(), String> {
    for (ignored, untracked) in [(false, false), (true, false), (true, true)] {
        let repo = Repo::new(&format!("dangling-alias-{ignored}-{untracked}"))?;
        repo.write("A", b"tracked contents\n")?;
        repo.git(&["add", "A"])?;
        repo.git(&["commit", "-qm", "tracked alias candidate"])?;
        repo.write("a.txt", b"eligible queued change\n")?;
        let request = StashRequest {
            mode: StashMode::Everything,
            message: String::new(),
            untracked,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(StashMode::Everything, untracked);
        result?;
        let undone = repo.undo(entry(kind)?)?;
        repo.git(&["rm", "A"])?;
        std::os::unix::fs::symlink("missing-target", repo.path().join("a"))
            .map_err(|e| e.to_string())?;
        if ignored {
            std::fs::write(repo.path().join(".git/info/exclude"), "/a\n")
                .map_err(|e| e.to_string())?;
        }
        // On a case-insensitive target A and a can be one dangling directory entry.
        // Refuse unresolved links conservatively even on this case-sensitive fixture.
        let before = repo.state()?;
        let head = git::head(repo.path());
        let stashes = repo.git(&["stash", "list"])?;
        let (result, _) = run_push(repo.path(), stale_args, head.clone(), false);
        assert!(result.is_err_and(|e| e.contains("unresolved symbolic link")));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo,
            OpKind::Redo(undone.clone()),
            None,
            false,
            None,
        );
        assert!(result.is_err_and(|e| e.contains("unresolved symbolic link")));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(git::head(repo.path()), head);
        assert_eq!(repo.git(&["stash", "list"])?, stashes);
        assert_eq!(
            std::fs::read_link(repo.path().join("a")).map_err(|e| e.to_string())?,
            PathBuf::from("missing-target")
        );
        assert!(everything_problem(&repo.status()?, untracked).is_none());
    }
    Ok(())
}

#[test]
fn unstaged_file_replaced_by_ignored_directory_refuses_push_and_redo() -> Result<(), String> {
    for untracked in [false, true] {
        let repo = Repo::new(if untracked {
            "unstaged-ignored-with-u"
        } else {
            "unstaged-ignored-without-u"
        })?;
        repo.write(".gitignore", b"a.txt/local\n")?;
        repo.git(&["add", ".gitignore"])?;
        repo.git(&["commit", "-q", "-m", "ignore replacement"])?;
        repo.write("a.txt", b"changed\n")?;
        let request = StashRequest {
            mode: StashMode::Everything,
            message: String::new(),
            untracked,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(StashMode::Everything, untracked);
        result?;
        let undone = repo.undo(entry(kind)?)?;
        std::fs::remove_file(repo.path().join("a.txt")).map_err(|e| e.to_string())?;
        std::fs::create_dir(repo.path().join("a.txt")).map_err(|e| e.to_string())?;
        repo.write("a.txt/local", b"excluded replacement bytes\n")?;
        let work = repo.status()?;
        assert!(work.staged.is_empty());
        assert!(
            work.unstaged
                .iter()
                .any(|f| f.path == "a.txt" && f.code == 'D')
        );
        assert!(!work.unstaged.iter().any(|f| f.path == "a.txt/local"));
        let before = repo.state()?;
        let head = git::head(repo.path());
        let stash_list = repo.git(&["stash", "list"])?;
        let (result, _) = run_push(repo.path(), stale_args, head.clone(), false);
        assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo,
            OpKind::Redo(undone.clone()),
            None,
            false,
            None,
        );
        assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(git::head(repo.path()), head);
        assert_eq!(repo.git(&["stash", "list"])?, stash_list);
        assert!(everything_problem(&work, untracked).is_none());
        let (result, _) = run_snapshot(repo.path(), "safe snapshot", untracked, head);
        result?;
        assert_eq!(repo.state()?, before);
    }
    Ok(())
}

#[test]
fn excluded_overlap_validation_handles_ancestry_and_message_options() {
    for (deleted, untracked, blocked) in [
        ("f", "f", true),
        ("f", "f/child", true),
        ("dir/f", "dir", true),
        ("dir/f", "dir/", true),
        ("dir/f", "dir///", true),
        ("dir/", "dir/f", true),
        ("dir/", "dir///", true),
        ("f", "foo", false),
        ("dir/", "directory/f", false),
        ("dir/f", "dir/other", false),
    ] {
        let work = WorkStatus {
            staged: vec![git::StatusFile {
                path: deleted.into(),
                old_path: None,
                code: 'D',
                area: git::Area::Staged,
            }],
            unstaged: vec![git::StatusFile {
                path: untracked.into(),
                old_path: None,
                code: '?',
                area: git::Area::Untracked,
            }],
            ..WorkStatus::default()
        };
        assert_eq!(push_everything_problem(&work, false).is_some(), blocked);
        assert!(push_everything_problem(&work, true).is_none());
        assert!(everything_problem(&work, false).is_none()); // Snapshot remains eligible.
        let request = StashRequest {
            mode: StashMode::Everything,
            message: "--include-untracked".into(),
            untracked: false,
        };
        assert_eq!(push_args(&request, &work).is_err(), blocked);
    }
    for message in ["--include-untracked", "--", "--staged"] {
        let args = ops(&[&["stash", "push", "-m", message]]).remove(0);
        let request = push_request(&args).expect("stash push");
        assert_eq!(request.mode, StashMode::Everything);
        assert!(!request.untracked);
    }
}

#[test]
fn fresh_push_and_redo_refuse_recreated_deleted_file() -> Result<(), String> {
    let repo = Repo::new("excluded-overlap-fresh")?;
    repo.write("a.txt", b"changed\n")?;
    let request = StashRequest {
        mode: StashMode::Everything,
        message: String::new(),
        untracked: false,
    };
    let stale_args = push_args(&request, &repo.status()?)?;
    let (result, kind) = repo.stash(StashMode::Everything, false);
    result?;
    let undone = repo.undo(entry(kind)?)?;
    repo.git(&["rm", "-f", "a.txt"])?;
    repo.write("a.txt", b"excluded new bytes\n")?;
    let before = repo.state()?;
    let stash_before = repo.git(&["stash", "list"])?;
    let (result, _) = run_push(repo.path(), stale_args, git::head(repo.path()), false);
    assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
    let (result, _, stopped) = run_steps_counted(
        repo.path(),
        &undone.redo.clone(),
        OpKind::Redo(undone),
        None,
        false,
        None,
    );
    assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
    assert_eq!(stopped, 0);
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.git(&["stash", "list"])?, stash_before);
    assert!(push_args(&request, &repo.status()?).is_err());
    // Saving a snapshot does not run destructive stash cleanup.
    let (result, _) = run_snapshot(repo.path(), "safe snapshot", false, git::head(repo.path()));
    result?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn rename_source_validation_preserves_git_path_spelling() {
    for (source, other, blocked) in [
        ("src", "src/child", true),
        ("src/child", "src", true),
        ("src/child", "src/", true),
        ("src/", "src/child", true),
        ("A", "a", false),
        ("dir", "directory/f", false),
        ("a\\b", "a/b", false),
        (" f ", "f", false),
    ] {
        let work = WorkStatus {
            staged: vec![git::StatusFile {
                path: "destination".into(),
                old_path: Some(source.into()),
                code: 'R',
                area: git::Area::Staged,
            }],
            unstaged: vec![git::StatusFile {
                path: other.into(),
                old_path: None,
                code: '?',
                area: git::Area::Untracked,
            }],
            ..WorkStatus::default()
        };
        assert_eq!(excluded_overlap_problem(&work, false).is_some(), blocked);
        assert!(everything_problem(&work, false).is_none());
    }
}

#[test]
fn embedded_repository_replacements_refuse_fresh_push_and_redo() -> Result<(), String> {
    for ignored in [false, true] {
        let repo = Repo::new(if ignored {
            "ignored-embedded-overlap"
        } else {
            "embedded-overlap"
        })?;
        std::fs::create_dir(repo.path().join("dir")).map_err(|e| e.to_string())?;
        repo.write("dir/file", b"tracked contents\n")?;
        repo.git(&["add", "dir/file"])?;
        repo.git(&["commit", "-qm", "tracked directory"])?;
        repo.write("a.txt", b"eligible queued change\n")?;
        let request = StashRequest {
            mode: StashMode::Everything,
            message: String::new(),
            untracked: false,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(StashMode::Everything, false);
        result?;
        let undone = repo.undo(entry(kind)?)?;

        repo.git(&["rm", "-r", "dir"])?;
        std::fs::create_dir(repo.path().join("dir")).map_err(|e| e.to_string())?;
        git::run(Some(&repo.path().join("dir")), &["init", "-q"])?;
        repo.write("dir/file", b"replacement must survive\n")?;
        if ignored {
            std::fs::write(repo.path().join(".git/info/exclude"), "/dir/\n")
                .map_err(|e| e.to_string())?;
            let excluded = repo.git(&[
                "ls-files",
                "--others",
                "--ignored",
                "--exclude-standard",
                "-z",
            ])?;
            assert!(excluded.split('\0').any(|p| p == "dir/"));
        }
        let work = repo.status()?;
        assert_eq!(
            work.unstaged
                .iter()
                .any(|row| row.area == git::Area::Untracked && row.path == "dir/"),
            !ignored,
        );
        if !ignored {
            assert!(push_args(&request, &work).is_err());
        }
        assert!(everything_problem(&work, false).is_none());
        let before = repo.state()?;
        let stashes = repo.git(&["stash", "list"])?;
        let head = git::head(repo.path());
        assert!(validate_push(repo.path(), &stale_args).is_err());
        let (result, _) = run_push(repo.path(), stale_args, head.clone(), false);
        assert!(result.is_err_and(|error| error.contains("dir/")));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo.clone(),
            OpKind::Redo(undone),
            None,
            false,
            None,
        );
        assert!(result.is_err_and(|error| error.contains("dir/")));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(repo.git(&["stash", "list"])?, stashes);
        assert_eq!(git::head(repo.path()), head);
    }
    Ok(())
}

#[test]
fn ignored_replacements_refuse_shared_agent_stash_execution() -> Result<(), String> {
    for case in ["file", "file-to-dir", "dir-to-file", "rename"] {
        let repo = Repo::new(&format!("ignored-overlap-{case}"))?;
        let replacement = match case {
            "dir-to-file" => {
                std::fs::create_dir(repo.path().join("folder")).map_err(|e| e.to_string())?;
                repo.write("folder/child", b"tracked\n")?;
                repo.git(&["add", "folder"])?;
                repo.git(&["commit", "-qm", "directory"])?;
                repo.git(&["rm", "-r", "folder"])?;
                repo.write("folder", b"excluded file\n")?;
                "folder"
            }
            "rename" => {
                repo.git(&["mv", "old.txt", "renamed.txt"])?;
                repo.write("old.txt", b"excluded rename source\n")?;
                "old.txt"
            }
            _ => {
                repo.git(&["rm", "del.txt"])?;
                if case == "file-to-dir" {
                    std::fs::create_dir(repo.path().join("del.txt")).map_err(|e| e.to_string())?;
                    repo.write("del.txt/child", b"excluded child\n")?;
                } else {
                    repo.write("del.txt", b"excluded replacement\n")?;
                }
                "del.txt"
            }
        };
        std::fs::write(
            repo.path().join(".git/info/exclude"),
            format!("/{replacement}\n"),
        )
        .map_err(|e| e.to_string())?;
        assert!(
            !repo
                .status()?
                .unstaged
                .iter()
                .any(|f| f.area == git::Area::Untracked)
        );
        let before = repo.state()?;
        let stash_before = repo.git(&["stash", "list"])?;
        // The agent plan uses these shared arguments and the shared execution runner.
        let args = ops(&[&stash_push_args()]);
        let (result, _, stopped) =
            run_steps_counted(repo.path(), &args, OpKind::plain(), None, false, None);
        assert!(
            result.is_err_and(|e| e.contains("overlaps a deletion")),
            "{case}"
        );
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(repo.git(&["stash", "list"])?, stash_before);
        let (result, _) = run_snapshot(repo.path(), "snapshot", false, git::head(repo.path()));
        result?;
        assert_eq!(repo.state()?, before);
    }
    Ok(())
}

#[test]
fn files_validation_refuses_outside_staged_but_allows_unstaged() {
    let file = |path: &str, area| git::StatusFile {
        path: path.into(),
        old_path: None,
        code: 'M',
        area,
    };
    let mut work = WorkStatus {
        staged: vec![file("a", git::Area::Staged), file("b", git::Area::Staged)],
        unstaged: vec![file("c", git::Area::Unstaged)],
        ..WorkStatus::default()
    };
    let selected = vec!["a".to_owned()];
    assert!(files_problem(&selected, &work).is_some_and(|e| e.contains("prevents Undo")));
    assert!(files_problem(&["a".into(), "b".into()], &work).is_none());
    work.staged.pop();
    work.unstaged.push(file("b", git::Area::Unstaged));
    assert!(files_problem(&selected, &work).is_none());
    let request = StashRequest {
        mode: StashMode::Files(selected),
        message: "--".into(),
        untracked: false,
    };
    let args = push_args(&request, &work).expect("unrelated unstaged files supported");
    assert_eq!(push_request(&args).expect("decode").mode, request.mode);
}

#[test]
fn fresh_files_push_and_redo_refuse_outside_staged_paths() -> Result<(), String> {
    let repo = Repo::new("files-outside-staged")?;
    repo.write("a.txt", b"staged a\n")?;
    repo.git(&["add", "a.txt"])?;
    repo.write("old.txt", b"unrelated unstaged\n")?;
    let request = StashRequest {
        mode: StashMode::Files(vec!["a.txt".into()]),
        message: String::new(),
        untracked: false,
    };
    let stale_args = push_args(&request, &repo.status()?)?;
    let before = repo.state()?;
    let (result, kind) = repo.stash(request.mode.clone(), false);
    result?;
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    // The same selection becomes unsupported after another file is staged.
    repo.write("sp ace.txt", b"staged b\n")?;
    repo.git(&["add", "sp ace.txt"])?;
    let before = repo.state()?;
    let stash_before = repo.git(&["stash", "list"])?;
    assert!(push_args(&request, &repo.status()?).is_err_and(|e| e.contains("prevents Undo")));
    let (result, _) = run_push(repo.path(), stale_args, git::head(repo.path()), false);
    assert!(result.is_err_and(|e| e.contains("prevents Undo")));
    let (result, _, stopped) = run_steps_counted(
        repo.path(),
        &undone.redo.clone(),
        OpKind::Redo(undone),
        None,
        false,
        None,
    );
    assert!(result.is_err_and(|e| e.contains("prevents Undo")));
    assert_eq!(stopped, 0);
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.git(&["stash", "list"])?, stash_before);
    // Selecting every staged path still round-trips the index and unrelated unstaged bytes.
    let (result, kind) = repo.stash(
        StashMode::Files(vec!["a.txt".into(), "sp ace.txt".into()]),
        false,
    );
    result?;
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    let redone = repo.redo(undone)?;
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn fresh_files_push_and_redo_revalidate_selected_deletions_and_renames() -> Result<(), String> {
    for renamed in [false, true] {
        let repo = Repo::new(if renamed {
            "fresh-selected-rename"
        } else {
            "fresh-selected-deletion"
        })?;
        let selected = if renamed { "picked.txt" } else { "a.txt" };
        repo.write(selected, b"selected modification\n")?;
        let request = StashRequest {
            mode: StashMode::Files(vec![selected.into()]),
            message: String::new(),
            untracked: renamed,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(request.mode.clone(), renamed);
        result?;
        let undone = repo.undo(entry(kind)?)?;
        if renamed {
            std::fs::remove_file(repo.path().join(selected)).map_err(|e| e.to_string())?;
            repo.git(&["mv", "old.txt", selected])?;
        } else {
            repo.git(&["rm", "-f", selected])?;
        }
        let work = repo.status()?;
        assert!(
            work.staged
                .iter()
                .any(|row| row.path == selected && row.code == if renamed { 'R' } else { 'D' })
        );
        let expected =
            files_problem(&[selected.into()], &work).ok_or("selected row must be refused")?;
        assert_eq!(
            validate_push(repo.path(), &stale_args),
            Err(expected.clone())
        );
        let before = repo.state()?;
        let stashes = repo.git(&["stash", "list"])?;
        let (result, _) = run_push(repo.path(), stale_args, git::head(repo.path()), false);
        assert_eq!(result, Err(expected.clone()));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo.clone(),
            OpKind::Redo(undone),
            None,
            false,
            None,
        );
        assert_eq!(result, Err(expected));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(repo.git(&["stash", "list"])?, stashes);
    }
    Ok(())
}

#[test]
fn newly_untracked_selection_requires_reprepare_before_push_and_redo() -> Result<(), String> {
    let repo = Repo::new("files-now-untracked")?;
    repo.write("new.txt", b"selected new bytes\n")?;
    repo.git(&["add", "new.txt"])?;
    let request = StashRequest {
        mode: StashMode::Files(vec!["new.txt".into()]),
        message: String::new(),
        untracked: false,
    };
    let args = push_args(&request, &repo.status()?)?;
    assert!(!args.iter().any(|a| a == "--include-untracked"));
    let (result, kind) = repo.stash(request.mode.clone(), false);
    result?;
    let undone = repo.undo(entry(kind)?)?;
    repo.git(&["reset", "HEAD", "--", "new.txt"])?;
    assert!(has_untracked(&["new.txt".into()], &repo.status()?));
    let before = repo.state()?;
    let bytes = std::fs::read(repo.path().join("new.txt")).map_err(|e| e.to_string())?;
    let stashes = repo.git(&["stash", "list"])?;
    assert!(validate_push(repo.path(), &args).is_err_and(|e| e.contains("Open Stash again")));
    let (result, _) = run_push(repo.path(), args, git::head(repo.path()), false);
    assert!(result.is_err_and(|e| e.contains("Open Stash again")));
    let (result, _, stopped) = run_steps_counted(
        repo.path(),
        &undone.redo.clone(),
        OpKind::Redo(undone),
        None,
        false,
        None,
    );
    assert!(result.is_err_and(|e| e.contains("Open Stash again")));
    assert_eq!(stopped, 0);
    assert_eq!(repo.state()?, before);
    assert_eq!(
        std::fs::read(repo.path().join("new.txt")).map_err(|e| e.to_string())?,
        bytes
    );
    assert_eq!(repo.git(&["stash", "list"])?, stashes);
    // Explicit preparation now includes -u for precisely the selected paths.
    assert!(
        push_args(&request, &repo.status()?)?
            .iter()
            .any(|a| a == "--include-untracked")
    );
    Ok(())
}

#[test]
fn shared_native_cleanup_guard_checks_excluded_replacements_and_failures() -> Result<(), String> {
    let removed = |path: &str, code, old: Option<&str>, area| git::StatusFile {
        path: path.into(),
        old_path: old.map(str::to_owned),
        code,
        area,
    };
    for row in [
        removed("private", 'D', None, git::Area::Staged),
        removed("renamed", 'R', Some("private"), git::Area::Staged),
        removed("private", 'D', None, git::Area::Unstaged),
    ] {
        for replacement in ["private", "private/content", "private/"] {
            let work = WorkStatus {
                staged: vec![row.clone()],
                ..Default::default()
            };
            let result = validate_excluded_cleanup_with(
                &work,
                false,
                || Ok(format!("{replacement}\0")),
                |_, _, _| panic!("lexical overlap reached filesystem inspection"),
            );
            assert!(result.is_err(), "{replacement}");
        }
    }
    let work = WorkStatus {
        staged: vec![removed("private", 'D', None, git::Area::Staged)],
        ..Default::default()
    };
    assert!(
        validate_excluded_cleanup_with(
            &work,
            false,
            || Err("inspection failed".into()),
            |_, _, _| Ok(())
        )
        .is_err()
    );
    assert!(
        validate_excluded_cleanup_with(
            &work,
            false,
            || Ok("PRIVATE/content\0".into()),
            |_, _, paths| {
                assert_eq!(paths, &["PRIVATE/content"]);
                Err("filesystem alias".into())
            }
        )
        .is_err()
    );
    validate_excluded_cleanup_with(
        &work,
        false,
        || Ok("private-other/content\0".into()),
        |seen, untracked, paths| {
            assert_eq!(seen.staged[0].path, "private");
            assert!(!untracked);
            assert_eq!(paths, &["private-other/content"]);
            Ok(())
        },
    )?;
    Ok(())
}
