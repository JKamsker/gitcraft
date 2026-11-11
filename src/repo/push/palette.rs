//! Command-palette contributions owned by this feature.
use crate::keymap::{Command, label};
use crate::repo::{Action, pull::PullMode};

use crate::repo::actions::PaletteContribution;

pub(in crate::repo) fn items(ctx: &egui::Context, for_review: bool) -> Vec<PaletteContribution> {
    let mut items = vec![
        ("Fetch".into(), label(ctx, Command::Fetch), Action::Fetch),
        ("Pull".into(), label(ctx, Command::Pull), Action::Pull),
        (
            if for_review {
                "Push for review"
            } else {
                "Push"
            }
            .into(),
            label(ctx, Command::Push),
            Action::Push,
        ),
        (
            "Sync (pull, then push)".into(),
            "remote".into(),
            Action::Sync,
        ),
        (
            if for_review {
                "Push for review, skipping hooks"
            } else {
                "Push, skipping hooks"
            }
            .into(),
            "--no-verify".into(),
            Action::PushSkippingHooks,
        ),
    ];
    if for_review {
        items.retain(|(_, _, action)| !matches!(action, Action::Sync));
    }
    items.extend(
        PullMode::ALL.map(|mode| (mode.label().into(), "remote".into(), Action::PullWith(mode))),
    );
    items
}
