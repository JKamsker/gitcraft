//! Modal preview and deliberate repairs; no Git read runs on the UI thread.

#[path = "core_seams/preflight_dialog.rs"]
mod preflight_dialog;
#[allow(unused_imports)]
pub(crate) use preflight_dialog::*;

use super::dialogs::Outcome;
use super::messages::spawn_or;
use super::{Action, Dialog, Msg, Refresh, RepoTab, Undo};
use crate::push_preflight::{self as check, Kind, Plan};
use crate::settings::Settings;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

impl RepoTab {
    pub(super) fn begin_push_preflight(&mut self, ops: Vec<Vec<String>>, ctx: &egui::Context) {
        let scope = self
            .preflight_pending
            .as_ref()
            .and_then(|pending| self.fork_execution_scope(&pending.label));
        self.begin_push_preflight_bound(ops, scope, ctx);
    }

    fn begin_push_preflight_bound(
        &mut self,
        ops: Vec<Vec<String>>,
        fork_scope: Option<super::forks::ExecutionScope>,
        ctx: &egui::Context,
    ) {
        static IDS: AtomicU64 = AtomicU64::new(1);
        let id = IDS.fetch_add(1, Ordering::Relaxed);
        let token = Arc::new(());
        let alive = Arc::downgrade(&token);
        self.dialog = Some(Dialog::PushLoading { id, token });
        let repo = self.path.clone();
        let profile = Settings::get(ctx).profile();
        let selection = self.preflight_selection.take();
        spawn_or(
            &self.tx,
            ctx,
            move |_| {
                Msg::Then(Box::new(move |tab, _| {
                    if matches!(&tab.dialog,Some(Dialog::PushLoading{id:current,..}) if *current==id)
                    {
                        tab.dialog = Some(Dialog::PushError(
                            crate::i18n::text("repo-core-3105").into(),
                        ));
                    }
                }))
            },
            move || {
                let began = Instant::now();
                let result = (|| {
                    if let Some(scope) = &fork_scope {
                        scope.check_remote(&repo)?;
                    }
                    if let Some(selected) = &selection {
                        selected.snapshot.check_publication(&repo)?;
                    }
                    let mut plan = check::scan(&repo, ops, profile, &|| {
                        alive.strong_count() > 0 && began.elapsed() < Duration::from_secs(120)
                    })?;
                    if let Some(scope) = fork_scope {
                        scope.check_remote(&repo)?;
                        plan.retain_fork_scope(scope);
                    }
                    if let Some(selected) = selection {
                        plan.retain_selection(selected)?;
                    }
                    Ok(plan)
                })();
                let result = if began.elapsed() >= Duration::from_secs(120) {
                    Err(crate::i18n::text("repo-core-3106").into())
                } else {
                    result
                };
                Msg::Then(Box::new(move |tab, _| {
                    if matches!(&tab.dialog,Some(Dialog::PushLoading{id:current,token})if *current==id&&Arc::strong_count(token)>0)
                    {
                        tab.dialog = Some(match result {
                            Ok(plan) => Dialog::PushPreflight {
                                plan: Box::new(plan),
                                acknowledge: false,
                            },
                            Err(error) => Dialog::PushError(error),
                        });
                    }
                }))
            },
        );
    }

    pub(super) fn reload_preflight(&mut self, plan: Plan, ctx: &egui::Context) {
        self.preflight_selection = plan.selection().cloned();
        self.begin_push_preflight_bound(plan.again(), plan.fork_scope().cloned(), ctx);
    }

