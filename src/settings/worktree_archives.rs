//! Settings → Git → Worktree archives: where the untracked files of cleaned-up worktrees are
//! archived, how much the archives may take together, whether they come back by themselves
//! when the worktree is made again, and the archives themselves, to look at and delete.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use super::layout::{self, Card};
use super::{FIELD_WIDTH, Settings, parse_localized_number};
use crate::git::worktree_cleanup::archive::{Listed, Store, default_root};

/// How the archives are kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorktreeArchives {
    /// The archives' folder; empty for the default one in the app's data folder.
    pub location: String,
    /// What all archives may take together, in MiB. Archiving more is refused: none is
    /// deleted to make room.
    pub limit_mib: u32,
    /// Restore an archive's files when its branch is checked out again in a worktree at the
    /// commit it was archived from.
    pub auto_restore: bool,
}

impl Default for WorktreeArchives {
    fn default() -> Self {
        Self {
            location: String::new(),
            limit_mib: 256,
            auto_restore: false,
        }
    }
}

impl WorktreeArchives {
    /// The store these settings describe; none when no folder is known.
    pub(crate) fn store(&self) -> Option<Store> {
        let root = match self.location.trim() {
            "" => default_root()?,
            typed => PathBuf::from(typed),
        };
        Some(Store {
            root,
            quota: u64::from(self.limit_mib) << 20,
        })
    }
}

/// What the store held when last looked at, looked at again every few seconds.
#[derive(Clone)]
struct Seen {
    at: Instant,
    root: PathBuf,
    usage: u64,
    archives: Vec<Listed>,
}

fn seen(ui: &Ui, store: &Store, again: bool) -> Seen {
    let id = egui::Id::new("settings-worktree-archives");
    let kept: Option<Seen> = ui.data(|d| d.get_temp(id));
    match kept {
        Some(s) if !again && s.root == store.root && s.at.elapsed() < Duration::from_secs(3) => s,
        _ => {
            let fresh = Seen {
                at: Instant::now(),
                root: store.root.clone(),
                usage: store.usage(),
                archives: store.list(None),
            };
            ui.data_mut(|d| d.insert_temp(id, fresh.clone()));
            fresh
        }
    }
}

impl Settings {
    pub(super) fn worktree_archives_ui(&mut self, card: &mut Card) {
        let archives = &mut self.worktree_archives;
        card.row_hint(
            "Archive location",
            crate::i18n::text("worktree-cleanup-settings-location-hint"),
            "worktree archive untracked zip folder location",
            |ui| {
                let default = default_root()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                ui.add(
                    layout::field(&mut archives.location)
                        .hint_text(default)
                        .desired_width(layout::fit(ui, FIELD_WIDTH)),
                );
            },
        );
        card.row_hint(
            "Archive storage limit",
            crate::i18n::text("worktree-cleanup-settings-limit-hint"),
            "worktree archive maximum storage quota size limit",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut archives.limit_mib)
                        .range(1..=65_536)
                        .custom_formatter(|value, _| crate::i18n::number(value))
                        .custom_parser(parse_localized_number)
                        .suffix(" MiB"),
                );
            },
        );
        card.row(
            "Restore automatically",
            "worktree archive restore automatically recreate",
            |ui| {
                ui.checkbox(
                    &mut archives.auto_restore,
                    crate::i18n::text("worktree-cleanup-settings-auto-restore"),
                )
                .on_hover_text(crate::i18n::text(
                    "worktree-cleanup-settings-auto-restore-tip",
                ));
            },
        );
        let Some(store) = archives.store() else {
            return;
        };
        card.row("Archives", "worktree archive list delete usage", |ui| {
            archive_list(ui, &store);
        });
    }
}

/// The archives with their sizes, newest first, each with Delete (asked twice).
fn archive_list(ui: &mut Ui, store: &Store) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let seen = seen(ui, store, false);
    let used = crate::git::worktree_cleanup::archive::describe_usage(seen.usage, store.quota);
    ui.vertical(|ui| {
        layout::note(ui, used);
        if seen.archives.is_empty() {
            layout::note(ui, crate::i18n::text("worktree-cleanup-settings-none"));
            return;
        }
        let armed_id = egui::Id::new("settings-worktree-archive-armed");
        let armed: Option<String> = ui.data(|d| d.get_temp(armed_id));
        let mut deleted = false;
        for listed in &seen.archives {
            let m = &listed.manifest;
            ui.horizontal_wrapped(|ui| {
                let line = crate::git::worktree_cleanup::archive::describe(listed);
                ui.label(RichText::new(line).small()).on_hover_text(format!(
                    "{}\n{}",
                    m.worktree,
                    listed.zip.display()
                ));
                let asking = armed.as_deref() == Some(m.id.as_str());
                let label = if asking {
                    crate::i18n::text("worktree-cleanup-delete-confirm")
                } else {
                    crate::i18n::text("worktree-cleanup-delete")
                };
                let color = if asking { t.danger } else { t.text };
                let button = egui::Button::new(RichText::new(label).color(color)).small();
                if ui.add(button).clicked() {
                    if asking {
                        if let Err(e) = store.delete(m) {
                            layout::note(ui, e);
                        }
                        deleted = true;
                        ui.data_mut(|d| d.remove::<String>(armed_id));
                    } else {
                        ui.data_mut(|d| d.insert_temp(armed_id, m.id.clone()));
                    }
                }
            });
        }
        if deleted {
            // Looked at again at once: the list and the usage without it.
            self::seen(ui, store, true);
        }
    });
}
