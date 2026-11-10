//! The list of the issues assigned to you, to pick one and start work on it: on a new branch
//! named after it, here or in a worktree of its own.

mod rows;
use rows::{choose, matches};

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
    ui.label(
        RichText::new(crate::i18n::text("repo-workflows-issue_work-show-fd3baaea"))
            .font(theme::semibold(15.0)),
    );
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-issue_work-show-8bd32aa3",
            &[("picker-forge-1", (picker.forge).clone().into())],
        ))
        .color(t.text_dim),
    );
    ui.add_space(10.0);
    let mut outcome = Outcome::Open;
    let mut start_now = false;
    let mut source = picker.source;
    egui::ComboBox::from_id_salt("issue-source")
        .selected_text(source.map_or(
            crate::i18n::text("repo-workflows-issue_work-show-3a89bbe3"),
            |k| k.name(),
        ))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut source,
                None,
                crate::i18n::text("repo-workflows-issue_work-show-3a89bbe3"),
            );
            for kind in crate::trackers::Kind::ALL {
                ui.selectable_value(&mut source, Some(kind), kind.name());
            }
        });
    if source != picker.source {
        return Outcome::Source(source);
    }
    if picker.context != crate::connections::current() {
        ui.label(crate::i18n::text("repo-workflows-issue_work-show-b6db3846"));
        return Outcome::Retry;
    }

    match &picker.issues {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new().size(16.0));
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-issue_work-show-b32a9827",
                        &[("picker-forge-1", (picker.forge).clone().into())],
                    ))
                    .color(t.text_dim),
                );
            });
        }
        Some(Err(e)) => {
            ui.add(egui::Label::new(RichText::new(e.as_str()).color(t.danger)).wrap());
            ui.add_space(6.0);
            if ui
                .button(crate::i18n::text("repo-workflows-issue_work-show-d8b8392e"))
                .clicked()
            {
                outcome = Outcome::Retry;
            }
        }
        Some(Ok(list)) if list.is_empty() => {
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-issue_work-show-d3965ab6"))
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
        (Some(_), Some(busy)) => Some(crate::i18n::message(
            "repo-workflows-issue_work-show-b37705f6",
            &[("busy", (busy).into())],
        )),
        (Some(_), None) => None,
    };
    let ready = plan.is_some() && why_not.is_none();
    ui.horizontal(|ui| {
        let start = widgets::primary_button(
            ui,
            crate::i18n::text("repo-workflows-issue_work-show-6a7da0d2"),
            ready,
            100.0,
        );
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
            .add(
                egui::Button::new(crate::i18n::text("repo-workflows-issue_work-show-19766ed6"))
                    .min_size(egui::vec2(80.0, 30.0)),
            )
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

#[cfg(test)]
#[path = "picker/tests.rs"]
mod tests;