    fn same_preflight_profile(&mut self, plan: &Plan, ctx: &egui::Context) -> bool {
        if Settings::get(ctx).profile() != plan.profile {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-3107").into(),
                true,
            )));
            false
        } else {
            true
        }
    }

    pub(super) fn submit_preflight(&mut self, plan: Plan, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-3108").into(),
                true,
            )));
            return;
        }
        if !self.same_preflight_profile(&plan, ctx) {
            return;
        }
        if self
            .preflight_timeline
            .is_some_and(|at| at != self.timeline.current)
        {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-3109").into(),
                true,
            )));
            return;
        }
        let pending = self.preflight_pending.take().unwrap_or(super::PendingOp {
            label: "Push reviewed history".into(),
            ops: vec![],
            refresh: Refresh::All,
            kind: super::OpKind::plain(),
            quiet: false,
        });
        self.preflight_timeline = None;
        self.run_ops_ex(
            &pending.label,
            check::approved_ops(plan),
            pending.refresh,
            pending.kind,
            pending.quiet,
            ctx,
        );
    }

    pub(super) fn fix_preflight(&mut self, plan: Plan, kind: &str, ctx: &egui::Context) {
        if !self.same_preflight_profile(&plan, ctx) {
            return;
        }
        let text = match kind {
            "author" => crate::i18n::message(
                "repo-core-3111",
                &[
                    ("expected-name", plan.expected_name.to_string().into()),
                    ("expected-email", plan.expected_email.to_string().into()),
                ],
            ),
            "sign" => crate::i18n::text("repo-core-3113").into(),
            "secrets" => crate::i18n::text("repo-core-3115").into(),
            "lfs" => crate::i18n::text("repo-core-3116").into(),
            "remote" => crate::i18n::text("repo-core-3117").into(),
            _ => return,
        };
        let mut step = check::register(plan);
        step[0] = check::FIX.into();
        step.push(kind.into());
        self.dialog = Some(
            Dialog::confirm(
                "Fix outgoing history",
                crate::i18n::message("repo-core-3119", &[("text", text.to_string().into())]),
                crate::i18n::text("repo-core-3120"),
                vec![step],
                None,
            )
            .undo(Undo::HeadMove { mode: "--merge" }),
        );
    }

    pub(super) fn preflight_messages(&mut self, plan: Plan, ctx: &egui::Context) {
        if !plan.is_current_branch() {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-3122").into(),
                true,
            )));
            return;
        }
        if !self.same_preflight_profile(&plan, ctx) {
            return;
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            move || -> Result<super::rebase::RebaseEditor, String> {
                check::check(&repo, &plan)?;
                if crate::git::forecast_run(&repo, &["rev-parse", "HEAD"], &[], &[0], &|| true)
                    .map_err(|_| crate::i18n::text("repo-core-3123"))?
                    .trim()
                    != plan.head
                {
                    return Err(crate::i18n::text("repo-core-3124").into());
                }
                let mut commits = Vec::new();
                for c in &plan.commits {
                    let message = crate::git::forecast_run(
                        &repo,
                        &["show", "--no-show-signature", "-s", "--format=%B", &c.oid],
                        &[("GIT_NO_LAZY_FETCH", "1".into())],
                        &[0],
                        &|| true,
                    )?;
                    commits.push(crate::git::RebaseCommit {
                        hash: c.oid.clone(),
                        author: c.name.clone(),
                        email: c.email.clone(),
                        summary: c.subject.clone(),
                        message,
                        merge: c.parents.len() > 1,
                    });
                }
                let mut editor = super::rebase::RebaseEditor::new(
                    plan.base.unwrap_or_default(),
                    crate::i18n::text("repo-core-3127").into(),
                    commits,
                );
                editor.branch_guard = Some(super::operations::expect_symref("HEAD", &plan.branch));
                // Autosquash matched fixups into their earlier target. Unmatched temporary
                // subjects remain editable rather than silently dropping their changes.
                let mut i = 0;
                while i < editor.entries.len() {
                    let summary = editor.entries[i].commit.summary.clone();
                    let fix = summary
                        .strip_prefix("fixup! ")
                        .map(|s| (s, super::rebase::Step::Fixup))
                        .or_else(|| {
                            summary
                                .strip_prefix("squash! ")
                                .map(|s| (s, super::rebase::Step::Squash))
                        });
                    if let Some((target, step)) = fix
                        && let Some(at) = editor.entries[..i]
                            .iter()
                            .position(|e| e.commit.summary == target)
                    {
                        let mut entry = editor.entries.remove(i);
                        entry.step = step;
                        let mut into = at + 1;
                        while into < i
                            && matches!(
                                editor.entries[into].step,
                                super::rebase::Step::Fixup | super::rebase::Step::Squash
                            )
                        {
                            into += 1;
                        }
                        editor.entries.insert(into, entry);
                    } else if summary.to_ascii_lowercase().starts_with("wip")
                        || summary.starts_with("[WIP]")
                        || fix.is_some()
                    {
                        editor.entries[i].step = super::rebase::Step::Reword;
                    }
                    i += 1;
                }
                Ok(editor)
            },
            |tab, result, _| match result {
                Ok(editor) => {
                    tab.diff = None;
                    tab.rebase = Some(editor);
                }
                Err(e) => tab.status = Some(crate::repo::Notice::plain((e, true))),
            },
        );
    }
}

#[cfg(test)]
#[path = "direct_tests/push_preflight_tests.rs"]
mod tests;

/// Small shared driver for the existing push dialog, used only by workflow fixtures.
#[cfg(test)]
#[path = "direct_tests/preflight_test_support.rs"]
pub(in crate::repo) mod test_support;

#[cfg(test)]
#[path = "direct_tests/preflight_core_guard_paint_tests.rs"]
pub(in crate::repo) mod core_guard_paint_tests;

#[cfg(test)]
#[path = "direct_tests/push_preflight_route_caption_tests.rs"]
mod route_caption_tests;
