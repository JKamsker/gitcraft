use super::{Act, Data, State};
use crate::repo::Action;
fn lines(ui: &mut egui::Ui, title: &str, values: &mut Vec<String>) {
    ui.label(title);
    let mut text = values.join("\n");
    if ui
        .add(
            egui::TextEdit::multiline(&mut text)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        )
        .changed()
    {
        *values = text
            .lines()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
    }
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-c23fbbaa",
    ));
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let e = match result {
        Ok(Data::Remote(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e.as_str());
            return;
        }
    };
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-show-67572031",
        &[("e-draft-name-1", (e.draft.name).clone().into())],
    ));
    egui::ScrollArea::vertical()
        .id_salt("remote_refspec_editor")
        .max_height(570.0)
        .show(ui, |ui| {
            lines(
                ui,
                crate::i18n::text("repo-workflows-repository_tools-show-89f4921c"),
                &mut e.draft.fetch_urls,
            );
            lines(
                ui,
                crate::i18n::text("repo-workflows-repository_tools-show-d05d82b6"),
                &mut e.draft.push_urls,
            );
            lines(
                ui,
                crate::i18n::text("repo-workflows-repository_tools-show-03dc5c4f"),
                &mut e.draft.fetch,
            );
            lines(
                ui,
                crate::i18n::text("repo-workflows-repository_tools-show-f279f80b"),
                &mut e.draft.push,
            );
            ui.checkbox(
                &mut e.draft.mirror,
                crate::i18n::text("repo-workflows-repository_tools-show-e37c5523"),
            );
            ui.label(crate::i18n::text(
                "repo-workflows-repository_tools-show-ea2af9ef",
            ));
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-repository_tools-show-72353618",
                ))
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::RemotePreview(Box::new(
                    e.clone(),
                ))));
            }
            if let Some(p) = &e.preview {
                if p.draft != e.draft {
                    ui.label(crate::i18n::text(
                        "repo-workflows-repository_tools-show-1b5dcef8",
                    ));
                }
                for mapping in &p.mappings {
                    ui.monospace(mapping);
                }
                ui.label(crate::i18n::message(
                    "repo-workflows-repository_tools-show-d5680e1f",
                    &[("p-prune-len-1", (p.prune.len()).into())],
                ));
                for name in &p.prune {
                    ui.monospace(name);
                }
            }
            ui.checkbox(
                &mut state.acknowledged,
                crate::i18n::text("repo-workflows-repository_tools-show-e6eadbe7"),
            );
            if let Some(why) = &e.saving_blocked {
                ui.colored_label(egui::Color32::YELLOW, why);
            }
            let ready = e.saving_blocked.is_none()
                && state.acknowledged
                && e.preview.as_ref().is_some_and(|p| p.draft == e.draft);
            if ui
                .add_enabled(
                    ready,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-repository_tools-show-4e161156",
                    )),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::SaveRemote(Box::new(
                    e.clone(),
                ))));
            }
        });
}
