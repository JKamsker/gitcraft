//! "Remove this, keep later work" in the timeline's detail: what it takes out and plays again,
//! the new branch's name, and how it went.

use egui::{RichText, Stroke, Ui};

use super::Entry;
use super::remove::{self, Outcome, Plan};
use super::remove_tab::RemoveState;
use super::view::TimelineAct;
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// The section for `entry`, when it is one that only made commits on a branch.
    pub(super) fn remove_section(
        &self,
        ui: &mut Ui,
        entry: &Entry,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some((branch, ..)) = remove::candidate(entry) else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let state = self
            .timeline_view
            .as_ref()
            .and_then(|v| v.remove.as_ref())
            .filter(|s| s.id() == entry.id);
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-timeline-remove-section-e0e6aea3",
            ))
            .font(theme::semibold(10.5))
            .color(t.text_faint),
        );
        ui.add_space(2.0);
        let Some(state) = state else {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-timeline-remove-section-08b980d5",
                    &[("branch", (branch).clone().into())],
                ))
                .size(12.0)
                .color(t.text_dim),
            );
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-timeline-remove-section-013b5c2e",
                    )),
                )
                .clicked()
            {
                acts.push(Action::Timeline(TimelineAct::AskRemove(entry.id)));
            }
            return;
        };
        egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| match state {
                RemoveState::Planning(_) => {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-timeline-remove-section-17123003",
                        ))
                        .color(t.text_dim),
                    );
                }
                RemoveState::Running { .. } => {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-timeline-remove-section-50ad636b",
                        ))
                        .color(t.text_dim),
                    );
                }
                RemoveState::Failed { error, .. } => {
                    ui.label(
                        RichText::new(crate::repo::activity::display_workflow_error(error))
                            .color(t.danger),
                    );
                    close_button(ui, acts);
                }
                RemoveState::Planned { plan, name } => planned(ui, plan, name, idle, acts),
                RemoveState::Finished {
                    outcome, confirm, ..
                } => finished(ui, outcome, *confirm, idle, acts),
            });
    }
}

/// What it would take out and play again, the name, and the button that does it.
fn planned(ui: &mut Ui, plan: &Plan, name: &str, idle: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-timeline-planned-464cf7b4",
            &[("plan-removed-total-1", (plan.removed_total).clone().into())],
        ))
        .color(t.text),
    );
    commits(ui, &plan.removed, plan.removed_total);
    ui.add_space(4.0);
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-timeline-planned-bc5f9685",
            &[("plan-later-len-1", (plan.later.len()).clone().into())],
        ))
        .color(t.text),
    );
    if plan.later.is_empty() {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-timeline-planned-32b970db",
                &[("plan-branch-1", (plan.branch).clone().into())],
            ))
            .size(12.0)
            .color(t.text_faint),
        );
    } else {
        commits(ui, &plan.later_listed, plan.later.len());
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-timeline-planned-7f445154",
            ))
            .color(t.text_dim),
        );
        let mut typed = name.to_owned();
        if ui
            .add(egui::TextEdit::singleline(&mut typed).desired_width(240.0))
            .changed()
        {
            acts.push(Action::Timeline(TimelineAct::RemoveName(typed)));
        }
    });
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-timeline-planned-5c26ac7a",
            &[("plan-branch-1", (plan.branch).clone().into())],
        ))
        .size(11.5)
        .color(t.text_faint),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if widgets::primary_button(
            ui,
            crate::i18n::text("repo-workflows-timeline-planned-9ae4d55a"),
            idle,
            120.0,
        )
        .clicked()
        {
            acts.push(Action::Timeline(TimelineAct::RunRemove));
        }
        close_button(ui, acts);
    });
}

