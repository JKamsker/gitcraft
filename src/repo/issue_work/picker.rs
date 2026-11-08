//! The list of the issues assigned to you, to pick one and start work on it: on a new branch
//! named after it, here or in a worktree of its own.

use egui::{Key, RichText, Ui};

use super::{IssueAct, Plan};
use crate::forge::{ISSUES_MAX, Issue};
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// The picker's state.
pub(super) struct Picker {
    /// "GitHub", "Forgejo"…: who the issues come from.
    forge: String,
    pub(super) source: Option<crate::trackers::Kind>,
    pub(super) context: crate::connections::Context,
    /// `None` while the forge is asked.
    issues: Option<Result<Vec<Issue>, String>>,
    pub(super) filter: String,
    /// The number of the issue picked.
    chosen: Option<usize>,
    /// The branch's name, suggested from the issue picked and editable.
    name: String,
    worktree: bool,
    /// The field to put the keyboard in on the next frame.
    focus: Option<Field>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Filter,
    Name,
}

/// What the picker leaves to do.
enum Outcome {
    Open,
    Close,
    Retry,
    Source(Option<crate::trackers::Kind>),
    /// Start work on the issue of that number, on the branch named so, in a worktree or not.
    Start(usize, String, bool),
}

/// What the picker is told of the tab, each frame.
struct View<'a> {
    /// What starting would do, for an issue's number, a branch's name and whether in a worktree.
    plan: &'a dyn Fn(&Issue, &str, bool) -> Plan,
    /// The operation running, which Start waits for.
    busy: Option<&'a str>,
}

impl Picker {
    pub(super) fn new(forge: &str, filter: String) -> Picker {
        Picker {
            forge: forge.into(),
            source: None,
            context: crate::connections::current(),
            issues: None,
            filter,
            chosen: None,
            name: String::new(),
            worktree: false,
            focus: Some(Field::Filter),
        }
    }

    /// The forge's answer came.
    pub(super) fn listed(&mut self, result: Result<Vec<Issue>, String>) {
        self.issues = Some(result);
    }

    fn list(&self) -> &[Issue] {
        match &self.issues {
            Some(Ok(list)) => list,
            _ => &[],
        }
    }

    fn chosen_issue(&self) -> Option<&Issue> {
        self.chosen.and_then(|n| self.list().get(n))
    }

    /// Picks the issue numbered `number` of the list, suggesting its branch's name.
    fn choose(&mut self, number: usize) {
        let Some(issue) = self.list().get(number) else {
            return;
        };
        self.name = issue.branch_name();
        self.chosen = Some(number);
    }

    /// The numbers of the issues the filter shows, in the list's order.
    fn shown(&self) -> Vec<usize> {
        let query = self.filter.trim().to_lowercase();
        self.list()
            .iter()
            .enumerate()
            .filter(|(_, i)| matches(i, &query))
            .map(|(n, _)| n)
            .collect()
    }
}

impl RepoTab {
    /// Whether the picker is open: the keys are its own then (Escape closes it, not the diff).
    pub(in crate::repo) fn issue_picker_open(&self) -> bool {
        self.issue_work.picker.is_some()
    }

    /// Draws the picker when it's open; what is picked goes to `acts`.
    pub(in crate::repo) fn issue_picker(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        // Taken out while it's drawn, so that what starting would do can be read from the tab.
        let Some(mut picker) = self.issue_work.picker.take() else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let plan = |issue: &Issue, name: &str, worktree: bool| {
            self.issue_plan(issue.key(), name, worktree)
        };
        let view = View {
            plan: &plan,
            busy: self.busy_op.as_deref(),
        };
        let response = egui::Modal::new(egui::Id::new("gitcraft-issue-picker"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(460.0);
                show(ui, &mut picker, &view)
            });
        let outcome = match response.inner {
            Outcome::Open if response.should_close() => Outcome::Close,
            outcome => outcome,
        };
        match outcome {
            Outcome::Open => self.issue_work.picker = Some(picker),
            Outcome::Source(source) => acts.push(Action::Issue(IssueAct::OpenSource(source))),
            Outcome::Close => {}
            Outcome::Retry => {
                let source = picker.source;
                self.issue_work.picker = Some(picker);
                acts.push(Action::Issue(IssueAct::OpenSource(source)));
            }
            Outcome::Start(number, branch, worktree) => match picker.list().get(number) {
                Some(issue) if picker.context == crate::connections::current() => {
                    acts.push(Action::Issue(IssueAct::Start {
                        issue: Box::new(issue.clone()),
                        branch,
                        worktree,
                    }))
                }
                _ => self.issue_work.picker = Some(picker),
            },
        }
    }
}

