//! The scope drift window of a worktree: its task (or linked issue) and what it changed beyond
//! it, every item opening its files.

mod items;
#[cfg(test)]
mod tests;

use egui::RichText;

use super::{Action, DiffKey, RepoTab};
use crate::drift::{self, Report};
use crate::git::Worktree;
use crate::theme::{self, Tokens};

/// What the window holds.
pub(crate) struct DriftView {
    worktree: Worktree,
    /// The main worktree's commit, compared with.
    base: String,
    /// The task as edited, and as kept.
    task: String,
    saved: String,
    /// The linked issue: its number and its text, or why it couldn't be read.
    issue: Option<(u64, Result<String, String>)>,
    report: Option<Result<Report, String>>,
    busy: bool,
    /// The coding agent's answer, while asked (`None`: not asked).
    second_opinion: Option<Result<String, String>>,
    asking: bool,
    /// What saving the task said.
    said: Option<String>,
    /// Put away while a diff it opened is looked at; the menu brings it back.
    pub(crate) hidden: bool,
}

/// What a measure found: the task kept, the linked issue, the report.
type Measured = (
    String,
    Option<(u64, Result<String, String>)>,
    Result<Report, String>,
);

impl DriftView {
    /// The text the figures are set against: the task written, else the linked issue's.
    fn task_text(&self) -> String {
        if !self.task.trim().is_empty() {
            return self.task.clone();
        }
        match &self.issue {
            Some((_, Ok(text))) => text.clone(),
            _ => String::new(),
        }
    }

    /// Where a file of this worktree's changes is read from: its working tree when it is the
    /// tab's own (an untracked file as one), else its HEAD (what is committed); `None` for a
    /// change only another worktree's working tree has, which opens that worktree instead.
    fn diff_key(&self, report: &Report, path: &str) -> Option<DiffKey> {
        if self.worktree.current && report.untracked.iter().any(|u| u == path) {
            return Some(DiffKey::Work(crate::git::StatusFile {
                path: path.to_owned(),
                old_path: None,
                code: '?',
                area: crate::git::Area::Untracked,
            }));
        }
        let only_here = report.uncommitted.iter().any(|u| u == path)
            && !report.committed.iter().any(|c| c == path);
        if !self.worktree.current && only_here {
            return None;
        }
        Some(DiffKey::Range {
            from: report.fork.clone(),
            to: (!self.worktree.current).then(|| report.head.clone()),
            path: path.to_owned(),
        })
    }
}

impl RepoTab {
    /// Opens the window for the worktree at `path` (or brings back the one put away).
    pub(crate) fn open_drift(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        if let Some(view) = &mut self.drift
            && view.worktree.path == path
        {
            view.hidden = false;
            return;
        }
        let Some(worktree) = self.worktrees.iter().find(|w| w.path == path).cloned() else {
            return;
        };
        let base = self
            .worktrees
            .iter()
            .find(|w| w.main)
            .map(|w| w.head.clone())
            .unwrap_or_default();
        self.drift = Some(DriftView {
            worktree,
            base,
            task: String::new(),
            saved: String::new(),
            issue: None,
            report: None,
            busy: false,
            second_opinion: None,
            asking: false,
            said: None,
            hidden: false,
        });
        self.measure_drift(true, ctx);
    }

