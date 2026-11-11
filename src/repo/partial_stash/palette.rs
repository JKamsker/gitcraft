//! Command-palette contributions owned by this feature.
use crate::keymap::{Command, label};
use crate::repo::dialogs::StashChoice;
use crate::repo::{Action, Dialog};

use crate::repo::actions::PaletteContribution;

pub(in crate::repo) fn snapshots() -> [PaletteContribution; 2] {
    [
        (
            "Stash only what is staged…".into(),
            "stash".into(),
            Action::Dialog(Dialog::stash_as(StashChoice::Staged)),
        ),
        (
            "Save snapshot (stash, keep the changes)…".into(),
            "stash".into(),
            Action::Dialog(Dialog::stash_as(StashChoice::Snapshot)),
        ),
    ]
}

pub(in crate::repo) fn stash(ctx: &egui::Context) -> [PaletteContribution; 1] {
    [(
        "Stash changes…".into(),
        label(ctx, Command::Stash),
        Action::Dialog(Dialog::stash()),
    )]
}
