//! Palette stash choices retain distinct mutation/undo semantics before confirmation.
use super::Choice;
use crate::repo::{Action, Dialog, partial_stash};

#[test]
fn palette_stash_intents_keep_their_exact_confirmation_choices() -> Result<(), String> {
    let ctx = egui::Context::default();
    let entries = partial_stash::palette::snapshots()
        .into_iter()
        .chain(partial_stash::palette::stash(&ctx));
    for ((_, _, action), expected) in
        entries.zip([Choice::Staged, Choice::Snapshot, Choice::Everything])
    {
        let Action::Dialog(Dialog::Stash(dialog)) = action else {
            return Err("stash palette entry must ask for confirmation".into());
        };
        assert_eq!(dialog.choice, expected);
        assert!(dialog.files.is_empty());
        assert!(dialog.message.is_empty());
        assert!(dialog.untracked);
        assert!(dialog.focus);
        assert!(!dialog.seeded);
        assert!(!dialog.opened);
    }
    Ok(())
}