    /// Measures the worktree's changes in the background; `first` reads its task and linked
    /// issue as well.
    fn measure_drift(&mut self, first: bool, ctx: &egui::Context) {
        let forge = self.hosting.as_ref().map(|h| h.forge);
        let Some(view) = &mut self.drift else {
            return;
        };
        view.busy = true;
        let path = view.worktree.path.clone();
        let asked = path.clone();
        let branch = view.worktree.branch.clone().unwrap_or_default();
        let base = view.base.clone();
        let edited = view.task_text();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || -> Measured {
                let task = if first {
                    drift::task::read(&path).unwrap_or_default()
                } else {
                    edited.clone()
                };
                let issue = drift::task::issue_number(&branch, &task).map(|n| {
                    let text = match forge {
                        Some(forge) => drift::task::issue_text(forge, &repo, n),
                        None => Err(crate::i18n::text(
                            "repo-workflows-drift_view-measure-drift-27f1506d",
                        )
                        .to_owned()),
                    };
                    (n, text)
                });
                let against = if !task.trim().is_empty() {
                    task.clone()
                } else if let Some((_, Ok(text))) = &issue {
                    text.clone()
                } else {
                    edited
                };
                let report = drift::measure(&path, &base, &against);
                (task, issue, report)
            },
            move |tab, (task, issue, report), _| {
                let Some(view) = &mut tab.drift else {
                    return;
                };
                // Another worktree's window opened since: not this one's answer.
                if view.worktree.path != asked {
                    return;
                }
                // What was typed meanwhile stays.
                if first && view.task == view.saved {
                    view.saved = task.clone();
                    view.task = task;
                } else if first {
                    view.saved = task;
                }
                if issue.is_some() {
                    view.issue = issue;
                }
                view.report = Some(report);
                view.busy = false;
            },
        );
    }

    fn save_task(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.drift else {
            return;
        };
        let path = view.worktree.path.clone();
        let asked = path.clone();
        let task = view.task.clone();
        self.then(
            ctx,
            move || drift::task::write(&path, &task).map(|()| task),
            move |tab, saved, ctx| {
                if let Some(view) = &mut tab.drift
                    && view.worktree.path == asked
                {
                    match saved {
                        Ok(task) => {
                            view.saved = task;
                            view.said = Some(
                                crate::i18n::text("repo-workflows-drift_view-save-task-a40b28e7")
                                    .to_owned(),
                            );
                        }
                        Err(e) => view.said = Some(e),
                    }
                }
                tab.measure_drift(false, ctx);
            },
        );
    }

    fn ask_second_opinion(&mut self, ctx: &egui::Context) {
        let Some(view) = &mut self.drift else {
            return;
        };
        let Some(Ok(report)) = &view.report else {
            return;
        };
        let question = drift::llm::prompt(&view.task_text(), report);
        let asked = view.worktree.path.clone();
        view.asking = true;
        view.second_opinion = None;
        self.then(
            ctx,
            move || drift::llm::ask(&question),
            move |tab, answer, _| {
                if let Some(view) = &mut tab.drift
                    && view.worktree.path == asked
                {
                    view.asking = false;
                    view.second_opinion = Some(answer);
                }
            },
        );
    }

    /// The window, while it is open and not put away.
    pub(crate) fn drift_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if self.drift.as_ref().is_none_or(|v| v.hidden) {
            return;
        }
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let before = acts.len();
        let mut todo = Todo::default();
        let response = egui::Modal::new(egui::Id::new("gitcraft-drift"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(640.0);
                if let Some(view) = &mut self.drift {
                    todo = body(ui, view, acts);
                }
            });
        if todo.close {
            self.drift = None;
            return;
        }
        if (response.should_close() || acts.len() > before)
            && let Some(view) = &mut self.drift
        {
            view.hidden = true;
        }
        if todo.save {
            self.save_task(ctx);
        } else if todo.measure {
            self.measure_drift(false, ctx);
        }
        if todo.ask {
            self.ask_second_opinion(ctx);
        }
    }
}

/// What a click in the window asked for.
#[derive(Default)]
struct Todo {
    save: bool,
    measure: bool,
    ask: bool,
    close: bool,
}

