//! The dialog that starts a search for the commits of two branches that fail together.

use egui::{RichText, Ui};

use crate::git::{InteractionSpec, MAX_CHANGES};
use crate::repo::Action;
use crate::repo::interaction::InteractionAct;
use crate::theme::Tokens;

use super::{Outcome, buttons, field, title, usable};

/// `spec` as it is being edited; `machine` is where the tests run.
pub(super) fn find_interaction(
    ui: &mut Ui,
    enter: bool,
    machine: &str,
    spec: &mut InteractionSpec,
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-1338"),
        &crate::i18n::message(
            "repo-core-1339",
            &[
                ("machine", machine.to_string().into()),
                ("MAX-CHANGES", format!("{}", MAX_CHANGES).into()),
            ],
        ),
        &t,
    );
    let mut no_focus = false;
    ui.label(RichText::new(crate::i18n::text("repo-core-1340")).color(t.text_dim));
    field(ui, &mut spec.a, "feature", &mut no_focus);
    field(ui, &mut spec.b, "main", &mut no_focus);
    ui.add_space(6.0);
    ui.label(RichText::new(crate::i18n::text("repo-core-1342")).color(t.text_dim));
    field(
        ui,
        &mut spec.command,
        "cargo test, make check, ./test.sh…",
        focus,
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::i18n::text("repo-core-1343")).color(t.text_dim));
        ui.add(egui::DragValue::new(&mut spec.budget).range(3..=500))
            .on_hover_text(crate::i18n::text("repo-core-1344"));
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::i18n::text("repo-core-1345")).color(t.text_dim));
        ui.add(egui::DragValue::new(&mut spec.reruns).range(0..=5))
            .on_hover_text(crate::i18n::text("repo-core-1346"));
        ui.label(RichText::new(crate::i18n::text("repo-core-1347")).color(t.text_dim));
    });
    ui.add_space(12.0);
    let valid = usable(&spec.a)
        && usable(&spec.b)
        && spec.a.trim() != spec.b.trim()
        && !spec.command.trim().is_empty();
    buttons(
        ui,
        crate::i18n::text("repo-core-1348"),
        valid,
        enter,
        || {
            let mut spec = spec.clone();
            spec.a = spec.a.trim().to_owned();
            spec.b = spec.b.trim().to_owned();
            spec.command = spec.command.trim().to_owned();
            Action::Interaction(InteractionAct::Start(spec))
        },
    )
}
