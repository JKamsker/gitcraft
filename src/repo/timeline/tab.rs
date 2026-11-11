//! The tab's side of the timeline: reading it from disk when the tab opens, an entry for each
//! operation once the read after it says what it did, one for each change found made
//! elsewhere, undo / redo / jumps, and keeping it on disk.

#[path = "navigation.rs"]
mod navigation;
#[path = "persistence.rs"]
mod persistence;

use std::collections::HashSet;

use super::store::{self, Saved};
use super::{Awaiting, Ended, Entry, Origin, RefState, observe, snapshot};
use crate::git;
use crate::repo::{OpKind, Refresh, RepoTab, Undo, UndoEntry};

impl RepoTab {
    /// A multi-step feature finished outside run_ops; the next read records its undo.
    pub(in crate::repo) fn push_undo(&mut self, entry: UndoEntry, ctx: &egui::Context) {
        let label = entry.label.clone();
        self.timeline_op_started(false, Refresh::All);
        self.timeline_op_ended(
            &label,
            &OpKind::plain(),
            Some(entry),
            true,
            Refresh::All,
            ctx,
        );
        self.reload(ctx);
    }

    /// Resuming a release fills in the same undo entry rather than making a second step.
    pub(in crate::repo) fn replace_recent_undo(
        &mut self,
        mut undo: UndoEntry,
        ctx: &egui::Context,
    ) {
        if let Some(entry) = self
            .timeline
            .entries
            .iter_mut()
            .rev()
            .find(|e| e.label == undo.label)
        {
            undo.id = entry.id;
            entry.undo = Some(undo);
            self.save_timeline(ctx);
        } else {
            self.push_undo(undo, ctx);
        }
    }

    pub(in crate::repo) fn update_recent_redo(
        &mut self,
        label: &str,
        redo: Vec<Vec<String>>,
        ctx: &egui::Context,
    ) {
        fn update(then: &mut Ended, label: &str, redo: &[Vec<String>]) {
            if let Ended::New {
                label: name,
                undo: Some(undo),
            } = then
                && name == label
            {
                undo.redo = redo.to_vec();
            }
        }
        if let Awaiting::Ended { then, .. } = &mut self.timeline.awaiting {
            update(then, label, &redo);
        }
        for then in &mut self.timeline.backlog {
            update(then, label, &redo);
        }
        if let Some(entry) = self
            .timeline
            .entries
            .iter_mut()
            .rev()
            .find(|e| e.label == label)
            && let Some(undo) = &mut entry.undo
        {
            undo.redo = redo;
        }
        self.save_timeline(ctx);
    }

    pub(in crate::repo) fn remember_undo(
        &mut self,
        label: &str,
        undo: UndoEntry,
        ctx: &egui::Context,
    ) {
        let state = self.ref_state();
        self.record(
            label.to_owned(),
            Origin::Gitcraft,
            state.clone(),
            state,
            Some(undo),
            ctx,
        );
    }

    /// Reads what was kept for this folder, in the background.
    pub(in crate::repo) fn load_timeline(&mut self, ctx: &egui::Context) {
        let folder = self.path.clone();
        self.then(
            ctx,
            move || store::load(&folder),
            |tab, saved, ctx| tab.adopt_timeline(saved, ctx),
        );
    }

    fn adopt_timeline(&mut self, saved: Result<Saved, String>, ctx: &egui::Context) {
        match saved {
            Ok(saved) => {
                let had_own = !self.timeline.entries.is_empty();
                self.timeline.adopt(saved);
                if had_own {
                    self.save_timeline(ctx);
                }
                self.release_stale_refs(ctx);
            }
            Err(e) => {
                // Kept in memory only: the file stays as it is.
                self.timeline.loaded = true;
                self.timeline.read_only = true;
                self.timeline_error = Some(e);
            }
        }
        if let Some((read, state)) = self.timeline.early_read.take() {
            self.take_state(read, state, ctx);
        }
    }

    /// An operation starts: what is read until it ends is only part of it. One that only
    /// changes the working tree or the index (`Refresh::Status`: staging) moves no branch, and
    /// leaves the timeline alone.
    pub(in crate::repo) fn timeline_op_started(&mut self, quiet: bool, refresh: Refresh) {
        if quiet || refresh == Refresh::Status {
            return;
        }
        // The one before still waits for its read: it is taken in with this one's.
        if let Awaiting::Ended { then, .. } = std::mem::take(&mut self.timeline.awaiting) {
            self.timeline.backlog.push(then);
        }
        self.timeline.awaiting = Awaiting::Running;
    }

