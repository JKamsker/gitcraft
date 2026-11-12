//! Command-palette contributions owned by this feature.
use crate::keymap::{Command, label};
use crate::repo::Action;

use crate::repo::actions::PaletteContribution;

pub(in crate::repo) fn items(ctx: &egui::Context) -> [PaletteContribution; 3] {
    [
        ("Undo".into(), label(ctx, Command::Undo), Action::Undo),
        ("Redo".into(), label(ctx, Command::Redo), Action::Redo),
        (
            "Timeline and undo tree".into(),
            label(ctx, Command::Timeline),
            Action::ToggleTimeline,
        ),
    ]
}
