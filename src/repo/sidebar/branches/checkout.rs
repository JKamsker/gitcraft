//! Branch checkout, worktree navigation, and integration choices.

use super::*;

/// Check out `b`, here or in a new worktree; or open the worktree it is checked out in.
impl RepoTab {
    pub(super) fn checkout_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        let elsewhere = self
            .worktrees
            .iter()
            .find(|w| !w.current && w.branch.as_deref() == Some(b.name.as_str()));
        if let Some(wt) = elsewhere {
            let label = if Settings::get(ui.ctx()).worktrees_in_same_tab {
                crate::i18n::text("repo-core-3404")
            } else {
                crate::i18n::text("repo-core-3405")
            };
            if crate::menu_item::MenuItem::new(label)
                .icon("folder-git-2")
                .show(ui)
                .clicked()
            {
                acts.push(Action::OpenWorktree(wt.path.clone()));
            }
            return;
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3406"))
            .icon("log-in")
            .enabled(idle)
            .refused(b.is_head.then_some(crate::i18n::text("repo-core-0796")))
            .show(ui)
            .clicked()
        {
            acts.push(Action::Checkout(b.name.clone()));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-3407"))
            .icon("folder-git-2")
            .enabled(idle)
            .refused(b.is_head.then_some(crate::i18n::text("repo-core-0796")))
            .show(ui)
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::add_worktree(
                &self.path,
                Some(&b.name),
                &self.refs.head,
            )));
        }
    }
}

impl RepoTab {
    /// Fast-forward, merge, rebase and compare `current` with `b`.
    pub(super) fn integrate_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        current: &str,
        idle: bool,
    ) {
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-0011",
            &[
                ("current", current.to_string().into()),
                ("name", b.name.to_string().into()),
            ],
        ))
        .icon("arrow-right")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(fast_forward_action(BranchRef::local(&b.name), current));
        }
        let merge = crate::i18n::message(
            "repo-core-0006",
            &[
                ("name", b.name.to_string().into()),
                ("current", current.to_string().into()),
            ],
        );
        if self
            .conflict_button(ui, merge, idle, Op::Merge, &b.target)
            .clicked()
        {
            acts.push(merge_action(BranchRef::local(&b.name), current));
        }
        let rebase = crate::i18n::message(
            "repo-core-0013",
            &[
                ("current", current.to_string().into()),
                ("name", b.name.to_string().into()),
            ],
        );
        let rebase = self.conflict_button(ui, rebase, idle, Op::Rebase, &b.target);
        offer(&rebase, || GhostOp::Rebase {
            onto: BranchRef::local(&b.name).full(),
            label: b.name.clone(),
        });
        if rebase.clicked() {
            acts.push(rebase_action(BranchRef::local(&b.name), current));
        }
        if crate::menu_item::MenuItem::new(crate::i18n::message(
            "repo-core-3415",
            &[
                ("current", current.to_string().into()),
                ("name", b.name.to_string().into()),
            ],
        ))
        .icon("git-merge")
        .enabled(idle)
        .show(ui)
        .clicked()
        {
            acts.push(interactive_rebase_action(BranchRef::local(&b.name)));
        }
        ui.separator();
        graph_view::compare_items(ui, acts, BranchRef::local(&b.name), current);
        if Settings::get(ui.ctx()).context_menus.rehearsal {
            crate::menu_item::group_named(
                ui,
                "Advanced",
                crate::i18n::text("dev-integration-ui-advanced"),
                |ui| {
                    crate::repo::interaction::interaction_item(ui, acts, &b.name);
                    crate::repo::rehearse::rehearse_item(ui, acts, BranchRef::local(&b.name));
                },
            );
        }
    }
}
