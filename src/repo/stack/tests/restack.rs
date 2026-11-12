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
            t.status.as_deref().is_some_and(|s| s.0.starts_with("Undo"))
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
            t.status.as_deref().is_some_and(|s| s.0.starts_with("Redo"))
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
        let status = tab.status.as_deref().cloned().unwrap_or_default();
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
    use crate::repo::hang_diagnostics::{run, trace};
    let _trace = trace("stack-update-refs-undo");
    let repo = TestRepo::new("tab update refs undo")?;
    repo.stack(&["schema", "api", "ui"])?;
    repo.commit("m2.txt", "2\n", "main moves on")?;
    repo.checkout("ui")?;
    let before = (repo.tip("schema")?, repo.tip("api")?, repo.tip("ui")?);
    let ctx = egui::Context::default();
    let mut tab = run(format_args!("open stack tab"), || open(&repo, &ctx))?;
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
    run(format_args!("prepare interactive rebase"), || {
        tab.apply(Action::RunRebase, &ctx)
    });
    run(format_args!("review interactive rebase"), || {
        tab.review_rebase_for_test(&ctx, "ui", &before.2, &onto)
    })?;
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
    let accepted = run(format_args!("click interactive confirmation"), || {
        tab.accept_rebase_for_test(&ctx, "ui", &before.2, &onto)
    })?;
    run(format_args!("start confirmed rebase"), || tab.apply(accepted, &ctx));
    settle(&mut tab, &ctx, |t| t.rebase.is_none() && t.status.is_some())?;
    assert!(is_ancestor(&repo, "main", "schema") && is_ancestor(&repo, "main", "api"));
    let tips = || -> Result<(String, String, String), String> {
        Ok((repo.tip("schema")?, repo.tip("api")?, repo.tip("ui")?))
    };
    tab.apply(Action::Undo, &ctx);
    settle(&mut tab, &ctx, |t| {
        t.status.as_deref().is_some_and(|s| s.0.starts_with("Undo"))
    })?;
    assert_eq!(tips()?, before, "{:?}", tab.status);
    tab.apply(Action::Redo, &ctx);
    settle(&mut tab, &ctx, |t| {
        t.status.as_deref().is_some_and(|s| s.0.starts_with("Redo"))
    })?;
    assert!(is_ancestor(&repo, "main", "schema") && is_ancestor(&repo, "schema", "ui"));
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"])?.trim(), "ui");
    // And undone again, after the redo.
    tab.apply(Action::Undo, &ctx);
    settle(&mut tab, &ctx, |t| {
        t.status.as_deref().is_some_and(|s| s.0.starts_with("Undo"))
    })?;
    assert_eq!(tips()?, before, "{:?}", tab.status);
    run(format_args!("drop stack tab"), || drop(tab));
    run(format_args!("drop stack fixture"), || drop(repo));
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
