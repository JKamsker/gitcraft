//! Stack branch context menu actions.

use super::*;

impl RepoTab {
    /// The branch menu's entries for stacks: pick the branch it is based on, or take it out of
    /// its stack.
    pub(in crate::repo) fn stack_menu_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        b: &Branch,
        idle: bool,
    ) {
        let recorded = self.stacks.recorded.get(&b.name);
        // Its own changes: since it forked from its parent, as its pull request will show them.
        if let Some(r) = recorded
            && self.refs.local.iter().any(|l| l.name == r.parent)
            && crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-workflows-stack-stack-menu-items-3a261297",
                &[
                    ("b-name-1", (b.name).clone().into()),
                    ("r-parent-2", (r.parent).clone().into()),
                ],
            ))
            .icon("command")
            .show(ui)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-stack-stack-menu-items-42e9f303",
            ))
            .clicked()
        {
            acts.push(compare_branches_action(
                BranchRef::local(&r.parent),
                BranchRef::local(&b.name),
                true,
            ));
        }
        let hover = match recorded {
            Some(r) => crate::i18n::message(
                "repo-workflows-stack-stack-menu-items-8511ea38",
                &[("r-parent-1", (r.parent).clone().into())],
            ),
            None => crate::i18n::text("repo-workflows-stack-stack-menu-items-4537653e").to_owned(),
        };
        if crate::menu_item::MenuItem::new(crate::i18n::text(
            "repo-workflows-stack-stack-menu-items-ce8e45b9",
        ))
        .icon("git-branch-plus")
        .enabled(idle)
        .show(ui)
        .on_hover_text(hover)
        .clicked()
        {
            acts.push(Action::Stack(StackAct::OpenBasedOn(b.name.clone())));
        }
        let on_it = dependents(&self.stacks.layers, &b.name).len();
        if on_it > 0
            && crate::menu_item::MenuItem::new(if on_it == 1 {
                crate::i18n::text("repo-workflows-stack-stack-menu-items-30af0952").to_owned()
            } else {
                crate::i18n::message(
                    "repo-workflows-stack-stack-menu-items-f7763ef9",
                    &[("on-it", (on_it).into())],
                )
            })
            .icon("git-branch-plus")
            .enabled(idle)
            .refused(self.stacks.pending.is_some().then_some(crate::i18n::text(
                "repo-workflows-stack-stack-menu-items-d8ad545a",
            )))
            .show(ui)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-stack-stack-menu-items-1485e8a2",
            ))
            .on_disabled_hover_text(crate::i18n::text(
                "repo-workflows-stack-stack-menu-items-d8ad545a",
            ))
            .clicked()
        {
            acts.push(Action::Stack(StackAct::OpenRestack(Some(b.name.clone()))));
        }
        if let Some(forge) = self.forge()
            && self.stacks.layer(&b.name).is_some()
            && crate::menu_item::MenuItem::new(crate::i18n::message(
                "repo-workflows-stack-stack-menu-items-e5c65f7a",
                &[("forge-noun-1", (forge.noun_label()).into())],
            ))
            .icon("external-link")
            .enabled(idle)
            .show(ui)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-stack-stack-menu-items-68f6577e",
            ))
            .clicked()
        {
            acts.push(Action::Stack(StackAct::OpenPublish(b.name.clone())));
        }
        if self.stacks.recorded.contains_key(&b.name)
            && crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-stack-stack-menu-items-8400b536",
            ))
            .icon("minus")
            .enabled(idle)
            .show(ui)
            .clicked()
        {
            acts.push(Action::Stack(StackAct::RemoveParent(b.name.clone())));
        }
    }
}
