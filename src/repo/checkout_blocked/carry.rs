//! Carrying the files in a checkout's way over to what is checked out, with what they hold
//! here. Untracked and ignored ones are saved as blobs, the checkout replaces them, and they
//! are written back, as changes there. A changed file the target doesn't have is only taken
//! out of the index, so the checkout leaves it be (an ignored file again, say); other local
//! changes are merged along (`--merge`).

use std::path::Path;

use super::{BlockedCheckout, RepoTab};
use crate::git;
use crate::repo::Refresh;

/// A file saved before the checkout: (its path, its blob).
pub(in crate::repo) type Saved = (String, String);

/// What carrying needs found out first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::repo) struct Carried {
    /// The untracked and ignored files in the way, saved.
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

fn checkout_at(ops: &[Vec<String>]) -> Option<usize> {
    ops.iter()
        .position(|op| op.first().is_some_and(|c| c == "checkout"))
}

/// Saves the untracked and ignored files in the way in the repository's object store, and
/// finds the changed ones the target doesn't have.
pub(super) fn save(path: &Path, b: &BlockedCheckout) -> Result<Carried, String> {
    let mut carried = Carried::default();
    for file in b.blockers.untracked.iter().chain(&b.blockers.ignored) {
        let blob = git::run(Some(path), &["hash-object", "-w", "--", file])?;
        carried.saved.push((file.clone(), blob.trim().to_owned()));
    }
    let target = checkout_at(&b.ops).and_then(|i| target(&b.ops[i]));
    if let Some(target) = target
        && !b.blockers.changed.is_empty()
    {
        let mut args = vec!["ls-tree", "-z", "--name-only", target, "--"];
        args.extend(b.blockers.changed.iter().map(String::as_str));
        let there = git::run(Some(path), &args)?;
        let there: Vec<&str> = there.split('\0').collect();
        carried.untrack = (b.blockers.changed.iter())
            .filter(|file| !there.contains(&file.as_str()))
            .cloned()
            .collect();
    }
    Ok(carried)
}

impl BlockedCheckout {
    /// The checkout's steps, carrying the files over: untracked ones in the way removed (they
    /// are saved) and changed ones the target lacks taken out of the index before it; the
    /// checkout let replace ignored files and merge other changes; right after it, the saved
    /// files written back.
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
        let paths: Vec<String> = carried.saved.iter().map(|(p, _)| p.clone()).collect();
        let mut back = Vec::new();
        if !paths.is_empty() {
            let mut index = vec!["update-index".to_owned(), "--add".to_owned()];
            for (file, blob) in &carried.saved {
                index.extend(["--cacheinfo".to_owned(), format!("100644,{blob},{file}")]);
            }
            back.push(index);
            back.push(with_paths(&["checkout-index", "-f", "--"], &paths));
            back.push(with_paths(&["reset", "-q", "--"], &paths));
        }
        ops.splice(at + 1..at + 1, back);
        if !carried.untrack.is_empty() {
            ops.insert(
                at,
                with_paths(&["rm", "--cached", "-q", "--"], &carried.untrack),
            );
        }
        if !self.blockers.untracked.is_empty() {
            ops.insert(
                at,
                with_paths(&["clean", "-f", "-q", "--"], &self.blockers.untracked),
            );
        }
        ops
    }
}

fn with_paths(args: &[&str], paths: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| (*a).to_owned())
        .chain(paths.iter().cloned())
        .collect()
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
mod tests {
    use super::*;
    use crate::repo::checkout_blocked::Blockers;
    use crate::repo::{Undo, ops};

    #[test]
    fn the_target_is_the_first_name_that_is_no_option_nor_a_new_branch() {
        let t = |args: &[&str]| target(&ops(&[args])[0]).map(str::to_owned);
        assert_eq!(
            t(&["checkout", "--no-overwrite-ignore", "dev", "--"]).as_deref(),
            Some("dev")
        );
        assert_eq!(
            t(&["checkout", "-b", "feat", "--track", "origin/feat", "--"]).as_deref(),
            Some("origin/feat")
        );
        assert_eq!(t(&["checkout", "--", "dev"]), None);
    }

    #[test]
    fn saved_files_are_written_back_right_after_the_checkout_and_untracked_ones_cleared_first() {
        let b = blocked(
            &[
                &["checkout", "--no-overwrite-ignore", "develop", "--"],
                &["pull", "--ff-only"],
            ],
            Blockers {
                changed: vec!["a.rs".into()],
                untracked: vec!["new.txt".into()],
                ignored: vec!["gen.json".into()],
            },
        );
        let carried = Carried {
            saved: vec![
                ("new.txt".to_owned(), "b1".to_owned()),
                ("gen.json".to_owned(), "b2".to_owned()),
            ],
            untrack: vec![],
        };
        assert_eq!(
            b.carry(&carried),
            ops(&[
                &["clean", "-f", "-q", "--", "new.txt"],
                &["checkout", "--merge", "develop", "--"],
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    "100644,b1,new.txt",
                    "--cacheinfo",
                    "100644,b2,gen.json"
                ],
                &["checkout-index", "-f", "--", "new.txt", "gen.json"],
                &["reset", "-q", "--", "new.txt", "gen.json"],
                &["pull", "--ff-only"],
            ])
        );
    }

    #[test]
    fn only_ignored_files_are_carried_by_a_plain_checkout() {
        let b = blocked(
            &[&["checkout", "--no-overwrite-ignore", "develop", "--"]],
            Blockers {
                ignored: vec!["gen.json".into()],
                ..Default::default()
            },
        );
        let carried = b.carry(&Carried {
            saved: vec![("gen.json".to_owned(), "b".to_owned())],
            untrack: vec![],
        });
        assert_eq!(carried[0], ["checkout", "develop", "--"]);
        assert_eq!(carried.len(), 4);
    }

    #[test]
    fn a_changed_file_the_target_lacks_leaves_the_index_and_is_not_merged() {
        // Back from a branch that tracks a file this one ignores: it stays, ignored again.
        let b = blocked(
            &[&["checkout", "--no-overwrite-ignore", "main", "--"]],
            Blockers {
                changed: vec!["gen.json".into()],
                ..Default::default()
            },
        );
        let carried = Carried {
            saved: vec![],
            untrack: vec!["gen.json".into()],
        };
        assert_eq!(
            b.carry(&carried),
            ops(&[
                &["rm", "--cached", "-q", "--", "gen.json"],
                &["checkout", "main", "--"],
            ])
        );
    }

    fn blocked(steps: &[&[&str]], blockers: Blockers) -> BlockedCheckout {
        BlockedCheckout {
            label: "Checkout develop".to_owned(),
            ops: ops(steps),
            undo: Undo::None,
            blockers,
        }
    }
}