fn show(ui: &mut Ui, picker: &mut Picker, view: &View) -> Outcome {
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
    let mut start_now = false;
    let mut source = picker.source;
    egui::ComboBox::from_id_salt("issue-source")
        .selected_text(source.map_or("Repository forge", |k| k.name()))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut source, None, "Repository forge");
            for kind in crate::trackers::Kind::ALL {
                ui.selectable_value(&mut source, Some(kind), kind.name());
            }
        });
    if source != picker.source {
        return Outcome::Source(source);
    }
    if picker.context != crate::connections::current() {
        ui.label("Profile changed. Reload this source to start work.");
        return Outcome::Retry;
    }

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
        Some(Ok(_)) => start_now = choose(ui, picker),
    }
    ui.add_space(10.0);
    // What starting would do, worked out as Start works it out.
    let plan = picker
        .chosen_issue()
        .map(|issue| (view.plan)(issue, &picker.name, picker.worktree));
    if let Some(plan) = &plan {
        place_note(ui, plan);
    }
    let why_not = match (&plan, view.busy) {
        (None, _) => None,
        (
            Some(Plan {
                start: Err(why), ..
            }),
            _,
        ) => Some(why.clone()),
        (Some(_), Some(busy)) => Some(format!("Wait for {busy} to finish first")),
        (Some(_), None) => None,
    };
    let ready = plan.is_some() && why_not.is_none();
    ui.horizontal(|ui| {
        let start = widgets::primary_button(ui, "Start work", ready, 100.0);
        let start = match &why_not {
            Some(why) => start.on_disabled_hover_text(why),
            None => start,
        };
        if (start.clicked() || (start_now && ready))
            && let Some(index) = picker.chosen
        {
            outcome = Outcome::Start(index, picker.name.clone(), picker.worktree);
        }
        if ui
            .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
            .clicked()
        {
            outcome = Outcome::Close;
        }
    });
    if let Some(why) = why_not {
        ui.label(RichText::new(why).size(11.5).color(t.danger));
    }
    outcome
}

/// What starting does, and what to know before.
fn place_note(ui: &mut Ui, plan: &Plan) {
    let t = Tokens::get(ui.ctx());
    if plan.start.is_ok() {
        ui.add(egui::Label::new(RichText::new(&plan.note).size(11.5).color(t.text_faint)).wrap());
    }
    for warning in &plan.warnings {
        ui.add(egui::Label::new(RichText::new(warning).size(11.5).color(t.warning)).wrap());
    }
    ui.add_space(8.0);
}

