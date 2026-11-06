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
            RichText::new("REMOVE THIS, KEEP LATER WORK")
                .font(theme::semibold(10.5))
                .color(t.text_faint),
        );
        ui.add_space(2.0);
        let Some(state) = state else {
            ui.label(
                RichText::new(format!(
                    "Plays the commits made on {branch} since this again without it, on a new \
                     branch; {branch} stays as it is."
                ))
                .size(12.0)
                .color(t.text_dim),
            );
            if ui
                .add_enabled(idle, egui::Button::new("Remove this, keep later work…"))
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
                    ui.label(RichText::new("Reading the commits…").color(t.text_dim));
                }
                RemoveState::Running { .. } => {
                    ui.label(RichText::new("Playing the later commits again…").color(t.text_dim));
                }
                RemoveState::Failed { error, .. } => {
                    ui.label(RichText::new(error).color(t.danger));
                    close_button(ui, acts);
                }
                RemoveState::Planned { plan, name } => planned(ui, plan, name, idle, acts),
                RemoveState::Finished { outcome, .. } => finished(ui, outcome, idle, acts),
            });
    }
}

/// What it would take out and play again, the name, and the button that does it.
fn planned(ui: &mut Ui, plan: &Plan, name: &str, idle: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(format!("Taken out ({}):", plan.removed_total)).color(t.text));
    commits(ui, &plan.removed, plan.removed_total);
    ui.add_space(4.0);
    ui.label(RichText::new(format!("Played again ({}):", plan.later.len())).color(t.text));
    if plan.later.is_empty() {
        ui.label(
            RichText::new(format!(
                "Nothing was committed on {} since: the new branch is where it was before this.",
                plan.branch
            ))
            .size(12.0)
            .color(t.text_faint),
        );
    } else {
        commits(ui, &plan.later_listed, plan.later.len());
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("New branch").color(t.text_dim));
        let mut typed = name.to_owned();
        if ui
            .add(egui::TextEdit::singleline(&mut typed).desired_width(240.0))
            .changed()
        {
            acts.push(Action::Timeline(TimelineAct::RemoveName(typed)));
        }
    });
    ui.label(
        RichText::new(format!(
            "{} stays as it is. The commits are played in a worktree beside the repository, \
             removed again once they all apply.",
            plan.branch
        ))
        .size(11.5)
        .color(t.text_faint),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, "Make the branch", idle, 120.0).clicked() {
            acts.push(Action::Timeline(TimelineAct::RunRemove));
        }
        close_button(ui, acts);
    });
}

/// How it went, and what can be done next.
fn finished(ui: &mut Ui, outcome: &Outcome, idle: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    match outcome {
        Outcome::Done { branch, tip } => {
            ui.label(
                RichText::new(format!("{branch} is made, at {}.", short(tip))).color(t.success),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(idle, egui::Button::new(format!("Check out {branch}")))
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
                RichText::new(format!(
                    "{} {summary} doesn't apply without it: conflicts in",
                    short(commit)
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
            ui.label(
                RichText::new(format!(
                    "{branch} has the commits before it, in the worktree {}. Resolve the \
                     conflicts there and continue the cherry-pick{}.",
                    crate::remote::display(folder),
                    match left {
                        0 => String::new(),
                        1 => " (1 more commit follows)".to_owned(),
                        n => format!(" ({n} more commits follow)"),
                    }
                ))
                .size(12.0)
                .color(t.text_dim),
            );
            ui.horizontal(|ui| {
                if ui.button("Open the worktree").clicked() {
                    acts.push(Action::OpenWorktree(folder.clone()));
                }
                if ui
                    .add_enabled(idle, egui::Button::new("Discard this attempt"))
                    .on_hover_text(format!("Removes the worktree and the branch {branch}"))
                    .clicked()
                {
                    acts.push(Action::Timeline(TimelineAct::DiscardRemove));
                }
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
        ui.label(RichText::new(format!("and {} more", total - list.len())).color(t.text_faint));
    }
}

fn close_button(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui.button("Close").clicked() {
        acts.push(Action::Timeline(TimelineAct::CloseRemove));
    }
}
