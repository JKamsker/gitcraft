//! The dialog that rehearses a rebase or merge of the checked-out branch with another.

use egui::{RichText, Ui};

use crate::git::RehearseKind;
use crate::repo::Action;
use crate::repo::rehearse::RehearseAct;
use crate::theme::Tokens;

use super::{Outcome, buttons, field, title};

/// Rehearses with `other` (remote-tracking when `remote`) the checked-out `current`: rebasing
/// onto it (`rebase`) or merging it, `compare` with the other way too, `command` run on the
/// result, `machine` where.
#[allow(clippy::too_many_arguments)]
pub(super) fn rehearse(
    ui: &mut Ui,
    enter: bool,
    machine: &str,
    current: Option<&str>,
    other: &str,
    remote: bool,
    rebase: &mut bool,
    compare: &mut bool,
    command: &mut String,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        "Rehearse",
        &format!(
            "Plays it through {machine} in a scratch worktree and says where it stops, which \
             files conflict and whether the tests pass. Nothing of yours changes until you pick \
             one."
        ),
        &t,
    );
    let Some(current) = current else {
        ui.label(
            RichText::new("Check out a branch first: rehearsals are of the checked-out one.")
                .color(t.warning),
        );
        ui.add_space(12.0);
        return buttons(ui, "Rehearse", false, false, || {
            Action::Rehearse(RehearseAct::Stop)
        });
    };
    ui.radio_value(rebase, true, format!("Rebase {current} onto {other}"));
    ui.radio_value(rebase, false, format!("Merge {other} into {current}"));
    let the_other = if *rebase { "the merge" } else { "the rebase" };
    ui.checkbox(compare, format!("Rehearse {the_other} too, to compare"));
    ui.add_space(6.0);
    ui.label(RichText::new("Test the result with (leave empty not to):").color(t.text_dim));
    field(ui, command, "cargo test, make check, ./test.sh…", focus);
    ui.add_space(12.0);
    buttons(ui, "Rehearse", true, enter, || {
        let first = if *rebase {
            RehearseKind::Rebase
        } else {
            RehearseKind::Merge
        };
        let mut kinds = vec![first];
        if *compare {
            kinds.push(match first {
                RehearseKind::Rebase => RehearseKind::Merge,
                RehearseKind::Merge => RehearseKind::Rebase,
            });
        }
        let command = command.trim();
        Action::Rehearse(RehearseAct::Start {
            other: other.to_owned(),
            remote,
            kinds,
            command: (!command.is_empty()).then(|| command.to_owned()),
        })
    })
}
