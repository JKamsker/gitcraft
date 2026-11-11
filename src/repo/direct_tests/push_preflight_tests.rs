use super::*;
use crate::push_preflight::tests::Fixture;
use crate::repo::{OpKind, UndoEntry};

#[test]
fn reviewed_auth_dialog_uses_failed_push_endpoint_and_never_rebinds_an_expired_receipt()
-> Result<(), String> {
    let f = Fixture::new()?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
    let actual = "https://actual-push.invalid/other/project.git";
    f.git(&["remote", "set-url", "--push", "origin", actual])?;
    // An opaque diagnostic alone never authorizes sign-in or a fetch-URL fallback.
    let proofless = crate::push_preflight::remember_auth_endpoint(actual);
    tab.op_done(
        "Push".into(),
        Err(proofless.clone()),
        Refresh::Status,
        OpKind::plain(),
        false,
        &ctx,
    );
    assert!(tab.dialog.is_none() && tab.https_challenge.is_none());
    assert!(tab.credentials.is_none() && tab.running.is_none() && tab.busy_op.is_none());
    assert_eq!(
        crate::push_preflight::take_auth_endpoint(&proofless).as_deref(),
        Some(actual)
    );

    // Capture the actual configured push route, then record an authentication failure
    // through the same receipt consumer used by APPROVED dispatch. No endpoint is contacted.
    let original = vec!["push".to_owned(), "origin".into()];
    let binding = crate::git::RouteBinding::capture(&f.repo, &["push", "origin"])?;
    let control = tab.follow_op(std::slice::from_ref(&original), false, &ctx);
    let receipt = crate::push_preflight::remember_auth_endpoint(actual);
    let failed = crate::repo::run_reviewed_push(
        &original,
        |_| Ok(binding.clone()),
        || Err(receipt.clone()),
        Some(&control),
    );
    tab.op_done(
        "Push".into(),
        failed,
        Refresh::Status,
        OpKind::plain(),
        false,
        &ctx,
    );
    assert!(matches!(&tab.dialog, Some(Dialog::Credentials { url, .. }) if url == actual));
    let ticket = tab
        .https_challenge
        .as_ref()
        .ok_or("No bound sign-in ticket")?;
    assert_eq!(ticket.binding, Some(binding));
    assert!(ticket.authorizes_endpoint(actual));
    assert!(!ticket.authorizes_endpoint(&f.receiver.to_string_lossy()));
    assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());

    // Replaying the consumed diagnostic has no typed route proof and closes sign-in.
    tab.op_done(
        "Push".into(),
        Err(receipt),
        Refresh::Status,
        OpKind::plain(),
        false,
        &ctx,
    );
    assert!(tab.dialog.is_none() && tab.https_challenge.is_none());
    assert!(tab.credentials.is_none() && tab.running.is_none() && tab.busy_op.is_none());
    assert!(crate::settings::network_page::take_rejections(&ctx).is_empty());
    Ok(())
}

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
        let remote_delete = ops(&[&["push", &lease, "origin", "--delete", "refs/heads/victim"]]);
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
            .as_deref()
            .is_some_and(|(text, error)| *error && text.contains("profile"))
    );
    assert_eq!(f.receiver_head()?, before);
    Ok(())
}
