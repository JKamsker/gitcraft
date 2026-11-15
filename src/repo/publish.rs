//! Explicit publication chooses a destination before entering the normal push checklist.
use crate::git::repository_tools::routes::{self, PreparedPush};
use crate::repo::{Action, dialogs::Outcome, repository_tools::Act};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

pub struct Form {
    repo: PathBuf,
    branch: String,
    remotes: Vec<String>,
    remote: String,
    destination: String,
    pending: Option<Receiver<Result<PreparedPush, String>>>,
    preview: Option<PreparedPush>,
    error: Option<String>,
}
impl Form {
    pub(super) fn new(repo: PathBuf, branch: String, mut remotes: Vec<String>) -> Self {
        remotes.sort();
        let remote = remotes
            .iter()
            .find(|r| *r == "origin")
            .or_else(|| remotes.first())
            .cloned()
            .unwrap_or_default();
        Self {
            repo,
            destination: branch.clone(),
            branch,
            remotes,
            remote,
            pending: None,
            preview: None,
            error: None,
        }
    }
    pub(in crate::repo) fn show(&mut self, ui: &mut egui::Ui, idle: bool) -> Outcome {
        if let Some(result) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.pending = None;
            match result {
                Ok(preview) => {
                    self.preview = Some(preview);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        ui.heading(crate::i18n::text("publish-title"));
        ui.label(crate::i18n::text("publish-source"));
        ui.label(format!("refs/heads/{}", self.branch));
        if let Some(error) = &self.error {
            ui.label(error);
        }
        let mut outcome = Outcome::Open;
        if let Some(preview) = &self.preview {
            ui.label(&preview.route.commit);
            ui.label(crate::i18n::text("publish-remote"));
            ui.label(&preview.route.remote);
            ui.label(crate::i18n::text("publish-destination"));
            ui.label(&preview.route.destination);
            ui.label(crate::i18n::text("publish-urls"));
            for url in &preview.route.urls {
                ui.label(url);
            }
            ui.label(crate::i18n::text("publish-checklist"));
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text("publish-confirm")),
                )
                .clicked()
            {
                outcome = Outcome::Done(Box::new(Action::RepositoryTools(Act::PublishPrepared(
                    Box::new(preview.clone()),
                ))));
            }
            if ui.button(crate::i18n::text("publish-edit")).clicked() {
                self.preview = None;
            }
        } else {
            ui.add_enabled_ui(self.pending.is_none(), |ui| {
                if self.remotes.is_empty() {
                    ui.label(crate::i18n::text("publish-no-remotes"));
                }
                egui::ComboBox::from_label(crate::i18n::text("publish-remote"))
                    .selected_text(&self.remote)
                    .show_ui(ui, |ui| {
                        for remote in &self.remotes {
                            ui.selectable_value(&mut self.remote, remote.clone(), remote);
                        }
                    });
                let label = ui.label(crate::i18n::text("publish-destination"));
                ui.text_edit_singleline(&mut self.destination)
                    .labelled_by(label.id);
                if ui
                    .add_enabled(
                        idle && !self.remote.is_empty() && !self.destination.is_empty(),
                        egui::Button::new(crate::i18n::text("publish-preview")),
                    )
                    .clicked()
                {
                    let (tx, rx) = mpsc::channel();
                    let (repo, branch, remote, destination) = (
                        self.repo.clone(),
                        self.branch.clone(),
                        self.remote.clone(),
                        self.destination.clone(),
                    );
                    crate::connections::spawn(move || {
                        let _ = tx.send(routes::prepare_publication(
                            &repo,
                            &branch,
                            &remote,
                            &destination,
                        ));
                    });
                    self.pending = Some(rx);
                }
            });
        }
        if self.pending.is_some() {
            ui.add(crate::spinner::Spinner::new());
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        if ui.button(crate::i18n::text("tracking-cancel")).clicked() {
            outcome = Outcome::Cancel;
        }
        outcome
    }
}

#[cfg(test)]
mod tests;

impl super::RepoTab {
    pub(super) fn publish_item(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        branch: &str,
        idle: bool,
    ) {
        if crate::menu_item::MenuItem::new(crate::i18n::text("publish-title"))
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(super::Dialog::Publish(Box::new(Form::new(
                self.path.clone(),
                branch.into(),
                self.remotes.iter().map(|r| r.name.clone()).collect(),
            )))));
        }
    }
}
