use super::{Act, Data, State};
use crate::{
    git::drafts::lists as logic,
    repo::{Action, RepoTab},
};
pub(super) fn act(tab: &mut RepoTab, act: Act, ctx: &egui::Context) -> Option<Act> {
    match act {
        Act::EditList(view, edit) => {
            tab.repository_tool_mutation("Save changelist", false, ctx, move |r| {
                logic::edit(r.repo, &view, edit, &|| !r.control.is_cancelled())
            })
        }
        Act::CommitList(view, id) => {
            tab.repository_tool_mutation("Commit one changelist", false, ctx, move |r| {
                logic::commit(r.repo, &view, &id, &|| !r.control.is_cancelled())
            })
        }
        Act::ShelveList(view, id, restore) => tab.repository_tool_mutation(
            if restore {
                "Unshelve one changelist"
            } else {
                "Shelve one changelist"
            },
            false,
            ctx,
            move |r| logic::shelve(r.repo, &view, &id, restore, &|| !r.control.is_cancelled()),
        ),
        other => return Some(other),
    }
    None
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-16f5b799",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-1c0954b2",
    ));
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-04245473",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Lists));
    }
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-79af8f6d",
        ));
        ui.text_edit_singleline(&mut state.query);
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-4759498a",
            ))
            .clicked()
            && let Some(Ok(Data::Lists(v))) = &state.result
        {
            acts.push(Action::RepositoryTools(Act::EditList(
                Box::new(v.clone()),
                logic::Edit::Create(state.query.clone(), String::new()),
            )));
        }
    });
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-ab27abcb",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::StateReceipts));
    }
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let view = match result {
        Ok(Data::Lists(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e.as_str());
            return;
        }
    };
    egui::ScrollArea::vertical()
        .max_height(500.0)
        .show(ui, |ui| {
            for list in &view.lists.lists {
                egui::CollapsingHeader::new(format!(
                    "{} · {} files{}",
                    list.name,
                    list.paths.len(),
                    if list.shelf.is_some() {
                        " · shelved"
                    } else {
                        ""
                    }
                ))
                .id_salt(&list.id)
                .default_open(true)
                .show(ui, |ui| {
                    let rename_id = ui.id().with(("list-name", &view.version, &list.id));
                    let mut name = ui
                        .ctx()
                        .data_mut(|d| d.get_temp::<String>(rename_id))
                        .unwrap_or_else(|| list.name.clone());
                    if ui.text_edit_singleline(&mut name).changed() {
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(rename_id, name.clone()));
                    }
                    if name != list.name
                        && ui
                            .button(crate::i18n::text(
                                "repo-workflows-repository_tools-show-e01205cf",
                            ))
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::EditList(
                            Box::new(view.clone()),
                            logic::Edit::Rename(list.id.clone(), name),
                        )));
                    }
                    let id = ui.id().with(("draft-message", &view.version, &list.id));
                    let mut message = ui
                        .ctx()
                        .data_mut(|d| d.get_temp::<String>(id))
                        .unwrap_or_else(|| list.message.clone());
                    if ui
                        .add(
                            egui::TextEdit::multiline(&mut message)
                                .hint_text(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-454ee70f",
                                ))
                                .desired_rows(2),
                        )
                        .changed()
                    {
                        ui.ctx().data_mut(|d| d.insert_temp(id, message.clone()));
                    }
                    if message != list.message
                        && ui
                            .button(crate::i18n::text(
                                "repo-workflows-repository_tools-show-ae30be2e",
                            ))
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::EditList(
                            Box::new(view.clone()),
                            logic::Edit::Message(list.id.clone(), message),
                        )));
                    }
                    ui.horizontal(|ui| {
                        if list.shelf.is_some() {
                            if ui
                                .button(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-92958e20",
                                ))
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::ShelveList(
                                    Box::new(view.clone()),
                                    list.id.clone(),
                                    true,
                                )));
                            }
                        } else {
                            if ui
                                .add_enabled(
                                    !list.paths.is_empty() && !list.message.trim().is_empty(),
                                    egui::Button::new(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-41a222be",
                                    )),
                                )
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::CommitList(
                                    Box::new(view.clone()),
                                    list.id.clone(),
                                )));
                            }
                            if ui
                                .add_enabled(
                                    !list.paths.is_empty(),
                                    egui::Button::new(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-17c47592",
                                    )),
                                )
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::ShelveList(
                                    Box::new(view.clone()),
                                    list.id.clone(),
                                    false,
                                )));
                            }
                        }
                    });
                    for path in &list.paths {
                        ui.label(path);
                    }
                });
            }
            ui.separator();
            ui.label(crate::i18n::text(
                "repo-workflows-repository_tools-show-9aa26ad9",
            ));
            for path in &view.paths {
                let owner = view
                    .lists
                    .lists
                    .iter()
                    .find(|l| l.shelf.is_none() && l.paths.contains(path));
                ui.horizontal(|ui| {
                    ui.label(path);
                    egui::ComboBox::from_id_salt(("changelist-owner", path))
                        .selected_text(owner.map_or(
                            crate::i18n::text("repo-workflows-repository_tools-show-14d33bd0"),
                            |l| l.name.as_str(),
                        ))
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    owner.is_none(),
                                    crate::i18n::text(
                                        "repo-workflows-repository_tools-show-14d33bd0",
                                    ),
                                )
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::EditList(
                                    Box::new(view.clone()),
                                    logic::Edit::Assign(String::new(), path.clone()),
                                )));
                            }
                            for list in view.lists.lists.iter().filter(|l| l.shelf.is_none()) {
                                if ui
                                    .selectable_label(
                                        owner.is_some_and(|l| l.id == list.id),
                                        &list.name,
                                    )
                                    .clicked()
                                {
                                    acts.push(Action::RepositoryTools(Act::EditList(
                                        Box::new(view.clone()),
                                        logic::Edit::Assign(list.id.clone(), path.clone()),
                                    )));
                                }
                            }
                        });
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_lists_show_their_own_messages_and_commit_button_pins_one_list() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let repo = dir.path();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@example.com"],
            vec!["config", "commit.gpgSign", "false"],
            vec!["config", "core.autocrlf", "false"],
        ] {
            crate::git::run(Some(repo), &args)?;
        }
        std::fs::write(repo.join("a"), b"base\n").map_err(|e| e.to_string())?;
        crate::git::run(Some(repo), &["add", "a"])?;
        crate::git::run(Some(repo), &["commit", "-qm", "base"])?;
        std::fs::write(repo.join("a"), b"feature\n").map_err(|e| e.to_string())?;
        let read = || logic::read(repo, &|| true);
        logic::edit(
            repo,
            &read()?,
            logic::Edit::Create("Feature".into(), "Only feature draft".into()),
            &|| true,
        )?;
        let id = read()?.lists.lists[0].id.clone();
        logic::edit(
            repo,
            &read()?,
            logic::Edit::Assign(id.clone(), "a".into()),
            &|| true,
        )?;
        logic::edit(
            repo,
            &read()?,
            logic::Edit::Create("Cleanup".into(), "Independent cleanup draft".into()),
            &|| true,
        )?;
        let view = read()?;
        let version = view.version.clone();
        let mut state = State {
            id: 1,
            query: String::new(),
            result: Some(Ok(Data::Lists(view))),
            filter: String::new(),
            tool: super::super::Tool::Lists,
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
                    egui::vec2(1000.0, 1600.0),
                )),
                ..Default::default()
            },
            |ui| show(ui, &mut state, &mut acts),
        );
        let texts = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(texts.iter().any(|t| t.contains("Only feature draft")));
        assert!(
            texts
                .iter()
                .any(|t| t.contains("Independent cleanup draft"))
        );
        let pos = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Commit this list only" => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                _ => None,
            })
            .ok_or("No commit button")?;
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
        assert!(acts.iter().any(|a|matches!(a,Action::RepositoryTools(Act::CommitList(v,selected)) if selected==&id && v.version==version)));
        Ok(())
    }
}