fn body(ui: &mut egui::Ui, view: &mut DriftView, acts: &mut Vec<Action>) -> Todo {
    let t = Tokens::get(ui.ctx());
    let mut todo = Todo::default();
    let branch = view
        .worktree
        .branch
        .clone()
        .unwrap_or_else(|| "detached".to_owned());
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-drift_view-body-a8164b33",
            &[
                (
                    "view-worktree-name-1",
                    (view.worktree.name()).clone().into(),
                ),
                ("branch", (branch).clone().into()),
            ],
        ))
        .font(theme::semibold(15.0)),
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new(crate::i18n::text("repo-workflows-drift_view-body-4bc74b21"))
            .font(theme::medium(12.5)),
    );
    ui.add(
        egui::TextEdit::multiline(&mut view.task)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text(crate::i18n::text("repo-workflows-drift_view-body-65fb660d")),
    );
    if let Some((n, issue)) = &view.issue {
        let line = match issue {
            Ok(text) => crate::i18n::message(
                "repo-workflows-drift_view-body-cdc38d02",
                &[
                    ("n", (n).clone().into()),
                    (
                        "text-lines-next-unwrap-or-defaul-1",
                        (text.lines().next().unwrap_or_default()).clone().into(),
                    ),
                ],
            ),
            Err(e) => crate::i18n::message(
                "repo-workflows-drift_view-body-fa4f662d",
                &[
                    ("n", (n).clone().into()),
                    ("e", crate::drift::display::diagnostic(e).into()),
                ],
            ),
        };

        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-drift-task-source",
                &[
                    ("line", (line.as_str()).into()),
                    (
                        "task",
                        (if view.task.trim().is_empty() {
                            "yes"
                        } else {
                            "no"
                        })
                        .into(),
                    ),
                ],
            ))
            .size(12.0)
            .color(t.text_dim),
        );
    }
    ui.horizontal(|ui| {
        let dirty = view.task != view.saved;
        todo.save = ui
            .add_enabled(
                dirty && !view.busy,
                egui::Button::new(crate::i18n::text("repo-workflows-drift_view-body-42a9eb31")),
            )
            .clicked();
        todo.measure = ui
            .add_enabled(
                !view.busy,
                egui::Button::new(crate::i18n::text("repo-workflows-drift_view-body-31794226")),
            )
            .clicked();
        if view.busy {
            ui.add(crate::spinner::Spinner::new());
        }
        if let Some(said) = &view.said {
            ui.label(RichText::new(said).size(11.5).color(t.text_faint));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            todo.close = ui
                .button(crate::i18n::text("repo-workflows-drift_view-body-7d9eb7ac"))
                .clicked();
        });
    });
    ui.add_space(8.0);
    match &view.report {
        Some(Ok(report)) => {
            let report = report.clone();
            items::report(ui, view, &report, acts);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                todo.ask = ui
                    .add_enabled(
                        !view.asking,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-drift_view-body-3e453620",
                        )),
                    )
                    .on_hover_text(crate::i18n::text("repo-workflows-drift_view-body-e039d62e"))
                    .clicked();
                if view.asking {
                    ui.add(crate::spinner::Spinner::new());
                }
            });
            match &view.second_opinion {
                Some(Ok(answer)) => {
                    ui.label(RichText::new(answer).size(12.0).color(t.text_dim));
                }
                Some(Err(e)) => {
                    ui.label(
                        RichText::new(crate::drift::display::diagnostic(e))
                            .size(12.0)
                            .color(t.danger),
                    );
                }
                None => {}
            }
        }
        Some(Err(e)) => {
            ui.label(RichText::new(crate::drift::display::diagnostic(e)).color(t.danger));
        }
        None => {}
    }
    todo
}

#[cfg(test)]
mod fluent_guard_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_drift_actual_paint_reprojects_cached_raw_results() -> Result<(), String> {
        let raw = "There is no main branch commit to compare the worktree with";
        let view = DriftView {
            worktree: Worktree {
                path: "/supplied/ß".into(),
                branch: Some("feature/1234".into()),
                ..Default::default()
            },
            base: "abc123".into(),
            task: "User supplied task".into(),
            saved: "User supplied task".into(),
            issue: None,
            report: Some(Err(raw.into())),
            busy: false,
            second_opinion: None,
            asking: false,
            said: None,
            hidden: false,
        };
        let mut initialized = false;
        let mut h = Harness::new_ui_state(
            |ui, state: &mut DriftView| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    initialized = true;
                    ui.ctx().request_discard("install supplied fonts");
                    return;
                }
                body(ui, state, &mut vec![]);
            },
            view,
        );
        for lang in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(lang);
            h.run();
            h.get_by_label(&crate::drift::display::diagnostic(raw));
            assert_eq!(h.state().report, Some(Err(raw.into())));
            assert_eq!(h.state().worktree.branch.as_deref(), Some("feature/1234"));
        }
        Ok(())
    }
}
