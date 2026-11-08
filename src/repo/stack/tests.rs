//! Stacks through the tab, as the views drive them: the dialog's suggestion, recording a
//! parent, undoing and redoing it, and what is refused.

use std::time::{Duration, Instant};

use super::StackAct;
use crate::git::stack::{self, test_repo::TestRepo};
use crate::repo::{Action, RepoTab};

/// Takes in what the tab's background work sends until `done` holds and it is idle.
pub(crate) fn settle(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        tab.poll(ctx);
        if done(tab) && !tab.is_busy() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!("the tab never settled: {:?}", tab.status));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A tab on `repo`, its first read in.
pub(crate) fn open(repo: &TestRepo, ctx: &egui::Context) -> Result<RepoTab, String> {
    let mut tab = RepoTab::new(repo.path().to_path_buf(), ctx);
    settle(&mut tab, ctx, |t| !t.opening())?;
    Ok(tab)
}

#[test]
fn based_on_is_suggested_recorded_undone_and_redone() -> Result<(), String> {
    let repo = TestRepo::new("tab based on")?;
    repo.branch("schema", "main")?;
    repo.commit("s.txt", "s\n", "schema")?;
    repo.branch("api", "schema")?;
    repo.commit("a.txt", "a\n", "api")?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    assert!(tab.stacks.layers.is_empty());

    tab.apply(Action::Stack(StackAct::OpenBasedOn("api".into())), &ctx);
    settle(&mut tab, &ctx, |t| {
        t.stacks
            .dialog
            .as_ref()
            .is_some_and(|d| d.suggestion.is_some())
    })?;
    let choice = tab.stacks.dialog.as_ref().map(|d| d.choice.clone());
    assert_eq!(choice.as_deref(), Some("schema"));

    tab.apply(
        Action::Stack(StackAct::SetParent {
            branch: "api".into(),
            parent: "schema".into(),
        }),
        &ctx,
    );
    assert!(tab.stacks.dialog.is_none());
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_some())?;
    let recorded = stack::recorded(repo.path())?;
    let api = recorded.get("api").ok_or("api has no parent")?;
    assert_eq!(api.parent, "schema");
    assert_eq!(api.base.as_deref(), Some(repo.tip("schema")?.as_str()));
    assert_eq!(
        tab.stack_line("api").as_deref(),
        Some("Stack: schema → api")
    );

    tab.apply(Action::Undo, &ctx);
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_none())?;
    assert!(stack::recorded(repo.path())?.is_empty());
    tab.apply(Action::Redo, &ctx);
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_some())?;

    // schema on api would be a loop: refused, nothing recorded.
    tab.apply(
        Action::Stack(StackAct::SetParent {
            branch: "schema".into(),
            parent: "api".into(),
        }),
        &ctx,
    );
    settle(&mut tab, &ctx, |_| true)?;
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(s, failed)| *failed && s.contains("based on it"))
    );
    assert!(!stack::recorded(repo.path())?.contains_key("schema"));

    tab.apply(Action::Stack(StackAct::RemoveParent("api".into())), &ctx);
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_none())?;
    assert!(stack::recorded(repo.path())?.is_empty());
    Ok(())
}

#[test]
fn branches_without_history_in_common_are_refused() -> Result<(), String> {
    let repo = TestRepo::new("tab unrelated")?;
    repo.git(&["checkout", "-q", "--orphan", "lonely"])?;
    repo.git(&["rm", "-rfq", "."])?;
    repo.commit("l.txt", "l\n", "lonely")?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    tab.apply(
        Action::Stack(StackAct::SetParent {
            branch: "lonely".into(),
            parent: "main".into(),
        }),
        &ctx,
    );
    settle(&mut tab, &ctx, |t| t.status.is_some())?;
    let status = tab.status.clone().unwrap_or_default();
    assert!(
        status.1 && status.0.contains("no commit in common"),
        "{status:?}"
    );
    assert!(stack::recorded(repo.path())?.is_empty());
    Ok(())
}

