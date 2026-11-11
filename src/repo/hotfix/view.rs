//! The dialog that finishes a hotfix: the tag and its notes, what to merge it into, and what
//! each step will do.

use egui::{Context, RichText, Ui};

use super::{FinishView, HotfixAct};
use crate::repo::{Action, RepoTab, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// The dialog, while a hotfix is being finished.
    pub(in crate::repo) fn hotfix_dialog(&mut self, ctx: &Context, acts: &mut Vec<Action>) {
        let busy = self.busy_op.is_some();
        let Some(view) = &mut self.hotfix.finish else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let room = (ctx.content_rect().height() - 220.0).max(160.0);
        let response = egui::Modal::new(egui::Id::new("gitcraft-hotfix"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(540.0);
                contents(ui, view, room, busy, acts);
            });
        if response.should_close() {
            acts.push(Action::Hotfix(HotfixAct::Close));
        }
    }
}

fn contents(ui: &mut Ui, view: &mut FinishView, room: f32, busy: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(crate::i18n::text("repo-workflows-hotfix-contents-296ee3d7"))
            .font(theme::semibold(15.0)),
    );
    let plan = match &view.plan {
        None => {
            ui.add(crate::spinner::Spinner::new());
            cancel(ui, acts);
            return;
        }
        Some(Err(e)) => {
            ui.colored_label(t.warning, e);
            cancel(ui, acts);
            return;
        }
        Some(Ok(plan)) => plan.clone(),
    };
    let c = &mut view.choices;
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-hotfix-change-summary",
            &[
                ("branch", (plan.hotfix.name.as_str()).into()),
                ("count", (plan.changes.len()).into()),
                (
                    "has-base",
                    (if plan.base_tag.is_some() { "yes" } else { "no" }).into(),
                ),
                ("base", (plan.base_tag.as_deref().unwrap_or("")).into()),
            ],
        ))
        .color(t.text_dim),
    );
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .max_height(room)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-hotfix-contents-1503916a"))
                        .color(t.text_dim),
                );
                ui.add(egui::TextEdit::singleline(&mut c.tag).desired_width(140.0));
                ui.checkbox(
                    &mut c.sign,
                    crate::i18n::text("repo-workflows-hotfix-contents-08251562"),
                )
                .on_hover_text(crate::i18n::text("repo-workflows-hotfix-contents-c2cdf43f"));
            });
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-hotfix-contents-3b7c3548"))
                    .color(t.text_dim)
                    .small(),
            );
            egui::ScrollArea::vertical()
                .id_salt("hotfix-notes")
                .max_height(140.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut c.notes)
                            .desired_width(f32::INFINITY)
                            .desired_rows(5)
                            .font(theme::mono(11.5)),
                    );
                });
            ui.add_space(4.0);
            ui.label(crate::i18n::message(
                "repo-workflows-hotfix-contents-f5346a60",
                &[("plan-main-name-1", (plan.main.name).clone().into())],
            ));
            match &plan.develop {
                Some(d) => {
                    ui.checkbox(
                        &mut c.develop,
                        crate::i18n::message(
                            "repo-workflows-hotfix-contents-ba6c71e9",
                            &[("d-name-1", (d.name).clone().into())],
                        ),
                    );
                }
                None => {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-workflows-hotfix-contents-9d164a95"))
                            .color(t.text_faint)
                            .small(),
                    );
                }
            }
            ui.checkbox(
                &mut c.delete,
                crate::i18n::message(
                    "repo-workflows-hotfix-contents-96176159",
                    &[("plan-hotfix-name-1", (plan.hotfix.name).clone().into())],
                ),
            );
            let push = match &plan.remote {
                Some(r) => crate::i18n::message(
                    "repo-workflows-hotfix-contents-a2e52e9b",
                    &[("r", (r).clone().into())],
                ),
                None => crate::i18n::text("repo-workflows-hotfix-contents-c5ecd20d").to_owned(),
            };
            ui.add_enabled(
                plan.remote.is_some(),
                egui::Checkbox::new(&mut c.push, push),
            );
            if plan.conflicts.is_none() {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-hotfix-contents-0e86de5c"))
                        .color(t.text_faint)
                        .small(),
                );
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-hotfix-contents-3d5a5069"))
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
            );
            for (i, text) in plan.preview(c).iter().enumerate() {
                ui.add(egui::Label::new(format!("{}. {text}", i + 1)).wrap());
            }
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-hotfix-contents-89b9dd15"))
                    .color(t.text_faint)
                    .small(),
            );
        });
    let problem = plan.problem(c);
    if let Some(p) = &problem {
        ui.colored_label(t.warning, p);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let finish = widgets::primary_button(
            ui,
            crate::i18n::text("repo-workflows-hotfix-contents-296ee3d7"),
            problem.is_none() && !busy,
            120.0,
        );
        if busy {
            finish.clone().on_disabled_hover_text(crate::i18n::text(
                "repo-workflows-hotfix-contents-0f2dabe6",
            ));
        }
        if finish.clicked() {
            acts.push(Action::Hotfix(HotfixAct::Finish));
        }
        cancel(ui, acts);
    });
}

fn cancel(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui
        .add(
            egui::Button::new(crate::i18n::text("repo-workflows-hotfix-cancel-19766ed6"))
                .min_size(egui::vec2(80.0, 30.0)),
        )
        .clicked()
    {
        acts.push(Action::Hotfix(HotfixAct::Close));
    }
}
