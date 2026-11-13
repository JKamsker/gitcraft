//! The window: what merged means here, a row per worktree, the archive option, the explicit
//! confirmation, what the last cleanup did, and the archives.

use std::path::PathBuf;

use egui::{RichText, Ui};

use super::{Window, archives, rows};
use crate::git::worktree_cleanup::Outcome;
use crate::git::worktree_cleanup::archive::{Listed, describe_usage};
use crate::repo::RepoTab;
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// What the window asks for, done once it is drawn.
#[derive(Default)]
pub(super) struct Asks {
    pub run: bool,
    pub look_again: bool,
    pub restore: Option<(Listed, PathBuf)>,
    pub delete: Option<Listed>,
}

impl RepoTab {
    pub(in crate::repo) fn worktree_cleanup_window(&mut self, ctx: &egui::Context) {
        let idle = self.busy_op.is_none();
        let checking = self.branch_health.checking();
        let worktrees: Vec<(PathBuf, String)> = self
            .worktrees
            .iter()
            .filter(|w| !w.bare && w.prunable.is_none())
            .map(|w| (w.path.clone(), w.branch.clone().unwrap_or_default()))
            .collect();
        let settings = Settings::get(ctx).worktree_archives;
        let Some(window) = &mut self.worktree_cleanup.window else {
            return;
        };
        let mut open = true;
        let mut asks = Asks::default();
        egui::Window::new(crate::i18n::text("worktree-cleanup-title"))
            .id(egui::Id::new(("worktree-cleanup", &self.path)))
            .open(&mut open)
            .default_width(640.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(620.0)
                    .show(ui, |ui| {
                        body(ui, window, checking, idle, &mut asks);
                        let quota = settings.store().map(|s| s.quota).unwrap_or(0);
                        archives::show(ui, window, &worktrees, quota, &mut asks);
                    });
            });
        let running = window.running;
        if !open && !running {
            self.worktree_cleanup.window = None;
            return;
        }
        if asks.run {
            self.run_cleanup(ctx);
        }
        if asks.look_again {
            self.read_worktrees(ctx);
        }
        if let Some((listed, into)) = asks.restore {
            self.restore_archive(listed, into, ctx);
        }
        if let Some(listed) = asks.delete {
            self.delete_archive(&listed, ctx);
        }
    }

    fn delete_archive(&mut self, listed: &Listed, ctx: &egui::Context) {
        let Some(store) = Settings::get(ctx).worktree_archives.store() else {
            return;
        };
        let said = match store.delete(&listed.manifest) {
            Ok(()) => (
                crate::i18n::text("worktree-cleanup-archive-deleted").to_owned(),
                false,
            ),
            Err(e) => (e, true),
        };
        if let Some(window) = &mut self.worktree_cleanup.window {
            window.said = Some(said);
            window.armed = None;
        }
        self.read_archives(ctx);
    }
}

fn body(ui: &mut Ui, window: &mut Window, checking: bool, idle: bool, asks: &mut Asks) {
    let t = Tokens::get(ui.ctx());
    ui.set_min_width(420.0);
    let dim = |ui: &mut Ui, text: &str| {
        ui.add(egui::Label::new(RichText::new(text).color(t.text_dim)).wrap());
    };
    dim(ui, crate::i18n::text("worktree-cleanup-intro"));
    dim(ui, &window.basis);
    if checking {
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            dim(ui, crate::i18n::text("worktree-cleanup-checking"));
        });
    }
    ui.add_space(8.0);
    if window.rows.is_empty() && window.reading == 0 {
        ui.label(crate::i18n::text("worktree-cleanup-none"));
    }
    if window.reading != 0 {
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            dim(ui, crate::i18n::text("worktree-cleanup-reading"));
        });
    }
    let archive = window.archive;
    for row in &mut window.rows {
        rows::show(ui, row, archive);
    }
    ui.add_space(8.0);
    if ui
        .checkbox(
            &mut window.archive,
            crate::i18n::text("worktree-cleanup-archive-option"),
        )
        .on_hover_text(crate::i18n::text("worktree-cleanup-archive-option-tip"))
        .changed()
    {
        window.untick_blocked();
        window.reviewed = false;
    }
    let count = window.chosen().len();
    let confirm = crate::i18n::message("worktree-cleanup-confirm", &[("count", count.into())]);
    ui.add_enabled(
        count > 0,
        egui::Checkbox::new(&mut window.reviewed, confirm),
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let label = crate::i18n::message("worktree-cleanup-remove", &[("count", count.into())]);
        let danger = egui::Button::new(
            RichText::new(label)
                .color(egui::Color32::WHITE)
                .font(theme::medium(12.5)),
        )
        .fill(t.danger)
        .min_size(egui::vec2(90.0, 30.0));
        let can = count > 0 && window.reviewed && !window.running && idle;
        if ui.add_enabled(can, danger).clicked() {
            asks.run = true;
        }
        if ui
            .add_enabled(
                !window.running,
                egui::Button::new(crate::i18n::text("worktree-cleanup-look-again")),
            )
            .clicked()
        {
            asks.look_again = true;
        }
        if window.running {
            ui.add(crate::spinner::Spinner::new());
        }
    });
    results(ui, &window.results, &t);
}

/// What the last cleanup did, worktree by worktree.
fn results(ui: &mut Ui, results: &[(String, Outcome)], t: &Tokens) {
    if results.is_empty() {
        return;
    }
    ui.add_space(8.0);
    for (name, outcome) in results {
        let (text, bad) = describe(outcome);
        let color = if bad { t.danger } else { t.success };
        ui.add(egui::Label::new(RichText::new(format!("{name}: {text}")).color(color)).wrap());
    }
}

/// What became of a worktree, and whether that is a failure to look at.
pub(super) fn describe(outcome: &Outcome) -> (String, bool) {
    match outcome {
        Outcome::Removed { archive, freed } => {
            let freed = crate::git::worktree_cleanup::describe_size(*freed);
            let text = match archive {
                Some(listed) => crate::i18n::message(
                    "worktree-cleanup-removed-archived",
                    &[
                        ("freed", freed.into()),
                        ("count", listed.manifest.files.len().into()),
                    ],
                ),
                None => {
                    crate::i18n::message("worktree-cleanup-removed", &[("freed", freed.into())])
                }
            };
            (text, false)
        }
        Outcome::Kept(blockers) => {
            let why: Vec<String> = blockers.iter().map(|b| b.describe()).collect();
            (
                crate::i18n::message("worktree-cleanup-kept", &[("why", why.join("; ").into())]),
                true,
            )
        }
        Outcome::Changed => (
            crate::i18n::text("worktree-cleanup-changed").to_owned(),
            true,
        ),
        Outcome::Failed(e) => (
            crate::i18n::message("worktree-cleanup-kept", &[("why", e.clone().into())]),
            true,
        ),
    }
}

/// "12.0 MB of 268.4 MB used" for the archives' line.
pub(super) fn usage(used: u64, quota: u64) -> String {
    describe_usage(used, quota)
}
