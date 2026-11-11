//! Workspace cards, naming controls, and explicit UI requests.

use super::*;

impl App {
    /// The field a workspace's name is typed in, with Create (or Rename) and Cancel.
    pub(super) fn naming_field(&mut self, ui: &mut Ui, t: &Tokens) {
        let Some(naming) = &mut self.workspaces_view.naming else {
            return;
        };
        let mut done = None;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut naming.text)
                    .hint_text(crate::i18n::text("interaction-workspace-name"))
                    .desired_width(260.0)
                    .char_limit(across::workspaces::MAX_NAME),
            );
            if std::mem::take(&mut naming.focus) {
                field.request_focus();
            }
            let (enter, escape) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            // Only the field's own keys: Escape closing a menu elsewhere keeps the name.
            let own = field.has_focus() || field.lost_focus();
            let label = if naming.index.is_some() {
                crate::i18n::text("interaction-workspace-rename")
            } else {
                crate::i18n::text("interaction-workspace-create")
            };
            if ui.button(label).clicked() || (field.lost_focus() && enter) {
                done = Some(true);
            } else if ui
                .button(crate::i18n::text("interaction-workspace-cancel"))
                .clicked()
                || (own && escape)
            {
                done = Some(false);
            }
        });
        if let Some(err) = &naming.error {
            ui.colored_label(t.danger, across::workspaces::display_error(err));
        }
        match done {
            Some(true) => {
                let result = match naming.index {
                    Some(i) => self.workspaces.rename(i, &naming.text),
                    None => self.workspaces.create(&naming.text).map(|_| ()),
                };
                match result {
                    Ok(()) => self.workspaces_view.naming = None,
                    Err(e) => {
                        naming.error = Some(e);
                        naming.focus = true;
                    }
                }
            }
            Some(false) => self.workspaces_view.naming = None,
            None => {}
        }
    }

    /// One workspace: its header with the group's buttons, then a row per repository.
    pub(super) fn workspace_card(
        &mut self,
        ui: &mut Ui,
        index: usize,
        t: &Tokens,
    ) -> Option<Request> {
        let (name, repos) = {
            let ws = self.workspaces.list.get(index)?;
            (ws.name.clone(), ws.repos.clone())
        };
        let mut request = None;
        egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let renaming = self
                    .workspaces_view
                    .naming
                    .as_ref()
                    .is_some_and(|n| n.index == Some(index));
                if renaming {
                    self.naming_field(ui, t);
                } else if let Some(r) = self.workspace_header(ui, index, &name, &repos, t) {
                    request = Some(r);
                }
                if let Some(r) = self.confirm_line(ui, index, &name, &repos, t) {
                    request = Some(r);
                }
                if let Some((i, text)) = &self.workspaces_view.notice
                    && *i == index
                {
                    ui.colored_label(t.warning, across::display_error(text));
                }
                if repos.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::text("interaction-workspace-empty"))
                            .color(t.text_faint)
                            .small(),
                    );
                }
                ui.spacing_mut().item_spacing.y = 0.0;
                for path in &repos {
                    let state = self.workspaces_view.repos.get(path);
                    match workspace_row(ui, path, state, t) {
                        Some(RowClick::Open) => request = Some(Request::Open(vec![path.clone()])),
                        Some(RowClick::Remove) => {
                            request = Some(Request::RemoveRepo(index, path.clone()));
                        }
                        None => {}
                    }
                }
            });
        request
    }

    /// The question a Delete or a big Open all asks first, under the card's header.
    pub(super) fn confirm_line(
        &mut self,
        ui: &mut Ui,
        index: usize,
        name: &str,
        repos: &[PathBuf],
        t: &Tokens,
    ) -> Option<Request> {
        let asked = self.workspaces_view.confirm?;
        let (text, button, request) = match asked {
            Confirm::Delete(i) if i == index => (
                crate::i18n::message(
                    "interaction-workspace-delete-question",
                    &[("name", name.into())],
                ),
                crate::i18n::text("interaction-workspace-delete"),
                Request::Delete(index),
            ),
            Confirm::OpenAll(i) if i == index => (
                crate::i18n::message(
                    "interaction-workspace-open-question",
                    &[("count", repos.len().into())],
                ),
                crate::i18n::text("interaction-workspace-open-all"),
                Request::Open(repos.to_vec()),
            ),
            _ => return None,
        };
        let mut answer = None;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(text).color(t.text_dim));
            if ui.button(button).clicked() {
                answer = Some(true);
            }
            if ui
                .button(crate::i18n::text("interaction-workspace-cancel"))
                .clicked()
            {
                answer = Some(false);
            }
        });
        let yes = answer?;
        self.workspaces_view.confirm = None;
        yes.then_some(request)
    }

    pub(super) fn workspace_header(
        &mut self,
        ui: &mut Ui,
        index: usize,
        name: &str,
        repos: &[PathBuf],
        t: &Tokens,
    ) -> Option<Request> {
        let mut request = None;
        let busy = repos.iter().any(|p| {
            self.workspaces_view
                .repos
                .get(p)
                .is_some_and(|s| s.busy.is_some())
        });
        ui.horizontal(|ui| {
            // The buttons first, from the right: a long name takes what is left and is cut.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let more = icons::small_button(
                    ui,
                    "settings",
                    crate::i18n::text("interaction-workspace-more"),
                );
                egui::Popup::menu(&more)
                    .style(theme::menu_style)
                    .show(|ui| {
                        if ui
                            .button(crate::i18n::text("interaction-workspace-rename-menu"))
                            .clicked()
                        {
                            self.workspaces_view.naming = Some(Naming {
                                index: Some(index),
                                text: name.to_owned(),
                                error: None,
                                focus: true,
                            });
                        }
                        if ui
                            .button(crate::i18n::text("interaction-workspace-delete-menu"))
                            .clicked()
                        {
                            self.workspaces_view.confirm = Some(Confirm::Delete(index));
                        }
                    });
                let has = !repos.is_empty();
                let open = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(
                        ui,
                        "folder-open",
                        crate::i18n::text("interaction-workspace-open-tip"),
                    )
                });
                if open.inner.clicked() {
                    if repos.len() > OPEN_WITHOUT_ASKING {
                        self.workspaces_view.confirm = Some(Confirm::OpenAll(index));
                    } else {
                        request = Some(Request::Open(repos.to_vec()));
                    }
                }
                let fetch = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(
                        ui,
                        "cloud-download",
                        crate::i18n::text("interaction-workspace-fetch-tip"),
                    )
                });
                if fetch.inner.clicked() {
                    request = Some(Request::Fetch(repos.to_vec()));
                }
                let look = ui.add_enabled_ui(has, |ui| {
                    icons::small_button(
                        ui,
                        "refresh-cw",
                        crate::i18n::text("interaction-workspace-look-tip"),
                    )
                });
                if look.inner.clicked() {
                    request = Some(Request::Look(repos.to_vec()));
                }
                let add =
                    icons::small_button(ui, "plus", crate::i18n::text("interaction-workspace-add"));
                if let Some(r) = self.add_menu(&add, index, repos) {
                    request = Some(r);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    icons::inline(ui, "rows-2", 14.0, t.accent);
                    let count = crate::i18n::message(
                        "interaction-workspace-count",
                        &[("count", repos.len().into())],
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(name)
                                .font(theme::semibold(13.0))
                                .color(t.text),
                        )
                        .truncate(),
                    )
                    .on_hover_text(format!("{name} · {count}"));
                    ui.label(RichText::new(count).color(t.text_faint).small());
                    if busy {
                        ui.add(crate::spinner::Spinner::new().size(12.0));
                    }
                });
            });
        });
        request
    }

    /// What can be added: the open tabs' and the recent repositories not in it, or another
    /// folder. Gathered only while the menu is open.
    pub(super) fn add_menu(
        &self,
        button: &egui::Response,
        index: usize,
        repos: &[PathBuf],
    ) -> Option<Request> {
        let mut request = None;
        egui::Popup::menu(button)
            .style(theme::menu_style)
            .show(|ui| {
                let mut candidates: Vec<PathBuf> = self.open_repos().0;
                for p in &self.recent {
                    if !candidates.contains(p) {
                        candidates.push(p.clone());
                    }
                }
                candidates.retain(|p| !repos.contains(p));
                for path in &candidates {
                    let label = format!("{}  ·  {}", repo_name(path), crate::remote::display(path));
                    if ui.button(label).clicked() {
                        request = Some(Request::Add(index, path.clone()));
                    }
                }
                if !candidates.is_empty() {
                    ui.separator();
                }
                if ui
                    .button(crate::i18n::text("interaction-workspace-another"))
                    .clicked()
                {
                    request = Some(Request::Browse(index));
                }
            });
        request
    }
}
