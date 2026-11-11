//! Forge links and branch/tag actions for graph context menus.

use super::*;

impl RepoTab {
    pub(in crate::repo::graph_view::menu) fn link_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        pill: Option<&str>,
        branch: Option<&str>,
        acts: &mut Vec<Action>,
    ) {
        let mut any = false;
        if let Some(menu) = self.link_menu(pill) {
            let target = Target::Commit {
                rev: hash.to_owned(),
            };
            menu.items(ui, "commit", &target, acts);
            any = true;
        }
        if let Some(name) = branch
            && let Some((menu, target)) = self.branch_link_menu(name)
        {
            menu.items(
                ui,
                &crate::i18n::message("repo-core-2631", &[("name", name.to_string().into())]),
                &target,
                acts,
            );
            any = true;
        }
        if any {
            ui.separator();
        }
    }

    /// Actions on the right-clicked `branch`: check it out, or bring it into the checked-out
    /// branch.
    pub(in crate::repo::graph_view::menu) fn pill_items(
        &self,
        ui: &mut Ui,
        branch: &str,
        hash: &str,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        // A pill names the local branch when there is one here, else `remote/branch`: a local
        // branch elsewhere that happens to be called `origin/topic` is not this pill's.
        let local = self
            .refs
            .local
            .iter()
            .any(|b| b.name == branch && b.target == hash);
        let git_ref = BranchRef {
            name: branch,
            remote: !local,
        };
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2632",
            &[("branch", branch.to_string().into())],
        ))
        .icon("log-in")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(if local {
                Action::Checkout(branch.to_owned())
            } else {
                Action::CheckoutRemote(branch.to_owned())
            });
        }
        let Some(current) = self.refs.current.as_deref() else {
            return;
        };
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2634",
            &[
                ("current", current.to_string().into()),
                ("branch", branch.to_string().into()),
            ],
        ))
        .icon("arrow-right")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(fast_forward_action(git_ref, current));
        }
        let merge = crate::i18n::message(
            "repo-core-2636",
            &[
                ("branch", branch.to_string().into()),
                ("current", current.to_string().into()),
            ],
        );
        if self
            .conflict_button(ui, merge, idle, Op::Merge, hash)
            .clicked()
        {
            acts.push(merge_action(git_ref, current));
        }
        let rebase = crate::i18n::message(
            "repo-core-2637",
            &[
                ("current", current.to_string().into()),
                ("branch", branch.to_string().into()),
            ],
        );
        let rebase = self.conflict_button(ui, rebase, idle, Op::Rebase, hash);
        offer(&rebase, || GhostOp::Rebase {
            onto: git_ref.full(),
            label: branch.to_owned(),
        });
        if rebase.clicked() {
            acts.push(rebase_action(git_ref, current));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-2638",
            &[
                ("current", current.to_string().into()),
                ("branch", branch.to_string().into()),
            ],
        ))
        .icon("git-merge")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(interactive_rebase_action(git_ref));
        }
        if crate::settings::Settings::get(ui.ctx())
            .context_menus
            .rehearsal
        {
            crate::menu_item::group_named(
                ui,
                "Advanced",
                crate::i18n::text("dev-integration-ui-advanced"),
                |ui| {
                    crate::repo::rehearse::rehearse_item(ui, acts, git_ref);
                    if branch != current {
                        crate::repo::interaction::interaction_item(ui, acts, branch);
                    }
                },
            );
        }
        ui.separator();
        compare_items(ui, acts, git_ref, current);
    }

    /// A submenu for each local branch pointing at the commit, creating a branch or tag there,
    /// and checking it out detached.
    pub(in crate::repo::graph_view::menu) fn branch_here_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        short: &str,
        idle: bool,
        is_head: bool,
        acts: &mut Vec<Action>,
    ) {
        // Local branches pointing here get their own submenu.
        let here: Vec<&git::Branch> = self
            .refs
            .local
            .iter()
            .filter(|b| b.target == hash)
            .collect();
        for b in &here {
            let id = ui.make_persistent_id(("branch-actions", &b.name));
            ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
                crate::menu_item::MenuItem::new(crate::i18n::message(
                    "repo-core-2639",
                    &[("name", b.name.to_string().into())],
                ))
                .icon("git-branch-plus")
                .submenu(ui, |ui| {
                    self.branch_menu_items(ui, acts, b, idle);
                });
            });
        }
        if !here.is_empty() {
            ui.separator();
        }
        // At HEAD, the new-branch key does the same.
        let mut create = MenuItem::new(crate::i18n::text("repo-core-2641")).icon("git-branch-plus");
        if is_head {
            create = create.keys(crate::keymap::Command::NewBranch);
        }
        if create.show(ui).clicked() {
            acts.push(Action::Dialog(Dialog::create_branch(hash, short)));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2642"))
            .icon("tag")
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::tag(
                hash,
                short,
                self.default_remote(),
            )));
        }
        if crate::settings::Settings::get(ui.ctx())
            .context_menus
            .release
            && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-0139"))
                .icon("package")
                .enabled(idle)
                .show(ui)
                .on_hover_text(crate::i18n::text("repo-core-2643"))
                .clicked()
        {
            acts.push(Action::Release(crate::repo::release::ReleaseAct::Open(
                hash.to_owned(),
            )));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2644"))
            .icon("folder-git-2")
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-2646"))
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::worktree_at(&self.path, hash)));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2647"))
            .icon("log-in")
            .enabled(idle)
            .refused(is_head.then_some(crate::i18n::text("repo-core-0796")))
            .show(ui)
            .clicked()
        {
            acts.push(Action::Checkout(hash.to_owned()));
        }
    }
}
