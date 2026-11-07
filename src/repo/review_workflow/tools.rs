use super::echo;
use crate::repo::{Action, RepoTab};
#[derive(Default)]
pub(crate) struct Tools {
    pub open: bool,
    pub echo: echo::State,
}
#[derive(Clone)]
pub(crate) enum ToolsAct {
    Open,
    Echo(echo::Act),
}
impl RepoTab {
    pub(in crate::repo) fn review_tools_act(&mut self, act: ToolsAct, ctx: &egui::Context) {
        match act {
            ToolsAct::Open => {
                self.review_tools.open = true;
                self.echo_act(echo::Act::Load, ctx);
            }
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
                    echo::show(ui, &mut self.review_tools.echo, acts);
                });
            });
        self.review_tools.open = open;
    }
}
