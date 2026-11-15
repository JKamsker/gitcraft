//! Owned, asynchronous upstream picker; presentation never fetches or publishes.
use crate::git::repository_tools::tracking::{self, View};
use crate::repo::dialogs::Outcome;
use crate::repo::{Action, Dialog, Refresh, RepoTab, Undo};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
};

pub struct Form {
    repo: PathBuf,
    branch: String,
    labels: Arc<BTreeMap<String, crate::ref_labels::Label>>,
    hidden: BTreeSet<String>,
    pending: Option<Receiver<Result<View, String>>>,
    view: Option<View>,
    selected: Option<String>,
    query: String,
    include_hidden: bool,
    applying: bool,
    unset: bool,
    error: Option<String>,
}
impl Form {
    fn new(
        repo: PathBuf,
        branch: String,
        labels: Arc<BTreeMap<String, crate::ref_labels::Label>>,
        hidden: BTreeSet<String>,
    ) -> Self {
        let mut form = Self {
            repo,
            branch,
            labels,
            hidden,
            pending: None,
            view: None,
            selected: None,
            query: String::new(),
            include_hidden: false,
            applying: false,
            unset: false,
            error: None,
        };
        form.refresh();
        form
    }
    fn refresh(&mut self) {
        let (tx, rx) = mpsc::channel();
        let repo = self.repo.clone();
        let branch = self.branch.clone();
        crate::connections::spawn(move || {
            let _ = tx.send(tracking::read(&repo, &branch));
        });
        self.pending = Some(rx);
    }
    fn settle(&mut self) -> Option<Outcome> {
        let result = self.pending.as_ref()?.try_recv().ok()?;
        self.pending = None;
        match result {
            Err(error) => {
                self.error = Some(error);
                self.applying = false;
            }
            Ok(view) => {
                if self.applying {
                    self.applying = false;
                    let selected = if self.unset {
                        None
                    } else {
                        self.selected.as_deref()
                    };
                    let next = view.plan(selected);
                    let prior = self.view.as_ref().map(|v| v.plan(selected));
                    if prior.as_ref() == Some(&next)
                        && let Ok(tracking::Plan { ops, undo, redo }) = next
                    {
                        return Some(Outcome::Done(Box::new(Action::UndoableOps {
                            label: crate::i18n::text("tracking-change").into(),
                            ops,
                            refresh: Refresh::All,
                            undo: Undo::Custom { undo, redo },
                        })));
                    }
                    self.selected = None;
                    self.error = Some(crate::i18n::text("tracking-stale").into());
                } else {
                    self.selected = view
                        .choices
                        .iter()
                        .find(|c| c.full == view.current_full)
                        .map(|c| c.full.clone());
                }
                self.view = Some(view);
            }
        }
        None
    }
    pub(in crate::repo) fn show(&mut self, ui: &mut egui::Ui, idle: bool) -> Outcome {
        if let Some(outcome) = self.settle() {
            return outcome;
        }
        ui.heading(crate::i18n::text("tracking-change"));
        ui.label(&self.branch);
        if let Some(error) = &self.error {
            ui.label(error);
        }
        if self.pending.is_some() {
            ui.add(crate::spinner::Spinner::new());
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        if let Some(view) = &self.view {
            let current = if view.current.is_empty() {
                crate::i18n::text("tracking-none")
            } else {
                &view.current
            };
            ui.label(format!(
                "{}: {current}",
                crate::i18n::text("tracking-current")
            ));
            if !view.current.is_empty() && !view.choices.iter().any(|c| c.full == view.current_full)
            {
                ui.label(crate::i18n::text("tracking-unavailable"));
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text(crate::i18n::text("tracking-search")),
            );
            ui.checkbox(
                &mut self.include_hidden,
                crate::i18n::text("tracking-hidden"),
            );
            let query = self.query.to_lowercase();
            let choices: Vec<_> = view
                .choices
                .iter()
                .filter_map(|c| {
                    if !self.include_hidden && self.hidden.contains(&c.remote) {
                        return None;
                    }
                    let caption = self
                        .labels
                        .get(&c.full)
                        .map_or(c.path.as_str(), |l| &l.caption);
                    format!("{} {} {caption}", c.path, c.remote)
                        .to_lowercase()
                        .contains(&query)
                        .then_some((c, caption))
                })
                .collect();
            let count = choices.len();
            let height = ui.text_style_height(&egui::TextStyle::Body) * 3.0
                + ui.spacing().item_spacing.y * 2.0;
            egui::ScrollArea::vertical().max_height(280.0).show_rows(
                ui,
                height,
                count,
                |ui, rows| {
                    let mut group = String::new();
                    for index in rows {
                        let (c, caption) = choices[index];
                        ui.push_id(&c.full, |ui| {
                            ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                if group != c.remote {
                                    ui.strong(&c.remote);
                                    group.clone_from(&c.remote);
                                }
                                if ui
                                    .selectable_label(
                                        self.selected.as_ref() == Some(&c.full),
                                        format!("{caption} — {}", c.path),
                                    )
                                    .on_hover_text(format!("{}\n{}", c.full, c.url))
                                    .clicked()
                                {
                                    self.selected = Some(c.full.clone());
                                }
                                ui.small(&c.url);
                            });
                        });
                    }
                },
            );
            if count == 0 {
                ui.label(crate::i18n::text("tracking-empty"));
            }
        }
        let mut outcome = Outcome::Open;
        ui.horizontal_wrapped(|ui| {
            let ready = idle && self.pending.is_none() && self.view.is_some();
            if ui
                .add_enabled(
                    ready && self.selected.is_some(),
                    egui::Button::new(crate::i18n::text("tracking-apply")),
                )
                .clicked()
            {
                self.applying = true;
                self.unset = false;
                self.refresh();
            }
            if ui
                .add_enabled(
                    ready && self.view.as_ref().is_some_and(|v| !v.current.is_empty()),
                    egui::Button::new(crate::i18n::text("tracking-unset")),
                )
                .clicked()
            {
                self.applying = true;
                self.unset = true;
                self.refresh();
            }
            if ui.button(crate::i18n::text("tracking-refresh")).clicked() {
                self.applying = false;
                self.refresh();
            }
            if ui.button(crate::i18n::text("tracking-cancel")).clicked() {
                outcome = Outcome::Cancel;
            }
        });
        outcome
    }
}

#[cfg(test)]
mod tests;

impl RepoTab {
    pub(in crate::repo) fn tracking_dialog(&self, branch: &str) -> Dialog {
        Dialog::Tracking(Box::new(Form::new(
            self.path.clone(),
            branch.to_owned(),
            self.ref_labels(),
            self.visibility
                .sidebar_remotes
                .union(&self.visibility.graph_remotes)
                .cloned()
                .collect(),
        )))
    }
}

impl RepoTab {
    pub(in crate::repo) fn tracking_items(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        b: &crate::git::Branch,
        idle: bool,
    ) {
        ui.label(format!(
            "{}: {}",
            crate::i18n::text("tracking-current"),
            if b.upstream.is_empty() {
                crate::i18n::text("tracking-none")
            } else {
                &b.upstream
            }
        ));
        if crate::menu_item::MenuItem::new(crate::i18n::text(if b.upstream.is_empty() {
            "tracking-set"
        } else {
            "tracking-change"
        }))
        .icon("git-branch")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(Action::Dialog(self.tracking_dialog(&b.name)));
        }
    }
}
