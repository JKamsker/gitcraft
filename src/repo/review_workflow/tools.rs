use super::echo;
use crate::repo::{Action, RepoTab};
#[derive(Default)]
pub(crate) struct Tools {
    pub open: bool,
    pub echo: echo::State,
    pub intents: super::intents::State,
    pub contracts: super::contracts::State,
    pub radar: super::radar::State,
    pub reproducer: super::reproducer::State,
    pub choreography: super::choreography::State,
}
#[derive(Clone)]
pub(crate) enum ToolsAct {
    Open,
    Echo(echo::Act),
    Intent(super::intents::Act),
    Contract(super::contracts::Act),
    Radar(super::radar::Act),
    Reproducer(super::reproducer::Act),
    Choreography(super::choreography::Act),
}
impl RepoTab {
    pub(in crate::repo) fn review_tools_act(&mut self, act: ToolsAct, ctx: &egui::Context) {
        match act {
            ToolsAct::Open => {
                self.review_tools.open = true;
                self.echo_act(echo::Act::Load, ctx);
            }
            ToolsAct::Choreography(act) => self.choreography_act(act, ctx),
            ToolsAct::Reproducer(act) => self.reproducer_act(act, ctx),
            ToolsAct::Radar(act) => self.radar_act(act, ctx),
            ToolsAct::Contract(act) => self.contract_act(act, ctx),
            ToolsAct::Intent(act) => self.intent_act(act, ctx),
            ToolsAct::Echo(act) => self.echo_act(act, ctx),
        }
    }
    pub(in crate::repo) fn review_tools_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let mut open = self.review_tools.open;
        if !open {
            return;
        }
        egui::Window::new("Review evidence and release coordination")
            .id(egui::Id::new(("review-tools", &self.path)))
            .open(&mut open)
            .default_size([760., 620.])
            .show(ctx, |ui| {
                egui::ScrollArea::both().show(ui, |ui| {
                    egui::CollapsingHeader::new("Personal reviewer echo")
                        .show(ui, |ui| echo::show(ui, &mut self.review_tools.echo, acts));
                    egui::CollapsingHeader::new("Release choreography across repositories").show(
                        ui,
                        |ui| {
                            super::choreography::show(
                                ui,
                                &mut self.review_tools.choreography,
                                &self.path,
                                acts,
                            )
                        },
                    );
                    egui::CollapsingHeader::new("AI objections with a reproducer").show(ui, |ui| {
                        super::reproducer::show(ui, &mut self.review_tools.reproducer, acts)
                    });
                    egui::CollapsingHeader::new("Fix survival radar").show(ui, |ui| {
                        super::radar::show(ui, &mut self.review_tools.radar, acts)
                    });
                    egui::CollapsingHeader::new("Backport by contract").show(ui, |ui| {
                        super::contracts::show(
                            ui,
                            &mut self.review_tools.contracts,
                            &self.path,
                            &self
                                .refs
                                .local
                                .iter()
                                .map(|b| b.name.clone())
                                .collect::<Vec<_>>(),
                            acts,
                        )
                    });
                    egui::CollapsingHeader::new("Intent cards").show(ui, |ui| {
                        super::intents::show(
                            ui,
                            &mut self.review_tools.intents,
                            &self.remotes,
                            acts,
                        )
                    });
                });
            });
        self.review_tools.open = open;
    }
}
