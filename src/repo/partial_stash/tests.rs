//! Stashing picked files, only what is staged, and snapshots, on real repositories, through
//! what the tab runs: the arguments, the background job, then undo and redo as the history
//! keeps them.

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
    assert_eq!(files_problem(&["a.txt".to_owned()], &work), None);
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
    assert_eq!(blocker.tx.repo, tab.tx.repo);
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
    let queued = crate::tasks::pool()?.queued_priorities_for(&tab.tx.repo);
    assert!(queued == vec![crate::tasks::Priority::Background]);
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
fn excluded_overlap_validation_handles_ancestry_and_message_options() {
    for (deleted, untracked, blocked) in [
        ("f", "f", true),
        ("f", "f/child", true),
        ("dir/f", "dir", true),
        ("f", "foo", false),
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
    assert!(result.is_err_and(|e| e.contains("overlaps a staged deletion")));
    let (result, _, stopped) = run_steps_counted(
        repo.path(),
        &undone.redo.clone(),
        OpKind::Redo(undone),
        None,
        false,
        None,
    );
    assert!(result.is_err_and(|e| e.contains("overlaps a staged deletion")));
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
