//! The dialog that tests a range of commits: which ones, with what command, how many at once.

use egui::{RichText, Ui};

use crate::git::MAX_WORKERS;
use crate::repo::Action;
use crate::repo::test_lane::{LaneAct, StartTests};
use crate::theme::Tokens;

use super::{Outcome, buttons, field, title, usable};

/// `start` as it is being edited; `stored` is the repository's test command, `machine` where
/// it runs ("on this machine", "on WSL · Ubuntu").
pub(super) fn test_commits(
    ui: &mut Ui,
    enter: bool,
    machine: &str,
    start: &mut StartTests,
    stored: Option<&str>,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        "Test commits",
        &format!(
            "Runs the command {machine} at each commit, in scratch worktrees, in the background, \
             and shows a pass / fail dot by each. Results are kept by tree, as git-test keeps them."
        ),
        &t,
    );
    ui.label(RichText::new("Commits (base..tip, or one commit):").color(t.text_dim));
    let mut no_focus = false;
    field(ui, &mut start.range, "main..feature", &mut no_focus);
    ui.add_space(6.0);
    ui.label(RichText::new("Command, run at the top of the worktree:").color(t.text_dim));
    field(
        ui,
        &mut start.command,
        "cargo test, make check, ./test.sh…",
        focus,
    );
    ui.label(RichText::new("Exit code 0 passes; any other fails.").color(t.text_faint));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("At once:").color(t.text_dim));
        ui.add(egui::DragValue::new(&mut start.workers).range(1..=MAX_WORKERS))
            .on_hover_text(
                "How many commits are tested at the same time, each in its own worktree",
            );
    });
    ui.checkbox(&mut start.retest, "Test again those with a result");
    let command = start.command.trim();
    let changed = stored.filter(|s| !command.is_empty() && *s != command);
    if let Some(old) = changed {
        ui.checkbox(
            &mut start.forget,
            format!("Forget the results of the command before (`{old}`)"),
        );
    }
    ui.add_space(12.0);
    let valid = usable(&start.range) && !command.is_empty();
    buttons(ui, "Test", valid, enter, || {
        let mut start = start.clone();
        start.range = start.range.trim().to_owned();
        start.command = start.command.trim().to_owned();
        start.forget = start.forget && changed.is_some();
        Action::TestLane(LaneAct::Start(start))
    })
}
