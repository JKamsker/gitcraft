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
    ui.label(RichText::new("Finish hotfix").font(theme::semibold(15.0)));
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
        RichText::new(format!(
            "{} · {} change{}{}",
            plan.hotfix.name,
            plan.changes.len(),
            if plan.changes.len() == 1 { "" } else { "s" },
            plan.base_tag
                .as_deref()
                .map(|t| format!(" since {t}"))
                .unwrap_or_default()
        ))
        .color(t.text_dim),
    );
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .max_height(room)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Tag").color(t.text_dim));
                ui.add(egui::TextEdit::singleline(&mut c.tag).desired_width(140.0));
                ui.checkbox(&mut c.sign, "Signed")
                    .on_hover_text("git tag -s, with your signing key; otherwise annotated");
            });
            ui.label(
                RichText::new("The tag's notes (Markdown)")
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
            ui.label(format!("Merged into {}", plan.main.name));
            match &plan.develop {
                Some(d) => {
                    ui.checkbox(&mut c.develop, format!("And into {}", d.name));
                }
                None => {
                    ui.label(
                        RichText::new("No development branch here (develop, or git-flow's)")
                            .color(t.text_faint)
                            .small(),
                    );
                }
            }
            ui.checkbox(&mut c.delete, format!("Delete {} here", plan.hotfix.name));
            let push = match &plan.remote {
                Some(r) => format!("Push to {r}"),
                None => "Push (this repository has no remote)".to_owned(),
            };
            ui.add_enabled(
                plan.remote.is_some(),
                egui::Checkbox::new(&mut c.push, push),
            );
            if plan.conflicts.is_none() {
                ui.label(
                    RichText::new(
                        "This git can't tell beforehand whether the merges conflict (2.38 or \
                         later can): if one does, it stops there",
                    )
                    .color(t.text_faint)
                    .small(),
                );
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new("WHAT IT WILL DO")
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
            );
            for (i, text) in plan.preview(c).iter().enumerate() {
                ui.add(egui::Label::new(format!("{}. {text}", i + 1)).wrap());
            }
            ui.label(
                RichText::new(
                    "Ctrl+Z puts the branches and the tag back here as they were; what was \
                     pushed stays.",
                )
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
        let finish =
            widgets::primary_button(ui, "Finish hotfix", problem.is_none() && !busy, 120.0);
        if busy {
            finish
                .clone()
                .on_disabled_hover_text("Another operation is running: wait for it");
        }
        if finish.clicked() {
            acts.push(Action::Hotfix(HotfixAct::Finish));
        }
        cancel(ui, acts);
    });
}

fn cancel(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui
        .add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 30.0)))
        .clicked()
    {
        acts.push(Action::Hotfix(HotfixAct::Close));
    }
}
