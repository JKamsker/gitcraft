//! Remote and remote-branch context menus and visibility actions.

use super::*;

impl RepoTab {
    pub(super) fn remote_branch_menu_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        remote: &str,
        short: &str,
        b: &Branch,
        idle: bool,
    ) {
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3486"))
            .icon("log-in")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::CheckoutRemote(b.name.clone()));
        }
        if let Some(current) = self.refs.current.as_deref() {
            if crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-core-3487",
                &[("current", current.to_string().into())],
            ))
            .icon("arrow-right")
            .enabled(idle)
            .show(ui)
            .clicked()
            {
                acts.push(fast_forward_action(BranchRef::remote(&b.name), current));
            }
            let merge =
                crate::i18n::message("repo-core-3488", &[("current", current.to_string().into())]);
            if self
                .conflict_button(ui, merge, idle, Op::Merge, &b.target)
                .clicked()
            {
                acts.push(merge_action(BranchRef::remote(&b.name), current));
            }
            let rebase =
                crate::i18n::message("repo-core-3489", &[("current", current.to_string().into())]);
            let rebase = self.conflict_button(ui, rebase, idle, Op::Rebase, &b.target);
            offer(&rebase, || GhostOp::Rebase {
                onto: BranchRef::remote(&b.name).full(),
                label: b.name.clone(),
            });
            if rebase.clicked() {
                acts.push(rebase_action(BranchRef::remote(&b.name), current));
            }
            crate::repo::rehearse::rehearse_item(ui, acts, BranchRef::remote(&b.name));
            ui.separator();
            graph_view::compare_items(ui, acts, BranchRef::remote(&b.name), current);
            ui.separator();
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2641"))
            .icon("git-branch-plus")
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::create_branch(&b.target, &b.name)));
        }
        ui.separator();
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3390"))
            .icon("copy")
            .show(ui)
            .clicked()
        {
            ui.ctx().copy_text(b.name.clone());
        }
        if let Some((menu, target)) = self.branch_link_menu(&b.name) {
            menu.items(ui, "branch", &target, acts);
        }
        ui.separator();
        self.visibility_items(ui, acts, &remote_ref(&b.name), false);
        ui.separator();
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3490"))
            .icon("minus")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(
                Dialog::confirm(
                    "Delete remote branch",
                    crate::i18n::message(
                        "repo-core-3492",
                        &[
                            ("short", short.to_string().into()),
                            ("remote", remote.to_string().into()),
                        ],
                    ),
                    crate::i18n::text("repo-core-1115"),
                    // Only while it is where it was last fetched: commits pushed since are others'.
                    ops(&[&[
                        "push",
                        &format!("--force-with-lease=refs/heads/{short}:{}", b.target),
                        remote,
                        "--delete",
                        &format!("refs/heads/{short}"),
                    ]]),
                    None,
                )
                .preview_of(
                    self.delete_preview(&[&b.target], vec![format!("refs/remotes/{}", b.name)]),
                ),
            ));
        }
        crate::repo::custom_commands::menu_items(
            ui,
            crate::custom_commands::Placement::Branch,
            self.branch_scope(&b.name, &b.target, Some(remote)),
            acts,
        );
    }
}

impl RepoTab {
    pub(super) fn remote_menu_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        remote: &Remote,
        idle: bool,
    ) {
        let before = acts.len();
        // Popup placement can move a menu, but cannot shrink an unbroken menu label.
        // Wrap within the logical viewport at high UI scales and on narrow windows.
        ui.set_max_width((ui.ctx().content_rect().width() - 16.0).clamp(32.0, 280.0));
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        let primary = self.primary_forge_remote() == Some(remote.name.as_str());
        let usable = self
            .forge_remotes
            .iter()
            .any(|(name, _)| name == &remote.name);
        let label = if primary {
            crate::i18n::text("dev-integration-ui-primary-selected")
        } else {
            crate::i18n::text("dev-integration-ui-primary-set")
        };
        let tip = if usable {
            crate::i18n::text("dev-integration-ui-primary-about")
        } else {
            crate::i18n::text("dev-integration-ui-primary-unknown")
        };
        if ui
            .add_enabled(
                usable && !self.forge_change_blocked(),
                egui::Button::new(label),
            )
            .on_hover_text(tip)
            .on_disabled_hover_text(if self.forge_change_blocked() {
                crate::i18n::text("dev-integration-ui-primary-busy")
            } else {
                tip
            })
            .clicked()
        {
            acts.push(Action::PrimaryForge(Some(remote.name.clone())));
        }
        if !self.forge_selection.name.is_empty()
            && ui
                .add_enabled(
                    !self.forge_change_blocked(),
                    egui::Button::new(crate::i18n::text("dev-integration-ui-primary-auto")),
                )
                .clicked()
        {
            acts.push(Action::PrimaryForge(None));
        }
        let sidebar_hidden = self.visibility.sidebar_remotes.contains(&remote.name);
        if ui
            .button(if sidebar_hidden {
                crate::i18n::text("dev-integration-ui-show-sidebar")
            } else {
                crate::i18n::text("dev-integration-ui-hide-sidebar")
            })
            .clicked()
        {
            acts.push(Action::BranchVisibility(Change::RemoteSidebar(
                remote.name.clone(),
            )));
        }
        let graph_hidden = self.visibility.graph_remotes.contains(&remote.name);
        let label = if graph_hidden {
            crate::i18n::text("dev-integration-ui-show-graph")
        } else {
            crate::i18n::text("dev-integration-ui-hide-graph")
        };
        if ui
            .button(label)
            .on_hover_text(crate::i18n::text("dev-integration-ui-visibility-only"))
            .clicked()
        {
            acts.push(Action::BranchVisibility(Change::RemoteGraph(
                remote.name.clone(),
            )));
        }
        ui.separator();
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-3495",
            &[("name", remote.name.to_string().into())],
        ))
        .icon("cloud-download")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(Action::Ops {
                label: format!("Fetch {}", remote.name),
                ops: ops(&[&["fetch", "--prune", &remote.name]]),
                refresh: Refresh::All,
            });
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3497"))
            .icon("wrench")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::remote(Some((
                remote.name.clone(),
                remote.url.clone(),
            )))));
        }
        if ui
            .add_enabled(idle, egui::Button::new(crate::i18n::text("repo-core-3498")))
            .clicked()
        {
            acts.push(Action::RepositoryTools(
                crate::repo::repository_tools::Act::Remote(remote.name.clone()),
            ));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3456"))
            .icon("copy")
            .show(ui)
            .clicked()
        {
            ui.ctx().copy_text(remote.url.clone());
            ui.close();
        }
        ui.separator();
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3499"))
            .icon("minus")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::confirm(
                "Remove remote",
                crate::i18n::message(
                    "repo-core-3501",
                    &[
                        ("name", remote.name.to_string().into()),
                        ("url", remote.url.to_string().into()),
                    ],
                ),
                crate::i18n::text("repo-core-0305"),
                ops(&[&["remote", "remove", &remote.name]]),
                None,
            )));
        }
        // Pointer clicks close a Popup automatically; keyboard activation must close too.
        if acts.len() != before {
            ui.close();
        }
    }
}
