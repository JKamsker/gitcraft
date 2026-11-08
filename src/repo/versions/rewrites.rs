//! Keeping a branch's version before and after each rewrite gitcraft makes (whatever moves
//! HEAD so that its old commit is no longer part of it: a rebase, an amend, a quick edit, a
//! pull that rebases, a reset), and offering to compare the two: beside the message saying the
//! operation is done, and in the branch's menu.

use egui::Ui;

use super::VersionsAct;
use crate::git::{self, Branch};
use crate::repo::{Action, OpKind, RepoTab, Undo};

/// A rewrite that stopped on conflicts, to keep once it is continued to its end.
pub(super) struct Pending {
    label: String,
    before: String,
    onto: Option<String>,
}

impl RepoTab {
    /// After an operation that worked: if it moved HEAD so that its old commit is no longer
    /// part of the branch, keeps both versions (in the background), and offers to compare
    /// them beside the message saying it's done. A continued rebase that had stopped on
    /// conflicts is kept from where it started.
    pub(in crate::repo) fn note_rewrite(
        &mut self,
        label: &str,
        kind: &OpKind,
        ctx: &egui::Context,
    ) {
        if label.starts_with("Abort ") {
            self.versions.pending = None;
            return;
        }
        // The branch as the operation found it once done (`heads` says), not as it is when the
        // background job runs: it may have been checked out of since.
        let (op_label, before, after, branch, onto) = match kind {
            OpKind::Normal(Undo::HeadMove { .. }, heads) => {
                let mut parts = heads.split('\u{0}');
                let (before, after) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                (
                    label.to_owned(),
                    before.to_owned(),
                    Some(after.to_owned()),
                    Some(parts.next().unwrap_or("").to_owned()),
                    self.onto_of_last_op(label),
                )
            }
            OpKind::Normal(..) if label.starts_with("Continue ") => {
                let Some(pending) = self.versions.pending.take() else {
                    return;
                };
                (pending.label, pending.before, None, None, pending.onto)
            }
            _ => return,
        };
        let path = self.path.clone();
        let done = format!("{label}: done");
        let heads = after.as_ref().map(|after| (before.clone(), after.clone()));
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                let after = after.unwrap_or_else(|| git::head(&path));
                keep_rewrite(&path, &before, &after, branch, onto.as_deref())
            },
            move |tab, kept, ctx| tab.rewrite_kept(&op_label, &done, heads, kept, ctx),
        );
    }

    /// After an operation that failed: a rewrite stopped on conflicts is remembered, to keep
    /// once it is continued.
    pub(in crate::repo) fn note_stopped_rewrite(
        &mut self,
        label: &str,
        kind: &OpKind,
        error: &str,
    ) {
        let stopped = error.contains("CONFLICT") || error.contains("could not apply");
        if let OpKind::Normal(Undo::HeadMove { .. }, before) = kind
            && stopped
            && !before.is_empty()
        {
            self.versions.pending = Some(Pending {
                label: label.to_owned(),
                before: before.clone(),
                onto: self.onto_of_last_op(label),
            });
        }
    }

    /// Where the operation `label` (the last one started) rebased onto, if it did.
    fn onto_of_last_op(&self, label: &str) -> Option<String> {
        self.last_op
            .as_ref()
            .filter(|op| op.label == label)
            .and_then(|op| onto_of(&op.ops))
    }

    pub(in crate::repo) fn rewrite_kept(
        &mut self,
        label: &str,
        done: &str,
        heads: Option<(String, String)>,
        kept: Result<Option<(git::Rewrite, Vec<git::Rewrite>)>, String>,
        ctx: &egui::Context,
    ) {
        // Only this exact worker's successfully retained rewrite can satisfy DCO.
        let dco_kept = match (&heads, &kept) {
            (Some((before, after)), Ok(Some((rewrite, _)))) => {
                &rewrite.old.tip == before
                    && &rewrite.new.tip == after
                    && label == format!("Sign off the commits of {}", rewrite.branch)
            }
            _ => false,
        };
        let says_done = self
            .status
            .as_deref()
            .is_some_and(|(msg, err)| msg == done && !err);
        match kept {
            Ok(Some((rewrite, list))) => {
                self.versions.rewrites = list;
                self.versions.rewrites_read = true;
                if says_done {
                    self.versions.fresh = Some((done.to_owned(), rewrite));
                }
            }
            Ok(None) => {}
            // Done all the same; only the version from before couldn't be kept.
            Err(e) if says_done => {
                self.status = Some(crate::repo::Notice::plain((
                    format!("{label}: done, but its earlier version couldn't be kept: {e}"),
                    true,
                )));
            }
            Err(_) => {}
        }
        if let Some(heads) = heads {
            self.dco_rewrite_kept(label, &heads, dco_kept, ctx);
        }
    }

    /// After a read of the repository: the rewrites kept are read (and the old ones pruned)
    /// once, and a rewrite waiting for a continue that won't come (aborted in the terminal)
    /// is let go of.
    pub(in crate::repo) fn versions_after_load(&mut self, ctx: &egui::Context) {
        if self.work.in_progress.is_none() && self.busy_op.is_none() {
            self.versions.pending = None;
        }
        if std::mem::replace(&mut self.versions.rewrites_read, true) {
            return;
        }
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                // A tab that can't prune (a read-only repository) still lists what is there.
                let _ = git::prune_rewrites(&path, git::now());
                git::rewrites(&path)
            },
            |tab, list, _| match list {
                Ok(list) => tab.versions.rewrites = list,
                Err(_) => tab.versions.rewrites_read = false,
            },
        );
    }

    /// The newest rewrite kept of the local branch `name`.
    fn latest_rewrite(&self, name: &str) -> Option<&git::Rewrite> {
        self.versions.rewrites.iter().find(|r| r.branch == name)
    }

    /// "Compare before / after" beside the message saying the rewrite is done, while it says so.
    pub(in crate::repo) fn fresh_rewrite_button(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some((done, rewrite)) = &self.versions.fresh else {
            return;
        };
        let shown = self
            .status
            .as_deref()
            .is_some_and(|(msg, err)| msg == done && !err);
        if shown
            && ui
                .small_button(crate::i18n::text(
                    "repo-workflows-versions-fresh-rewrite-button-e6be1703",
                ))
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-versions-fresh-rewrite-button-fce3e9e3",
                ))
                .clicked()
        {
            acts.push(Action::Versions(VersionsAct::Rewrite(rewrite.clone())));
        }
    }

    /// The branch menu's comparisons of versions: before and after its last rewrite, and with
    /// its upstream when the two have diverged (after a rebase not pushed yet, a force push).
    pub(in crate::repo) fn versions_items(&self, ui: &mut Ui, acts: &mut Vec<Action>, b: &Branch) {
        let rewrite = self.latest_rewrite(&b.name);
        let (ahead, behind) = b.ahead_behind();
        let diverged = !b.upstream.is_empty() && ahead > 0 && behind > 0;
        if rewrite.is_none() && !diverged {
            return;
        }
        ui.separator();
        if let Some(rewrite) = rewrite {
            let when = git::relative_time(rewrite.time, git::now());
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-versions-versions-items-a2e93753",
                ))
                .on_hover_text(crate::i18n::message(
                    "repo-workflows-versions-versions-items-8e286a1d",
                    &[
                        ("b-name-1", (b.name).clone().into()),
                        ("when", (when).clone().into()),
                    ],
                ))
                .clicked()
            {
                acts.push(Action::Versions(VersionsAct::Rewrite(rewrite.clone())));
            }
        }
        if diverged
            && ui
                .button(crate::i18n::message(
                    "repo-workflows-versions-versions-items-a0142396",
                    &[("b-upstream-1", (b.upstream).clone().into())],
                ))
                .on_hover_text(crate::i18n::message(
                    "repo-workflows-versions-versions-items-28feb7ce",
                    &[
                        ("b-upstream-1", (b.upstream).clone().into()),
                        ("b-name-2", (b.name).clone().into()),
                    ],
                ))
                .clicked()
        {
            acts.push(Action::Versions(VersionsAct::Branches {
                old: crate::repo::branch_visibility::remote_ref(&b.upstream),
                old_label: b.upstream.clone(),
                new: crate::repo::branch_visibility::local_ref(&b.name),
                new_label: b.name.clone(),
            }));
        }
    }

    /// The palette's entry for the newest rewrite kept, if any.
    pub(in crate::repo) fn versions_palette_item(&self) -> Option<(String, String, Action)> {
        // The checked-out branch's, else the newest of any.
        let current = self.refs.current.as_deref().unwrap_or("HEAD");
        let rewrite = self
            .latest_rewrite(current)
            .or_else(|| self.versions.rewrites.first())?;
        Some((
            crate::i18n::message(
                "repo-workflows-versions-versions-palette-item-67f7dd27",
                &[("rewrite-branch-1", (rewrite.branch).clone().into())],
            ),
            "compare".into(),
            Action::Versions(VersionsAct::Rewrite(rewrite.clone())),
        ))
    }
}

