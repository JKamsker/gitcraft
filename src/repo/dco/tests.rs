//! The sign-off against real repositories: finding the rules, committing signed off, and signing
//! off a pushed branch, then undoing and redoing that, here and on a bare remote.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::signoff::{self, Rules};
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{Action, Dialog, OpKind, RepoTab, Undo, UndoEntry};

/// A fresh folder named after `name`, with a space and some unicode in it.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft dco-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?} failed: {out:?}");
    out.unwrap_or_default()
}

/// A repository with one commit on `main`, by Zoë.
fn repo(name: &str) -> PathBuf {
    let dir = folder(name);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Zoë Ångström"],
        &["config", "user.email", "zoe@example.org"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        run(&dir, args);
    }
    dir
}

/// Commits a file on the branch checked out, without a sign-off unless `signed`.
fn commit(dir: &Path, file: &str, signed: bool) {
    let _ = std::fs::write(dir.join(file), format!("{file}\n"));
    run(dir, &["add", "--", file]);
    let message = format!("add {file}");
    let mut args = vec!["commit", "-q", "-m", &message];
    if signed {
        args.push("--signoff");
    }
    run(dir, &args);
}

fn remove(dirs: &[&Path]) {
    for dir in dirs {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn the_rules_come_from_a_committed_dco_file_and_the_kept_choice() {
    let dir = repo("rules");
    let none = signoff::read_rules(&dir);
    std::fs::create_dir_all(dir.join(".github")).ok();
    let _ = std::fs::write(dir.join(".github/dco.yml"), "require:\n  members: false\n");
    // Not committed yet: not the repository's rule.
    let uncommitted = signoff::read_rules(&dir);
    run(&dir, &["add", "--", ".github/dco.yml"]);
    run(&dir, &["commit", "-q", "-m", "dco"]);
    let app = signoff::read_rules(&dir);
    let _ = std::fs::write(dir.join("DCO"), "Developer Certificate of Origin\n");
    run(&dir, &["add", "DCO"]);
    run(&dir, &["commit", "-q", "-m", "certificate"]);
    let both = signoff::read_rules(&dir);
    let off = signoff::choice_command(Some(false));
    let off: Vec<&str> = off.iter().map(String::as_str).collect();
    run(&dir, &off);
    let chosen = signoff::read_rules(&dir);
    let unset = signoff::choice_command(None);
    let unset: Vec<&str> = unset.iter().map(String::as_str).collect();
    run(&dir, &unset);
    let automatic = signoff::read_rules(&dir);
    remove(&[&dir]);
    assert_eq!(none, Rules::default());
    assert_eq!(uncommitted.file, None);
    assert_eq!(app.file.as_deref(), Some(".github/dco.yml"));
    assert_eq!(both.file.as_deref(), Some("DCO"), "the certificate first");
    assert_eq!(chosen.choice, Some(false));
    assert_eq!(automatic.choice, None);
}

#[test]
fn a_repository_without_commits_has_no_rules() {
    let dir = folder("empty");
    run(&dir, &["init", "-q"]);
    let rules = signoff::read_rules(&dir);
    remove(&[&dir]);
    assert_eq!(rules, Rules::default());
}

/// Polls the tab until `done` or a while has passed.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(30);
    while !done(tab) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        tab.poll(ctx);
    }
}

/// Waits for the tab's background work: the operation running, and what it reads after.
fn settle(tab: &mut RepoTab, ctx: &egui::Context) {
    wait(tab, ctx, |tab| {
        tab.busy_op.is_none() && !tab.loading && !tab.dco.reading
    });
}

fn open(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.reload(ctx);
    settle(&mut tab, ctx);
    tab.dco_frame(ctx);
    settle(&mut tab, ctx);
    tab
}

fn commit_in_tab(tab: &mut RepoTab, ctx: &egui::Context, dir: &Path, file: &str) -> String {
    let _ = std::fs::write(dir.join(file), "x\n");
    tab.commit_summary = format!("feat: {file}");
    tab.reload_status(ctx);
    settle(tab, ctx);
    tab.commit(ctx);
    settle(tab, ctx);
    run(dir, &["log", "-1", "--format=%B"])
}