    /// An operation ended (`ok` or not); `undo` is how to undo it, when it can be. What it
    /// did is known from the next read of the repository.
    pub(in crate::repo) fn timeline_op_ended(
        &mut self,
        label: &str,
        kind: &OpKind,
        undo: Option<UndoEntry>,
        ok: bool,
        refresh: Refresh,
        ctx: &egui::Context,
    ) {
        let jump = self.timeline.jump.take_if(|(l, _)| l == label);
        let then = match (kind, jump) {
            (_, Some((_, to))) if ok => Ended::Moved { to, undo: None },
            (OpKind::Undo(entry), _) if ok => Ended::Moved {
                to: self.timeline.get(entry.id).and_then(|e| e.parent),
                undo: Some(entry.clone()),
            },
            (OpKind::Redo(entry), _) if ok => Ended::Moved {
                to: Some(entry.id),
                undo: Some(entry.clone()),
            },
            // Stopped partway, it may have moved something all the same.
            _ if !ok => Ended::New {
                label: format!("{label} (stopped partway)"),
                undo: None,
            },
            _ => Ended::New {
                label: label.to_owned(),
                undo,
            },
        };
        if refresh == Refresh::Status {
            // No read of the branches follows, and it moved none: only one that can be undone
            // is an entry, at once.
            if let (
                Ended::New {
                    label,
                    undo: Some(undo),
                },
                Some(state),
            ) = (then, self.timeline.seen.clone())
            {
                self.record(
                    label,
                    Origin::Gitcraft,
                    state.clone(),
                    state,
                    Some(undo),
                    ctx,
                );
            }
            return;
        }
        self.timeline.awaiting = Awaiting::Ended {
            after_read: self.reads,
            then,
        };
    }

    /// The branches as the read numbered `read` found them.
    pub(in crate::repo) fn timeline_read(&mut self, read: u64, ctx: &egui::Context) {
        let state = self.ref_state();
        if !self.timeline.loaded {
            self.timeline.early_read = Some((read, state));
            return;
        }
        self.take_state(read, state, ctx);
    }

    fn take_state(&mut self, read: u64, state: RefState, ctx: &egui::Context) {
        match std::mem::take(&mut self.timeline.awaiting) {
            // Read while it ran (or before it ended): only part of it.
            awaiting @ Awaiting::Running => self.timeline.awaiting = awaiting,
            awaiting @ Awaiting::Ended { after_read, .. } if read <= after_read => {
                self.timeline.awaiting = awaiting;
            }
            Awaiting::Ended { then, .. } => {
                // Those that ended before it, whose own reads were missed: each is an entry,
                // the first with what they all moved.
                for earlier in std::mem::take(&mut self.timeline.backlog) {
                    self.resolve(earlier, state.clone(), true, ctx);
                }
                self.resolve(then, state, false, ctx);
            }
            Awaiting::Nothing => self.observe(state, ctx),
        }
    }

    /// What an operation that ended did, now that `state` says where the branches are; with
    /// `always`, it is an entry even if nothing it did shows any more.
    fn resolve(&mut self, then: Ended, state: RefState, always: bool, ctx: &egui::Context) {
        let before = self.timeline.seen.clone().unwrap_or_else(|| state.clone());
        let before = self.without_elsewhere(&before, &state);
        let seen_changed = self.timeline.seen.as_ref() != Some(&state);
        self.timeline.seen = Some(state.clone());
        match then {
            Ended::New { label, undo } => {
                if always || undo.is_some() || before != state {
                    self.record(label, Origin::Gitcraft, before, state, undo, ctx);
                } else if seen_changed {
                    self.save_timeline(ctx);
                }
            }
            Ended::Moved { to, undo } => {
                self.timeline.move_to(to, undo);
                self.save_timeline(ctx);
            }
        }
    }