/// How it went, and what can be done next.
fn finished(ui: &mut Ui, outcome: &Outcome, confirm: bool, idle: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    match outcome {
        Outcome::Done { branch, tip } => {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-timeline-finished-9f36aac7",
                    &[
                        ("branch", (branch).clone().into()),
                        ("short-tip-1", (short(tip)).clone().into()),
                    ],
                ))
                .color(t.success),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        idle,
                        egui::Button::new(crate::i18n::message(
                            "repo-workflows-timeline-finished-c36998e0",
                            &[("branch", (branch).clone().into())],
                        )),
                    )
                    .clicked()
                {
                    acts.push(Action::Checkout(branch.clone()));
                }
                close_button(ui, acts);
            });
        }
        Outcome::Conflict {
            branch,
            folder,
            commit,
            summary,
            files,
            left,
            ..
        } => {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-timeline-finished-69c5b35c",
                    &[
                        ("short-commit-1", (short(commit)).clone().into()),
                        ("summary", (summary).clone().into()),
                    ],
                ))
                .color(t.warning),
            );
            for file in files {
                ui.label(
                    RichText::new(file)
                        .font(theme::mono(12.0))
                        .color(t.text_dim),
                );
            }
            if files.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-timeline-finished-c8854cb8",
                    ))
                    .color(t.text_faint),
                );
            }
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-timeline-finished-34a13b7e",
                    &[
                        ("branch", (branch).clone().into()),
                        (
                            "crate-remote-display-folder-1",
                            (crate::remote::display(folder)).clone().into(),
                        ),
                        (
                            "match-left-0-string-new-1-1-more-2",
                            (match left {
                                0 => String::new(),
                                1 => crate::i18n::text("repo-workflows-timeline-finished-635b324e")
                                    .to_owned(),
                                n => crate::i18n::message(
                                    "repo-workflows-timeline-finished-c9361f4d",
                                    &[("n", (n).clone().into())],
                                ),
                            })
                            .clone()
                            .into(),
                        ),
                    ],
                ))
                .size(12.0)
                .color(t.text_dim),
            );
            if confirm {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-timeline-finished-57272758",
                        &[("branch", (branch).clone().into())],
                    ))
                    .color(t.text),
                );
                ui.horizontal(|ui| {
                    if widgets::primary_button(
                        ui,
                        crate::i18n::text("repo-workflows-timeline-finished-eb1a70e3"),
                        idle,
                        80.0,
                    )
                    .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::DiscardRemove));
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-timeline-finished-19766ed6",
                        ))
                        .clicked()
                    {
                        acts.push(Action::Timeline(TimelineAct::AskDiscard(false)));
                    }
                });
                return;
            }
            ui.horizontal(|ui| {
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-timeline-finished-65b04ffc",
                    ))
                    .clicked()
                {
                    acts.push(Action::OpenWorktree(folder.clone()));
                }
                if ui
                    .add_enabled(
                        idle,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-timeline-finished-475e7702",
                        )),
                    )
                    .on_hover_text(crate::i18n::message(
                        "repo-workflows-timeline-finished-f63a600e",
                        &[("branch", (branch).clone().into())],
                    ))
                    .clicked()
                {
                    acts.push(Action::Timeline(TimelineAct::AskDiscard(true)));
                }
                close_button(ui, acts);
            });
        }
    }
}

/// A list of commits, and how many more there are.
fn commits(ui: &mut Ui, list: &[(String, String)], total: usize) {
    let t = Tokens::get(ui.ctx());
    for (hash, summary) in list {
        ui.label(
            RichText::new(format!("{}  {summary}", short(hash)))
                .font(theme::mono(12.0))
                .color(t.text_dim),
        );
    }
    if total > list.len() {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-timeline-commits-683768ab",
                &[("total-list-len-1", (total - list.len()).clone().into())],
            ))
            .color(t.text_faint),
        );
    }
}

fn close_button(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui
        .button(crate::i18n::text(
            "repo-workflows-timeline-close-button-7d9eb7ac",
        ))
        .clicked()
    {
        acts.push(Action::Timeline(TimelineAct::CloseRemove));
    }
}