#[test]
fn the_commit_box_signs_off_where_a_dco_file_asks_and_as_chosen() {
    let dir = repo("commit-box");
    let _ = std::fs::write(dir.join("DCO.md"), "certificate\n");
    run(&dir, &["add", "DCO.md"]);
    run(&dir, &["commit", "-q", "-m", "dco"]);
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let auto = tab.signs_off();
    let first = commit_in_tab(&mut tab, &ctx, &dir, "a.txt");
    // Turned off by hand: kept in the repository's config, and the next commit isn't.
    tab.dco_act(super::DcoAct::SetSignOff(false), &ctx);
    wait(&mut tab, &ctx, |_| {
        signoff::read_rules(&dir).choice == Some(false)
    });
    let second = commit_in_tab(&mut tab, &ctx, &dir, "b.txt");
    let kept = signoff::read_rules(&dir).choice;
    // Back on, which is what the repository asks: the choice is taken out again.
    tab.dco_act(super::DcoAct::SetSignOff(true), &ctx);
    wait(&mut tab, &ctx, |_| {
        signoff::read_rules(&dir).choice.is_none()
    });
    let automatic = signoff::read_rules(&dir).choice;
    remove(&[&dir]);
    assert!(auto, "on by itself with a DCO.md");
    assert!(
        first.contains("Signed-off-by: Zoë Ångström <zoe@example.org>"),
        "{first}"
    );
    assert!(!second.contains("Signed-off-by"), "{second}");
    assert_eq!(kept, Some(false));
    assert_eq!(automatic, None);
}

/// A repository that doesn't use DCO is as it was: nothing signed off, nothing flagged.
#[test]
fn a_repository_without_dco_is_left_alone() {
    let dir = repo("no-dco");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let message = commit_in_tab(&mut tab, &ctx, &dir, "a.txt");
    // Signing off by choice flags nobody else's commits.
    tab.dco_act(super::DcoAct::SetSignOff(true), &ctx);
    let required = tab.dco.required();
    remove(&[&dir]);
    assert!(!message.contains("Signed-off-by"));
    assert!(!required);
}

/// A repository with `feat` pushed to a bare `origin`: one commit signed off, two not.
fn pushed_branch(name: &str) -> (PathBuf, PathBuf) {
    let dir = repo(name);
    let bare = folder(&format!("{name}-remote.git"));
    run(&bare, &["init", "-q", "--bare"]);
    let bare_arg = bare.to_string_lossy().into_owned();
    run(&dir, &["remote", "add", "origin", &bare_arg]);
    run(&dir, &["push", "-q", "-u", "origin", "main"]);
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    commit(&dir, "one file.txt", false);
    commit(&dir, "twö.txt", true);
    commit(&dir, "three.txt", false);
    run(&dir, &["push", "-q", "-u", "origin", "feat"]);
    (dir, bare)
}

fn entry(kind: OpKind) -> Option<UndoEntry> {
    match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            id: 0,
            agent: None,
            label: "sign off".to_owned(),
            undo,
            redo,
            heads: None,
        }),
        _ => None,
    }
}

fn missing_on(dir: &Path, rev: &str) -> usize {
    let list = signoff::commits(dir, &format!("main..{rev}")).unwrap_or_default();
    signoff::missing(&list).count()
}

