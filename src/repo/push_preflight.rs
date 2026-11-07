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
        if self
            .preflight_timeline
            .is_some_and(|at| at != self.timeline.current)
        {
            self.status = Some((
                "The operation timeline changed; reopen push from its original action".into(),
                true,
            ));
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
        if plan.deleting {
            "Delete remote ref"
        } else {
            &plan.branch
        },
        plan.destination,
        plan.targets.join(", ")
    ));
    if plan.deleting {
        ui.colored_label(crate::theme::Tokens::get(ui.ctx()).warning, format!(
            "Delete {} only while the server tip is {}. No outgoing objects or LFS payloads are sent.",
            plan.destination, super::short(plan.base.as_deref().unwrap_or(""))
        ));
    }
    for (before, step) in plan.local_steps() {
        ui.label(format!(
            "{} reviewed push: {}",
            if before { "Before" } else { "After" },
            step
        ));
    }
    ui.label(format!(
        "{} outgoing commits ({} have embedded signatures), pinned to {}. Author policy: {} <{}>.",
        plan.commits.len(),
        plan.commits.iter().filter(|c| c.signed).count(),
        super::short(&plan.head),
        plan.expected_name,
        plan.expected_email
    ));
    if plan.skip_hooks {
        ui.label("Skip original pre-push hook for this push (as requested). The reviewed LFS upload guard still runs.");
    }
    for (host, account) in &plan.expected_accounts {
        ui.label(format!("Selected account for {host}: {account} (profile metadata; server identity not queried)."));
    }
    ui.label("Checks use local/fetched history only. Secret detection is heuristic; it does not download or inspect external LFS contents, verify signature trust, or replace a dedicated secret scanner.");
    if !plan.lfs_objects.is_empty() {
        ui.label(format!("{} LFS objects will be uploaded as part of the confirmed push to {}; their external content was not scanned. Endpoint settings use committed .lfsconfig plus inspected Git configuration, ignoring later working-tree edits.",plan.lfs_objects.len(),plan.lfs_targets().join(", ")));
    }
    if let Err(reason) = check::push_args(plan) {
        ui.colored_label(crate::theme::Tokens::get(ui.ctx()).warning, reason);
    }
    let mut out = Outcome::Open;
    if !plan.deleting && !plan.is_current_branch() {
        ui.label("Repairs require the underlying branch checked out. The push itself still sends the inspected object.");
        if let Some(name) = plan.branch.strip_prefix("refs/heads/")
            && ui.button("Check out for repairs").clicked()
        {
            out = Outcome::Done(Box::new(Action::Checkout(name.to_owned())));
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
                        out = Outcome::Done(Box::new(if fix == "messages" {
                            Action::PreflightMessages(Box::new(plan.clone()))
                        } else {
                            Action::FixPreflight(Box::new(plan.clone()), fix.into())
                        }));
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
    if !plan.deleting && !plan.destination.starts_with("refs/for/") {
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
                egui::Button::new(if plan.deleting {
                    "Delete inspected remote ref"
                } else if plan.force {
                    "Push with inspected lease"
                } else {
                    "Push inspected commit"
                }),
            )
            .clicked()
        {
            out = Outcome::Done(Box::new(Action::SubmitPreflight(Box::new(plan.clone()))));
        }
        if ui.button("Recheck").clicked() {
            out = Outcome::Done(Box::new(Action::ReloadPreflight(Box::new(plan.clone()))));
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
                if matches!(tab.dialog, Some(Dialog::PushLoading { .. })) {
                    continue;
                }
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
            id: 0,
            agent: None,
            label: "Restore remote branch".into(),
            undo: vec![vec!["push".into()]],
            redo: vec![vec!["push".into()]],
            heads: None,
        };
        let current = tab.timeline.current;
        for kind in [
            super::super::OpKind::Undo(entry.clone()),
            super::super::OpKind::Redo(entry.clone()),
        ] {
            tab.run_ops(
                "Undo/redo push",
                vec![vec!["push".into()]],
                Refresh::All,
                kind,
                &ctx,
            );
            assert_eq!(tab.timeline.current, current);
            receive(&mut tab, &ctx)?;
            tab.dialog = None;
            assert_eq!(tab.timeline.current, current);
            assert!(tab.preflight_pending.is_some());
        }
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
    fn existing_delete_restore_tag_and_push_dispatch_matrix() -> Result<(), String> {
        use crate::repo::operations::run_steps;
        use crate::repo::{OpKind, ops};
        let mut rows = Vec::new();
        for case in [
            "remote_delete",
            "both_delete_d",
            "both_delete_D",
            "undo_both",
            "redo_both",
            "remote_tag_delete",
            "normal_ff",
            "publish",
            "force_lease",
            "branch_context",
            "annotated_tag",
            "detached_tag",
            "gerrit",
            "create_then_push_tag",
        ] {
            let f = Fixture::new()?;
            let base = f.receiver_head()?;
            f.git(&["branch", "victim"])?;
            f.git(&["push", "-q", "origin", "victim"])?;
            f.git(&["branch", "--set-upstream-to=origin/victim", "victim"])?;
            let lease = format!("--force-with-lease=refs/heads/victim:{base}");
            let remote_delete =
                ops(&[&["push", &lease, "origin", "--delete", "refs/heads/victim"]]);
            let both = |flag: &str| {
                let here = ops(&[&["branch", flag, "--", "victim"]]);
                if flag == "-d" {
                    [here, remote_delete.clone()].concat()
                } else {
                    [remote_delete.clone(), here].concat()
                }
            };
            let kind = OpKind::plain();
            let steps = match case {
                "remote_delete" => remote_delete.clone(),
                "both_delete_d" => both("-d"),
                "both_delete_D" | "redo_both" => both("-D"),
                "undo_both" => {
                    let (result, _) = run_steps(&f.repo, &both("-D"), OpKind::plain(), None);
                    result?;
                    ops(&[
                        &["push", "origin", &format!("{base}:refs/heads/victim")],
                        &["branch", "--", "victim", &base],
                        &["branch", "--set-upstream-to=origin/victim", "--", "victim"],
                    ])
                }
                "remote_tag_delete" => {
                    f.git(&["tag", "release"])?;
                    f.git(&["push", "-q", "origin", "refs/tags/release"])?;
                    ops(&[&["push", "origin", "--delete", "refs/tags/release"]])
                }
                "normal_ff" => {
                    f.commit("next", "next", "Next")?;
                    ops(&[&["push"]])
                }
                "publish" => {
                    f.git(&["checkout", "-qb", "newbranch"])?;
                    f.commit("next", "next", "Next")?;
                    ops(&[&[
                        "-c",
                        "push.default=simple",
                        "push",
                        "-u",
                        "origin",
                        "refs/heads/newbranch",
                    ]])
                }
                "force_lease" => {
                    f.advance_receiver()?;
                    f.git(&["fetch", "-q", "origin"])?;
                    f.commit("next", "next", "Next")?;
                    ops(&[&["push", "--force-with-lease"]])
                }
                "branch_context" => {
                    f.git(&["checkout", "-qb", "feature"])?;
                    f.commit("next", "next", "Next")?;
                    f.git(&["checkout", "-q", "main"])?;
                    ops(&[&["push", "origin", "refs/heads/feature:refs/heads/feature"]])
                }
                "annotated_tag" | "detached_tag" => {
                    f.git(&["tag", "-a", "-m", "Release", "release"])?;
                    if case == "detached_tag" {
                        f.git(&["checkout", "-q", "--detach"])?;
                    }
                    ops(&[&["push", "origin", "refs/tags/release"]])
                }
                "gerrit" => {
                    let head = f.commit(
                        "next",
                        "next",
                        "Next\n\nChange-Id: I0123456789abcdef0123456789abcdef01234567",
                    )?;
                    crate::gerrit::review_ops("origin", "main", "", &head, &base)?
                }
                "create_then_push_tag" => ops(&[
                    &["tag", "-a", "-m", "Release", "release"],
                    &["push", "origin", "refs/tags/release"],
                ]),
                _ => unreachable!(),
            };
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
            if case == "undo_both" {
                let entry = UndoEntry {
                    id: 0,
                    agent: None,
                    label: case.into(),
                    undo: steps.clone(),
                    redo: both("-D"),
                    heads: None,
                };
                tab.run_ops(case, steps.clone(), Refresh::All, OpKind::Undo(entry), &ctx);
            } else if case == "redo_both" {
                let entry = UndoEntry {
                    id: 0,
                    agent: None,
                    label: case.into(),
                    undo: vec![],
                    redo: steps.clone(),
                    heads: None,
                };
                tab.run_ops(case, steps.clone(), Refresh::All, OpKind::Redo(entry), &ctx);
            } else {
                tab.run_ops(case, steps.clone(), Refresh::All, kind.clone(), &ctx);
            }
            // Run the exact local-tag completion path before its preview.
            if case == "create_then_push_tag" {
                let deadline = Instant::now() + Duration::from_secs(20);
                while tab.busy_op.is_some() && Instant::now() < deadline {
                    if let Ok(Msg::OpDone {
                        label,
                        result,
                        refresh,
                        kind,
                        quiet,
                        ..
                    }) = tab.rx.recv_timeout(Duration::from_secs(1))
                    {
                        tab.op_done(label, result, refresh, kind, quiet, &ctx);
                    }
                }
            }
            receive(&mut tab, &ctx)?;
            let dialog = tab.dialog.take();
            let (preview, error, control) = match dialog {
                Some(Dialog::PushPreflight { plan, .. }) => {
                    let approved = check::approved_ops(*plan);
                    let (r, _) = run_steps(&f.repo, &approved, OpKind::plain(), None);
                    (true, String::new(), r.err().unwrap_or_default())
                }
                Some(Dialog::PushError(error)) => {
                    return Err(format!("{case}: preview refused: {error}"));
                }
                _ => return Err(format!("{case}: unexpected dialog")),
            };
            if matches!(
                case,
                "remote_delete" | "both_delete_d" | "both_delete_D" | "redo_both"
            ) {
                assert!(
                    crate::git::run(
                        Some(&f.receiver),
                        &["rev-parse", "--verify", "refs/heads/victim"]
                    )
                    .is_err()
                );
                assert_eq!(
                    f.git(&["rev-parse", "--verify", "refs/heads/victim"])
                        .is_ok(),
                    case == "remote_delete"
                );
            }
            if case == "undo_both" {
                assert_eq!(f.git(&["rev-parse", "refs/heads/victim"])?, base);
                assert_eq!(
                    crate::git::run(Some(&f.receiver), &["rev-parse", "refs/heads/victim"])?.trim(),
                    base
                );
            }
            if case == "remote_tag_delete" {
                assert!(
                    crate::git::run(
                        Some(&f.receiver),
                        &["rev-parse", "--verify", "refs/tags/release"]
                    )
                    .is_err()
                );
            }
            rows.push(serde_json::json!({"case":case,"preview":preview,"preview_error":error,"production_dispatch_or_raw_git_error":control}));
            assert!(
                rows.last()
                    .and_then(|r| r["production_dispatch_or_raw_git_error"].as_str())
                    .is_some_and(str::is_empty),
                "{case}: {:?}",
                rows.last()
            );
        }
        assert!(rows.iter().all(|row| row["preview"] == true));
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