/// Keeps the versions before and after, if HEAD's move from `before` to `after` rewrote the
/// branch, then prunes the old ones; gives the rewrite and every one kept.
fn keep_rewrite(
    path: &std::path::Path,
    before: &str,
    after: &str,
    branch: Option<String>,
    onto: Option<&str>,
) -> Result<Option<(git::Rewrite, Vec<git::Rewrite>)>, String> {
    let Some((old, new)) = git::rewritten(path, before, after, onto)? else {
        return Ok(None);
    };
    // Only for a continued rebase, which says nothing of it: the branch checked out now.
    let branch = branch.unwrap_or_else(|| {
        git::run_ok_codes(Some(path), &["symbolic-ref", "-q", "HEAD"], &[0, 1])
            .map(|s| s.trim().to_owned())
            .unwrap_or_default()
    });
    let now = git::now();
    let rewrite = git::record_rewrite(path, &branch, &old, &new, now)?;
    let _ = git::prune_rewrites(path, now);
    Ok(Some((rewrite, git::rewrites(path)?)))
}

/// What the steps `ops` rebase onto: the revision a `rebase` names (`rebase main`, the
/// interactive rebase's `rebase -i … <onto>`), the upstream for `pull --rebase`.
pub(in crate::repo) fn onto_of(ops: &[Vec<String>]) -> Option<String> {
    ops.iter().find_map(|op| {
        let mut args: &[String] = op;
        // Options before the subcommand: `-c name=value` pairs.
        while let [flag, _, rest @ ..] = args
            && flag == "-c"
        {
            args = rest;
        }
        let (command, rest) = args.split_first()?;
        let flags = |name: &str| rest.iter().any(|a| a == name);
        match command.as_str() {
            "rebase"
                if ["--continue", "--abort", "--skip", "--quit"]
                    .iter()
                    .any(|f| flags(f)) =>
            {
                None
            }
            "rebase" => rest.last().filter(|a| !a.starts_with('-')).cloned(),
            "pull" if flags("--rebase") => Some("@{upstream}".to_owned()),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::ops;

    #[test]
    fn what_an_operation_rebases_onto() {
        assert_eq!(
            onto_of(&ops(&[&["rebase", "refs/heads/main"]])),
            Some("refs/heads/main".to_owned())
        );
        let interactive = ops(&[&[
            "-c",
            "sequence.editor=cp /tmp/todo",
            "rebase",
            "-i",
            "--autostash",
            "--end-of-options",
            "abc^",
        ]]);
        assert_eq!(onto_of(&interactive), Some("abc^".to_owned()));
        assert_eq!(
            onto_of(&ops(&[&["pull", "--rebase", "--autostash"]])),
            Some("@{upstream}".to_owned())
        );
        // Guards before it don't count; a pull that merges, a continue and an amend rebase
        // onto nothing.
        let guarded = vec![
            vec!["\u{0}expect".to_owned(), "HEAD".to_owned(), "x".to_owned()],
            vec!["rebase".to_owned(), "main".to_owned()],
        ];
        assert_eq!(onto_of(&guarded), Some("main".to_owned()));
        assert_eq!(onto_of(&ops(&[&["pull", "--ff-only"]])), None);
        assert_eq!(onto_of(&ops(&[&["rebase", "--continue"]])), None);
        assert_eq!(onto_of(&ops(&[&["commit", "--amend", "-m", "x"]])), None);
        assert_eq!(onto_of(&ops(&[&["rebase", "-i"]])), None);
    }
}