#[test]
fn signing_off_a_branch_rewrites_and_pushes_it_and_undo_puts_it_back_everywhere() {
    let (dir, bare) = pushed_branch("fix");
    let before = git::head(&dir);
    let plan = signoff::plan(&dir, "main", Some(&before));
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    let preview = super::fix_preview(&plan, "Zoë Ångström <zoe@example.org>", "zoe@example.org");
    let kind = OpKind::Normal(
        Undo::Custom {
            undo: plan.undo_steps(),
            redo: plan.redo_steps(),
        },
        String::new(),
    );
    let (done, kind) = run_steps(&dir, &plan.steps(), kind, None);
    let signed = git::head(&dir);
    let remote_after = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let missing_after = missing_on(&dir, "HEAD");
    let entry = entry(kind);
    let undone = entry
        .clone()
        .map(|e| run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None));
    let head_undone = git::head(&dir);
    let remote_undone = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let redone = undone.and_then(|(r, kind)| match kind {
        OpKind::Undo(e) if r.is_ok() => {
            Some(run_steps(&dir, &e.redo.clone(), OpKind::Redo(e), None))
        }
        _ => None,
    });
    let remote_redone = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let head_redone = git::head(&dir);
    let missing_redone = missing_on(&dir, "HEAD");
    remove(&[&dir, &bare]);
    assert_eq!(plan.unsigned().len(), 2);
    assert!(preview.contains("2 of them are ones"), "{preview}");
    assert!(preview.contains("origin/feat"), "{preview}");
    assert!(done.is_ok(), "{done:?}");
    assert_ne!(signed, before);
    assert_eq!(remote_after, signed);
    assert_eq!(missing_after, 0);
    assert_eq!(head_undone, before);
    assert_eq!(remote_undone, before);
    assert!(matches!(redone, Some((Ok(_), _))), "{redone:?}");
    assert_eq!(missing_redone, 0);
    assert_eq!(remote_redone, head_redone);
    assert_ne!(head_redone, before);
}

/// Clones `bare`'s `feat`, commits on it as Bob and pushes: someone else's work.
fn push_as_someone_else(bare: &Path, name: &str) -> (PathBuf, String) {
    let other = folder(name);
    let bare_arg = bare.to_string_lossy().into_owned();
    let other_arg = other.to_string_lossy().into_owned();
    run(
        &std::env::temp_dir(),
        &["clone", "-q", "-b", "feat", &bare_arg, &other_arg],
    );
    run(&other, &["config", "user.name", "Bob"]);
    run(&other, &["config", "user.email", "bob@example.org"]);
    commit(&other, "theirs.txt", true);
    run(&other, &["push", "-q", "origin", "feat"]);
    let theirs = git::head(&other);
    (other, theirs)
}

/// Someone pushed to the branch and it was fetched: the plan is refused, rather than a push that
/// throws their commit away.
#[test]
fn commits_on_the_remote_that_the_branch_hasnt_are_never_pushed_over() {
    let (dir, bare) = pushed_branch("remote-ahead");
    let (other, _) = push_as_someone_else(&bare, "remote-ahead-other");
    run(&dir, &["fetch", "-q", "origin"]);
    let head = git::head(&dir);
    let plan = signoff::plan(&dir, "main", Some(&head));
    remove(&[&dir, &bare, &other]);
    assert!(plan.is_err_and(|e| e.contains("pull them first")));
}

