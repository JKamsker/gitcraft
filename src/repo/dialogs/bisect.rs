//! Dialogs that start a bisect and run a command for it.

use egui::{RichText, Ui};

use crate::repo::bisect::BisectAct;
use crate::repo::{Action, short};
use crate::theme::Tokens;

use super::{Outcome, buttons, field, title, usable};

/// Starts a bisect from the commit `hash`, `this_good` saying which end it is; `other` is the
/// other end (the bad one by default HEAD; a good one may be left to mark later). `blocker`
/// says why it can't start, if it can't.
pub(super) fn start_bisect(
    ui: &mut Ui,
    enter: bool,
    hash: &str,
    this_good: &mut bool,
    other: &mut String,
    focus: &mut bool,
    blocker: Option<&str>,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-1199"),
        crate::i18n::text("repo-core-1200"),
        &t,
    );
    let at = short(hash);
    let was_good = *this_good;
    ui.radio_value(
        this_good,
        true,
        crate::i18n::message("repo-core-1201", &[("at", format!("{}", at).into())]),
    );
    ui.radio_value(this_good, false, crate::i18n::message("repo-core-1202", &[("at", format!("{}", at).into())]));
    // The other end's usual answer, unless one was typed.
    if *this_good != was_good {
        match (*this_good, other.trim()) {
            (true, "") => *other = "HEAD".to_owned(),
            (false, "HEAD") => other.clear(),
            _ => {}
        }
    }
    ui.add_space(8.0);
    let (label, hint) = if *this_good {
        (
            crate::i18n::text("repo-core-1203"),
            crate::i18n::text("repo-core-1204"),
        )
    } else {
        (
            crate::i18n::text("repo-core-1205"),
            crate::i18n::text("repo-core-1206"),
        )
    };
    ui.label(RichText::new(label).color(t.text_dim));
    field(ui, other, hint, focus);
    if let Some(why) = blocker {
        ui.add_space(6.0);
        ui.label(RichText::new(why).color(t.warning));
    }
    ui.add_space(12.0);
    let other_ok = usable(other) || (!*this_good && other.trim().is_empty());
    let valid = blocker.is_none() && other_ok;
    buttons(ui, crate::i18n::text("repo-core-1207"), valid, enter, || {
        let other = other.trim().to_owned();
        let (bad, good) = if *this_good {
            (other, Some(hash.to_owned()))
        } else {
            (hash.to_owned(), (!other.is_empty()).then_some(other))
        };
        Action::Bisect(BisectAct::Start { bad, good })
    })
}

/// `git bisect run` with a command typed, on `machine` (where the repository is).
pub(super) fn bisect_run(
    ui: &mut Ui,
    enter: bool,
    machine: &str,
    command: &mut String,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-1208"),
        &crate::i18n::message("repo-core-1209", &[("machine", format!("{}", machine).into())]),
        &t,
    );
    field(ui, command, "cargo test, make check, ./test.sh…", focus);
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            crate::i18n::text("repo-core-1211"),
        )
        .color(t.text_dim),
    );
    ui.add_space(12.0);
    let valid = !command.trim().is_empty();
    buttons(ui, crate::i18n::text("repo-core-1212"), valid, enter, || {
        Action::Bisect(BisectAct::Run(command.trim().to_owned()))
    })
}
