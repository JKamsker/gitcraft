//! The card above the graph while a branch made for an issue is checked out: the issue's number
//! and title, and its acceptance criteria, folded away with a click.

use egui::{RichText, Ui, vec2};

use super::IssueAct;
use super::criteria::{self, Line};
use super::link::IssueLink;
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How tall the criteria get before they scroll.
const CRITERIA_H: f32 = 150.0;

impl RepoTab {
    /// Draws the card of the checked-out branch's issue, when it has one, along the top of the
    /// central area; reads the branches' issues again first when the repository was read since.
    pub(in crate::repo) fn issue_card(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        self.refresh_issue_links(&ctx);
        let Some(branch) = self.refs.current.clone() else {
            return;
        };
        let Some(link) = self.issue_work.links.get(&branch) else {
            return;
        };
        let t = Tokens::get(&ctx);
        egui::Panel::top("repo_issue_card")
            .frame(
                egui::Frame::new()
                    .fill(t.card)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(egui::Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| card(ui, &branch, link, acts));
    }
}

fn card(ui: &mut Ui, branch: &str, link: &IssueLink, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    // Folded or not, per branch, and kept across restarts.
    let id = egui::Id::new(("issue-card-folded", branch));
    let mut folded = ui.data_mut(|d| *d.get_persisted_mut_or(id, false));
    ui.horizontal(|ui| {
        let chevron = if folded {
            "chevron-right"
        } else {
            "chevron-down"
        };
        let tip = if folded {
            "Show the acceptance criteria"
        } else {
            "Hide the acceptance criteria"
        };
        if icons::small_button(ui, chevron, tip).clicked() {
            folded = !folded;
        }
        ui.label(
            RichText::new(format!("#{}", link.number))
                .font(theme::mono(12.0))
                .color(t.accent),
        );
        ui.add(egui::Label::new(RichText::new(&link.title).font(theme::semibold(12.5))).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", "Forget which issue this branch is for").clicked() {
                acts.push(Action::Issue(IssueAct::Unlink(branch.to_owned())));
            }
            if !link.url.is_empty()
                && icons::small_button(ui, "external-link", "Open the issue in the browser")
                    .clicked()
            {
                acts.push(Action::OpenUrl(link.url.clone()));
            }
        });
    });
    ui.data_mut(|d| d.insert_persisted(id, folded));
    if folded {
        return;
    }
    let lines = criteria::lines(&link.criteria);
    if lines.is_empty() {
        ui.label(
            RichText::new("The issue says nothing more than its title.")
                .size(11.5)
                .color(t.text_faint),
        );
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt(("issue-card-criteria", branch))
        .max_height(CRITERIA_H)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for line in &lines {
                criteria_line(ui, line);
            }
        });
}

/// One line of the criteria: a task with its box, ticked or not, an item of a list, or text.
fn criteria_line(ui: &mut Ui, line: &Line) {
    let t = Tokens::get(ui.ctx());
    let text = |s: &str| RichText::new(s).size(12.0).color(t.text_dim);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        match line {
            Line::Task { done, text: what } => {
                task_box(ui, *done);
                let what = text(what);
                ui.label(if *done {
                    what.strikethrough().color(t.text_faint)
                } else {
                    what
                });
            }
            Line::Bullet(what) => {
                ui.label(RichText::new("•").color(t.text_faint));
                ui.label(text(what));
            }
            Line::Text(what) => {
                ui.label(text(what));
            }
            Line::Blank => {
                ui.add_space(2.0);
            }
        }
    });
}

/// A task's box: a check in it when it's done.
fn task_box(ui: &mut Ui, done: bool) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
    let square = egui::Rect::from_center_size(rect.center(), vec2(12.0, 12.0));
    if done {
        ui.painter()
            .rect_filled(square, t.radius_sm, t.success.gamma_multiply(0.25));
        icons::paint(ui, square, "check", 11.0, t.success);
    } else {
        ui.painter().rect_stroke(
            square,
            t.radius_sm,
            egui::Stroke::new(1.0, t.field_border),
            egui::StrokeKind::Inside,
        );
    }
}
