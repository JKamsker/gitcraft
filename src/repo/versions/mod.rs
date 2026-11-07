//! Two versions of a series of commits side by side in the big area, as `git range-diff` pairs
//! them: a branch before and after gitcraft rewrote it, a branch and its upstream, a pull
//! request as last looked at and as it is now. Clicking a pair shows how its patch changed.

mod pr_since;
mod rewrites;
#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;

use super::{RepoTab, diff, short};
use crate::git::{self, PairKind, RangeDiff, Series};

pub(super) use pr_since::{Since, since_banner};

/// What the tab keeps for comparing versions.
#[derive(Default)]
pub(crate) struct Versions {
    /// The rewrites kept for this repository, newest first.
    pub(super) rewrites: Vec<git::Rewrite>,
    /// They have been read once (they change only when gitcraft records or prunes one).
    rewrites_read: bool,
    /// The rewrite the last operation made, offered beside the message saying it's done: the
    /// message it goes with, and the rewrite.
    pub(super) fresh: Option<(String, git::Rewrite)>,
    /// A rewrite that stopped on conflicts: what it started from, kept once it is continued.
    pending: Option<rewrites::Pending>,
    /// The comparison the big area shows.
    pub(super) view: Option<VersionsView>,
    /// The head of each pull request when last looked at, by number (for the list).
    pub(super) seen_heads: HashMap<u64, String>,
    /// What changed in the pull request shown since it was last looked at.
    pub(super) since: Option<Since>,
    /// Numbers the readings, so that an older answer doesn't replace a newer one.
    requests: u64,
}

/// The comparison shown.
pub(crate) struct VersionsView {
    pub(super) title: String,
    pub(super) old_label: String,
    pub(super) new_label: String,
    pub(super) old: Series,
    pub(super) new: Series,
    pub(super) data: Option<Result<RangeDiff, String>>,
    /// The pair clicked, by its place.
    pub(super) selected: Option<usize>,
    /// How its patch changed (`None` while it is read).
    pub(super) interdiff: Option<Result<diff::DiffData, String>>,
    /// A pull request's version (by number) to take as seen once the comparison is shown.
    pub(super) seen_when_shown: Option<(u64, Series)>,
    /// Scroll the list to the selected pair on the next frame.
    pub(super) reveal: bool,
    /// Two branches, neither older than the other: a commit is only on the left or the
    /// right, not added or dropped.
    pub(super) sides: bool,
    request: u64,
}

/// What the comparison views ask the tab to do.
pub(crate) enum VersionsAct {
    /// Compare what a kept rewrite replaced with what it made.
    Rewrite(git::Rewrite),
    /// Compare two branches (full refs) by the commits each has that the other hasn't, as
    /// `git range-diff old...new` does: a branch with its upstream after a rebase.
    Branches {
        old: String,
        old_label: String,
        new: String,
        new_label: String,
    },
    /// Show how the patch of the pair at this place changed (or stop showing it, when it is
    /// shown already).
    Select(usize),
    /// Stop showing a pair's patch.
    Deselect,
    Close,
    /// Compare the pull request numbered so with the version last looked at, which it then is.
    PrSince(u64),
    /// Take the pull request's version now as looked at, without comparing.
    PrSeen(u64),
}

impl RepoTab {
    pub(super) fn versions_act(&mut self, act: VersionsAct, ctx: &egui::Context) {
        match act {
            VersionsAct::Rewrite(rewrite) => self.open_rewrite(rewrite, ctx),
            VersionsAct::Branches {
                old,
                old_label,
                new,
                new_label,
            } => self.open_branch_versions(old, old_label, new, new_label, ctx),
            VersionsAct::Select(i) => self.select_pair(i, ctx),
            VersionsAct::Deselect => {
                if let Some(view) = &mut self.versions.view {
                    view.selected = None;
                    view.interdiff = None;
                }
            }
            VersionsAct::Close => self.versions.view = None,
            VersionsAct::PrSince(number) => self.open_pr_since(number, ctx),
            VersionsAct::PrSeen(number) => self.mark_pr_seen(number, ctx),
        }
    }

