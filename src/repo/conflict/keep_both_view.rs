//! The Keep both dialog: the two files it would write, each version's origin and size, a word
//! about references to the renamed file; then, once written, staging them when checked.

use egui::{RichText, Stroke, Ui};

use super::keep_both::{self, Plan, Version};
use crate::repo::{Action, Refresh, RepoTab, widgets};
use crate::theme::{self, Tokens};

/// Where the dialog is.
pub enum KeepBoth {
    /// Reading both versions of the file.
    Reading(String),
    Preview(Plan),
    Writing(Plan),
    /// Written, to be checked before they are staged.
    Written(Plan),
    Failed {
        path: String,
        error: String,
    },
}

impl KeepBoth {
    fn path(&self) -> &str {
        match self {
            KeepBoth::Reading(path) | KeepBoth::Failed { path, .. } => path,
            KeepBoth::Preview(plan) | KeepBoth::Writing(plan) | KeepBoth::Written(plan) => {
                &plan.path
            }
        }
    }
}

/// What a click in the dialog asks for.
enum Step {
    Write,
    Retry,
    Stage,
    Close,
}

impl RepoTab {
    /// Opens Keep both for the conflicted `path`: both versions read in the background.
    pub(in crate::repo) fn keep_both(&mut self, path: String, ctx: &egui::Context) {
        self.keep_both = Some(KeepBoth::Reading(path.clone()));
        let (repo, state, read) = (self.path.clone(), self.work.in_progress, path.clone());
        self.then(
            ctx,
            move || keep_both::plan(&repo, &read, state),
            move |tab, plan, _| {
                if matches!(&tab.keep_both, Some(KeepBoth::Reading(p)) if *p == path) {
                    tab.keep_both = Some(match plan {
                        Ok(plan) => KeepBoth::Preview(plan),
                        Err(error) => KeepBoth::Failed { path, error },
                    });
                }
            },
        );
    }

    /// Writes both versions of the previewed plan, in the background.
    pub(super) fn keep_both_write(&mut self, ctx: &egui::Context) {
        let Some(KeepBoth::Preview(plan)) = self.keep_both.take() else {
            return;
        };
        self.keep_both = Some(KeepBoth::Writing(plan.clone()));
        let repo = self.path.clone();
        let job = plan.clone();
        self.then(
            ctx,
            move || keep_both::write(&repo, &job),
            move |tab, written, ctx| {
                if !matches!(&tab.keep_both, Some(KeepBoth::Writing(p)) if *p == plan) {
                    return;
                }
                tab.keep_both = Some(match written {
                    Ok(()) => KeepBoth::Written(plan),
                    Err(error) => KeepBoth::Failed {
                        path: plan.path,
                        error,
                    },
                });
                tab.reload_status(ctx);
            },
        );
    }

    /// The Keep both dialog, when open.
    pub(in crate::repo) fn keep_both_dialog(
        &mut self,
        ctx: &egui::Context,
        acts: &mut Vec<Action>,
    ) {
        let Some(state) = &self.keep_both else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let title = format!("Keep both versions of {}", state.path());
        let body = |ui: &mut Ui| {
            ui.set_width(440.0_f32.min(ctx.content_rect().width() - 64.0).max(200.0));
            ui.label(RichText::new(&title).font(theme::semibold(15.0)));
            ui.add_space(8.0);
            contents(ui, &t, state)
        };
        // Once written, out of the way (not modal): the files are checked in the window.
        let step = if matches!(state, KeepBoth::Written(_)) {
            egui::Window::new("keep-both-written")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
                .frame(frame)
                .show(ctx, body)
                .and_then(|r| r.inner)
                .flatten()
        } else {
            let response = egui::Modal::new(egui::Id::new("gitcraft-keep-both"))
                .frame(frame)
                .backdrop_color(egui::Color32::from_black_alpha(110))
                .show(ctx, body);
            let writing = matches!(state, KeepBoth::Writing(_));
            match response.inner {
                None if response.should_close() && !writing => Some(Step::Close),
                step => step,
            }
        };
        match step {
            Some(Step::Write) => self.keep_both_write(ctx),
            Some(Step::Retry) => {
                let path = state.path().to_owned();
                self.keep_both(path, ctx);
            }
            Some(Step::Stage) => {
                if let Some(KeepBoth::Written(plan)) = self.keep_both.take() {
                    acts.push(Action::Ops {
                        label: format!("Keep both versions of {}", plan.path),
                        ops: keep_both::stage_steps(&plan),
                        refresh: Refresh::Status,
                    });
                }
            }
            Some(Step::Close) => self.keep_both = None,
            None => {}
        }
    }
}

