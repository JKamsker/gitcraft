//! The first steps of a new repository, drawn in the graph area: make the first commit, add a
//! remote, publish; those done ticked off.

use egui::{RichText, Ui};

use super::GuideAct;
use super::empty_commit::EMPTY_COMMIT_MESSAGE;
use super::first_steps::{FirstSteps, StepState};
use crate::icons;
use crate::repo::{Action, Dialog, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// Width of the guide's card.
const CARD_W: f32 = 440.0;

impl RepoTab {
    /// The first steps, in a card centred in what is left of the graph area: in the middle
    /// when there are no commits, under them otherwise.
    pub(in crate::repo) fn first_steps_view(
        &mut self,
        ui: &mut Ui,
        steps: FirstSteps,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        // Under the graph's rows, which are laid out with none.
        ui.spacing_mut().item_spacing.y = 3.0;
        let idle = self.busy_op.is_none();
        let room = ui.available_height();
        let remotes: Vec<&str> = self.remotes.iter().map(|r| r.name.as_str()).collect();
        let branch = self.refs.current.clone();
        let mut hide = false;
        egui::ScrollArea::vertical()
            .id_salt("first_steps")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    let top = if steps.empty {
                        (room - 330.0) / 2.0
                    } else {
                        0.0
                    };
                    ui.add_space(top.max(18.0));
                    egui::Frame::new()
                        .fill(t.card)
                        .stroke(egui::Stroke::new(1.0, t.field_border))
                        .corner_radius(t.radius_lg)
                        .inner_margin(egui::Margin::same(20))
                        .show(ui, |ui| {
                            ui.set_width(CARD_W.min(ui.available_width()));
                            ui.vertical(|ui| {
                                header(ui, &t, steps.empty);
                                first_commit_step(ui, steps, idle, acts);
                                remote_step(ui, steps.add_remote, &remotes, idle, acts);
                                publish_step(ui, steps.publish, branch.as_deref(), idle, acts);
                                if !steps.empty {
                                    ui.add_space(6.0);
                                    hide = ui
                                        .link(RichText::new("Hide these steps").small())
                                        .on_hover_text(
                                            "Hidden until this repository is opened again",
                                        )
                                        .clicked();
                                }
                            });
                        });
                    ui.add_space(18.0);
                });
            });
        if hide {
            self.guide.steps_hidden = true;
        }
    }
}

/// The card's title and what it is about.
fn header(ui: &mut Ui, t: &Tokens, empty: bool) {
    let (title, sub) = if empty {
        (
            "This repository has no commits yet",
            "Three steps to get it going:",
        )
    } else {
        (
            "Next steps for this repository",
            "It is only on this machine so far:",
        )
    };
    ui.label(
        RichText::new(title)
            .font(theme::semibold(16.0))
            .color(t.text),
    );
    ui.add_space(2.0);
    ui.label(RichText::new(sub).color(t.text_dim));
    ui.add_space(10.0);
}

/// Make the first commit: write the message of the changes there are, or make an empty one.
fn first_commit_step(ui: &mut Ui, steps: FirstSteps, idle: bool, acts: &mut Vec<Action>) {
    let explain = match steps.first_commit {
        StepState::Done => "Done.",
        _ if steps.nothing_to_commit => {
            "Nothing to commit yet: files put in the repository's folder show up at the top of \
             the graph by themselves. Or start with an empty commit."
        }
        _ => "Your files are ready: write a summary and commit them.",
    };
    let can = idle && steps.first_commit == StepState::Todo;
    step(
        ui,
        1,
        "Make the first commit",
        steps.first_commit,
        explain,
        |ui| {
            if steps.nothing_to_commit {
                if ui
                    .add_enabled(can, egui::Button::new("Make an empty commit…"))
                    .on_hover_text(format!("A commit with no files, “{EMPTY_COMMIT_MESSAGE}”"))
                    .clicked()
                {
                    acts.push(Action::Guide(GuideAct::EmptyCommit));
                }
            } else if widgets::primary_button(ui, "Write the message", can, 0.0)
                .on_hover_text("Opens the changes in the right pane, at the commit summary")
                .clicked()
            {
                acts.push(Action::Guide(GuideAct::StartCommit));
            }
        },
    );
}

/// Add a remote: where the repository is published.
fn remote_step(
    ui: &mut Ui,
    state: StepState,
    remotes: &[&str],
    idle: bool,
    acts: &mut Vec<Action>,
) {
    let done = format!("Done: {}.", remotes.join(", "));
    let explain = match state {
        StepState::Done => done.as_str(),
        _ => "Where it will be published, such as a new, empty repository on a forge.",
    };
    step(ui, 2, "Add a remote", state, explain, |ui| {
        if ui
            .add_enabled(idle, egui::Button::new("Add a remote…"))
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::remote(None)));
        }
    });
}

/// Publish: push the branch and track it.
fn publish_step(
    ui: &mut Ui,
    state: StepState,
    branch: Option<&str>,
    idle: bool,
    acts: &mut Vec<Action>,
) {
    let todo = match branch {
        Some(b) => format!("Push {b} to the remote and track it, so Push and Pull just work."),
        None => "Push the branch to the remote and track it, so Push and Pull just work.".into(),
    };
    let explain = match state {
        StepState::Done => "Done.",
        _ => todo.as_str(),
    };
    step(ui, 3, "Publish", state, explain, |ui| {
        if ui
            .add_enabled(
                idle && state == StepState::Todo,
                egui::Button::new("Publish"),
            )
            .clicked()
        {
            acts.push(Action::Push);
        }
    });
}

/// One step: its number (a tick once done), title and explanation, and while it is to do its
/// button, disabled with the reason under it while it can't be done yet.
fn step(
    ui: &mut Ui,
    n: usize,
    title: &str,
    state: StepState,
    explain: &str,
    button: impl FnOnce(&mut Ui),
) {
    let t = Tokens::get(ui.ctx());
    let done = state == StepState::Done;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
        let color = if done { t.success } else { t.accent };
        ui.painter()
            .circle_filled(rect.center(), 11.0, color.gamma_multiply(0.18));
        if done {
            icons::paint(ui, rect.shrink(4.0), "check", 14.0, color);
        } else {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                n.to_string(),
                theme::semibold(12.0),
                color,
            );
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let title_color = if done { t.text_dim } else { t.text };
            ui.label(
                RichText::new(title)
                    .font(theme::medium(13.0))
                    .color(title_color),
            );
            ui.add(egui::Label::new(RichText::new(explain).color(t.text_faint).small()).wrap());
            if !done {
                button(ui);
            }
            if let StepState::Blocked(reason) = state {
                ui.label(RichText::new(reason).color(t.text_faint).small());
            }
        });
    });
    ui.add_space(10.0);
}
