//! The commit pane of a merge: the "What this merge resolved" box, and while it is ticked the
//! files that differ from git's own merge, the conflicts resolved first.

use std::collections::HashMap;

use egui::{RichText, Ui};

use crate::git::{self, ChangedFile, CommitDetail, Remerge, RemergeFile};
use crate::repo::detail::files_header;
use crate::repo::{Action, DiffKey, RepoTab};
use crate::theme::{self, Tokens};

/// What the commit pane shows of the merge it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) enum Mode {
    /// Not a merge (or a stash): no box.
    NoBox,
    /// The box is there, not ticked.
    Off,
    /// It can't be ticked, for this reason.
    Unavailable(String),
    /// Ticked, while it isn't known yet whether git can redo merges.
    Checking,
    /// Ticked: what the merge resolved.
    On,
}

impl RepoTab {
    /// What the commit pane shows of `detail`.
    pub(in crate::repo) fn remerge_mode(&self, detail: &CommitDetail) -> Mode {
        if detail.parents.len() < 2 || self.stash_view.is_some() {
            return Mode::NoBox;
        }
        if let Some(why) = git::remerge_unavailable(detail.parents.len()) {
            return Mode::Unavailable(why);
        }
        if let Some(why) = self.remerges.unsupported() {
            return Mode::Unavailable(why.to_owned());
        }
        match (self.remerges.on, self.remerges.supported()) {
            (false, _) => Mode::Off,
            (true, false) => Mode::Checking,
            (true, true) => Mode::On,
        }
    }

    /// Reads what the merge shown resolved, when that is to be shown and isn't read yet.
    pub(in crate::repo) fn want_remerge_shown(&mut self, ctx: &egui::Context) {
        let Some(detail) = &self.detail else { return };
        if matches!(self.remerge_mode(detail), Mode::Checking | Mode::On) {
            let hash = detail.hash.clone();
            self.want_remerge(&hash, ctx);
        }
    }

    /// The "What this merge resolved" box of a merge, with why it can't be ticked if it can't.
    pub(in crate::repo) fn remerge_toggle(&self, ui: &mut Ui, acts: &mut Vec<Action>, mode: &Mode) {
        let t = Tokens::get(ui.ctx());
        if *mode == Mode::NoBox {
            return;
        }
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                let available = !matches!(mode, Mode::Unavailable(_));
                let mut on = matches!(mode, Mode::Checking | Mode::On);
                let resp = ui
                    .add_enabled(
                        available,
                        egui::Checkbox::new(&mut on, "What this merge resolved"),
                    )
                    .on_hover_text(
                        "Only what differs from the merge git makes of the parents today: how \
                         its conflicts were resolved, and anything changed beyond that (by \
                         hand, or by a merge made with other options). Replaces the list of \
                         files it changed.",
                    );
                if resp.changed() {
                    acts.push(Action::ShowRemerge(on));
                }
                if let Mode::Unavailable(why) = mode {
                    ui.add(egui::Label::new(RichText::new(why).small().color(t.text_faint)).wrap());
                }
            });
    }

    /// While the box is ticked: the files the merge `detail` resolved or edited beyond the
    /// merge, each opening its part of the remerge diff; or why there are none.
    pub(in crate::repo) fn remerge_files(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        detail: &CommitDetail,
    ) {
        let t = Tokens::get(ui.ctx());
        let remerge = match self.remerges.of(&detail.hash) {
            Some(Ok(remerge)) => remerge,
            Some(Err(e)) => {
                note(ui, RichText::new(e).color(t.danger));
                return;
            }
            None => {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    ui.label(RichText::new("Redoing the merge…").color(t.text_faint));
                });
                return;
            }
        };
        if remerge.is_clean() {
            clean_state(ui);
            return;
        }
        let all: Vec<ChangedFile> = remerge.files.iter().map(changed_file).collect();
        files_header(ui, &all, Some("vs. git's own merge"));
        let by_path: HashMap<&str, &RemergeFile> =
            remerge.files.iter().map(|f| (f.path.as_str(), f)).collect();
        let key = |path: &str| {
            let file = by_path.get(path);
            DiffKey::Remerge {
                hash: detail.hash.clone(),
                path: path.to_owned(),
                old_path: file.and_then(|f| f.old_path.clone()),
                conflicts: file.map(|f| f.conflicts.clone()).unwrap_or_default(),
            }
        };
        for (tree, title, files) in sections(remerge) {
            if files.is_empty() {
                continue;
            }
            section_title(ui, title, files.len());
            self.file_list(ui, acts, tree, &files, Some(&detail.hash), key);
        }
    }
}

/// The lists of the pane: (its name, its title, its files), conflicts resolved first.
fn sections(remerge: &Remerge) -> [(&'static str, &'static str, Vec<ChangedFile>); 2] {
    [
        (
            "remerge-conflicts",
            "Conflicts resolved",
            remerge.conflicted().map(changed_file).collect(),
        ),
        (
            "remerge-edits",
            "Edited beyond the merge",
            remerge.edited().map(changed_file).collect(),
        ),
    ]
}

/// `file` as the file lists show one.
fn changed_file(file: &RemergeFile) -> ChangedFile {
    ChangedFile {
        status: file.status,
        path: file.path.clone(),
        old_path: file.old_path.clone(),
        added: Some(file.added),
        removed: Some(file.removed),
    }
}

/// A list's title and how many files it has.
fn section_title(ui: &mut Ui, title: &str, count: usize) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 14,
            right: 14,
            top: 8,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!("{} · {count}", title.to_uppercase()))
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
            );
        });
}

/// A merge that resolved nothing: said so, and what that means.
fn clean_state(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.label(RichText::new("A clean merge").font(theme::medium(13.0)));
            ui.add_space(2.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "The recorded merge is exactly the one git makes of its parents: it \
                         had no conflicts to resolve and nothing else was changed.",
                    )
                    .color(t.text_dim),
                )
                .wrap(),
            );
        });
}

/// A line of text in the pane, wrapping.
fn note(ui: &mut Ui, text: RichText) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.add(egui::Label::new(text).wrap());
        });
}
