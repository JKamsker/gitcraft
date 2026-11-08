//! The dialog that tests a range of commits: which ones, with what command, how many at once.

use egui::{RichText, Ui};

use crate::git::MAX_WORKERS;
use crate::repo::Action;
use crate::repo::test_lane::{LaneAct, REMOTE_WORKERS, StartTests};
use crate::theme::Tokens;

use super::{Outcome, buttons, field, title, usable};

/// `start` as it is being edited; `stored` is the repository's test command, `machine` where
/// it runs ("on this machine", "on WSL · Ubuntu"), `remote` when that is another one.
pub(super) fn test_commits(
    ui: &mut Ui,
    enter: bool,
    machine: &str,
    remote: bool,
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
             and shows a pass / fail square by each. Results are kept by tree, as git-test keeps \
             them."
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
        ui.label(RichText::new("Commits at once:").color(t.text_dim));
        ui.add(egui::DragValue::new(&mut start.workers).range(1..=MAX_WORKERS))
            .on_hover_text(
                "How many commits are tested at the same time, each in its own worktree",
            );
    });
    if remote && start.workers > REMOTE_WORKERS {
        ui.label(
            RichText::new(format!(
                "At most {REMOTE_WORKERS} at once on another machine, to keep its connections \
                 free for the rest of the window"
            ))
            .color(t.text_faint),
        );
    }
    ui.checkbox(&mut start.retest, "Test again commits that have a result");
    let command = start.command.trim();
    let changed = stored
        .map(str::trim)
        .filter(|s| !command.is_empty() && *s != command);
    if let Some(old) = changed {
        ui.checkbox(
            &mut start.forget,
            format!("Forget the results of the old command (`{old}`); this can't be undone"),
        );
        if !start.forget {
            ui.label(
                RichText::new(
                    "Its results stay, and show as the new command's: commits with one are \
                     not tested again unless you tick Test again above",
                )
                .color(t.warning),
            );
        }
    }
    ui.add_space(12.0);
    let why_not = if command.is_empty() {
        Some("Type the command to test with")
    } else if !usable(&start.range) {
        Some("Name the commits to test: a range can't be empty or start with -")
    } else {
        None
    };
    if let Some(why) = why_not {
        ui.label(RichText::new(why).color(t.text_faint));
        ui.add_space(4.0);
    }
    let valid = why_not.is_none();
    buttons(ui, crate::i18n::text("repo-core-0554"), valid, enter, || {
        let mut start = start.clone();
        start.range = start.range.trim().to_owned();
        start.command = start.command.trim().to_owned();
        start.forget = start.forget && changed.is_some();
        Action::TestLane(LaneAct::Start(start))
    })
}
