//! Fork checkout and rebase planning with scope-bound confirmation.

use super::*;

impl RepoTab {
    pub(in crate::repo::forks) fn checkout_fork(&mut self, number: u64, ctx: &Context) {
        let Some(info) = self.forks.of(number).cloned() else {
            self.status = Some(crate::repo::Notice::plain((
                format!("Where #{number} comes from isn't known yet"),
                true,
            )));
            return;
        };
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        let Some(primary) = self
            .primary_forge_remote()
            .and_then(|name| self.remotes.iter().find(|r| r.name == name))
            .cloned()
        else {
            return;
        };
        let locals: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let previous = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| self.refs.head.clone());
        match plan::checkout(
            &info,
            &self.remotes,
            &self.forks.setup,
            &locals,
            &previous,
            self.hosting.as_ref(),
        ) {
            Ok(plan) => {
                let label = checkout_operation_label(number, &info.owner);
                let url = self
                    .remotes
                    .iter()
                    .find(|r| r.name == plan.remote)
                    .map_or_else(|| plan.url.clone(), |r| r.url.clone());
                let fork_hosting = crate::forge::Hosting {
                    path: info.repo_path.clone(),
                    ..hosting.clone()
                };
                self.replace_fork_scope(
                    guard::Scope::new(self.forge_generation, hosting, primary)
                        .with_forge_lifetime(ctx, self.repo_key())
                        .with_fork(plan.remote.clone(), url, fork_hosting, plan.new_remote),
                );
                self.forks.scope_label = Some(label.clone());
                self.run_undoable(
                    &label,
                    plan.steps.clone(),
                    Refresh::All,
                    Undo::Custom {
                        undo: plan.undo,
                        redo: plan.steps,
                    },
                    ctx,
                );
            }
            Err(e) => self.status = Some(crate::repo::Notice::plain((e, true))),
        }
    }

    pub(in crate::repo::forks) fn plan_fork_rebase(&mut self, number: u64, ctx: &Context) {
        let Some(info) = self.forks.of(number).cloned() else {
            return;
        };
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        let Some(upstream) = self
            .primary_forge_remote()
            .and_then(|name| self.remotes.iter().find(|r| r.name == name))
            .filter(|r| hosting.matches_git_remote(&r.url))
            .cloned()
        else {
            self.status = Some(crate::repo::Notice::plain((
                "This repository has no remote to rebase onto".to_owned(),
                true,
            )));
            return;
        };
        let repo = self.path.clone();
        let setup = self.forks.setup.clone();
        let scope = guard::Scope::new(self.forge_generation, hosting.clone(), upstream.clone())
            .with_forge_lifetime(ctx, self.repo_key());
        self.replace_fork_scope(scope.clone());
        let creds = self.credentials.clone();
        #[cfg(test)]
        let recorded = tests::take_plan_recording(&upstream.name);
        let work_scope = scope.clone();
        self.status = Some(crate::repo::Notice::plain((
            format!("Fetching #{number}'s branch and its base…"),
            false,
        )));
        self.forge_then(
            ctx,
            move || {
                #[cfg(test)]
                if let Some(recorded) = recorded {
                    return recorded().map(|p| (info, p));
                }
                plan::plan_rebase(
                    &repo,
                    &info,
                    &upstream,
                    &setup,
                    creds.as_ref(),
                    Some(&hosting),
                    Some(&work_scope),
                )
                .map(|p| (info, p))
            },
            move |tab, plan, _| {
                if scope.check().is_err() {
                    if tab.forks.scope.as_ref().is_some_and(|s| s.id == scope.id) {
                        tab.forks.scope = None;
                        tab.forks.running = None;
                        tab.forks.scope_label = None;
                        tab.status = Some(crate::repo::Notice::plain((
                            "The primary forge changed; plan the fork action again".into(),
                            true,
                        )));
                    }
                    return;
                }
                match plan {
                    Ok((info, plan)) => {
                        tab.status = None;
                        let label = format!("Rebase #{} onto {} and push", info.number, info.base);
                        let body = rebase_preview(&info, &plan);
                        tab.dialog = Some(
                            Dialog::confirm(
                                &label,
                                body,
                                crate::i18n::text("repo-workflows-forks-plan-fork-rebase-a3647cfa"),
                                plan.steps(),
                                None,
                            )
                            .instead_ops(crate::repo::Action::Fork(ForkAct::ConfirmRebase(
                                scope.id,
                            )))
                            .undo(Undo::Custom {
                                undo: plan.undo_steps(),
                                redo: plan.redo_steps(),
                            }),
                        );
                        tab.forks.scope_label = Some(label.clone());
                        tab.forks.running = Some((label, plan));
                    }
                    Err(e) => {
                        tab.forks.scope = None;
                        tab.status = Some(crate::repo::Notice::plain((e, true)));
                    }
                }
            },
        );
    }

    pub(in crate::repo::forks) fn replace_fork_scope(&mut self, scope: guard::Scope) {
        if let Some(old) = &self.forks.scope {
            old.invalidate();
        }
        if matches!(&self.dialog, Some(Dialog::Confirm { preview, .. })
            if preview.instead.as_ref().is_some_and(|i| matches!(i.action,
                crate::repo::Action::Fork(ForkAct::ConfirmRebase(_) | ForkAct::ConfirmPush(_)))))
        {
            self.dialog = None;
        }
        self.forks.running = None;
        self.forks.staged_rebase = None;
        self.forks.after_conflict = None;
        self.forks.push_tip = None;
        self.forks.scope_label = None;
        self.forks.scope = Some(scope);
    }

    pub(in crate::repo::forks) fn confirm_fork_rebase(
        &mut self,
        id: u64,
        push_only: bool,
        ctx: &Context,
    ) {
        self.sync_forge_selection(ctx);
        let valid = self.forks.scope.as_ref().is_some_and(|s| {
            s.id == id
                && s.generation == self.forge_generation
                && s.check().is_ok()
                && self
                    .hosting
                    .as_ref()
                    .is_some_and(|h| h.same_repository(&s.hosting))
                && self.primary_forge_remote() == Some(s.primary.name.as_str())
                && self.remotes.iter().any(|r| r == &s.primary)
        });
        if !valid {
            self.status = Some(crate::repo::Notice::plain((
                "The primary forge changed; plan the fork action again".into(),
                true,
            )));
            return;
        }
        let Some((label, plan)) = self.forks.running.clone() else {
            return;
        };
        let (steps, undo, redo) = if push_only {
            let Some(tip) = &self.forks.push_tip else {
                return;
            };
            (
                plan.push_now(tip),
                plan.fork_undo_steps(),
                plan.push_now(tip),
            )
        } else {
            // Never scan a push against the tip from before its local rewrite.
            // OpDone records local undo first and then uses preflight_after to
            // preview the worker-owned rewritten tip with the original fork lease.
            if self.busy_op.is_some() {
                return;
            }
            self.forks.staged_rebase = Some(label.clone());
            self.run_ops(
                &label,
                plan.local_steps(),
                Refresh::All,
                OpKind::plain(),
                ctx,
            );
            return;
        };
        self.run_undoable(
            &label,
            steps,
            Refresh::All,
            Undo::Custom { undo, redo },
            ctx,
        );
    }
}
