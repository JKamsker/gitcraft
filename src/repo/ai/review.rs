//! _Review this branch_: the tool reviews what a branch adds to its base, and its answer shows
//! in a panel over the tab, as Markdown (images as links: the answer can't make gitcraft
//! contact a server).

use std::time::Duration;

use egui::{RichText, Ui};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use super::{Running, coverage, elapsed};
use crate::ai::{self, Answer, Question, RunError};
use crate::repo::RepoTab;
use crate::settings::Settings;
use crate::theme::Tokens;

/// A review asked for, and where it stands.
pub(in crate::repo) struct Review {
    pub(super) branch: String,
    pub(super) base: String,
    pub(super) state: State,
    markdown: CommonMarkCache,
}

pub(super) enum State {
    Running(Running),
    /// The answer, made safe to show.
    Done(Answer),
    Failed(String),
    Stopped,
}

impl Review {
    /// Stops the tool, if it is still at work.
    pub(super) fn cancel(&self) {
        if let State::Running(running) = &self.state {
            running.cancel.cancel();
        }
    }
}

/// What the panel's buttons ask for.
enum Asked {
    Cancel,
    Again,
    Close,
}

impl RepoTab {
    /// Has the tool the settings pick review what `branch` adds to `base`, in the panel.
    pub(in crate::repo) fn start_review(
        &mut self,
        ctx: &egui::Context,
        branch: String,
        base: String,
    ) {
        if let Some(old) = self.ai.review.take() {
            old.cancel();
        }
        let settings = Settings::get(ctx).ai;
        let mut review = Review {
            branch: branch.clone(),
            base: base.clone(),
            state: State::Stopped,
            markdown: CommonMarkCache::default(),
        };
        if !settings.enabled() {
            review.state = State::Failed(
                "No AI tool is picked: choose one in Settings → AI, or with the sparkles button \
                 of the commit box."
                    .to_owned(),
            );
            self.ai.review = Some(review);
            return;
        }
        let running = self.ai.start(settings.tool);
        let (id, cancel) = (running.id, running.cancel.clone());
        review.state = State::Running(running);
        self.ai.review = Some(review);
        let repo = self.path.clone();
        let question = Question::Review { branch, base };
        self.then(
            ctx,
            move || ai::ask(&repo, &settings, &question, ai::TIMEOUT, &cancel),
            move |tab, answer, _| tab.review_answered(id, answer),
        );
    }

    /// The answer to review request `id`, if that is still the one the panel waits for.
    pub(in crate::repo) fn review_answered(&mut self, id: u64, answer: Result<Answer, RunError>) {
        let Some(review) = &mut self.ai.review else {
            return;
        };
        if !matches!(&review.state, State::Running(r) if r.id == id) {
            return;
        }
        review.state = match answer {
            Ok(mut answer) => {
                answer.text = crate::pull_request::safe_markdown(&answer.text, "");
                State::Done(answer)
            }
            Err(RunError::Cancelled) => State::Stopped,
            Err(RunError::Failed(e)) => State::Failed(e),
        };
    }

    /// The review panel, while there is a review.
    pub(in crate::repo) fn ai_review_window(&mut self, ctx: &egui::Context) {
        let Some(review) = &mut self.ai.review else {
            return;
        };
        let t = Tokens::get(ctx);
        let mut open = true;
        let mut asked = None;
        egui::Window::new(format!("Review of {}", review.branch))
            .id(egui::Id::new(("ai-review", self.path.clone())))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([620.0, 560.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .frame(
                egui::Frame::window(&ctx.global_style())
                    .fill(t.card)
                    .stroke(egui::Stroke::new(1.0, t.field_border)),
            )
            .show(ctx, |ui| asked = review_ui(ui, review));
        if !open {
            asked = Some(Asked::Close);
        }
        match asked {
            Some(Asked::Cancel) => {
                if let Some(review) = &mut self.ai.review {
                    review.cancel();
                    review.state = State::Stopped;
                }
            }
            Some(Asked::Again) => {
                if let Some(review) = self.ai.review.take() {
                    self.start_review(ctx, review.branch, review.base);
                }
            }
            Some(Asked::Close) => {
                if let Some(review) = self.ai.review.take() {
                    review.cancel();
                }
            }
            None => {}
        }
    }
}

/// The panel's contents; what its buttons asked for.
fn review_ui(ui: &mut Ui, review: &mut Review) -> Option<Asked> {
    let t = Tokens::get(ui.ctx());
    let mut asked = None;
    ui.label(
        RichText::new(format!(
            "What {} adds to {} since they diverged",
            review.branch, review.base
        ))
        .color(t.text_faint)
        .small(),
    );
    ui.add_space(6.0);
    match &review.state {
        State::Running(running) => {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(
                    RichText::new(format!(
                        "{} is reviewing… {}",
                        running.tool.short(),
                        elapsed(running.started)
                    ))
                    .color(t.text_dim),
                );
                if ui.button("Cancel").clicked() {
                    asked = Some(Asked::Cancel);
                }
            });
        }
        State::Done(answer) => {
            if let Some(note) = coverage(answer.files, answer.left_out) {
                ui.label(RichText::new(note).color(t.text_faint).small());
            }
            ui.horizontal(|ui| {
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(answer.text.clone());
                }
                if ui.button("Review again").clicked() {
                    asked = Some(Asked::Again);
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    CommonMarkViewer::new().show(ui, &mut review.markdown, &answer.text);
                });
            crate::repo::pr_view::only_web_links(ui.ctx());
        }
        State::Failed(e) => {
            ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
            if ui.button("Try again").clicked() {
                asked = Some(Asked::Again);
            }
        }
        State::Stopped => {
            ui.label(RichText::new("Stopped.").color(t.text_dim));
            if ui.button("Start again").clicked() {
                asked = Some(Asked::Again);
            }
        }
    }
    asked
}