/// What is recorded is read again before an operation, not taken from the last read: a
/// parent recorded since (here, outside the tab) is what undo puts back.
#[test]
fn removing_reads_what_is_recorded_now_for_its_undo() -> Result<(), String> {
    let repo = TestRepo::new("tab fresh read")?;
    repo.stack(&["api"])?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    // Changed behind the tab's back: based on main still, at another base.
    repo.steps(&stack::set_parent_ops("api", "main", "1234567"))?;
    let before = stack::recorded(repo.path())?.get("api").cloned();
    tab.apply(Action::Stack(StackAct::RemoveParent("api".into())), &ctx);
    settle(&mut tab, &ctx, |t| {
        t.stacks.layer("api").is_none() && t.status.is_some()
    })?;
    assert!(stack::recorded(repo.path())?.is_empty());
    tab.apply(Action::Undo, &ctx);
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_some())?;
    assert_eq!(stack::recorded(repo.path())?.get("api").cloned(), before);
    Ok(())
}

/// An operation started while the parent was being looked up is not dropped silently.
#[test]
fn an_operation_that_cannot_start_says_so() -> Result<(), String> {
    let repo = TestRepo::new("tab busy")?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    tab.busy_op = Some("Fetch".to_owned());
    tab.run_stack_op("Base a on main".to_owned(), Vec::new(), Vec::new(), &ctx);
    let status = tab.status.clone().unwrap_or_default();
    assert!(
        status.1 && status.0.contains("once Fetch is done"),
        "{status:?}"
    );
    tab.busy_op = None;
    Ok(())
}

fn is_ancestor(repo: &TestRepo, a: &str, b: &str) -> bool {
    repo.git(&["merge-base", "--is-ancestor", a, b]).is_ok()
}

/// Opens the restack of every layer that needs it, waits for its preview, starts it.
fn restack_all(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
    tab.apply(Action::Stack(StackAct::OpenRestack(None)), ctx);
    settle(tab, ctx, |t| {
        t.stacks
            .preview
            .as_ref()
            .is_some_and(|p| p.previews.is_some())
    })?;
    tab.apply(Action::Stack(StackAct::StartRestack), ctx);
    settle(tab, ctx, |t| {
        t.stacks.preview.is_none() && t.status.is_some()
    })
}

