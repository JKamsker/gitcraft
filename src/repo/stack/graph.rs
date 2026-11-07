//! Explicit parent edges beside the commit graph. Ref placement alone cannot express a
//! recorded dependency after a parent is rewritten, or when two layers share a tip.

use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

use super::{StackAct, looped, trees};

impl RepoTab {
    pub(in crate::repo) fn graph_stacks(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        if self.stacks.layers.is_empty() || self.ghost.drawing() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        egui::CollapsingHeader::new("Branch dependencies")
            .id_salt("graph-stack-relations")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("graph-stack-layers")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for tree in trees(&self.stacks.layers) {
                            ui.label(egui::RichText::new(tree.title()).strong());
                            for (depth, layer) in tree.rows {
                                ui.horizontal_wrapped(|ui| {
                                    ui.add_space((depth.saturating_sub(1).min(12) * 12) as f32);
                                    if ui.link(&layer.parent).clicked()
                                        && let Some(b) =
                                            self.refs.local.iter().find(|b| b.name == layer.parent)
                                    {
                                        acts.push(Action::SelectHash(b.target.clone()));
                                    }
                                    ui.label("→");
                                    if ui.link(&layer.branch).clicked()
                                        && let Some(b) =
                                            self.refs.local.iter().find(|b| b.name == layer.branch)
                                    {
                                        acts.push(Action::SelectHash(b.target.clone()));
                                    }
                                    if ui
                                        .small_button(format!("+{} own commits", layer.own))
                                        .on_hover_text("Compare this layer with its parent")
                                        .clicked()
                                    {
                                        acts.push(crate::repo::actions::compare_branches_action(
                                            crate::repo::actions::BranchRef::local(&layer.parent),
                                            crate::repo::actions::BranchRef::local(&layer.branch),
                                            true,
                                        ));
                                    }
                                    if let Some(pr) =
                                        self.prs.iter().find(|p| p.branch == layer.branch)
                                    {
                                        if ui
                                            .link(format!("PR #{}", pr.number))
                                            .on_hover_text(&pr.title)
                                            .clicked()
                                        {
                                            acts.push(Action::OpenPr(pr.number));
                                        }
                                    } else {
                                        ui.weak("No open PR");
                                    }
                                    if layer.parent_missing {
                                        ui.colored_label(t.warning, "Parent missing");
                                        if ui.small_button("Choose parent…").clicked() {
                                            acts.push(Action::Stack(StackAct::OpenBasedOn(
                                                layer.branch.clone(),
                                            )));
                                        }
                                    } else if layer.needs_restack() {
                                        ui.colored_label(
                                            t.warning,
                                            format!(
                                                "Needs restack ({} parent commits)",
                                                layer.behind
                                            ),
                                        );
                                        if ui
                                            .add_enabled(
                                                !self.is_busy(),
                                                egui::Button::new("Restack…").small(),
                                            )
                                            .clicked()
                                        {
                                            acts.push(Action::Stack(StackAct::OpenRestack(Some(
                                                layer.parent.clone(),
                                            ))));
                                        }
                                    } else {
                                        ui.weak("Up to date");
                                    }
                                });
                            }
                        }
                        let cycles = looped(&self.stacks.layers);
                        if !cycles.is_empty() {
                            ui.colored_label(
                                t.warning,
                                format!("Cyclic parents: {}", cycles.join(", ")),
                            );
                            if ui.button("Repair parent…").clicked() {
                                acts.push(Action::Stack(StackAct::OpenBasedOn(cycles[0].clone())));
                            }
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{Branch, stack::Layer};

    #[test]
    fn graph_dependency_edges_keep_shared_tips_and_stale_layers_visible() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/stack-graph".into(), &ctx);
        for (branch, parent, behind) in [
            ("schema", "main", 0),
            ("api", "schema", 2),
            ("ui", "api", 0),
        ] {
            tab.stacks.layers.push(Layer {
                branch: branch.into(),
                parent: parent.into(),
                base: None,
                own: 3,
                behind,
                parent_missing: false,
            });
            tab.refs.local.push(Branch {
                name: branch.into(),
                target: "same-tip".into(),
                is_head: false,
                upstream: String::new(),
                track: String::new(),
            });
        }
        let mut acts = Vec::new();
        let mut clicked = None;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            tab.graph_stacks(ui, &mut acts);
        });
        let text: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        assert!(
            text.iter().any(|t| t == "main → schema → api → ui"),
            "{text:?}"
        );
        assert!(text.iter().any(|t| t == "Needs restack (2 parent commits)"));
        assert_eq!(text.iter().filter(|t| *t == "+3 own commits").count(), 3);
        assert_eq!(text.iter().filter(|t| *t == "No open PR").count(), 3);
        for shape in &output.shapes {
            if let egui::Shape::Text(t) = &shape.shape
                && t.galley.text() == "ui"
            {
                clicked = Some(t.pos + t.galley.size() / 2.0);
            }
        }
        let pos = clicked.expect("ui link is painted despite shared tip");
        let input = egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| tab.graph_stacks(ui, &mut acts));
        assert!(
            acts.iter()
                .any(|a| matches!(a, Action::SelectHash(h) if h == "same-tip"))
        );
    }
}
