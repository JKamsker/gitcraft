//! Repository-tool dialog dispatch and virtualized reference browsing.

use super::*;

pub(in crate::repo) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.set_width(850.0);
    if state.tool == Tool::Subtrees {
        subtrees::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::StateReceipts {
        receipts::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Deferred {
        deferred::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Virtuals {
        virtuals::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Lists {
        lists::show(ui, state, acts);
        return;
    }
    if matches!(state.tool, Tool::Turn | Tool::AggregateTurn) {
        turns::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Outbox {
        outbox::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Sharing {
        sharing::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Dependencies {
        dependencies::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::SubmoduleConflict {
        submodule_conflict::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::ForceRoute {
        routes::force(ui, state, acts);
        return;
    }
    if state.tool == Tool::Routes {
        routes::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Publication {
        publication::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Remote {
        remotes::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Replacements {
        replacements_view(ui, state, acts);
        return;
    }
    if state.tool == Tool::Reftable {
        reftable_view(ui, state, acts);
        return;
    }
    if state.tool == Tool::Object {
        object_view(ui, state, acts);
        return;
    }
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-5d3cc263",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-df2937d4",
    ));
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-8dd5e5cc",
        ));
        ui.text_edit_singleline(&mut state.query);
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-9b9a8d05",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::AllRefs(state.query.clone())));
        }
    });
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let browser = match result {
        Ok(Data::Refs(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            return;
        }
    };
    if browser.root_fallback {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-f2d4cf1c",
        ));
    }
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-84e9b213",
        ));
        ui.text_edit_singleline(&mut state.filter);
    });
    refs_list(ui, &browser.refs, &state.filter, acts);
    ui.separator();
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-e27da1c1",
    ));
    egui::ScrollArea::both()
        .id_salt("namespace_history")
        .max_height(260.0)
        .show(ui, |ui| {
            ui.monospace(&browser.history);
        });
}

pub(super) fn refs_list(
    ui: &mut egui::Ui,
    refs: &[all_refs::Reference],
    filter: &str,
    acts: &mut Vec<Action>,
) -> usize {
    let filtered: Vec<_> = refs.iter().filter(|r| r.name.contains(filter)).collect();
    let mut rendered = 0;
    let height = ui
        .text_style_height(&egui::TextStyle::Body)
        .max(ui.spacing().interact_size.y);
    egui::ScrollArea::vertical()
        .id_salt("all_refs")
        .max_height(270.0)
        .show_rows(ui, height, filtered.len(), |ui, rows| {
            for index in rows {
                rendered += 1;
                let r = filtered[index];
                ui.horizontal(|ui| {
                    if ui.selectable_label(false, &r.name).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.name.clone())));
                    }
                    ui.monospace(&r.oid);
                    ui.label(&r.kind);
                    if !r.symbolic.is_empty() && ui.link(format!("→ {}", r.symbolic)).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.symbolic.clone())));
                    }
                    if ui
                        .small_button(crate::i18n::text(
                            "repo-workflows-repository_tools-refs-list-edec79bb",
                        ))
                        .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::Object(r.oid.clone())));
                    }
                    if ui
                        .small_button(crate::i18n::text(
                            "repo-workflows-repository_tools-refs-list-c2a1a3d6",
                        ))
                        .clicked()
                    {
                        ui.ctx().copy_text(r.oid.clone());
                    }
                });
            }
        });
    rendered
}
