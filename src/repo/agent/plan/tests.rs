//! Each request planned in a real repository, run as the tab runs operations, and undone the
//! way Ctrl+Z undoes it.

use std::path::{Path, PathBuf};

use super::{Plan, plan};
use crate::git;
use crate::mcp::wire::{ResetMode, WriteOp};
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, Undo, UndoEntry};

/// A repository with one commit of `a.txt`, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-agent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgsign", "false"],
        // Files come back as written, whatever the system's line-ending conversion (Windows).
        &["config", "core.autocrlf", "false"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    write(&dir, "a.txt", "one\n");
    commit_all(&dir, "base");
    dir
}

fn write(dir: &Path, file: &str, text: &str) {
    let _ = std::fs::write(dir.join(file), text);
}

fn commit_all(dir: &Path, message: &str) {
    assert!(git::run(Some(dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(dir), &["commit", "-q", "-m", message]).is_ok());
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    git::run(Some(dir), args).unwrap_or_default()
}

/// Runs `plan` as the tab does, and returns the undo step it leaves (if any).
fn run(dir: &Path, plan: &Plan) -> Result<Option<UndoEntry>, String> {
    let kind = OpKind::Normal(plan.undo.clone(), git::head(dir));
    let (result, kind) = run_steps(dir, &plan.ops, kind, None);
    result?;
    Ok(match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            id: 0,
            label: plan.label.clone(),
            undo,
            redo,
            heads: None,
            agent: None,
        }),
        OpKind::Normal(Undo::HeadMove { mode }, heads) => {
            let mut parts = heads.split('\u{0}');
            let before = parts.next().unwrap_or_default().to_owned();
            Some(UndoEntry {
                id: 0,
                label: plan.label.clone(),
                undo: crate::repo::ops(&[&["reset", mode, &before]]),
                redo: Vec::new(),
                heads: None,
                agent: None,
            })
        }
        _ => None,
    })
}

fn undo(dir: &Path, entry: &UndoEntry) -> Result<String, String> {
    run_steps(dir, &entry.undo, OpKind::Undo(entry.clone()), None).0
}

/// The plan for `op`; one that fails at once (its error in the label) when it can't be made.
fn planned(dir: &Path, op: WriteOp) -> Plan {
    plan(dir, &op).unwrap_or_else(|e| Plan {
        label: format!("planning failed: {e}"),
        ops: vec![vec![format!("planning failed: {e}")]],
        refresh: crate::repo::Refresh::All,
        undo: Undo::None,
        ask: None,
    })
}
include!("tests/local_edits.rs");
include!("tests/history.rs");
include!("tests/remote_and_guards.rs");