#[test]
fn recording_local_signoff_queues_review_only_after_success_and_keeps_its_original_lease()
-> Result<(), String> {
    use crate::repo::timeline::{Awaiting, Ended};
    let plan = signoff::Plan {
        branch: "feat".into(),
        head: "a".repeat(40),
        base: "b".repeat(40),
        commits: Vec::new(),
        push: Some(signoff::PushTarget {
            remote: "selected".into(),
            urls: vec!["/private/recorded/selected.git".into()],
            remote_ref: "refs/heads/review-target".into(),
            tracking: Some("refs/remotes/selected/review-target".into()),
            seen: Some("c".repeat(40)),
        }),
    };
    let label = "Sign off the commits of feat";
    assert_eq!(super::fix_label(&plan), label);
    let Dialog::Confirm {
        title,
        body,
        button,
        ops,
        undo,
        ..
    } = super::fix_dialog(
        &plan,
        "Fixture <fixture@example.org>",
        "fixture@example.org",
    )
    else {
        return Err("No local sign-off confirmation".into());
    };
    assert_eq!(title, label);
    assert_eq!(button, "Sign off, then review push");
    assert!(body.contains("Review and approve that push separately"));
    assert!(body.contains("Ctrl+Z restores the local commits"));
    assert!(!crate::push_preflight::has_push(&ops));
    assert!(ops.contains(&crate::repo::operations::expect_ref_at(
        "refs/heads/feat",
        &plan.head
    )));
    assert!(matches!(undo, Undo::HeadMove { mode: "--keep" }));

    for result in [
        Ok(String::new()),
        Err("local rewrite failed".into()),
        Err(git::CANCELLED.into()),
        Err("Authentication failed in local hook".into()),
    ] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/dco-completion-recording".into(), &ctx);
        tab.tx.cancel.cancel(); // no queued reader, command or process can execute
        tab.dco.running = Some(plan.clone());
        tab.timeline.awaiting = Awaiting::Ended {
            after_read: 0,
            then: Ended::New {
                label: label.into(),
                undo: Some(UndoEntry {
                    id: 0,
                    agent: None,
                    label: label.into(),
                    undo: Vec::new(),
                    redo: Vec::new(),
                    heads: Some((plan.head.clone(), "d".repeat(40))),
                }),
            },
        };
        tab.dco_op_done(label, &result, &ctx);
        assert!(tab.dco.running.is_none());
        assert!(tab.dialog.is_none() && tab.busy_op.is_none() && tab.running.is_none());
        assert!(tab.preflight_pending.is_none() && tab.last_op.is_none());
        if result.is_ok() {
            let prepared = super::completed_push(
                &plan,
                "d".repeat(40),
                crate::git::repository_tools::guard::Snapshot::recording(),
            )
            .ok_or("Missing reviewed continuation")?;
            let route = &prepared.route;
            assert_eq!(route.remote, "selected");
            assert_eq!(route.destination, "refs/heads/review-target");
            assert_eq!(route.urls, ["/private/recorded/selected.git"]);
            assert_eq!(route.commit, "d".repeat(40));
            assert_eq!(route.lease, Some("c".repeat(40)));
            let ops = route.operations();
            assert!(crate::push_preflight::has_push(&ops));
            assert!(
                !ops.iter()
                    .flatten()
                    .any(|arg| arg == crate::push_preflight::APPROVED)
            );
            let mut preview = crate::push_preflight::selected_continuation_recording::plan(route);
            preview.retain_selection(prepared.clone())?;
            for changed in ["endpoint", "lease", "source"] {
                let mut changed_route = route.clone();
                match changed {
                    "endpoint" => changed_route.urls = vec!["/private/recorded/changed.git".into()],
                    "lease" => changed_route.lease = Some("e".repeat(40)),
                    _ => changed_route.commit = "f".repeat(40),
                }
                let mut changed_preview =
                    crate::push_preflight::selected_continuation_recording::plan(&changed_route);
                assert!(changed_preview.retain_selection(prepared.clone()).is_err());
            }
            let Awaiting::Ended {
                then:
                    Ended::New {
                        label: raw,
                        undo: Some(undo),
                    },
                ..
            } = &tab.timeline.awaiting
            else {
                return Err("Local undo lost".into());
            };
            assert_eq!(raw, label);
            assert_eq!(undo.label, format!("{label} (here only)"));
            assert_eq!(undo.heads, Some((plan.head.clone(), "d".repeat(40))));
        } else {
            assert!(
                tab.preflight_after.is_none(),
                "local failure/cancel must queue zero pushes"
            );
        }
    }
    Ok(())
}

