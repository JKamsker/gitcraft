//! Local-branch menu composition, review actions, and pin controls.

use super::*;

impl RepoTab {
    /// The menu of a local branch: in the sidebar and in the graph's commit menu.
    pub(in crate::repo) fn branch_menu_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        crate::menu_item::bounded_menu(ui, ("branch-menu", &b.name), |ui| {
            self.branch_menu_contents(ui, acts, b, idle);
        });
    }

    pub(super) fn branch_menu_contents(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        self.checkout_items(ui, acts, b, idle);
        if b.is_head
            && b.name.starts_with(&self.hotfix_prefix())
            && crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-0137"))
                .icon("package")
                .enabled(idle)
                .show(ui)
                .on_hover_text(crate::i18n::text("repo-core-3386"))
                .clicked()
        {
            acts.push(Action::Hotfix(crate::repo::hotfix::HotfixAct::OpenFinish));
        }
        if let Some(current) = self.refs.current.as_deref()
            && !b.is_head
        {
            ui.separator();
            crate::menu_item::group_named(
                ui,
                "Integrate / Compare",
                crate::i18n::text("dev-integration-ui-integrate-compare"),
                |ui| self.integrate_items(ui, acts, b, current, idle),
            );
        }
        // The checked-out branch against the one pull requests usually go to.
        if b.is_head
            && let Some(base) = self.base_ref()
            && base.name != b.name
        {
            ui.separator();
            if crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-core-3387",
                &[
                    ("name", b.name.to_string().into()),
                    ("name2", base.name.to_string().into()),
                ],
            ))
            .icon("command")
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-3388"))
            .clicked()
            {
                acts.push(compare_branches_action(
                    base.as_ref(),
                    BranchRef::local(&b.name),
                    true,
                ));
            }
        }
        crate::menu_item::group_named(
            ui,
            "Review / Versions",
            crate::i18n::text("dev-integration-ui-review-versions"),
            |ui| {
                self.review_item(ui, acts, b);
                self.versions_items(ui, acts, b);
            },
        );
        ui.separator();
        crate::menu_item::group_named(
            ui,
            "Publish / Stack",
            crate::i18n::text("dev-integration-ui-publish-stack"),
            |ui| {
                self.publish_items(ui, acts, b, idle);
                self.stack_menu_items(ui, acts, b, idle);
            },
        );
        ui.separator();
        crate::menu_item::group_named(
            ui,
            "Branch actions",
            crate::i18n::text("dev-integration-ui-branch-actions"),
            |ui| {
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2641"))
                    .icon("git-branch-plus")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::Dialog(Dialog::create_branch(&b.target, &b.name)));
                }
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2642"))
                    .icon("tag")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::Dialog(Dialog::tag(
                        &b.target,
                        &b.name,
                        self.default_remote(),
                    )));
                }
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3389"))
                    .icon("wrench")
                    .enabled(idle)
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::Dialog(Dialog::rename_branch(&b.name)));
                }
            },
        );
        crate::menu_item::group_named(
            ui,
            "Links / Copy / History",
            crate::i18n::text("dev-integration-ui-links-copy-history"),
            |ui| {
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3390"))
                    .icon("copy")
                    .show(ui)
                    .clicked()
                {
                    ui.ctx().copy_text(b.name.clone());
                }
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3391"))
                    .icon("history")
                    .show(ui)
                    .on_hover_text(crate::i18n::text("repo-core-3392"))
                    .clicked()
                {
                    acts.push(Action::Reflog(ReflogAct::Show(Source::branch(&b.name))));
                }
                if let Some((menu, target)) = self.branch_link_menu(&b.name) {
                    menu.items(ui, "branch", &target, acts);
                }
            },
        );
        crate::menu_item::group_named(
            ui,
            "Visibility / Agents",
            crate::i18n::text("dev-integration-ui-visibility-agents"),
            |ui| {
                self.pin_item(ui, &b.name);
                if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3393"))
                    .icon("bot")
                    .show(ui)
                    .on_hover_text(crate::i18n::text("repo-core-3394"))
                    .clicked()
                {
                    acts.push(Action::BranchAuthorship(b.name.clone()));
                }
                ui.separator();
                self.visibility_items(ui, acts, &local_ref(&b.name), b.is_head);
            },
        );
        ui.separator();
        self.delete_items(ui, acts, b, idle);
        crate::repo::custom_commands::menu_items(
            ui,
            crate::custom_commands::Placement::Branch,
            self.branch_scope(&b.name, &b.target, None),
            acts,
        );
    }

    /// _Review this branch_: the AI tool reviews what `b` adds to the branch pull requests
    /// usually go to (to the checked-out one, for that branch itself).
    pub(super) fn review_item(&self, ui: &mut Ui, acts: &mut Vec<Action>, b: &Branch) {
        let base = self
            .base_ref()
            .map(|base| base.name)
            .filter(|base| *base != b.name)
            .or_else(|| self.refs.current.clone().filter(|c| *c != b.name));
        ui.separator();
        let button = crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3395"))
            .icon("git-pull-request")
            .refused(
                base.is_none()
                    .then_some(crate::i18n::text("repo-core-3396")),
            )
            .show(ui);
        let Some(base) = base else {
            button.on_disabled_hover_text(crate::i18n::text("repo-core-3397"));
            return;
        };
        let tip = if Settings::get(ui.ctx()).ai.enabled() {
            crate::i18n::message(
                "repo-core-3398",
                &[
                    ("name", b.name.to_string().into()),
                    ("base", base.to_string().into()),
                ],
            )
        } else {
            crate::i18n::text("repo-core-3399").to_owned()
        };
        if button.on_hover_text(tip).clicked() {
            acts.push(Action::ReviewBranch {
                branch: b.name.clone(),
                base,
            });
        }
    }

    /// Pin to top, or Unpin: kept for this repository.
    pub(super) fn pin_item(&self, ui: &mut Ui, name: &str) {
        let mut order = BranchOrder::load(ui.ctx(), self.repo_key());
        let label = if order.is_pinned(name) {
            crate::i18n::text("repo-core-3400")
        } else {
            crate::i18n::text("repo-core-3401")
        };
        if crate::menu_item::MenuItem::new(label)
            .icon("pin")
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-core-3403"))
            .clicked()
        {
            order.toggle_pin(name);
            order.store(ui.ctx(), self.repo_key());
        }
    }
}
