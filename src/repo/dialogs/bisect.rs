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
        "Start bisect",
        "Finds the commit that brought a change in (a bug, say): git checks out commits between \
         a good one and a bad one for you to test, halving the range each time.",
        &t,
    );
    let at = short(hash);
    let was_good = *this_good;
    ui.radio_value(
        this_good,
        true,
        format!("{at} is good: from before the change"),
    );
    ui.radio_value(this_good, false, format!("{at} is bad: it has the change"));
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
            "The bad commit, which has the change:",
            "HEAD, a branch, a tag or a hash",
        )
    } else {
        (
            "A good commit, from before the change (or leave it empty and mark one later):",
            "a branch, a tag or a hash",
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
    buttons(ui, "Start", valid, enter, || {
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
        "Run a command to bisect",
        &format!(
            "Runs it {machine}, in the repository, at each commit git picks, until the first \
             bad one is found. Its output shows under the banner."
        ),
        &t,
    );
    field(ui, command, "cargo test, make check, ./test.sh…", focus);
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "Exit code 0 marks a commit good, 125 skips it, any other up to 127 marks it bad, \
             and 128 or more stops the run.",
        )
        .color(t.text_dim),
    );
    ui.add_space(12.0);
    let valid = !command.trim().is_empty();
    buttons(ui, "Run", valid, enter, || {
        Action::Bisect(BisectAct::Run(command.trim().to_owned()))
    })
}
