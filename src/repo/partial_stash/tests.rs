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
include!("tests/selected_and_staged.rs");
include!("tests/snapshots.rs");
include!("tests/arguments_and_sparse.rs");
include!("tests/mutation_and_aliases.rs");
include!("tests/excluded_cleanup.rs");
include!("tests/selection_revalidation.rs");
