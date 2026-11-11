use super::*;

impl RepoTab {
    /// Branch, stash, terminal, search and the folder menu.
    pub(in crate::repo) fn repo_buttons(
        &mut self,
        ui: &mut Ui,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let head = self.refs.head.clone();
        let waits = self.waits_text();
        if icons::tool_button(
            ui,
            "git-branch-plus",
            crate::i18n::text("repo-core-3665"),
            idle && !head.is_empty(),
        )
        .on_hover_text(keymap::tip(
            ui.ctx(),
            crate::i18n::text("repo-core-3666"),
            Command::NewBranch,
        ))
        .on_disabled_hover_ui(|ui| self.why_not(ui, Command::NewBranch))
        .on_disabled_hover_text(match &waits {
            _ if head.is_empty() => crate::i18n::text("repo-core-3667").to_owned(),
            Some(w) => w.clone(),
            None => String::new(),
        })
        .clicked()
        {
            acts.push(Action::Dialog(Dialog::create_branch(&head, "HEAD")));
        }
        if icons::tool_button(
            ui,
            "archive",
            crate::i18n::text("repo-core-0862"),
            idle && !self.work.is_clean(),
        )
        .on_hover_text(keymap::tip(
            ui.ctx(),
            crate::i18n::text("repo-core-3668"),
            Command::Stash,
        ))
        .on_disabled_hover_text(match &waits {
            _ if self.work.is_clean() => crate::i18n::text("repo-core-2867").to_owned(),
            Some(w) => w.clone(),
            None => String::new(),
        })
        .clicked()
        {
            acts.push(Action::Dialog(Dialog::stash()));
        }
        let terminal = icons::tool_button(
            ui,
            "square-terminal",
            crate::i18n::text("repo-core-3669"),
            true,
        )
        .on_hover_text(keymap::tip(
            ui.ctx(),
            crate::i18n::text("repo-core-3670"),
            Command::Terminal,
        ));
        if self.terminals.is_open() {
            widgets::active_underline(ui, terminal.rect, t.accent);
        }
        if terminal.clicked() {
            acts.push(Action::Terminal(TerminalAct::Toggle));
        }
        self.branch_bands_header(ui);
        let activity = icons::tool_button(ui, "list", "", true)
            .on_hover_text(crate::i18n::text("repo-core-3671"));
        if self.activity.open {
            widgets::active_underline(ui, activity.rect, t.accent);
        }
        if activity.clicked() {
            acts.push(Action::ToggleActivity);
        }
        let search = icons::tool_button(ui, "search", "", true)
            .named(crate::i18n::text("repo-core-0057"))
            .on_hover_text(crate::i18n::message(
                "repo-core-3672",
                &[(
                    "Search",
                    keymap::label(ui.ctx(), keymap::Command::Search)
                        .to_string()
                        .into(),
                )],
            ));
        if self.search.open {
            widgets::active_underline(ui, search.rect, t.accent);
        }
        if search.clicked() {
            acts.push(Action::ToggleSearch);
        }
        if ui
            .button(crate::i18n::text("repo-core-3673"))
            .on_hover_text(crate::i18n::text("repo-core-3674"))
            .clicked()
        {
            acts.push(Action::OpenTour);
        }
        let insights = icons::tool_button(ui, "chart-column", "", true)
            .named("Insights")
            .on_hover_text(
                "Insights: change impact, CI critical path, worktrees, coupling, reviews, \
                 deployments…",
            );
        if self.insights.is_some() {
            widgets::active_underline(ui, insights.rect, t.accent);
        }
        if insights.clicked() {
            acts.push(Action::Insights(if self.insights.is_some() {
                crate::repo::insights::InsightsAct::Close
            } else {
                crate::repo::insights::InsightsAct::Open(crate::repo::insights::Kind::ALL[0])
            }));
        }
        let doctor = icons::tool_button(ui, "wrench", "", true)
            .on_hover_text(crate::i18n::text("repo-core-3675"));
        if self.doctor.is_some() {
            widgets::active_underline(ui, doctor.rect, t.accent);
        }
        if doctor.clicked() {
            acts.push(if self.doctor.is_some() {
                Action::CloseDoctor
            } else {
                Action::OpenDoctor(crate::repo::doctor::Section::ALL[0])
            });
        }
        let open = icons::tool_button(ui, "external-link", "", true)
            .named(crate::i18n::text("repo-core-3676"))
            .on_hover_text(crate::i18n::text("repo-core-3677"));
        egui::Popup::menu(&open)
            .style(crate::theme::menu_style)
            .show(|ui| {
                crate::menu_item::bounded_menu(ui, "open-in", |ui| self.open_in_items(ui, acts))
            });
    }