#[test]
fn restacking_from_the_tab_is_one_undo() -> Result<(), String> {
    let repo = TestRepo::new("tab restack undo")?;
    repo.stack(&["schema", "api", "ui"])?;
    // ui is checked out in another worktree: restacked there, and put back there.
    let wt = repo.dir.with_file_name(format!(
        "{} ui wt",
        repo.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&wt);
    repo.git(&["worktree", "add", "-q", &wt.to_string_lossy(), "ui"])?;
    let result = (|| -> Result<(), String> {
        repo.checkout("schema")?;
        repo.git(&["commit", "-q", "--amend", "-m", "schema amended"])?;
        repo.checkout("main")?;
        let (api, ui) = (repo.tip("api")?, repo.tip("ui")?);
        let ctx = egui::Context::default();
        let mut tab = open(&repo, &ctx)?;
        settle(&mut tab, &ctx, |t| {
            t.stacks.layer("api").is_some_and(|l| l.needs_restack())
        })?;
        let preview_len = {
            tab.apply(Action::Stack(StackAct::OpenRestack(None)), &ctx);
            settle(&mut tab, &ctx, |t| {
                t.stacks
                    .preview
                    .as_ref()
                    .is_some_and(|p| p.previews.is_some())
            })?;
            tab.stacks.preview.as_ref().map(|p| p.layers.len())
        };
        assert_eq!(preview_len, Some(2));
        tab.apply(Action::Stack(StackAct::StartRestack), &ctx);
        settle(&mut tab, &ctx, |t| {
            t.stacks.preview.is_none() && t.status.is_some()
        })?;
        assert!(is_ancestor(&repo, "schema", "api") && is_ancestor(&repo, "api", "ui"));
        let worktree_head = crate::git::run(Some(&wt), &["rev-parse", "HEAD"])?;
        assert_eq!(worktree_head.trim(), repo.tip("ui")?);
        let moved = (repo.tip("api")?, repo.tip("ui")?);
        let bases = stack::recorded(repo.path())?;

        tab.apply(Action::Undo, &ctx);
        settle(&mut tab, &ctx, |t| {
            t.status.as_ref().is_some_and(|s| s.0.starts_with("Undo"))
        })?;
        assert_eq!(
            (repo.tip("api")?, repo.tip("ui")?),
            (api.clone(), ui.clone())
        );
        let worktree_head = crate::git::run(Some(&wt), &["rev-parse", "HEAD"])?;
        assert_eq!(worktree_head.trim(), ui);
        assert!(
            crate::git::run(Some(&wt), &["status", "--porcelain"])?
                .trim()
                .is_empty()
        );
        tab.apply(Action::Redo, &ctx);
        settle(&mut tab, &ctx, |t| {
            t.status.as_ref().is_some_and(|s| s.0.starts_with("Redo"))
        })?;
        assert_eq!((repo.tip("api")?, repo.tip("ui")?), moved);
        assert_eq!(stack::recorded(repo.path())?, bases);
        Ok(())
    })();
    let _ = repo.git(&["worktree", "remove", "--force", &wt.to_string_lossy()]);
    let _ = std::fs::remove_dir_all(&wt);
    result
}

#[test]
fn a_restack_stopped_on_conflicts_continues_or_aborts_from_the_tab() -> Result<(), String> {
    for resolve in [true, false] {
        let repo = TestRepo::new(&format!("tab restack conflict {resolve}"))?;
        repo.stack(&["schema", "api"])?;
        repo.checkout("api")?;
        repo.commit("schema.txt", "api's\n", "api changes schema's file")?;
        repo.checkout("schema")?;
        repo.commit("schema.txt", "schema 2\n", "schema again")?;
        repo.checkout("main")?;
        let api = repo.tip("api")?;
        let ctx = egui::Context::default();
        let mut tab = open(&repo, &ctx)?;
        settle(&mut tab, &ctx, |t| {
            t.stacks.layer("api").is_some_and(|l| l.needs_restack())
        })?;
        restack_all(&mut tab, &ctx)?;
        settle(&mut tab, &ctx, |t| t.stacks.pending.is_some())?;
        let status = tab.status.clone().unwrap_or_default();
        assert!(
            status.1 && status.0.contains("stopped at api"),
            "{status:?}"
        );
        if resolve {
            std::fs::write(repo.dir.join("schema.txt"), "both\n").map_err(|e| e.to_string())?;
            repo.git(&["add", "schema.txt"])?;
            tab.apply(Action::Stack(StackAct::ResumeRestack), &ctx);
            settle(&mut tab, &ctx, |t| {
                t.stacks.pending.is_none() && t.status.is_some()
            })?;
            assert!(is_ancestor(&repo, "schema", "api"));
            assert_eq!(repo.git(&["show", "api:schema.txt"])?, "both\n");
        } else {
            tab.apply(Action::Stack(StackAct::AbortRestack), &ctx);
            settle(&mut tab, &ctx, |t| {
                t.stacks.pending.is_none() && t.status.is_some()
            })?;
            assert_eq!(repo.tip("api")?, api);
        }
        assert_eq!(
            repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(),
            "main"
        );
    }
    Ok(())
}

/// The interactive rebase of a stack's top with Update dependent branches: one undo puts the
/// layers it moved back too, and redo moves them again.
#[test]
fn undoing_a_rebase_that_updated_dependent_branches_puts_them_back() -> Result<(), String> {
    let repo = TestRepo::new("tab update refs undo")?;
    repo.stack(&["schema", "api", "ui"])?;
    repo.commit("m2.txt", "2\n", "main moves on")?;
    repo.checkout("ui")?;
    let before = (repo.tip("schema")?, repo.tip("api")?, repo.tip("ui")?);
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    let onto = repo.tip("main")?;
    tab.apply(
        Action::InteractiveRebase {
            onto: "refs/heads/main".into(),
            label: "main".into(),
        },
        &ctx,
    );
    settle(&mut tab, &ctx, |t| t.rebase.is_some())?;
    let editor = tab.rebase.as_mut().ok_or("no rebase editor")?;
    editor.update_refs = true;
    assert_eq!(editor.onto, onto);
    assert_eq!(editor.head, before.2);
    let id = editor.id;
    let mut described = vec![format!("git rebase -i --autostash --update-refs {onto}")];
    described.extend(
        editor
            .entries
            .iter()
            .map(|e| format!("todo: Pick {} {}", e.commit.hash, e.message)),
    );
    tab.apply(Action::RunRebase, &ctx);
    tab.review_rebase_for_test(&ctx, "ui", &before.2, &onto);
    let Some(crate::repo::Dialog::Confirm { preview, .. }) = &tab.dialog else {
        return Err("no interactive confirmation".into());
    };
    let instead = preview
        .instead
        .as_ref()
        .ok_or("no confirmed rebase action")?;
    assert!(matches!(instead.action, Action::ConfirmedRebase(approved) if approved == id));
    assert_eq!(instead.described, described);
    assert_eq!(
        (repo.tip("schema")?, repo.tip("api")?, repo.tip("ui")?),
        before
    );
    let accepted = tab.accept_rebase_for_test(&ctx, "ui", &before.2, &onto);
    tab.apply(accepted, &ctx);
    settle(&mut tab, &ctx, |t| t.rebase.is_none() && t.status.is_some())?;
    assert!(is_ancestor(&repo, "main", "schema") && is_ancestor(&repo, "main", "api"));
    let tips = || -> Result<(String, String, String), String> {
        Ok((repo.tip("schema")?, repo.tip("api")?, repo.tip("ui")?))
    };
    tab.apply(Action::Undo, &ctx);
    settle(&mut tab, &ctx, |t| {
        t.status.as_ref().is_some_and(|s| s.0.starts_with("Undo"))
    })?;
    assert_eq!(tips()?, before, "{:?}", tab.status);
    tab.apply(Action::Redo, &ctx);
    settle(&mut tab, &ctx, |t| {
        t.status.as_ref().is_some_and(|s| s.0.starts_with("Redo"))
    })?;
    assert!(is_ancestor(&repo, "main", "schema") && is_ancestor(&repo, "schema", "ui"));
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(), "ui");
    // And undone again, after the redo.
    tab.apply(Action::Undo, &ctx);
    settle(&mut tab, &ctx, |t| {
        t.status.as_ref().is_some_and(|s| s.0.starts_with("Undo"))
    })?;
    assert_eq!(tips()?, before, "{:?}", tab.status);
    Ok(())
}

