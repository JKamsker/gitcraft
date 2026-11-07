//! Modal preview and deliberate repairs; no Git read runs on the UI thread.

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
        static IDS: AtomicU64 = AtomicU64::new(1);
        let id = IDS.fetch_add(1, Ordering::Relaxed);
        let token = Arc::new(());
        let alive = Arc::downgrade(&token);
        self.dialog = Some(Dialog::PushLoading { id, token });
        let repo = self.path.clone();
        let profile = Settings::get(ctx).profile();
        spawn_or(
            &self.tx,
            ctx,
            move |_| {
                Msg::Then(Box::new(move |tab, _| {
                    if matches!(&tab.dialog,Some(Dialog::PushLoading{id:current,..}) if *current==id)
                    {
                        tab.dialog=Some(Dialog::PushError("The preflight worker stopped before completing its checks. No push was run.".into()));
                    }
                }))
            },
            move || {
                let began = Instant::now();
                let result = check::scan(&repo, ops, profile, &|| {
                    alive.strong_count() > 0 && began.elapsed() < Duration::from_secs(120)
                });
                let result = if began.elapsed() >= Duration::from_secs(120) {
                    Err("Push preflight exceeded its two-minute budget. Fetch missing history or split this push, then recheck; nothing was pushed.".into())
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
        self.begin_push_preflight(plan.again(), ctx);
    }

    fn same_preflight_profile(&mut self, plan: &Plan, ctx: &egui::Context) -> bool {
        if Settings::get(ctx).profile() != plan.profile {
            self.status = Some((
                "The profile or its identity policy changed. Reopen push preflight.".into(),
                true,
            ));
            false
        } else {
            true
        }
    }

    pub(super) fn submit_preflight(&mut self, plan: Plan, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some((
                "Wait for the current operation, then recheck the push preview.".into(),
                true,
            ));
            return;
        }
        if !self.same_preflight_profile(&plan, ctx) {
            return;
        }
        let kind = self.push_kind.take().unwrap_or_else(super::OpKind::plain);
        let entry_is_current = match &kind {
            super::OpKind::Undo(entry) => self.undo_stack.last() == Some(entry),
            super::OpKind::Redo(entry) => self.redo_stack.last() == Some(entry),
            super::OpKind::Normal(..) => true,
        };
        if !entry_is_current {
            self.status = Some((
                "The undo/redo journal changed. Reopen the push from its original action.".into(),
                true,
            ));
            return;
        }
        self.take_back(&kind);
        self.run_ops(
            "Push reviewed history",
            check::approved_ops(plan),
            Refresh::All,
            kind,
            ctx,
        );
    }

    pub(super) fn fix_preflight(&mut self, plan: Plan, kind: &str, ctx: &egui::Context) {
        if !self.same_preflight_profile(&plan, ctx) {
            return;
        }
        let text=match kind{
            "author"=>format!("Rewrite the listed outgoing author mismatches as {} <{}>. This can change attribution on other contributors' commits; review the checklist first.",plan.expected_name,plan.expected_email),
            "sign"=>"Recreate the outgoing commits with signatures using your configured signing key. Signing may ask you to unlock that key.".into(),
            "secrets"=>"Remove every listed likely-secret file from every outgoing commit, and replace entire flagged commit messages. The final working files change too. Review the file list first; recreate safe versions after the repair.".into(),
            "lfs"=>"Convert all outgoing versions of the listed large regular files to local Git LFS objects and pointers, and add exact path attributes. No LFS data is uploaded or downloaded here. git-lfs must be installed.".into(),
            "remote"=>"Merge the fetched target commit into this branch, keeping remote history. This may leave conflicts for you to resolve or abort.".into(),
            _=>return,
        };
        let mut step = check::register(plan);
        step[0] = check::FIX.into();
        step.push(kind.into());
        self.dialog=Some(Dialog::confirm("Fix outgoing history",format!("{text}\n\nRequires this branch checked out with a clean working tree. Ctrl+Z restores a completed repair; a later push is a separate explicit action."),"Apply repair",vec![step],None).undo(Undo::HeadMove{mode:"--merge"}));
    }

    pub(super) fn preflight_messages(&mut self, plan: Plan, ctx: &egui::Context) {
        if !plan.is_current_branch() {
            self.status = Some((
                "Check out the previewed branch before editing messages, then reopen preflight."
                    .into(),
                true,
            ));
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
                    .map_err(|_| "Could not check the branch")?
                    .trim()
                    != plan.head
                {
                    return Err(
                        "Check out the previewed branch before editing its messages.".into(),
                    );
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
                    "fetched target / root".into(),
                    commits,
                );
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
                Err(e) => tab.status = Some((e, true)),
            },
        );
    }
}