    pub(in crate::repo) fn open_in_items(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let page = self.web_page();
        // Over SSH, nothing of this machine can open the folder.
        let here = crate::remote::openable_here(&self.path);
        let why = here.as_ref().err().map(String::as_str);
        if MenuItem::new(reveal_label())
            .icon("folder-open")
            .refused(why)
            .explained_below(true)
            .show(ui)
            .clicked()
        {
            acts.push(Action::OpenPath(PathBuf::new()));
        }
        if MenuItem::new(crate::i18n::text("repo-core-0113"))
            .icon("code")
            .refused(why)
            .show(ui)
            .clicked()
        {
            acts.push(Action::OpenInEditor(PathBuf::new()));
        }
        ui.add_space(4.0);
        if MenuItem::new(crate::i18n::text("repo-core-3525"))
            .icon("copy")
            .show(ui)
            .clicked()
        {
            // Elsewhere, the path there: what a shell on that machine takes.
            let path = crate::remote::parse(&self.path)
                .map_or_else(|| self.path.display().to_string(), |t| t.path);
            ui.ctx().copy_text(path);
        }
        ui.separator();
        if ui.button(crate::i18n::text("repo-core-3678")).clicked() {
            acts.push(Action::OpenSshSetup);
            ui.close();
        }
        if MenuItem::new(crate::i18n::text("repo-core-3679"))
            .show(ui)
            .clicked()
        {
            acts.push(Action::ReviewTools(
                crate::repo::review_workflow::ToolsAct::Open,
            ));
            ui.close();
        }
        let browser = MenuItem::new(crate::i18n::text("repo-core-0115"))
            .icon("external-link")
            .refused(
                page.is_none()
                    .then_some(crate::i18n::text("repo-core-3680")),
            )
            .show(ui);
        if let Some(url) = &page {
            let browser = browser.on_hover_text(url);
            if browser.clicked() {
                acts.push(Action::OpenUrl(url.clone()));
            }
        }
    }

    /// The checked-out branch (or detached HEAD), and a merge or rebase in progress.
    pub(in crate::repo) fn head_label(&self, ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        // A repository on another machine says which, first.
        if let Some(target) = crate::remote::parse(&self.path) {
            widgets::chip(ui, &target.host.label(), t.accent_fg).on_hover_text(
                crate::i18n::message(
                    "repo-core-3681",
                    &[
                        ("label", target.host.label().to_string().into()),
                        ("path", target.path.to_string().into()),
                    ],
                ),
            );
            ui.add_space(6.0);
        }
        match &self.refs.current {
            Some(branch) => {
                icons::inline(ui, "git-branch", 14.0, t.text_dim);
                let label = ui.label(egui::RichText::new(branch).font(crate::theme::medium(12.5)));
                if let Some(e) = &self.ci.error {
                    label.on_hover_text(crate::i18n::message(
                        "repo-core-3682",
                        &[("e", e.to_string().into())],
                    ));
                }
            }
            None if self.refs.head.is_empty() => {
                ui.label(
                    egui::RichText::new(crate::i18n::text("repo-core-3683")).color(t.text_dim),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new(crate::i18n::message(
                        "repo-core-3684",
                        &[(
                            "head",
                            self.refs
                                .head
                                .get(..8)
                                .unwrap_or(&self.refs.head)
                                .to_string()
                                .into(),
                        )],
                    ))
                    .color(t.warning),
                );
            }
        }
        if let Some(status) = self.ci_status(&self.refs.head) {
            ui.add_space(4.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            let id = ui.id().with("head-ci");
            acts.extend(ci_status::dot(
                ui,
                id,
                rect.center(),
                &self.refs.head,
                status,
            ));
        }

        if let Some(state) = self.work.in_progress {
            ui.add_space(10.0);
            widgets::chip(
                ui,
                &crate::i18n::message(
                    "repo-core-0818",
                    &[("label", state.label().to_string().into())],
                ),
                t.warning,
            );
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::message(
                        "repo-core-3686",
                        &[("fold", state.label().to_lowercase().to_string().into())],
                    )),
                )
                .clicked()
            {
                acts.push(self.abort_for(state));
            }
        }
    }
}