/// Undoing a restack refuses when the worktree a layer was checked out in has another branch
/// checked out since: resetting there would move that branch instead.
#[test]
fn undo_refuses_once_a_layers_worktree_switched_branch() -> Result<(), String> {
    use crate::git::stack::restack::{Moved, Place};
    let repo = TestRepo::new("undo wt switched")?;
    repo.stack(&["api"])?;
    repo.git(&["branch", "other", "main"])?;
    let wt = repo.dir.with_file_name(format!(
        "{} switched wt",
        repo.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&wt);
    repo.git(&["worktree", "add", "-q", &wt.to_string_lossy(), "api"])?;
    let result = (|| -> Result<(), String> {
        let api = repo.tip("api")?;
        let moved = [Moved {
            branch: "api".into(),
            before: repo.tip("main")?,
            after: api.clone(),
            base_before: None,
            base_after: repo.tip("main")?,
            place: Place::Worktree(wt.clone()),
        }];
        let (undo, _) = super::restack::undo_steps(&moved);
        crate::git::run(Some(&wt), &["checkout", "-q", "other"])?;
        let other = repo.tip("other")?;
        let (result, _) = crate::repo::operations::run_steps(
            repo.path(),
            &undo,
            crate::repo::OpKind::plain(),
            None,
        );
        let err = result.err().unwrap_or_default();
        assert!(err.contains("in the worktree at"), "{err}");
        assert_eq!((repo.tip("api")?, repo.tip("other")?), (api, other));
        // Back on api there, it goes through.
        crate::git::run(Some(&wt), &["checkout", "-q", "api"])?;
        let (result, _) = crate::repo::operations::run_steps(
            repo.path(),
            &undo,
            crate::repo::OpKind::plain(),
            None,
        );
        result?;
        assert_eq!(repo.tip("api")?, repo.tip("main")?);
        Ok(())
    })();
    let _ = repo.git(&["worktree", "remove", "--force", &wt.to_string_lossy()]);
    let _ = std::fs::remove_dir_all(&wt);
    result
}

/// The restack's undo, run as the tab runs an undo.
fn run_undo(repo: &TestRepo, steps: &[Vec<String>]) -> Result<String, String> {
    let (result, _) =
        crate::repo::operations::run_steps(repo.path(), steps, crate::repo::OpKind::plain(), None);
    result
}

/// Undo neither moves a branch checked out since under its files, nor half undoes: a reset
/// that refuses comes before any branch has moved.
#[test]
fn undo_refuses_rather_than_move_a_checked_out_branch_or_stop_halfway() -> Result<(), String> {
    use crate::git::stack::restack::{Moved, Place};
    let repo = TestRepo::new("undo checked out since")?;
    repo.stack(&["b", "c"])?;
    let main = repo.tip("main")?;
    let (b, c) = (repo.tip("b")?, repo.tip("c")?);
    let moved = |place_b: Place, place_c: Place| {
        vec![
            Moved {
                branch: "b".into(),
                before: main.clone(),
                after: b.clone(),
                base_before: None,
                base_after: main.clone(),
                place: place_b,
            },
            Moved {
                branch: "c".into(),
                before: b.clone(),
                after: c.clone(),
                base_before: None,
                base_after: b.clone(),
                place: place_c,
            },
        ]
    };
    // b checked out here since the restack, which saw it nowhere.
    repo.checkout("b")?;
    let (undo, _) = super::restack::undo_steps(&moved(Place::Nowhere, Place::Nowhere));
    assert!(run_undo(&repo, &undo).is_err());
    assert_eq!((repo.tip("b")?, repo.tip("c")?), (b.clone(), c.clone()));
    assert!(repo.git(&["status", "--porcelain"])?.trim().is_empty());
    // c checked out here, with a change its reset would lose: refused before b moves.
    repo.checkout("c")?;
    std::fs::write(repo.dir.join("c.txt"), "changed\n").map_err(|e| e.to_string())?;
    let (undo, _) = super::restack::undo_steps(&moved(Place::Nowhere, Place::Here));
    assert!(run_undo(&repo, &undo).is_err());
    assert_eq!((repo.tip("b")?, repo.tip("c")?), (b.clone(), c));
    // Without the change, it all goes back.
    repo.git(&["checkout", "--", "c.txt"])?;
    run_undo(&repo, &undo)?;
    assert_eq!((repo.tip("b")?, repo.tip("c")?), (main, b));
    Ok(())
}

/// A repository on no forge gitcraft knows has nowhere to publish to: said, nothing opened.
#[test]
fn publishing_without_a_forge_says_so() -> Result<(), String> {
    let repo = TestRepo::new("tab publish no forge")?;
    repo.stack(&["api"])?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_some())?;
    tab.apply(Action::Stack(StackAct::OpenPublish("api".into())), &ctx);
    assert!(tab.stacks.publish.is_none());
    let status = tab.status.clone().unwrap_or_default();
    assert!(
        status.1 && status.0.contains("nowhere to publish"),
        "{status:?}"
    );
    Ok(())
}
