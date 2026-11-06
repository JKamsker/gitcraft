//! The list of the issues assigned to you, to pick one and start work on it: on a new branch
//! named after it, here or in a worktree of its own.

use egui::{Key, RichText, Ui};

use super::IssueAct;
use super::naming::branch_name;
use crate::forge::Issue;
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// The picker's state.
pub(super) struct Picker {
    /// "GitHub", "Forgejo"…: who the issues come from.
    forge: &'static str,
    /// `None` while the forge is asked.
    issues: Option<Result<Vec<Issue>, String>>,
    filter: String,
    /// The number of the issue picked.
    chosen: Option<u64>,
    /// The branch's name, suggested from the issue picked and editable.
    name: String,
    worktree: bool,
    focus: bool,
}

/// What the picker leaves to do.
enum Outcome {
    Open,
    Close,
    Retry,
    Start(Box<Issue>, String, bool),
}

impl Picker {
    pub(super) fn new(forge: &'static str) -> Picker {
        Picker {
            forge,
            issues: None,
            filter: String::new(),
            chosen: None,
            name: String::new(),
            worktree: false,
            focus: true,
        }
    }

    /// The forge's answer came.
    pub(super) fn listed(&mut self, result: Result<Vec<Issue>, String>) {
        self.issues = Some(result);
    }

    fn chosen_issue(&self) -> Option<&Issue> {
        let list = self.issues.as_ref()?.as_ref().ok()?;
        list.iter().find(|i| Some(i.number) == self.chosen)
    }

    fn choose(&mut self, issue: &Issue) {
        self.chosen = Some(issue.number);
        self.name = branch_name(issue.number, &issue.title);
    }
}

impl RepoTab {
    /// Whether the picker is open: the keys are its own then (Escape closes it, not the diff).
    pub(in crate::repo) fn issue_picker_open(&self) -> bool {
        self.issue_work.picker.is_some()
    }

    /// Draws the picker when it's open; what is picked goes to `acts`.
    pub(in crate::repo) fn issue_picker(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if self.issue_work.picker.is_none() {
            return;
        }
        let local: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let Some(picker) = self.issue_work.picker.as_mut() else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let response = egui::Modal::new(egui::Id::new("gitcraft-issue-picker"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(460.0);
                show(ui, picker, &local)
            });
        let outcome = match response.inner {
            Outcome::Open if response.should_close() => Outcome::Close,
            outcome => outcome,
        };
        match outcome {
            Outcome::Open => {}
            Outcome::Close => self.issue_work.picker = None,
            Outcome::Retry => acts.push(Action::Issue(IssueAct::OpenPicker)),
            Outcome::Start(issue, branch, worktree) => {
                self.issue_work.picker = None;
                acts.push(Action::Issue(IssueAct::Start {
                    issue,
                    branch,
                    worktree,
                }));
            }
        }
    }
}

fn show(ui: &mut Ui, picker: &mut Picker, local: &[String]) -> Outcome {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new("Start work from an issue").font(theme::semibold(15.0)));
    ui.label(
        RichText::new(format!(
            "The open issues assigned to you on {}",
            picker.forge
        ))
        .color(t.text_dim),
    );
    ui.add_space(10.0);
    let mut outcome = Outcome::Open;
    match &picker.issues {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new().size(16.0));
                ui.label(RichText::new(format!("Asking {}…", picker.forge)).color(t.text_dim));
            });
        }
        Some(Err(e)) => {
            ui.add(egui::Label::new(RichText::new(e.as_str()).color(t.danger)).wrap());
            ui.add_space(6.0);
            if ui.button("Try again").clicked() {
                outcome = Outcome::Retry;
            }
        }
        Some(Ok(list)) if list.is_empty() => {
            ui.label(
                RichText::new("No open issue of this repository is assigned to you.")
                    .color(t.text_dim),
            );
        }
        Some(Ok(_)) => outcome = choose(ui, picker, local),
    }
    ui.add_space(10.0);
    let ready =
        picker.chosen_issue().is_some() && super::super::sidebar::plain_branch_name(&picker.name);
    ui.horizontal(|ui| {
        let enter = ui.input(|i| i.key_pressed(Key::Enter));
        let clicked = widgets::primary_button(ui, "Start work", ready, 100.0).clicked();
        if (clicked || (enter && ready))
            && let Some(issue) = picker.chosen_issue()
        {
            outcome = Outcome::Start(
                Box::new(issue.clone()),
                picker.name.clone(),
                picker.worktree,
            );
        }
        if ui
            .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
            .clicked()
        {
            outcome = Outcome::Close;
        }
    });
    outcome
}