/// The filter, the issues that match it and, for the one picked, the branch's name and where it
/// goes. Whether Enter in the branch's name asks to start.
fn choose(ui: &mut Ui, picker: &mut Picker) -> bool {
    let t = Tokens::get(ui.ctx());
    let edit = ui.add(
        egui::TextEdit::singleline(&mut picker.filter)
            .hint_text("Filter by number, title or label")
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(8, 6)),
    );
    if picker.focus == Some(Field::Filter) {
        picker.focus = None;
        edit.request_focus();
    }
    ui.add_space(6.0);
    let shown = picker.shown();
    // In the filter: ↑ and ↓ go through the list, Enter takes the one match there is.
    if edit.has_focus() || edit.lost_focus() {
        let (up, down, enter) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
            )
        });
        let at = shown.iter().position(|n| Some(*n) == picker.chosen);
        let next = match (at, up, down) {
            (None, _, true) | (None, true, _) => shown.first(),
            (Some(i), true, _) => shown.get(i.saturating_sub(1)),
            (Some(i), _, true) => shown.get(i + 1).or(shown.last()),
            _ => None,
        };
        if let Some(&number) = next {
            picker.choose(number);
        }
        if enter && edit.lost_focus() {
            if let [only] = shown[..] {
                picker.choose(only);
                picker.focus = Some(Field::Name);
            } else {
                // Enter alone doesn't pick for you: the keys stay in the filter.
                picker.focus = Some(Field::Filter);
            }
        }
    }
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .max_height(260.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if shown.is_empty() {
                ui.label(RichText::new("No issue matches the filter.").color(t.text_faint));
            }
            for (index, issue) in picker
                .list()
                .iter()
                .enumerate()
                .filter(|(n, _)| shown.contains(n))
            {
                if issue_row(ui, issue, picker.chosen == Some(index)) {
                    clicked = Some(index);
                }
            }
        });
    if picker.list().len() >= ISSUES_MAX {
        ui.label(
            RichText::new(format!(
                "The first {ISSUES_MAX} came: an issue that isn't among them is found on the \
                 forge's page."
            ))
            .size(11.5)
            .color(t.text_faint),
        );
    }
    if let Some(number) = clicked {
        picker.choose(number);
    }
    if picker.chosen_issue().is_none() {
        return false;
    }
    ui.add_space(8.0);
    let mut start = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Branch").color(t.text_dim));
        let name = ui.add(
            egui::TextEdit::singleline(&mut picker.name)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 4)),
        );
        if picker.focus == Some(Field::Name) {
            picker.focus = None;
            name.request_focus();
        }
        start = name.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
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
    start
}

/// Whether `issue` is one the filter `query` (lowercase) asks for.
fn matches(issue: &Issue, query: &str) -> bool {
    query.is_empty()
        || issue.key().to_lowercase().contains(query)
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
                    RichText::new(issue.key())
                        .font(theme::mono(12.0))
                        .color(t.accent),
                );
                ui.label(RichText::new(&issue.title).color(t.text));
                for label in &issue.labels {
                    widgets::chip(ui, label, t.text_dim);
                }
            });
        });
    if chosen {
        inner.response.scroll_to_me(None);
    }
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
            tracker: None,
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
        let mut picker = Picker::new("GitHub", String::new());
        assert!(picker.chosen_issue().is_none());
        let list = vec![issue(42, "Fix login crash", &[]), issue(7, "Other", &[])];
        picker.listed(Ok(list.clone()));
        picker.choose(0);
        assert_eq!(picker.name, "42-fix-login-crash");
        assert_eq!(picker.chosen_issue().map(|i| i.number), Some(42));
        // One that isn't in the list picks nothing.
        picker.choose(99);
        assert_eq!(picker.chosen, Some(0));
        // A list asked again without it: nothing is picked any more.
        picker.listed(Ok(Vec::new()));
        assert!(picker.chosen_issue().is_none());
    }

    #[test]
    fn tracker_keys_choose_distinct_rows_without_forge_numbers() {
        let make = |key: &str, id: &str| Issue {
            tracker: Some(crate::trackers::Identity {
                kind: crate::trackers::Kind::Linear,
                origin: "https://api.linear.app".into(),
                account: "me@example.org".into(),
                viewer: "viewer".into(),
                id: id.into(),
                key: key.into(),
            }),
            ..issue(0, "Work", &[])
        };
        let mut picker = Picker::new("Linear", String::new());
        picker.listed(Ok(vec![make("ENG-9", "uuid-a"), make("OPS-9", "uuid-b")]));
        picker.choose(1);
        assert_eq!(picker.chosen_issue().unwrap().key(), "OPS-9");
        assert_eq!(picker.name, "ops-9-work");
        picker.filter = "eng-9".into();
        assert_eq!(picker.shown(), [0]);
    }

    #[test]
    fn the_filter_stays_when_asked_again() {
        let mut picker = Picker::new("Forgejo", "crash".into());
        picker.listed(Ok(vec![
            issue(1, "Login crash", &[]),
            issue(2, "Docs", &[]),
            issue(3, "Crash on save", &["bug"]),
        ]));
        assert_eq!(picker.shown(), [0, 2]);
        picker.filter = "#2".into();
        assert_eq!(picker.shown(), [1]);
    }
}
