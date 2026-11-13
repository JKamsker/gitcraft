//! Settings → Git → Worktree archives: where the untracked files of cleaned-up worktrees are
//! archived, how much the archives may take together, whether they come back by themselves
//! when the worktree is made again, and the archives themselves, to look at and delete.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use super::layout::{self, Card};
use super::{FIELD_WIDTH, Settings, parse_localized_number};
use crate::git::worktree_cleanup::archive::{Listed, Store};

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
            "" => Store::default_root()?,
            typed => PathBuf::from(typed),
        };
        Some(Store {
            root,
            quota: u64::from(self.limit_mib) << 20,
        })
    }
}

/// What the store held when last looked at, read again in the background every few seconds
/// while the page shows: a store can be large or slow, and never holds up drawing.
#[derive(Clone)]
struct Seen {
    at: Instant,
    root: PathBuf,
    usage: u64,
    archives: Vec<Listed>,
}

/// The last look, and whether one is under way.
static LOOK: Mutex<(Option<Seen>, bool)> = Mutex::new((None, false));

fn look() -> std::sync::MutexGuard<'static, (Option<Seen>, bool)> {
    LOOK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What `store` held when last looked at, if that was this store; starts a new look when there
/// is none, it is a few seconds old, or `again`.
fn seen(ctx: &egui::Context, store: &Store, again: bool) -> Option<Seen> {
    let mut held = look();
    let kept = held.0.clone().filter(|s| s.root == store.root);
    let fresh = kept
        .as_ref()
        .is_some_and(|s| s.at.elapsed() < Duration::from_secs(3));
    if (again || !fresh) && !held.1 {
        held.1 = true;
        let (store, ctx) = (store.clone(), ctx.clone());
        std::thread::spawn(move || {
            let fresh = Seen {
                at: Instant::now(),
                root: store.root.clone(),
                usage: store.usage(),
                archives: store.list(None),
            };
            *look() = (Some(fresh), false);
            ctx.request_repaint();
        });
    }
    kept
}

/// Takes the archive `id` off the last look.
fn forget(id: &str) {
    if let Some(seen) = &mut look().0 {
        seen.archives.retain(|a| a.id() != id);
    }
}

/// The archive location as typed: taken on Enter or when the field is left, so that the half
/// of a path typed so far is never looked in.
fn location_field(ui: &mut Ui, location: &mut String) {
    let id = egui::Id::new("settings-worktree-archive-location");
    let mut draft: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| location.clone());
    let default = Store::default_root()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let field = ui.add(
        layout::field(&mut draft)
            .hint_text(default)
            .desired_width(layout::fit(ui, FIELD_WIDTH)),
    );
    if field.lost_focus() {
        *location = draft;
        ui.data_mut(|d| d.remove::<String>(id));
    } else if field.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, draft));
    }
}

impl Settings {
    pub(super) fn worktree_archives_ui(&mut self, card: &mut Card) {
        let archives = &mut self.worktree_archives;
        card.row_hint(
            "Archive location",
            crate::i18n::text("worktree-cleanup-settings-location-hint"),
            "worktree archive untracked zip folder location",
            |ui| location_field(ui, &mut archives.location),
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
    let Some(seen) = seen(ui.ctx(), store, false) else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let used = store.usage_line(seen.usage);
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
            ui.horizontal_wrapped(|ui| {
                let line = listed.describe();
                ui.label(RichText::new(line).small()).on_hover_text(format!(
                    "{}\n{}",
                    listed.worktree(),
                    listed.zip.display()
                ));
                let asking = armed.as_deref() == Some(listed.id());
                let label = if asking {
                    crate::i18n::text("worktree-cleanup-delete-confirm")
                } else {
                    crate::i18n::text("worktree-cleanup-delete")
                };
                let color = if asking { t.danger } else { t.text };
                let button = egui::Button::new(RichText::new(label).color(color)).small();
                if ui.add(button).clicked() {
                    if asking {
                        match store.delete(listed) {
                            // Gone from the list at once; the usage follows with the next look.
                            Ok(()) => forget(listed.id()),
                            Err(e) => layout::note(ui, e),
                        }
                        deleted = true;
                        ui.data_mut(|d| d.remove::<String>(armed_id));
                    } else {
                        ui.data_mut(|d| d.insert_temp(armed_id, listed.id().to_owned()));
                    }
                }
            });
        }
        if deleted {
            // Looked at again at once: the list and the usage without it.
            self::seen(ui.ctx(), store, true);
        }
    });
}