/// The filter, the issues that match it and, for the one picked, the branch's name and where it
/// goes.
fn choose(ui: &mut Ui, picker: &mut Picker, local: &[String]) -> Outcome {
    let t = Tokens::get(ui.ctx());
    let edit = ui.add(
        egui::TextEdit::singleline(&mut picker.filter)
            .hint_text("Filter by number, title or label")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    if std::mem::take(&mut picker.focus) {
        edit.request_focus();
    }
    ui.add_space(6.0);
    let query = picker.filter.trim().to_lowercase();
    let list = match &picker.issues {
        Some(Ok(list)) => list.clone(),
        _ => Vec::new(),
    };
    let shown: Vec<&Issue> = list.iter().filter(|i| matches(i, &query)).collect();
    let mut picked = None;
    egui::ScrollArea::vertical()
        .max_height(260.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if shown.is_empty() {
                ui.label(RichText::new("No issue matches the filter.").color(t.text_faint));
            }
            for issue in &shown {
                if issue_row(ui, issue, picker.chosen == Some(issue.number)) {
                    picked = Some(*issue);
                }
            }
        });
    if let Some(issue) = picked {
        picker.choose(issue);
    }
    if picker.chosen_issue().is_none() {
        return Outcome::Open;
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Branch").color(t.text_dim));
        ui.add(
            egui::TextEdit::singleline(&mut picker.name)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 4)),
        );
    });
    if picker.name.contains(' ') {
        picker.name = picker.name.replace(' ', "-");
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Work").color(t.text_dim));
        if let Some(i) =
            widgets::switch(ui, &["Here", "In a new worktree"], picker.worktree as usize)
        {
            picker.worktree = i == 1;
        }
    });
    let note = if local.contains(&picker.name) {
        format!(
            "{} exists already: work goes on on it as it is.",
            picker.name
        )
    } else if picker.worktree {
        "A new branch from the default branch, in a folder beside the repository.".to_owned()
    } else {
        "A new branch from the default branch, checked out here.".to_owned()
    };
    ui.label(RichText::new(note).size(11.5).color(t.text_faint));
    Outcome::Open
}

/// Whether `issue` is one the filter `query` (lowercase) asks for.
fn matches(issue: &Issue, query: &str) -> bool {
    query.is_empty()
        || format!("#{}", issue.number).contains(query)
        || issue.title.to_lowercase().contains(query)
        || issue
            .labels
            .iter()
            .any(|l| l.to_lowercase().contains(query))
}

/// One issue of the list: its number, title and labels. Whether it was clicked.
fn issue_row(ui: &mut Ui, issue: &Issue, chosen: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let fill = if chosen {
        t.row_selected
    } else {
        egui::Color32::TRANSPARENT
    };
    let inner = egui::Frame::new()
        .fill(fill)
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("#{}", issue.number))
                        .font(theme::mono(12.0))
                        .color(t.accent),
                );
                ui.label(RichText::new(&issue.title).color(t.text));
                for label in &issue.labels {
                    widgets::chip(ui, label, t.text_dim);
                }
            });
        });
    let response = ui.interact(
        inner.response.rect,
        ui.id().with(("issue-row", issue.number)),
        egui::Sense::click(),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(&issue.url)
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(number: u64, title: &str, labels: &[&str]) -> Issue {
        Issue {
            number,
            title: title.into(),
            body: String::new(),
            url: String::new(),
            labels: labels.iter().map(|l| (*l).to_owned()).collect(),
        }
    }

    #[test]
    fn the_filter_goes_by_number_title_and_label() {
        let i = issue(42, "Fix Login crash", &["Bug"]);
        assert!(matches(&i, ""));
        assert!(matches(&i, "#42"));
        assert!(matches(&i, "42"));
        assert!(matches(&i, "login"));
        assert!(matches(&i, "bug"));
        assert!(!matches(&i, "#7"));
        assert!(!matches(&i, "feature"));
    }

    #[test]
    fn picking_an_issue_suggests_its_branch() {
        let mut picker = Picker::new("GitHub");
        assert!(picker.chosen_issue().is_none());
        let list = vec![issue(42, "Fix login crash", &[]), issue(7, "Other", &[])];
        picker.listed(Ok(list.clone()));
        picker.choose(&list[0]);
        assert_eq!(picker.name, "42-fix-login-crash");
        assert_eq!(picker.chosen_issue().map(|i| i.number), Some(42));
        // A list asked again without it: nothing is picked any more.
        picker.listed(Ok(vec![list[1].clone()]));
        assert!(picker.chosen_issue().is_none());
    }
}