pub(super) fn dialog(ui: &mut egui::Ui, plan: &mut Plan, acknowledge: &mut bool) -> Outcome {
    ui.set_width(640.0);
    ui.heading("Push preflight");
    ui.label(format!(
        "{} → {} on {}",
        plan.branch,
        plan.destination,
        plan.targets.join(", ")
    ));
    ui.label(format!(
        "{} outgoing commits ({} have embedded signatures), pinned to {}. Author policy: {} <{}>.",
        plan.commits.len(),
        plan.commits.iter().filter(|c| c.signed).count(),
        super::short(&plan.head),
        plan.expected_name,
        plan.expected_email
    ));
    ui.label("Checks use local/fetched history only. Secret detection is heuristic; it does not download or inspect external LFS contents, verify signature trust, or replace a dedicated secret scanner.");
    if !plan.lfs_objects.is_empty() {
        ui.label(format!("{} LFS objects will be uploaded as part of the confirmed push to {}; their external content was not scanned. Endpoint settings use committed .lfsconfig plus inspected Git configuration, ignoring later working-tree edits.",plan.lfs_objects.len(),plan.lfs_targets().join(", ")));
    }
    if let Err(reason) = check::push_args(plan) {
        ui.colored_label(crate::theme::Tokens::get(ui.ctx()).warning, reason);
    }
    let mut out = Outcome::Open;
    if !plan.is_current_branch() {
        ui.label("Repairs require the underlying branch checked out. The push itself still sends the inspected object.");
        if let Some(name) = plan.branch.strip_prefix("refs/heads/")
            && ui.button("Check out for repairs").clicked()
        {
            out = Outcome::Done(Action::Checkout(name.to_owned()));
        }
    }
    egui::ScrollArea::vertical()
        .max_height(380.0)
        .show(ui, |ui| {
            for (kind, title, fix, label) in [
                (
                    Kind::Identity,
                    "Author matches this profile's host policy",
                    "author",
                    "Amend authors…",
                ),
                (
                    Kind::Signature,
                    "Profile signing policy",
                    "sign",
                    "Sign outgoing commits…",
                ),
                (
                    Kind::Message,
                    "No leftover fixup/WIP subjects",
                    "messages",
                    "Autosquash / edit messages…",
                ),
                (
                    Kind::Secret,
                    "Likely secrets in outgoing objects/messages",
                    "secrets",
                    "Remove affected history…",
                ),
                (
                    Kind::Large,
                    "Large Git blobs (over 5 MiB)",
                    "lfs",
                    "Move to LFS…",
                ),
                (
                    Kind::RemoteHistory,
                    "Remote commits a force push would discard",
                    "remote",
                    "Keep remote commits…",
                ),
            ] {
                let found: Vec<_> = plan.findings.iter().filter(|f| f.kind == kind).collect();
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong(format!(
                        "{} {title} — {}",
                        if found.is_empty() { "OK" } else { "Review" },
                        if found.is_empty() {
                            "clear".into()
                        } else {
                            format!("{} findings", found.len())
                        }
                    ));
                    if !found.is_empty()
                        && plan.is_current_branch()
                        && ui.small_button(label).clicked()
                    {
                        out = Outcome::Done(if fix == "messages" {
                            Action::PreflightMessages(Box::new(plan.clone()))
                        } else {
                            Action::FixPreflight(Box::new(plan.clone()), fix.into())
                        });
                    }
                });
                for f in found.iter().take(100) {
                    ui.label(format!(
                        "{} {} {}",
                        super::short(&f.commit),
                        check::display_path(&f.path),
                        f.detail
                    ));
                }
                if found.len() > 100 {
                    ui.label("More findings exist; repair and recheck before pushing.");
                }
            }
        });
    if !plan.destination.starts_with("refs/for/") {
        ui.add_enabled_ui(plan.base.is_some(), |ui| {
            ui.checkbox(
                &mut plan.force,
                "Rewrite the target with an explicit force-with-lease",
            );
        });
        if plan.base.is_none() {
            ui.label("No fetched target tip is available: force push is disabled. Fetch first to inspect dropped remote commits.");
        }
    }
    ui.checkbox(
        acknowledge,
        "I reviewed the exact target and findings, including heuristic limitations",
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                *acknowledge && check::push_args(plan).is_ok(),
                egui::Button::new(if plan.force {
                    "Push with inspected lease"
                } else {
                    "Push inspected commit"
                }),
            )
            .clicked()
        {
            out = Outcome::Done(Action::SubmitPreflight(Box::new(plan.clone())));
        }
        if ui.button("Recheck").clicked() {
            out = Outcome::Done(Action::ReloadPreflight(Box::new(plan.clone())));
        }
        if ui.button("Cancel").clicked() {
            out = Outcome::Cancel;
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::push_preflight::tests::Fixture;
    use crate::repo::UndoEntry;

    fn receive(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            let msg = match tab.rx.recv_timeout(Duration::from_secs(1)) {
                Ok(msg) => msg,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(error) => return Err(error.to_string()),
            };
            if let Msg::Then(next) = msg {
                next(tab, ctx);
                return Ok(());
            }
            if let Msg::JobPanicked(error) = msg {
                return Err(error);
            }
            // Constructor watcher/count messages are unrelated to this continuation.
        }
        Err("Preflight continuation did not finish".into())
    }

    #[test]
    fn pushing_opens_a_deliberate_checklist_without_sending_and_cancelled_workers_stay_closed()
    -> Result<(), String> {
        let f = Fixture::new()?;
        f.commit("next", "next", "Next")?;
        let before = f.receiver_head()?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
        tab.run_ops(
            "Push",
            vec![vec!["push".into()]],
            Refresh::All,
            super::super::OpKind::plain(),
            &ctx,
        );
        assert!(tab.busy_op.is_none());
        assert!(matches!(tab.dialog, Some(Dialog::PushLoading { .. })));
        receive(&mut tab, &ctx)?;
        let Some(Dialog::PushPreflight { plan, acknowledge }) = &mut tab.dialog else {
            return Err("No checklist arrived".into());
        };
        assert!(!*acknowledge);
        ctx.run_ui(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(matches!(dialog(ui, plan, acknowledge), Outcome::Open));
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(f.receiver_head()?, before);
        tab.begin_push_preflight(vec![vec!["push".into()]], &ctx);
        tab.dialog = None;
        receive(&mut tab, &ctx)?;
        assert!(tab.dialog.is_none());
        assert_eq!(f.receiver_head()?, before);
        Ok(())
    }

    #[test]
    fn cancelling_a_push_preview_keeps_undo_and_redo_entries_available() -> Result<(), String> {
        let f = Fixture::new()?;
        f.commit("next", "next", "Next")?;
        let before = f.receiver_head()?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
        let entry = UndoEntry {
            label: "Restore remote branch".into(),
            undo: vec![vec!["push".into()]],
            redo: vec![vec!["push".into()]],
            heads: None,
        };
        tab.undo_stack.push(entry.clone());
        tab.undo(&ctx);
        assert_eq!(tab.undo_stack, vec![entry.clone()]);
        receive(&mut tab, &ctx)?;
        tab.dialog = None;
        assert_eq!(tab.undo_stack, vec![entry.clone()]);
        tab.undo_stack.clear();
        tab.redo_stack.push(entry.clone());
        tab.redo(&ctx);
        assert_eq!(tab.redo_stack, vec![entry.clone()]);
        receive(&mut tab, &ctx)?;
        tab.dialog = None;
        assert_eq!(tab.redo_stack, vec![entry]);
        assert_eq!(f.receiver_head()?, before);
        Ok(())
    }

    #[test]
    fn message_fix_prepares_ordered_autosquash_and_editable_wip_before_any_rewrite()
    -> Result<(), String> {
        let f = Fixture::new()?;
        f.commit("feature", "one", "Feature")?;
        f.commit("feature", "two", "fixup! Feature")?;
        let head = f.commit("other", "other", "WIP: finish this")?;
        let plan = f.plan(crate::settings::profiles::UserProfile::default())?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
        tab.preflight_messages(plan, &ctx);
        receive(&mut tab, &ctx)?;
        let editor = tab.rebase.as_ref().ok_or("No message editor")?;
        assert_eq!(editor.entries.len(), 3);
        assert_eq!(editor.entries[1].step, super::super::rebase::Step::Fixup);
        assert_eq!(editor.entries[2].step, super::super::rebase::Step::Reword);
        assert_eq!(f.git(&["rev-parse", "HEAD"])?, head);
        Ok(())
    }
    #[test]
    fn root_message_repair_runs_the_prepared_autosquash_without_flattening_away_root()
    -> Result<(), String> {
        let f = Fixture::new()?;
        f.git(&["update-ref", "-d", "refs/remotes/origin/main"])?;
        f.commit("feature", "one", "Feature")?;
        f.commit("feature", "two", "fixup! Feature")?;
        f.commit("other", "other", "WIP: unfinished")?;
        let plan = f.plan(crate::settings::profiles::UserProfile::default())?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
        tab.preflight_messages(plan, &ctx);
        receive(&mut tab, &ctx)?;
        let editor = tab.rebase.as_mut().ok_or("No root editor")?;
        assert!(editor.onto.is_empty());
        editor.entries.last_mut().ok_or("No WIP entry")?.message = "Finished work".into();
        let args = editor.command(&f.repo)?;
        let words: Vec<_> = args.iter().map(String::as_str).collect();
        crate::git::run(Some(&f.repo), &words)?;
        assert_eq!(f.git(&["rev-list", "--count", "HEAD"])?, "3");
        assert_eq!(
            f.git(&["show", "-s", "--format=%s", "HEAD"])?,
            "Finished work"
        );
        assert_eq!(f.git(&["show", "HEAD:feature"])?, "two");
        Ok(())
    }
    #[test]
    fn approval_from_a_different_profile_cannot_start_a_push() -> Result<(), String> {
        let f = Fixture::new()?;
        f.commit("next", "next", "Next")?;
        let before = f.receiver_head()?;
        let profile = crate::settings::profiles::UserProfile {
            id: 999,
            ..Default::default()
        };
        let plan = f.plan(profile)?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
        tab.submit_preflight(plan, &ctx);
        assert!(tab.busy_op.is_none());
        assert!(
            tab.status
                .as_ref()
                .is_some_and(|(text, error)| *error && text.contains("profile"))
        );
        assert_eq!(f.receiver_head()?, before);
        Ok(())
    }
}