    /// Shows the series `old` and `new` side by side in the big area, read in the background.
    pub(super) fn open_versions(
        &mut self,
        title: String,
        (old_label, new_label): (String, String),
        old: Series,
        new: Series,
        ctx: &egui::Context,
    ) {
        // Those editors hold choices written nowhere yet and take the big area until done.
        if self.conflict.is_some() || self.rebase.is_some() {
            let what = if self.conflict.is_some() {
                "Finish or close the conflict editor first"
            } else {
                "Finish or cancel the interactive rebase first"
            };
            self.status = Some((what.to_owned(), true));
            return;
        }
        self.versions.requests += 1;
        let request = self.versions.requests;
        // The comparison takes the big area: a diff or file open over it is closed.
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.versions.view = Some(VersionsView {
            title,
            old_label,
            new_label,
            old: old.clone(),
            new: new.clone(),
            data: None,
            selected: None,
            interdiff: None,
            seen_when_shown: None,
            reveal: false,
            sides: false,
            request,
        });
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::range_diff(&path, &old, &new),
            move |tab, result: Result<RangeDiff, String>, ctx| {
                let Some(view) = tab.shown_versions(request) else {
                    return;
                };
                let shown = result.is_ok();
                view.data = Some(result);
                if let Some((number, seen)) = view.seen_when_shown.take().filter(|_| shown) {
                    tab.mark_version_seen(number, seen, ctx);
                }
            },
        );
    }

    /// The comparison shown, if it is the one read with `request`.
    fn shown_versions(&mut self, request: u64) -> Option<&mut VersionsView> {
        self.versions
            .view
            .as_mut()
            .filter(|view| view.request == request)
    }

    fn open_rewrite(&mut self, rewrite: git::Rewrite, ctx: &egui::Context) {
        let when = git::relative_time(rewrite.time, git::now());
        let title = format!("{} before and after its rewrite ({when})", rewrite.branch);
        let labels = (
            format!("Before ({})", short(&rewrite.old.tip)),
            format!("After ({})", short(&rewrite.new.tip)),
        );
        self.open_versions(title, labels, rewrite.old, rewrite.new, ctx);
    }

    /// Compares two branches by what each has that the other hasn't, once both are resolved.
    fn open_branch_versions(
        &mut self,
        old: String,
        old_label: String,
        new: String,
        new_label: String,
        ctx: &egui::Context,
    ) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let old_tip = git::resolve(&path, &old)?;
                let new_tip = git::resolve(&path, &new)?;
                let base = git::merge_base(&path, &old_tip, &new_tip)?;
                Ok((Series::new(&base, &old_tip), Series::new(&base, &new_tip)))
            },
            move |tab, found: Result<(Series, Series), String>, ctx| match found {
                Ok((old, new)) => {
                    let title = format!("{old_label} and {new_label}");
                    tab.open_versions(title, (old_label, new_label), old, new, ctx);
                    // Neither is older: the marks say which side a commit is only on.
                    if let Some(view) = &mut tab.versions.view {
                        view.sides = true;
                    }
                }
                Err(e) => tab.status = Some((format!("Compare versions: {e}"), true)),
            },
        );
    }

    /// Shows how the patch of the pair at place `i` changed: from what range-diff printed for
    /// a changed pair, the whole patch for a commit only one side has.
    fn select_pair(&mut self, i: usize, ctx: &egui::Context) {
        let Some(view) = &mut self.versions.view else {
            return;
        };
        let Some(Ok(data)) = &view.data else {
            return;
        };
        let Some(pair) = data.pairs.get(i) else {
            return;
        };
        if view.selected == Some(i) {
            view.selected = None;
            view.interdiff = None;
            return;
        }
        view.selected = Some(i);
        view.reveal = true;
        let lone = match (pair.kind, &pair.old, &pair.new) {
            (PairKind::Added, _, Some(c)) => Some((c.hash.clone(), '+')),
            (PairKind::Dropped, Some(c), _) => Some((c.hash.clone(), '-')),
            _ => None,
        };
        let Some((hash, mark)) = lone else {
            view.interdiff = Some(Ok(diff::interdiff_data(&pair.body)));
            return;
        };
        view.interdiff = None;
        let request = view.request;
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::lone_patch(&path, &hash, mark).map(|body| diff::interdiff_data(&body)),
            move |tab, result, _| {
                if let Some(view) = tab.shown_versions(request)
                    && view.selected == Some(i)
                {
                    view.interdiff = Some(result);
                }
            },
        );
    }
}

/// How a summary of two versions reads: "2 commits changed, 1 added, 3 identical".
pub(crate) fn summary(counts: &git::Counts) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut push = |n: usize, what: &str| {
        if n > 0 {
            parts.push(format!("{n} {what}"));
        }
    };
    push(counts.changed, "changed");
    push(counts.added, "added");
    push(counts.dropped, "dropped");
    push(counts.moved, "moved");
    push(counts.same, "identical");
    let Some(first) = parts.first_mut() else {
        return "no commits".to_owned();
    };
    // "2 commits changed": the noun goes with the first count only.
    if let Some((n, what)) = first.split_once(' ') {
        let noun = if n == "1" { "commit" } else { "commits" };
        *first = format!("{n} {noun} {what}");
    }
    parts.join(", ")
}