/// The dialog's body for `state`, and what was clicked.
fn contents(ui: &mut Ui, t: &Tokens, state: &KeepBoth) -> Option<Step> {
    match state {
        KeepBoth::Reading(_) => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label("Reading both versions…");
            });
            buttons(ui, None, "Cancel")
        }
        KeepBoth::Failed { error, .. } => {
            ui.label(RichText::new("Both versions could not be kept").font(theme::medium(13.0)));
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(error).color(t.danger)).wrap());
            buttons(ui, Some(("Try again", Step::Retry)), "Close")
        }
        KeepBoth::Preview(plan) | KeepBoth::Writing(plan) => {
            let writing = matches!(state, KeepBoth::Writing(_));
            preview(ui, t, plan);
            if writing {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    ui.label("Writing both files…");
                });
                return None;
            }
            buttons(ui, Some(("Write both files", Step::Write)), "Cancel")
        }
        KeepBoth::Written(plan) => {
            ui.add(
                egui::Label::new(format!(
                    "Wrote {} and {}. They are in the changes list as new files: click them \
                     there to check them. Nothing is staged yet.",
                    plan.ours.target, plan.theirs.target
                ))
                .wrap(),
            );
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "Staging them also removes {} and marks its conflict resolved.",
                        plan.path
                    ))
                    .color(t.text_dim),
                )
                .wrap(),
            );
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "Not now leaves them as they are: to finish later, stage both files and \
                         delete {}.",
                        plan.path
                    ))
                    .color(t.text_dim),
                )
                .wrap(),
            );
            buttons(ui, Some(("Stage both", Step::Stage)), "Not now")
        }
    }
}

/// The two files to be written, and the warning about references to the old name.
fn preview(ui: &mut Ui, t: &Tokens, plan: &Plan) {
    ui.add(
        egui::Label::new(format!(
            "Neither version is thrown away: each is written as a file of its own, beside {}. \
             Nothing is staged before you have checked them.",
            plan.path
        ))
        .wrap(),
    );
    ui.add_space(8.0);
    version_row(ui, t, &plan.ours);
    ui.add_space(4.0);
    version_row(ui, t, &plan.theirs);
    ui.add_space(8.0);
    let name = plan.path.rsplit('/').next().unwrap_or(&plan.path);
    egui::Frame::new()
        .stroke(Stroke::new(1.0, t.warning))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "Anything that refers to {name} (code, stylesheets, scenes, docs) may \
                         need updating to one of the new names."
                    ))
                    .color(t.text),
                )
                .wrap(),
            );
        });
}

/// One version: the file it becomes, where it comes from, its size.
fn version_row(ui: &mut Ui, t: &Tokens, version: &Version) {
    egui::Frame::new()
        .fill(t.field)
        .stroke(Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(&version.target).font(theme::mono(12.5))).wrap());
            ui.label(
                RichText::new(format!(
                    "{} · {} · {}",
                    version.origin,
                    crate::git::human_size(version.size),
                    version.oid.get(..8).unwrap_or(&version.oid)
                ))
                .color(t.text_dim)
                .small(),
            );
            if !version.taken.is_empty() {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} already there, so not that name",
                            version.taken.join(", ")
                        ))
                        .color(t.text_faint)
                        .small(),
                    )
                    .wrap(),
                );
            }
        });
}

/// The dialog's buttons: the `primary` one (if any), then the one that closes it.
fn buttons(ui: &mut Ui, primary: Option<(&str, Step)>, close: &str) -> Option<Step> {
    ui.add_space(14.0);
    let mut step = None;
    ui.horizontal(|ui| {
        if let Some((label, then)) = primary
            && widgets::primary_button(ui, label, true, 0.0).clicked()
        {
            step = Some(then);
        }
        if ui
            .add(egui::Button::new(close).min_size(egui::vec2(80.0, 30.0)))
            .clicked()
        {
            step = Some(Step::Close);
        }
    });
    step
}
