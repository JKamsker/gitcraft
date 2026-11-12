//! Carrying the files in a checkout's way over to what is checked out, with what they hold
//! here. Ignored ones are saved as blobs, the checkout replaces them, and they are written
//! back, as changes there. A changed file the target doesn't have is only taken out of the
//! index, so the checkout leaves it be (an ignored file again, say); other local changes are
//! merged along (`--merge`). Untracked files git status shows are never deleted for this:
//! with them in the way, carrying isn't offered.

use std::path::Path;

use super::{BlockedCheckout, RepoTab};
use crate::git;
use crate::repo::Refresh;
use crate::repo::diff::literal_path;

/// An ignored file saved before the checkout, to write back after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) struct Saved {
    pub(in crate::repo) path: String,
    pub(in crate::repo) blob: String,
    /// The target's mode for it (executable or not); `100644` when it has none.
    pub(in crate::repo) mode: String,
}

/// What carrying needs found out first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::repo) struct Carried {
    pub(in crate::repo) saved: Vec<Saved>,
    /// Changed files the target doesn't have: out of the index, they stay as they are.
    pub(in crate::repo) untrack: Vec<String>,
}

/// What `checkout` checks out, from its step: the first name that is no option (nor the
/// new branch of `-b`), before `--`.
fn target(step: &[String]) -> Option<&str> {
    let mut args = step.iter().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => return None,
            "-b" | "-B" => {
                args.next();
            }
            a if a.starts_with('-') => {}
            a => return Some(a),
        }
    }
    None
}

pub(super) fn checkout_at(ops: &[Vec<String>]) -> Option<usize> {
    ops.iter()
        .position(|op| op.first().is_some_and(|c| c == "checkout"))
}

/// The target's entries for `files`: (path, mode), from `ls-tree -z`.
fn modes(path: &Path, target: &str, files: &[&String]) -> Result<Vec<(String, String)>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["ls-tree", "-z", target, "--"];
    args.extend(files.iter().map(|f| f.as_str()));
    let listed = git::run(Some(path), &args)?;
    Ok(listed
        .split('\0')
        .filter_map(|entry| {
            let (meta, file) = entry.split_once('\t')?;
            Some((file.to_owned(), meta.split(' ').next()?.to_owned()))
        })
        .collect())
}

/// Saves the ignored files in the way in the repository's object store, and finds the changed
/// ones the target doesn't have. A symbolic link is refused: its blob would be what it points
/// to, which can be anywhere.
pub(super) fn save(path: &Path, b: &BlockedCheckout) -> Result<Carried, String> {
    let target = checkout_at(&b.ops).and_then(|i| target(&b.ops[i]));
    let all: Vec<&String> = b
        .blockers
        .ignored
        .iter()
        .chain(&b.blockers.changed)
        .collect();
    let there = match target {
        Some(target) => modes(path, target, &all)?,
        None => Vec::new(),
    };
    let mode_of = |file: &str| {
        there
            .iter()
            .find(|(f, _)| f == file)
            .map(|(_, m)| m.as_str())
    };
    let mut carried = Carried::default();
    for file in &b.blockers.ignored {
        if crate::remote::try_is_link(&path.join(file)).map_err(|e| e.to_string())? {
            return Err(crate::i18n::message(
                "repo-checkout-blocked-link",
                &[("path", file.as_str().into())],
            ));
        }
        let blob = git::run(Some(path), &["hash-object", "-w", "--", file])?;
        let mode = match mode_of(file) {
            Some("100755") => "100755",
            _ => "100644",
        };
        carried.saved.push(Saved {
            path: file.clone(),
            blob: blob.trim().to_owned(),
            mode: mode.to_owned(),
        });
    }
    if target.is_some() {
        carried.untrack = (b.blockers.changed.iter())
            .filter(|file| mode_of(file).is_none())
            .cloned()
            .collect();
    }
    Ok(carried)
}

impl BlockedCheckout {
    /// The checkout's steps, carrying the files over: changed ones the target lacks taken out
    /// of the index before it; the checkout let replace ignored files and merge other changes;
    /// right after it, the saved files written back.
    pub(in crate::repo) fn carry(&self, carried: &Carried) -> Vec<Vec<String>> {
        let mut ops = self.ops.clone();
        let Some(at) = checkout_at(&ops) else {
            return ops;
        };
        let step = &mut ops[at];
        step.retain(|arg| arg != "--no-overwrite-ignore");
        if (self.blockers.changed.iter()).any(|f| !carried.untrack.contains(f)) {
            step.insert(1, "--merge".to_owned());
        }
        let mut back = Vec::new();
        if !carried.saved.is_empty() {
            let mut index = vec!["update-index".to_owned(), "--add".to_owned()];
            for s in &carried.saved {
                let info = format!("{},{},{}", s.mode, s.blob, s.path);
                index.extend(["--cacheinfo".to_owned(), info]);
            }
            back.push(index);
            let paths = carried.saved.iter().map(|s| s.path.clone());
            back.push(with_paths(&["checkout-index", "-f", "--"], paths.clone()));
            back.push(with_paths(
                &["reset", "-q", "--"],
                paths.map(|p| literal_path(&p)),
            ));
        }
        ops.splice(at + 1..at + 1, back);
        if !carried.untrack.is_empty() {
            let paths = carried.untrack.iter().map(|p| literal_path(p));
            ops.insert(at, with_paths(&["rm", "--cached", "-q", "--"], paths));
        }
        ops
    }
}

fn with_paths(args: &[&str], paths: impl Iterator<Item = String>) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).chain(paths).collect()
}

impl RepoTab {
    /// Saves the files in the way (in the background), then runs the checkout carrying them.
    pub(in crate::repo) fn carry_checkout(&mut self, b: BlockedCheckout, ctx: &egui::Context) {
        let path = self.path.clone();
        let blocked = b.clone();
        self.then(
            ctx,
            move || save(&path, &blocked),
            move |tab, saved, ctx| match saved {
                Ok(saved) => {
                    tab.run_undoable(&b.label, b.carry(&saved), Refresh::All, b.undo.clone(), ctx)
                }
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        crate::i18n::message(
                            "repo-checkout-blocked-save-failed",
                            &[("label", b.label.as_str().into()), ("error", e.into())],
                        ),
                        true,
                    )))
                }
            },
        );
    }
}

#[cfg(test)]
#[path = "carry_tests.rs"]
mod tests;
