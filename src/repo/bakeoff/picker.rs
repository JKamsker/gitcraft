//! Picking the attempts of a bake-off: worktrees (their uncommitted work included) and
//! branches, 2 to 4 of them.

use egui::{RichText, Ui};

use super::BakeAct;
use crate::agent::bakeoff::{Attempt, MAX, MIN};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

pub(crate) struct Picker {
    /// Every candidate, and whether it is picked.
    candidates: Vec<(Attempt, bool)>,
    filter: String,
}

impl Picker {
    pub(super) fn new(tab: &RepoTab) -> Self {
        // The agent tasks' worktrees come picked, when they are as many as can be compared.
        let agent_paths: Vec<&str> = tab.agents.tasks.iter().map(|t| t.path.as_str()).collect();
        let mut candidates: Vec<(Attempt, bool)> = candidates(tab)
            .into_iter()
            .map(|a| {
                let agent = a.worktree.as_ref().is_some_and(|w| {
                    agent_paths
                        .iter()
                        .any(|p| w.as_path() == std::path::Path::new(p))
                });
                (a, agent)
            })
            .collect();
        let preset = candidates.iter().filter(|(_, on)| *on).count();
        if !(MIN..=MAX).contains(&preset) {
            candidates.iter_mut().for_each(|(_, on)| *on = false);
        }
        Self {
            candidates,
            filter: String::new(),
        }
    }

    fn picked(&self) -> Vec<Attempt> {
        self.candidates
            .iter()
            .filter(|(_, on)| *on)
            .map(|(a, _)| a.clone())
            .collect()
    }

    pub(super) fn ui(&mut self, ui: &mut Ui) -> Option<BakeAct> {
        let t = Tokens::get(ui.ctx());
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-bakeoff-ui-43c40423"))
                .font(theme::semibold(15.0)),
        );
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-bakeoff-ui-09eb5aca",
                &[("MIN", (MIN).clone().into()), ("MAX", (MAX).clone().into())],
            ))
            .color(t.text_dim),
        );
        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text(crate::i18n::text("repo-workflows-bakeoff-ui-638e249f"))
                .desired_width(f32::INFINITY),
        );
        let query = self.filter.to_lowercase();
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                let picked = self.candidates.iter().filter(|(_, on)| *on).count();
                for (attempt, on) in &mut self.candidates {
                    if !attempt.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    let kind = match &attempt.worktree {
                        Some(w) => crate::i18n::message(
                            "repo-workflows-bakeoff-ui-6217d161",
                            &[(
                                "crate-remote-display-w-1",
                                (crate::remote::display(w)).clone().into(),
                            )],
                        ),
                        None => crate::i18n::text("repo-workflows-paint-branch-kind").to_owned(),
                    };
                    let full = picked >= MAX && !*on;
                    ui.add_enabled_ui(!full, |ui| {
                        ui.checkbox(on, &attempt.name).on_disabled_hover_text(
                            crate::i18n::message(
                                "repo-workflows-bakeoff-ui-fbbc3b24",
                                &[("MAX", (MAX).clone().into())],
                            ),
                        );
                    });
                    ui.label(RichText::new(kind).small().color(t.text_faint));
                }
                let shown = self
                    .candidates
                    .iter()
                    .any(|(a, _)| a.name.to_lowercase().contains(&query));
                if !shown && !self.candidates.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-workflows-bakeoff-ui-cd8d844d"))
                            .color(t.text_faint),
                    );
                }
                if self.candidates.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-workflows-bakeoff-ui-c51e8eaa"))
                            .color(t.text_faint),
                    );
                }
            });
        ui.add_space(10.0);
        let picked = self.picked();
        let valid = (MIN..=MAX).contains(&picked.len());
        let mut act = None;
        ui.horizontal(|ui| {
            if crate::repo::primary_button(
                ui,
                crate::i18n::text("repo-workflows-bakeoff-ui-e888155d"),
                valid,
                90.0,
            )
            .on_disabled_hover_text(crate::i18n::message(
                "repo-workflows-bakeoff-ui-ae96d29a",
                &[("MIN", (MIN).clone().into()), ("MAX", (MAX).clone().into())],
            ))
            .clicked()
            {
                act = Some(BakeAct::Compare(picked));
            }
            if ui
                .add(
                    egui::Button::new(crate::i18n::text("repo-workflows-bakeoff-ui-19766ed6"))
                        .min_size(egui::vec2(80.0, 30.0)),
                )
                .clicked()
            {
                act = Some(BakeAct::ClosePicker);
            }
        });
        act
    }
}

/// The attempts a bake-off can compare: every worktree but a bare one or one that is gone,
/// then the local branches no worktree has checked out.
fn candidates(tab: &RepoTab) -> Vec<Attempt> {
    let mut out: Vec<Attempt> = tab
        .worktrees
        .iter()
        .filter(|w| !w.bare && w.prunable.is_none() && !w.head.is_empty())
        .map(|w| Attempt {
            name: w.branch.clone().unwrap_or_else(|| w.name()),
            rev: w.branch.clone().unwrap_or_else(|| w.head.clone()),
            worktree: Some(w.path.clone()),
        })
        .collect();
    for branch in &tab.refs.local {
        if !tab
            .worktrees
            .iter()
            .any(|w| w.branch.as_deref() == Some(branch.name.as_str()))
        {
            out.push(Attempt {
                name: branch.name.clone(),
                rev: branch.name.clone(),
                worktree: None,
            });
        }
    }
    out
}

impl RepoTab {
    /// The attempt picker, over the tab.
    pub(in crate::repo) fn bakeoff_dialog(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(picker) = &mut self.bakeoff.picker else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let response = egui::Modal::new(egui::Id::new("gitcraft-bakeoff-picker"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(460.0);
                picker.ui(ui)
            });
        if response.should_close() {
            acts.push(Action::Bake(BakeAct::ClosePicker));
        }
        if let Some(act) = response.inner {
            acts.push(Action::Bake(act));
        }
    }
}
