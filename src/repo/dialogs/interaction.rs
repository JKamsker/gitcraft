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
        "Find the interaction",
        &format!(
            "For two branches that pass alone but fail together: tests subsets of their commits \
             {machine}, in a scratch worktree, until it finds the ones that fail together. Each \
             branch may have up to {MAX_CHANGES} commits since they diverged, and no merges."
        ),
        &t,
    );
    let mut no_focus = false;
    ui.label(RichText::new("The two branches:").color(t.text_dim));
    field(ui, &mut spec.a, "feature", &mut no_focus);
    field(ui, &mut spec.b, "main", &mut no_focus);
    ui.add_space(6.0);
    ui.label(RichText::new("Command (exit code 0 passes):").color(t.text_dim));
    field(
        ui,
        &mut spec.command,
        "cargo test, make check, ./test.sh…",
        focus,
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Tests at most:").color(t.text_dim));
        ui.add(egui::DragValue::new(&mut spec.budget).range(3..=500))
            .on_hover_text("Out of tests, it shows the smallest failing set it got to");
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("A failure is run again:").color(t.text_dim));
        ui.add(egui::DragValue::new(&mut spec.reruns).range(0..=5))
            .on_hover_text("A failure that doesn't repeat is set aside as flaky");
        ui.label(RichText::new("more time(s) to be sure").color(t.text_dim));
    });
    ui.add_space(12.0);
    let valid = usable(&spec.a)
        && usable(&spec.b)
        && spec.a.trim() != spec.b.trim()
        && !spec.command.trim().is_empty();
    buttons(ui, "Find", valid, enter, || {
        let mut spec = spec.clone();
        spec.a = spec.a.trim().to_owned();
        spec.b = spec.b.trim().to_owned();
        spec.command = spec.command.trim().to_owned();
        Action::Interaction(InteractionAct::Start(spec))
    })
}
