//! A window with what the test of one commit printed (its last lines), how it ended, and Copy.

use egui::RichText;

use super::LaneAct;
use crate::git::{TestEnd, Verdict};
use crate::repo::{Action, RepoTab, short};
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(in crate::repo) fn test_output_window(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(tree) = &self.test_lane.output_open else {
            return;
        };
        let Some(kept) = self.test_lane.outputs.get(tree) else {
            return;
        };
        let t = Tokens::get(ctx);
        let mut open = true;
        egui::Window::new(crate::i18n::message(
            "repo-workflows-test_lane-test-output-window-3dddddc9",
            &[("short-kept-commit-1", (short(&kept.commit)).into())],
        ))
        .id(egui::Id::new(("gitcraft-test-output", self.path.clone())))
        .open(&mut open)
        .collapsible(false)
        .default_size([560.0, 360.0])
        .show(ctx, |ui| {
            let (said, color) = end_text(&kept.end);
            ui.horizontal(|ui| {
                ui.label(RichText::new(said).color(color_of(&t, color)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-test_lane-test-output-window-e21f935f",
                        ))
                        .clicked()
                    {
                        ui.ctx().copy_text(kept.lines.join("\n"));
                    }
                });
            });
            if kept.lines.len() >= crate::git::KEPT_LINES {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-test_lane-test-output-window-0e95604a",
                        &[("crate-git-kept-lines-1", (crate::git::KEPT_LINES).into())],
                    ))
                    .color(t.text_dim),
                );
            }
            ui.separator();
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .auto_shrink(false)
                .show(ui, |ui| {
                    // A test that couldn't run printed nothing: the line above says why.
                    if kept.lines.is_empty() && !matches!(kept.end, TestEnd::Failed(_)) {
                        ui.label(
                            RichText::new(crate::i18n::text(
                                "repo-workflows-test_lane-test-output-window-8497354d",
                            ))
                            .color(t.text_dim),
                        );
                    }
                    for line in &kept.lines {
                        ui.label(RichText::new(line).font(theme::mono(11.5)));
                    }
                });
        });
        if !open {
            acts.push(Action::TestLane(LaneAct::CloseOutput));
        }
    }
}

/// Which colour a line about the end has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Good,
    Bad,
    Warning,
}

fn color_of(t: &Tokens, tone: Tone) -> egui::Color32 {
    match tone {
        Tone::Good => t.success,
        Tone::Bad => t.danger,
        Tone::Warning => t.warning,
    }
}

/// How a test ended, in a line.
pub(super) fn end_text(end: &TestEnd) -> (String, Tone) {
    match end {
        TestEnd::Ran {
            verdict: Verdict::Good,
            ..
        }
        | TestEnd::Known(Verdict::Good) => (
            crate::i18n::text("repo-workflows-test_lane-end-text-e1c45645").to_owned(),
            Tone::Good,
        ),
        TestEnd::Ran {
            verdict: Verdict::Bad,
            code: Some(code),
        } => (
            crate::i18n::message(
                "repo-workflows-test_lane-end-text-07f51b84",
                &[("code", (*(code)).into())],
            ),
            Tone::Bad,
        ),
        TestEnd::Ran {
            verdict: Verdict::Bad,
            code: None,
        } => (
            crate::i18n::text("repo-workflows-test_lane-end-text-70fd8e5f").to_owned(),
            Tone::Bad,
        ),
        TestEnd::Known(Verdict::Bad) => (
            crate::i18n::text("repo-workflows-test_lane-end-text-b971f3eb").to_owned(),
            Tone::Bad,
        ),
        TestEnd::Failed(e) => (
            crate::i18n::message(
                "repo-workflows-test_lane-end-text-83ee6541",
                &[("e", (e).clone().into())],
            ),
            Tone::Warning,
        ),
    }
}
