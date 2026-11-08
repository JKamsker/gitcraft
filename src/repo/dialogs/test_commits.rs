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
        crate::i18n::text("repo-core-1508"),
        &crate::i18n::message(
            "repo-core-1509",
            &[("machine", format!("{}", machine).into())],
        ),
        &t,
    );
    ui.label(RichText::new(crate::i18n::text("repo-core-1510")).color(t.text_dim));
    let mut no_focus = false;
    field(ui, &mut start.range, "main..feature", &mut no_focus);
    ui.add_space(6.0);
    ui.label(RichText::new(crate::i18n::text("repo-core-1512")).color(t.text_dim));
    field(
        ui,
        &mut start.command,
        "cargo test, make check, ./test.sh…",
        focus,
    );
    ui.label(RichText::new(crate::i18n::text("repo-core-1513")).color(t.text_faint));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::i18n::text("repo-core-1514")).color(t.text_dim));
        ui.add(egui::DragValue::new(&mut start.workers).range(1..=MAX_WORKERS))
            .on_hover_text(crate::i18n::text("repo-core-1515"));
    });
    if remote && start.workers > REMOTE_WORKERS {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-core-1516",
                &[("REMOTE-WORKERS", format!("{}", REMOTE_WORKERS).into())],
            ))
            .color(t.text_faint),
        );
    }
    ui.checkbox(&mut start.retest, crate::i18n::text("repo-core-1517"));
    let command = start.command.trim();
    let changed = stored
        .map(str::trim)
        .filter(|s| !command.is_empty() && *s != command);
    if let Some(old) = changed {
        ui.checkbox(
            &mut start.forget,
            crate::i18n::message("repo-core-1518", &[("old", format!("{}", old).into())]),
        );
        if !start.forget {
            ui.label(RichText::new(crate::i18n::text("repo-core-1519")).color(t.warning));
        }
    }
    ui.add_space(12.0);
    let why_not = if command.is_empty() {
        Some(crate::i18n::text("repo-core-1520"))
    } else if !usable(&start.range) {
        Some(crate::i18n::text("repo-core-1521"))
    } else {
        None
    };
    if let Some(why) = why_not {
        ui.label(RichText::new(why).color(t.text_faint));
        ui.add_space(4.0);
    }
    let valid = why_not.is_none();
    buttons(
        ui,
        crate::i18n::text("repo-core-0554"),
        valid,
        enter,
        || {
            let mut start = start.clone();
            start.range = start.range.trim().to_owned();
            start.command = start.command.trim().to_owned();
            start.forget = start.forget && changed.is_some();
            Action::TestLane(LaneAct::Start(start))
        },
    )
}
