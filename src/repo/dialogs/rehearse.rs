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
        crate::i18n::text("repo-core-1453"),
        &crate::i18n::message("repo-core-1454", &[("machine", format!("{}", machine).into())]),
        &t,
    );
    let Some(current) = current else {
        ui.label(
            RichText::new(crate::i18n::text("repo-core-1455"))
                .color(t.warning),
        );
        ui.add_space(12.0);
        return buttons(ui, crate::i18n::text("repo-core-1453"), false, false, || {
            Action::Rehearse(RehearseAct::Stop)
        });
    };
    ui.radio_value(rebase, true, crate::i18n::message("repo-core-1456", &[("current", format!("{}", current).into()), ("other", format!("{}", other).into())]));
    ui.radio_value(rebase, false, crate::i18n::message("repo-core-1457", &[("other", format!("{}", other).into()), ("current", format!("{}", current).into())]));
    let the_other = if *rebase { crate::i18n::text("repo-core-1458") } else { crate::i18n::text("repo-core-1459") };
    ui.checkbox(compare, crate::i18n::message("repo-core-1460", &[("the-other", format!("{}", the_other).into())]));
    ui.add_space(6.0);
    ui.label(RichText::new(crate::i18n::text("repo-core-1461")).color(t.text_dim));
    field(ui, command, "cargo test, make check, ./test.sh…", focus);
    ui.add_space(12.0);
    buttons(ui, crate::i18n::text("repo-core-1453"), true, enter, || {
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