/// Pushed meanwhile and not fetched: the lease refuses the push, the remote keeps their commit,
/// and the rewrite done here is undoable all the same.
#[test]
fn a_refused_push_leaves_the_rewrite_undoable_here() -> Result<(), String> {
    let (dir, bare) = pushed_branch("lease");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let head = git::head(&dir);
    tab.dco_act(
        super::DcoAct::PlanFix {
            base: "main".into(),
            head: head.clone(),
        },
        &ctx,
    );
    wait(&mut tab, &ctx, |tab| tab.dialog.is_some());
    // As the dialog's button does.
    if let Some(Dialog::Confirm {
        title, ops, undo, ..
    }) = tab.dialog.take()
    {
        assert_eq!(title, "Sign off the commits of feat");
        assert!(!crate::push_preflight::has_push(&ops));
        tab.apply(
            Action::UndoableOps {
                label: title,
                ops,
                refresh: crate::repo::Refresh::All,
                undo,
            },
            &ctx,
        );
    }
    assert_eq!(tab.busy_op.as_deref(), Some("Sign off the commits of feat"));
    assert!(
        tab.dialog.is_none(),
        "local rewrite must finish before push review"
    );
    wait(&mut tab, &ctx, |tab| {
        tab.busy_op.is_none() && tab.dialog.is_some()
    });
    wait(&mut tab, &ctx, |tab| {
        !matches!(tab.dialog, Some(Dialog::PushLoading { .. }))
    });
    let signed = git::head(&dir);
    assert_ne!(signed, head);
    if let Some(Dialog::PushError(error)) = &tab.dialog {
        return Err(format!(
            "DCO fresh checklist refused after local rewrite: {error}"
        ));
    }
    assert!(matches!(
        &tab.dialog,
        Some(Dialog::PushPreflight {
            acknowledge: false,
            ..
        })
    ));
    // Cancel as the dialog does: there was no consent and no send. The signed local
    // history and its undo remain available; retry uses the original retained action.
    let selection = match &tab.dialog {
        Some(Dialog::PushPreflight { plan, .. }) => plan.selection().cloned(),
        _ => None,
    }
    .ok_or("Original bound selection missing")?;
    tab.dialog = None;
    settle(&mut tab, &ctx);
    assert_eq!(git::head(&dir), signed);
    assert_eq!(git::resolve(&bare, "refs/heads/feat")?, head);
    assert!(tab.busy_op.is_none() && tab.preflight_after.is_none());
    assert!(
        tab.last_op
            .as_ref()
            .is_some_and(|op| !crate::push_preflight::has_push(&op.ops))
    );
    let pending = tab
        .preflight_pending
        .take()
        .ok_or("Original DCO push action was lost")?;
    tab.preflight_selection = Some(selection);
    tab.run_ops_ex(
        &pending.label,
        pending.ops,
        pending.refresh,
        pending.kind,
        pending.quiet,
        &ctx,
    );
    let plan = crate::repo::push_preflight::test_support::review(&mut tab, &ctx)
        .map_err(|e| format!("DCO push review before dispatch: {e}"))?;
    assert_ne!(
        git::head(&dir),
        head,
        "review must describe the rewritten source"
    );
    assert_eq!(missing_on(&dir, "HEAD"), 0);
    assert_eq!(
        git::resolve(&bare, "refs/heads/feat").ok().as_deref(),
        Some(head.as_str())
    );
    assert_eq!(plan.head, git::head(&dir));
    assert_eq!(plan.base.as_deref(), Some(head.as_str()));
    // The receiver moves only after inspection; the actual server lease must refuse it.
    let (other, theirs) = push_as_someone_else(&bare, "lease-other");
    tab.apply(Action::SubmitPreflight(Box::new(plan)), &ctx);
    settle(&mut tab, &ctx);
    assert!(
        tab.activity
            .last()
            .is_some_and(|entry| matches!(&entry.outcome,
        crate::repo::activity::Outcome::Failed(error) if error.contains("stale info"))),
        "{:?}",
        tab.activity.last()
    );
    wait(&mut tab, &ctx, |tab| {
        !tab.timeline.entries.iter().all(|e| e.undo.is_none())
    });
    let remote = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let rewritten = git::head(&dir) != head;
    let undo_label = tab
        .timeline
        .current_entry()
        .and_then(|e| e.undo.as_ref())
        .map(|e| e.label.clone());
    tab.undo(&ctx);
    settle(&mut tab, &ctx);
    let back = git::head(&dir);
    assert_eq!(git::resolve(&bare, "refs/heads/feat")?, theirs);
    tab.redo(&ctx);
    settle(&mut tab, &ctx);
    assert_eq!(git::head(&dir), signed);
    assert_eq!(git::resolve(&bare, "refs/heads/feat")?, theirs);
    assert!(
        tab.dialog.is_none(),
        "local redo must not request a remote push"
    );
    remove(&[&dir, &bare, &other]);
    assert_eq!(remote, theirs);
    assert!(rewritten);
    assert!(
        undo_label.is_some_and(|l| l.contains("here only")),
        "the rewrite is undoable"
    );
    assert_eq!(back, head);
    Ok(())
}