    /// A read with no operation of gitcraft's behind it: what differs from the last one was
    /// done elsewhere.
    fn observe(&mut self, state: RefState, ctx: &egui::Context) {
        let Some(before) = self.timeline.seen.replace(state.clone()) else {
            self.save_timeline(ctx);
            return;
        };
        let before = self.without_elsewhere(&before, &state);
        if before != state {
            let label = observe::fallback_label(&super::changes(&before, &state));
            self.record(label, Origin::Outside, before, state, None, ctx);
        }
    }

    /// Adds an entry, then finds out its files (and what it was, for a change made elsewhere)
    /// and keeps its commits from gc, in the background.
    fn record(
        &mut self,
        label: String,
        origin: Origin,
        before: RefState,
        after: RefState,
        undo: Option<UndoEntry>,
        ctx: &egui::Context,
    ) {
        let (before, after) = super::sparse(&before, &after);
        let id = self.timeline.push(Entry {
            id: 0,
            parent: None,
            time: git::now(),
            label,
            origin,
            before,
            after,
            files: Vec::new(),
            files_total: 0,
            undo,
            redo_child: None,
            receipt: None,
        });
        self.save_timeline(ctx);
        self.release_dropped(ctx);
        let Some(entry) = self.timeline.get(id).cloned() else {
            return;
        };
        let (path, key) = (self.path.clone(), store::key(&self.path));
        self.then_or(
            ctx,
            false,
            |error| (observe::Found::default(), Err(error)),
            move || {
                let found = observe::find(&path, &entry);
                let kept = snapshot::keep(
                    &path,
                    &key,
                    entry.id,
                    &entry.label,
                    &snapshot::commits_of(&entry),
                );
                (found, kept)
            },
            move |tab, (found, kept), ctx| {
                let mut receipt_error = None;
                if let Some(entry) = tab.timeline.entries.iter_mut().find(|e| e.id == id) {
                    entry.files = found.files;
                    entry.files_total = found.files_total;
                    if let Some(label) = found.label {
                        entry.label = label;
                    }
                    match found.receipt {
                        Some(Ok(receipt)) => entry.receipt = Some(receipt),
                        Some(Err(e)) => receipt_error = Some(e),
                        None => {}
                    }
                }
                if let Some(e) = receipt_error {
                    tab.timeline_error = Some(crate::i18n::message(
                        "repo-workflows-timeline-record-56546037",
                        &[("e", (e).clone().into())],
                    ));
                }
                tab.show_receipt(id);
                let keep_ok = kept.is_ok();
                if let Err(e) = kept {
                    tab.timeline_error = Some(crate::i18n::message(
                        "repo-workflows-timeline-record-ef197d90",
                        &[
                            (
                                "tab-timeline-get-id-map-or-e-e-l-1",
                                (tab.timeline.get(id).map_or("", |e| e.label.as_str())).into(),
                            ),
                            ("e", (e).clone().into()),
                        ],
                    ));
                }
                tab.save_timeline(ctx);
                tab.dco_timeline_kept(id, keep_ok, ctx);
            },
        );
    }

    /// Says what a rewrite gitcraft just did, in the status line, while that still says it's done.
    fn show_receipt(&mut self, id: u64) {
        let Some(entry) = self.timeline.get(id) else {
            return;
        };
        let (Some(receipt), Origin::Gitcraft) = (&entry.receipt, entry.origin) else {
            return;
        };
        let done = self
            .status
            .as_deref()
            .and_then(|(s, failed)| (!failed).then_some(s.as_str()))
            .and_then(|s| s.strip_suffix(": done"))
            .map(str::to_owned);
        if let Some(label) = done {
            self.status = Some(crate::repo::Notice::plain((
                format!(
                    "{label}: done. {} (receipt in the timeline, Ctrl+Shift+Z)",
                    receipt.sentence()
                ),
                false,
            )));
        }
    }

    /// Forgets the entries older than the newest `keep` (all of them with 0), but those leading
    /// to the current one, and lets go of their commits.
    pub(in crate::repo) fn forget_timeline(&mut self, keep: usize, ctx: &egui::Context) {
        if keep == 0 {
            let all: Vec<u64> = self.timeline.entries.iter().map(|e| e.id).collect();
            self.timeline.forget(&all);
        } else {
            self.timeline.forget_older(keep);
        }
        self.save_timeline(ctx);
        self.release_dropped(ctx);
    }
}
