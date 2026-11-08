//! The search's window: its progress and Stop, then what it found, each commit named (a click
//! selects it).

use egui::RichText;

use super::report::{found_line, progress_line, tally_line};
use super::{InteractionAct, Search};
use crate::git::{Found, Side};
use crate::repo::{Action, RepoTab, short};
use crate::theme::{self, Tokens};

impl RepoTab {
    pub(in crate::repo) fn interaction_window(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(search) = &self.interactions.search else {
            return;
        };
        let t = Tokens::get(ctx);
        let mut open = true;
        egui::Window::new(crate::i18n::message(
            "repo-core-2832",
            &[
                ("a", search.spec.a.to_string().into()),
                ("b", search.spec.b.to_string().into()),
            ],
        ))
        .id(egui::Id::new(("gitcraft-interaction", self.path.clone())))
        .open(&mut open)
        .collapsible(false)
        .default_width(520.0)
        .show(ctx, |ui| match &search.ended {
            None => going(ui, &t, search, acts),
            Some(Err(e)) if e == "stopped" => {
                ui.label(RichText::new(crate::i18n::text("repo-core-2835")).color(t.text_dim));
            }
            Some(Err(e)) => {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-core-2836",
                        &[("e", e.to_string().into())],
                    ))
                    .color(t.danger),
                );
            }
            Some(Ok(found)) => {
                let spec = &search.spec;
                let color = match found.found {
                    Found::Together { .. } => t.danger,
                    Found::PassTogether => t.success,
                    _ => t.warning,
                };
                ui.label(
                    RichText::new(found_line(&found.found, &spec.a, &spec.b, spec.budget))
                        .color(color),
                );
                if let Found::Together { commits, .. } = &found.found {
                    ui.add_space(4.0);
                    for c in commits {
                        let side = match c.side {
                            Side::A => &spec.a,
                            Side::B => &spec.b,
                        };
                        ui.horizontal(|ui| {
                            let hash = RichText::new(short(&c.commit)).font(theme::mono(12.0));
                            if ui
                                .link(hash)
                                .on_hover_text(crate::i18n::text("repo-core-2837"))
                                .clicked()
                            {
                                acts.push(Action::SelectHash(c.commit.clone()));
                            }
                            ui.label(&c.summary);
                            ui.label(
                                RichText::new(crate::i18n::message(
                                    "repo-core-2838",
                                    &[("side", side.to_string().into())],
                                ))
                                .color(t.text_faint),
                            );
                        });
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new(tally_line(found)).color(t.text_dim));
            }
        });
        if !open {
            acts.push(Action::Interaction(if search.ended.is_some() {
                InteractionAct::Close
            } else {
                InteractionAct::Stop
            }));
        }
    }
}

fn going(ui: &mut egui::Ui, t: &Tokens, search: &Search, acts: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        ui.add(crate::spinner::Spinner::new());
        let said = match &search.last {
            None => crate::i18n::text("repo-core-2839").to_owned(),
            Some(p) => progress_line(p),
        };
        ui.label(RichText::new(said).color(t.text_dim));
    });
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-core-2840",
            &[
                ("command", search.spec.command.to_string().into()),
                ("reruns", format!("{}", search.spec.reruns).into()),
            ],
        ))
        .color(t.text_faint),
    );
    if ui.button(crate::i18n::text("repo-core-1263")).clicked() {
        acts.push(Action::Interaction(InteractionAct::Stop));
    }
}