#[test]
fn a_branch_that_cannot_be_signed_off_that_way_is_refused_with_why() {
    let dir = repo("refused");
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    // Nothing of its own yet.
    let empty = signoff::plan(&dir, "main", None);
    commit(&dir, "signed.txt", true);
    let all_signed = signoff::plan(&dir, "main", None);
    commit(&dir, "unsigned.txt", false);
    // Not where the pull request is.
    let elsewhere = signoff::plan(&dir, "main", Some(&"a".repeat(40)));
    // Changes not committed.
    let _ = std::fs::write(dir.join("signed.txt"), "changed\n");
    let dirty = signoff::plan(&dir, "main", None);
    run(&dir, &["checkout", "--", "signed.txt"]);
    // A merge in it.
    run(&dir, &["checkout", "-q", "-b", "side", "main"]);
    commit(&dir, "side.txt", false);
    run(&dir, &["checkout", "-q", "feat"]);
    run(&dir, &["merge", "-q", "--no-edit", "side"]);
    let merges = signoff::plan(&dir, "main", None);
    // Detached.
    run(&dir, &["checkout", "-q", "--detach"]);
    let detached = signoff::plan(&dir, "main", None);
    remove(&[&dir]);
    assert!(empty.is_err_and(|e| e.contains("no commits of its own")));
    assert!(all_signed.is_err_and(|e| e.contains("already signed off")));
    assert!(elsewhere.is_err_and(|e| e.contains("pull or push it first")));
    assert!(dirty.is_err_and(|e| e.contains("Commit or stash")));
    assert!(merges.is_err_and(|e| e.contains("merge commits")));
    assert!(detached.is_err_and(|e| e.contains("detached")));
}

#[test]
fn a_branch_without_a_remote_is_signed_off_here_only() {
    let dir = repo("local-only");
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    commit(&dir, "x.txt", false);
    let plan = signoff::plan(&dir, "main", None);
    let push = plan.as_ref().map(|p| p.push.clone());
    let steps = plan.as_ref().map(|p| p.steps()).unwrap_or_default();
    let (result, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let missing = missing_on(&dir, "HEAD");
    remove(&[&dir]);
    assert_eq!(push, Ok(None));
    assert!(!steps.iter().any(|s| s.contains(&"push".to_owned())));
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(missing, 0);
}

/// A tag named like the branch doesn't make the branch `heads/x`; another branch on the
/// commits rewritten stays where it was (`rebase.updateRefs` would move it, and undo not back).
#[test]
fn a_tag_of_the_same_name_and_branches_stacked_on_it_are_fine() {
    let dir = repo("tag-stack");
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    commit(&dir, "x.txt", false);
    run(&dir, &["tag", "feat"]);
    run(&dir, &["branch", "stacked", "HEAD"]);
    commit(&dir, "y.txt", false);
    run(&dir, &["config", "rebase.updateRefs", "true"]);
    let stacked = git::resolve(&dir, "refs/heads/stacked");
    let plan = signoff::plan(&dir, "main", None);
    let branch = plan.as_ref().map(|p| p.branch.clone());
    let steps = plan.as_ref().map(|p| p.steps()).unwrap_or_default();
    let (result, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let stacked_after = git::resolve(&dir, "refs/heads/stacked");
    remove(&[&dir]);
    assert_eq!(branch, Ok("feat".to_owned()));
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(stacked, stacked_after);
}
