use super::{Act, Data, State};
use crate::{
    git::drafts::virtuals as logic,
    repo::{Action, Dialog, RepoTab},
};
pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    match act {
        Act::EditVirtual(view, edit) => {
            tab.repository_tool_mutation("Save virtual branch ownership", false, ctx, move |r| {
                logic::edit(r.repo, &view, edit, &|| !r.control.is_cancelled())
            })
        }
        Act::ToggleVirtual(view, name) => tab.repository_tool_mutation(
            "Change virtual branch materialization",
            false,
            ctx,
            move |r| logic::toggle(r.repo, &view, &name, &|| !r.control.is_cancelled()),
        ),
        Act::UndoVirtual(view) => {
            tab.repository_tool_mutation("Undo virtual branch operation", false, ctx, move |r| {
                logic::undo(r.repo, &view, &|| !r.control.is_cancelled())
            })
        }
        Act::CommitVirtual(plan) => tab.repository_tool_mutation(
            "Commit virtual branch owned hunks",
            false,
            ctx,
            move |r| logic::commit(r.repo, &plan, &|| !r.control.is_cancelled()),
        ),
        Act::PrepareVirtual(view, name) => {
            let repo = tab.path.clone();
            let expected = repo.clone();
            let request = match &tab.dialog {
                Some(Dialog::RepositoryTools(s)) => s.id,
                _ => return None,
            };
            tab.then_or(
                ctx,
                false,
                Err,
                move || {
                    logic::prepare_commit(&repo, &view, &name, &|| {
                        !crate::tasks::cancellation().is_cancelled()
                    })
                    .map(|p| Data::VirtualCommit(Box::new(p)))
                },
                move |tab, result, _| {
                    if tab.path == expected
                        && let Some(Dialog::RepositoryTools(s)) = &mut tab.dialog
                        && s.id == request
                    {
                        s.result = Some(result);
                        s.acknowledged = false;
                    }
                },
            );
        }
        other => return Some(other),
    }
    None
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Parallel branches in this folder");
    ui.label("Each hunk belongs to one real branch. The checked-out branch stays at its anchor commit, and Git's real index stays intact. Virtual branch commits use reviewed raw contents, create unsigned commits and bypass hooks and filters. Changed ownership and overlapping activation require explicit resolution. Working-folder changes require native macOS/Linux.");
    if ui
        .button("Refresh ownership and working snapshot")
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Virtuals));
    }
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(d) => d,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e.as_str());
            return;
        }
    };
    match data {
        Data::Virtual(view) => {
            if let Some(e) = &view.stale {
                ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            }
            if let Some(m) = &view.state.mode {
                ui.label(format!(
                    "Anchor: {} at {}",
                    m.anchor,
                    crate::repo::short(&m.base)
                ));
            }
            ui.horizontal(|ui| {
                ui.label("New real branch:");
                ui.text_edit_singleline(&mut state.query);
                if ui.button("Create virtual branch").clicked() {
                    acts.push(Action::RepositoryTools(Act::EditVirtual(
                        Box::new(view.clone()),
                        logic::Edit::Create(state.query.clone()),
                    )));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Undo last virtual operation").clicked() {
                    acts.push(Action::RepositoryTools(Act::UndoVirtual(Box::new(
                        view.clone(),
                    ))));
                }
                if view.state.mode.is_some()
                    && ui
                        .button("Leave mode; retain files and branch refs")
                        .clicked()
                {
                    acts.push(Action::RepositoryTools(Act::EditVirtual(
                        Box::new(view.clone()),
                        logic::Edit::Leave,
                    )));
                }
            });
            let Some(mode) = &view.state.mode else {
                ui.label("Create a new branch to start. Existing working hunks remain unassigned until you choose their owners.");
                return;
            };
            egui::ScrollArea::vertical().max_height(500.0).show(ui,|ui|{
                for b in &mode.branches {
                    egui::CollapsingHeader::new(format!("{} · {} · {} owned hunks",b.name,if b.active{"active"}else{"inactive"},b.owned.len())).id_salt(("virtual-branch",&b.name)).default_open(true).show(ui,|ui|{
                        ui.label(format!("Real branch tip: {}",b.tip));
                        let key=ui.id().with(("virtual-message",&view.version,&b.name));let mut message=ui.ctx().data_mut(|d|d.get_temp::<String>(key)).unwrap_or_else(||b.message.clone());
                        if ui.add(egui::TextEdit::multiline(&mut message).hint_text("Draft commit message").desired_rows(2)).changed(){ui.ctx().data_mut(|d|d.insert_temp(key,message.clone()));}
                        if message!=b.message&&ui.button("Save branch draft").clicked(){acts.push(Action::RepositoryTools(Act::EditVirtual(Box::new(view.clone()),logic::Edit::Message(b.name.clone(),message))));}
                        ui.horizontal(|ui|{
                            if ui.button(if b.active{"Deactivate these hunks"}else{"Activate these hunks"}).clicked(){acts.push(Action::RepositoryTools(Act::ToggleVirtual(Box::new(view.clone()),b.name.clone())));}
                            if ui.add_enabled(b.active&&!b.message.trim().is_empty(),egui::Button::new("Preview commit of owned hunks…")).clicked(){acts.push(Action::RepositoryTools(Act::PrepareVirtual(Box::new(view.clone()),b.name.clone())));}
                        });
                        for h in &b.owned {if b.active && view.hunks.iter().all(|now|now.key!=h.key){ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger,format!("Stale ownership: {}",h.path));if ui.button(format!("Release stale ownership {}",&h.key[..8])).clicked(){acts.push(Action::RepositoryTools(Act::EditVirtual(Box::new(view.clone()),logic::Edit::Release(b.name.clone(),h.key.clone()))));}}else if !b.active{ui.label(format!("Saved inactive: {} · {}",h.path,&h.key[..8]));}}
                    });
                }
                ui.separator();ui.label("Current hunks and their owners:");
                for h in &view.hunks {
                    let owner=mode.branches.iter().find(|b|b.active&&b.owned.iter().any(|own|own.key==h.key));
                    ui.horizontal(|ui|{
                        ui.label(&h.path);
                        egui::ComboBox::from_id_salt(("virtual-owner",&h.key)).selected_text(owner.map_or("Unassigned",|b|b.name.as_str())).show_ui(ui,|ui|{
                            if ui.selectable_label(owner.is_none(),"Unassigned").clicked(){acts.push(Action::RepositoryTools(Act::EditVirtual(Box::new(view.clone()),logic::Edit::Assign(String::new(),h.key.clone()))));}
                            for b in mode.branches.iter().filter(|b|b.active){if ui.selectable_label(owner.is_some_and(|o|o.name==b.name),&b.name).clicked(){acts.push(Action::RepositoryTools(Act::EditVirtual(Box::new(view.clone()),logic::Edit::Assign(b.name.clone(),h.key.clone()))));}}
                        });
                    });
                    ui.monospace(h.patch.chars().take(12000).collect::<String>());if h.patch.chars().count()>12000{ui.label("Hunk display truncated; inspect the file diff before assigning ownership.");}
                }
            });
        }
        Data::VirtualCommit(p) => {
            ui.label(format!("Only owned hunks of {} will enter this branch commit. The folder, anchor and real index remain as reviewed.",p.name));
            egui::ScrollArea::vertical()
                .max_height(430.0)
                .show(ui, |ui| {
                    ui.monospace(p.patch.chars().take(64000).collect::<String>());
                });
            if p.patch.chars().count() > 64000 {
                ui.label("Preview display truncated at 64000 characters.");
            }
            ui.checkbox(
                &mut state.acknowledged,
                "Commit this exact owned-hunk tree as an unsigned virtual branch commit",
            );
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new("Commit this branch only"),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::CommitVirtual(p.clone())));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_panel_exposes_stale_ownership_and_pins_activation_to_reviewed_state()
    -> Result<(), String> {
        let f = crate::git::drafts::virtuals::tests::Fixture::new()?;
        f.write("file", f.base().replace("line 3\n", "ONE\n").as_bytes())?;
        f.edit(logic::Edit::Create("alpha".into()))?;
        let key = f.own("alpha", "+ONE")?;
        f.write("file", f.base().replace("line 3\n", "CHANGED\n").as_bytes())?;
        let view = f.read()?;
        let version = view.version.clone();
        let mut state = State {
            id: 1,
            query: String::new(),
            result: Some(Ok(Data::Virtual(view))),
            filter: String::new(),
            tool: super::super::Tool::Virtuals,
            acknowledged: false,
            compatible: false,
            parent: String::new(),
            effective: true,
        };
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1600.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        let text = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(text.iter().any(|s| s.contains("Stale ownership: file")));
        let label = format!("Release stale ownership {}", &key[..8]);
        let pos = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                _ => None,
            })
            .ok_or("No stale release button")?;
        out.textures_delta.clear();
        let mut out = ctx.run_ui(
            egui::RawInput {
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
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        out.textures_delta.clear();
        assert!(acts.iter().any(|a|matches!(a,Action::RepositoryTools(Act::EditVirtual(v,logic::Edit::Release(name,selected))) if name=="alpha"&&selected==&key&&v.version==version)));
        Ok(())
    }
}
