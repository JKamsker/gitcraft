//! The card above the graph while a branch made for an issue is checked out: the issue's number
//! and title, and its acceptance criteria (or what stands in for them) as of the day the issue
//! was read, read again or folded away with a click.

use egui::{RichText, Ui, vec2};

use super::IssueAct;
use super::criteria::{self, Line, Tick};
use super::link::IssueLink;
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How tall the criteria get before they scroll.
const CRITERIA_H: f32 = 150.0;

impl RepoTab {
    /// Draws the card of the checked-out branch's issue, when it has one, along the top of the
    /// central area (the links are read with the rest of the repository).
    pub(in crate::repo) fn issue_card(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(branch) = self.refs.current.clone() else {
            return;
        };
        let Some(link) = self.issue_work.links.get(&branch) else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        // Folded or not, per repository and branch, and kept across restarts.
        let fold = egui::Id::new(("issue-card-folded", self.repo_key(), &branch));
        let refreshing = self.issue_work.refreshing.as_deref() == Some(branch.as_str());
        egui::Panel::top("repo_issue_card")
            .frame(
                egui::Frame::new()
                    .fill(t.card)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(egui::Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| {
                card(ui, &branch, link, fold, refreshing, acts);
            });
    }
}

fn card(
    ui: &mut Ui,
    branch: &str,
    link: &IssueLink,
    fold: egui::Id,
    refreshing: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let mut folded = ui.data_mut(|d| *d.get_persisted_mut_or(fold, false));
    let what = link.kept.what();
    ui.horizontal(|ui| {
        let chevron = if folded {
            "chevron-right"
        } else {
            "chevron-down"
        };
        let tip = if folded {
            crate::i18n::message(
                "repo-workflows-issue_work-card-5fa29511",
                &[("what", (what).into())],
            )
        } else {
            crate::i18n::message(
                "repo-workflows-issue_work-card-1c245f79",
                &[("what", (what).into())],
            )
        };
        if icons::small_button(ui, chevron, &tip).clicked() {
            folded = !folded;
        }
        ui.label(
            RichText::new(link.key())
                .font(theme::mono(12.0))
                .color(t.accent),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(
                ui,
                "x",
                crate::i18n::text("repo-workflows-issue_work-card-fb6318e9"),
            )
            .clicked()
            {
                acts.push(Action::Issue(IssueAct::Unlink(branch.to_owned())));
            }
            if !link.url.is_empty()
                && icons::small_button(
                    ui,
                    "external-link",
                    crate::i18n::text("repo-workflows-issue_work-card-7fe3e1e0"),
                )
                .clicked()
            {
                acts.push(Action::OpenUrl(link.url.clone()));
            }
            // What the card shows is the issue as it was read, not as it is.
            let (read, when) = if link.read.is_empty() {
                (
                    crate::i18n::text("repo-workflows-issue_work-card-1887e67c").to_owned(),
                    crate::i18n::text("repo-workflows-issue_work-card-6d2189c3").to_owned(),
                )
            } else {
                (
                    crate::i18n::message(
                        "repo-workflows-issue_work-card-9b1e287d",
                        &[("link-read-1", (link.read).clone().into())],
                    ),
                    crate::i18n::message(
                        "repo-workflows-issue_work-card-53611908",
                        &[("link-read-1", (link.read).clone().into())],
                    ),
                )
            };
            if refreshing {
                ui.add(crate::spinner::Spinner::new().size(12.0));
            } else if icons::small_button(
                ui,
                "refresh-cw",
                &crate::i18n::message(
                    "repo-workflows-issue_work-card-1e7fbd27",
                    &[("when", (when).clone().into())],
                ),
            )
            .clicked()
            {
                acts.push(Action::Issue(IssueAct::Refresh(branch.to_owned())));
            }
            ui.label(RichText::new(read).size(11.0).color(t.text_faint));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&link.title).font(theme::semibold(12.5)))
                        .truncate(),
                );
            });
        });
    });
    ui.data_mut(|d| d.insert_persisted(fold, folded));
    if folded {
        return;
    }
    let lines = criteria::lines(&link.criteria);
    if lines.is_empty() {
        ui.label(
            RichText::new(if link.kept == criteria::Kept::Criteria {
                crate::i18n::text("repo-workflows-issue_work-card-57fa7fe2")
            } else {
                crate::i18n::text("repo-workflows-issue_work-card-b9210e97")
            })
            .size(11.5)
            .color(t.text_faint),
        );
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt(("issue-card-criteria", fold))
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
            Line::Task { tick, text: what } => {
                task_box(ui, *tick);
                let what = text(&criteria::plain(what));
                ui.label(match tick {
                    Tick::Open => what,
                    Tick::Done | Tick::Dropped => what.strikethrough().color(t.text_faint),
                });
            }
            Line::Bullet(what) => {
                ui.label(RichText::new("•").color(t.text_faint));
                ui.label(text(&criteria::plain(what)));
            }
            Line::Text(what) => {
                ui.label(text(&criteria::plain(what)));
            }
            Line::Blank => {
                ui.add_space(2.0);
            }
        }
    });
}

/// A task's box: a check in it when it's done, a dash when it no longer applies.
fn task_box(ui: &mut Ui, tick: Tick) {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
    let square = egui::Rect::from_center_size(rect.center(), vec2(12.0, 12.0));
    if tick == Tick::Dropped {
        ui.painter().rect_stroke(
            square,
            t.radius_sm,
            egui::Stroke::new(1.0, t.field_border),
            egui::StrokeKind::Inside,
        );
        let y = square.center().y;
        ui.painter().hline(
            square.left() + 3.0..=square.right() - 3.0,
            y,
            egui::Stroke::new(1.5, t.text_faint),
        );
    } else if tick == Tick::Done {
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
